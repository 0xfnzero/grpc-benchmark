use super::{entries, Mode, Observation, SampleKey};
use crate::{
    shred_client::{bind_shred_udp, RawShredConfig, ShredEngine},
    BenchmarkError, Result,
};
use clap::ValueEnum;
use serde::Serialize;
use std::{
    net::SocketAddr,
    time::{Duration, Instant},
};
use tokio::{net::UdpSocket, sync::mpsc};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Transport {
    Udp,
    EntriesGrpc,
}

// Deliberately not Debug: endpoints and tokens may contain credentials.
pub struct SourceConfig {
    pub transport: Transport,
    pub udp: RawShredConfig,
    pub url: Option<String>,
    pub method: String,
    pub token: Option<String>,
    pub token_header: String,
}

pub struct PreparedSource {
    inner: PreparedTransport,
}

enum PreparedTransport {
    Udp {
        socket: UdpSocket,
        engine: Box<ShredEngine>,
    },
    Entries(Box<tonic::Streaming<entries::EntryMessage>>),
}

#[derive(Debug, Default, Serialize)]
pub struct SourceRunStats {
    pub packets_or_messages: u64,
    pub invalid: u64,
    pub observations: u64,
    pub queue_dropped: u64,
    pub decode_errors: u64,
    pub fec_recovered: u64,
}

impl SourceConfig {
    pub async fn prepare(mut self, mode: Mode) -> Result<PreparedSource> {
        match self.transport {
            Transport::Udp => {
                self.udp.decode_entries = mode == Mode::Transaction;
                self.udp.collect_signatures = mode == Mode::Transaction;
                // Reordered slots remain decodable; slot completion is not a global watermark.
                self.udp.forward_slot_watermark = false;
                let addr: SocketAddr =
                    self.udp.bind_addr.parse().map_err(|_| {
                        BenchmarkError::ConfigError("invalid UDP bind address".into())
                    })?;
                let socket = bind_shred_udp(addr, self.udp.recv_buffer_bytes).await?;
                let engine = Box::new(ShredEngine::new(&self.udp));
                Ok(PreparedSource {
                    inner: PreparedTransport::Udp { socket, engine },
                })
            }
            Transport::EntriesGrpc => {
                if mode == Mode::Shred {
                    return Err(BenchmarkError::ConfigError("--mode shred requires two UDP sources; Entry streams do not contain shred IDs".into()));
                }
                let url = self.url.ok_or_else(|| {
                    BenchmarkError::ConfigError("Entry source requires --shred-url-N".into())
                })?;
                Ok(PreparedSource {
                    inner: PreparedTransport::Entries(Box::new(
                        entries::subscribe(url, self.method, self.token, self.token_header).await?,
                    )),
                })
            }
        }
    }
}

fn emit(
    tx: &mpsc::Sender<Observation>,
    source: usize,
    key: SampleKey,
    at: Instant,
    stats: &mut SourceRunStats,
) {
    stats.observations += 1;
    if tx.try_send(Observation { source, key, at }).is_err() {
        stats.queue_dropped += 1;
    }
}

impl PreparedSource {
    pub fn local_addr(&self) -> Result<Option<SocketAddr>> {
        match &self.inner {
            PreparedTransport::Udp { socket, .. } => Ok(Some(socket.local_addr()?)),
            PreparedTransport::Entries(_) => Ok(None),
        }
    }

    pub async fn run(
        mut self,
        source: usize,
        mode: Mode,
        stop_at: Instant,
        tx: mpsc::Sender<Observation>,
    ) -> Result<SourceRunStats> {
        let mut stats = SourceRunStats::default();
        let mut buffer = [0u8; 2048];
        let deadline = tokio::time::sleep_until(stop_at.into());
        tokio::pin!(deadline);
        let mut cleanup = tokio::time::interval(Duration::from_millis(100));
        cleanup.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            match &mut self.inner {
                PreparedTransport::Udp { socket, engine } => {
                    tokio::select! {
                        _ = &mut deadline => break,
                        _ = cleanup.tick() => { engine.evict_stale_slots(Instant::now()); }
                        packet = socket.recv_from(&mut buffer) => {
                            let (len, _) = packet?;
                            let received_at = Instant::now();
                            stats.packets_or_messages += 1;
                            let outcome = engine.push_packet(&buffer[..len], received_at);
                            let Some(slot) = outcome.slot else { stats.invalid += 1; continue; };
                            stats.decode_errors += outcome.decode_errors;
                            stats.fec_recovered += outcome.fec_recovered_data_shreds;
                            match mode {
                                Mode::Slot => emit(&tx, source, SampleKey::Slot(slot), received_at, &mut stats),
                                Mode::Shred => emit(&tx, source, SampleKey::Shred { slot, index: outcome.index.unwrap_or_default(), coding: outcome.shred_type == Some("code") }, received_at, &mut stats),
                                Mode::Transaction => {
                                    // Decode completion, not the last packet's receive time, measures usability.
                                    let decoded_at = Instant::now();
                                    for signature in outcome.signatures {
                                        emit(&tx, source, SampleKey::Transaction { slot, signature }, decoded_at, &mut stats);
                                    }
                                }
                            }
                        }
                    }
                }
                PreparedTransport::Entries(stream) => {
                    tokio::select! {
                        _ = &mut deadline => break,
                        message = stream.message() => {
                            let message = message?.ok_or_else(|| BenchmarkError::ConfigError(format!("source {} Entry stream ended before the benchmark deadline", source + 1)))?;
                            let received_at = Instant::now();
                            stats.packets_or_messages += 1;
                            // Validate even slot-mode messages so malformed Entry payloads cannot win.
                            let decoded = match entries::decode_entries(&message.entries) {
                                Ok(decoded) => decoded,
                                Err(_) => { stats.invalid += 1; stats.decode_errors += 1; continue; }
                            };
                            if mode == Mode::Slot {
                                emit(&tx, source, SampleKey::Slot(message.slot), received_at, &mut stats);
                            } else {
                                let decoded_at = Instant::now();
                                for entry in decoded {
                                    for transaction in entry.transactions {
                                        if let Some(signature) = transaction.first_signature {
                                            emit(&tx, source, SampleKey::Transaction { slot: message.slot, signature }, decoded_at, &mut stats);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(stats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_observation_queue_reports_overflow_without_blocking_receiver() {
        let (tx, _rx) = mpsc::channel(1);
        let mut stats = SourceRunStats::default();
        emit(&tx, 0, SampleKey::Slot(1), Instant::now(), &mut stats);
        emit(&tx, 0, SampleKey::Slot(2), Instant::now(), &mut stats);
        assert_eq!(stats.observations, 2);
        assert_eq!(stats.queue_dropped, 1);
    }
}
