use clap::Parser;
use colored::*;
use grpc_benchmark::output::ColoredOutput;
use grpc_benchmark::shred_client::{
    run_shred_stream, RawShredConfig, ShredDecodeMode, ShredEventKind,
};
use grpc_benchmark::Result;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::time::interval;

#[derive(Debug, Parser)]
#[command(name = "shred-benchmark")]
#[command(about = "Benchmark raw Solana shred UDP throughput and decode results")]
struct Args {
    #[arg(long, env = "SHRED_BIND", default_value = "0.0.0.0:20000")]
    bind: String,

    #[arg(long, env = "SHRED_DECODE_MODE", default_value = "ledger")]
    decode_mode: ShredDecodeMode,

    #[arg(long, env = "SHRED_DECODE_ENTRIES", default_value_t = true, action = clap::ArgAction::Set)]
    decode_entries: bool,

    #[arg(long, env = "SHRED_ENABLE_FEC", default_value_t = true, action = clap::ArgAction::Set)]
    enable_fec: bool,

    #[arg(long, env = "SHRED_BENCHMARK_DURATION_SEC", default_value_t = 30)]
    duration_sec: u64,

    #[arg(long, env = "SHRED_RECV_BUFFER_BYTES", default_value_t = 128 * 1024 * 1024)]
    recv_buffer_bytes: usize,

    #[arg(long, env = "SHRED_UDP_PAYLOAD_PREFIX_SKIP", default_value_t = 0)]
    udp_payload_prefix_skip: usize,

    #[arg(long, env = "SHRED_MAX_TRACKED_SLOTS", default_value_t = 128)]
    max_tracked_slots: usize,

    #[arg(long, env = "SHRED_REASSEMBLY_GAP_TIMEOUT_MS", default_value_t = 500)]
    reassembly_gap_timeout_ms: u64,

    #[arg(long, env = "SHRED_STUCK_BATCH_TIMEOUT_MS", default_value_t = 50)]
    stuck_batch_timeout_ms: u64,

    #[arg(long, env = "SHRED_MAX_FEC_SETS_PER_SLOT", default_value_t = 32)]
    max_fec_sets_per_slot: usize,
}

#[tokio::main]
async fn main() -> Result<()> {
    let _ = dotenvy::dotenv();
    let args = Args::parse();
    let duration = Duration::from_secs(args.duration_sec);
    let config = RawShredConfig {
        bind_addr: args.bind,
        recv_buffer_bytes: args.recv_buffer_bytes,
        decode_mode: args.decode_mode,
        decode_entries: args.decode_entries,
        enable_fec: args.enable_fec,
        udp_payload_prefix_skip: args.udp_payload_prefix_skip,
        max_tracked_slots: args.max_tracked_slots,
        reassembly_gap_timeout_ms: args.reassembly_gap_timeout_ms,
        stuck_batch_timeout_ms: args.stuck_batch_timeout_ms,
        max_fec_sets_per_slot: args.max_fec_sets_per_slot,
        ..RawShredConfig::default()
    };

    let output = ColoredOutput::new();
    output.header("Raw Shred Benchmark");
    output.metric("UDP bind", &config.bind_addr, "");
    output.metric("Decode mode", &config.decode_mode.to_string(), "");
    output.metric("Decode entries", &config.decode_entries.to_string(), "");
    output.metric("FEC", &config.enable_fec.to_string(), "");
    output.metric("Duration", &args.duration_sec.to_string(), "s");
    output.separator();

    let (event_tx, mut event_rx) = mpsc::channel(8192);
    let started_at = Instant::now();
    let handle = tokio::spawn(run_shred_stream(config, duration, event_tx));

    let mut first_slot_events = 0u64;
    let mut decoded_events = 0u64;
    let mut decoded_entries = 0u64;
    let mut decoded_transactions = 0u64;
    let mut progress = interval(Duration::from_secs(5));

    loop {
        tokio::select! {
            event = event_rx.recv() => {
                let Some(event) = event else {
                    break;
                };
                match event.kind {
                    ShredEventKind::FirstShred { .. } => {
                        first_slot_events += 1;
                    }
                    ShredEventKind::DecodedEntries { entry_count, transaction_count } => {
                        decoded_events += 1;
                        decoded_entries += entry_count as u64;
                        decoded_transactions += transaction_count as u64;
                    }
                }
            }
            _ = progress.tick() => {
                let elapsed = started_at.elapsed().as_secs().max(1);
                println!(
                    "{} slots={} decoded_segments={} entries={} txs={} elapsed={}s",
                    "progress".bright_blue(),
                    first_slot_events,
                    decoded_events,
                    decoded_entries,
                    decoded_transactions,
                    elapsed
                );
            }
        }
    }

    let stats = handle
        .await
        .map_err(|e| grpc_benchmark::BenchmarkError::ConfigError(e.to_string()))??;

    let elapsed = started_at.elapsed().as_secs_f64().max(0.001);
    output.separator();
    output.subheader("Summary");
    output.metric("UDP datagrams", &stats.datagrams_received.to_string(), "");
    output.metric(
        "Datagrams/sec",
        &format!("{:.2}", stats.datagrams_received as f64 / elapsed),
        "",
    );
    output.metric("Valid shreds", &stats.valid_shreds.to_string(), "");
    output.metric("Parse errors", &stats.parse_errors.to_string(), "");
    output.metric("Unique slots", &stats.unique_slots.to_string(), "");
    output.metric("Decoded segments", &stats.decoded_segments.to_string(), "");
    output.metric("Decoded entries", &stats.decoded_entries.to_string(), "");
    output.metric("Decoded txs", &stats.decoded_transactions.to_string(), "");
    output.metric("Decode errors", &stats.decode_errors.to_string(), "");
    output.metric(
        "FEC recovered data shreds",
        &stats.fec_recovered_data_shreds.to_string(),
        "",
    );

    Ok(())
}
