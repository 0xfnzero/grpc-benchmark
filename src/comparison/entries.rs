//! Wire-compatible Entry subscription without a Jito proxy crate or running proxy.
//! The default path is a protocol name, not a dependency on Jito's infrastructure.
use crate::{BenchmarkError, Result};
use bincode::Options;
use solana_entry::entry::Entry;
use std::time::Duration;
use tonic::{
    codec::ProstCodec,
    transport::{Channel, ClientTlsConfig, Endpoint},
    Request, Streaming,
};

pub const DEFAULT_ENTRIES_METHOD: &str = "/shredstream.ShredstreamProxy/SubscribeEntries";
pub const MAX_MESSAGE_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, PartialEq, prost::Message)]
pub struct SubscribeEntriesRequest {}

#[derive(Clone, PartialEq, prost::Message)]
pub struct EntryMessage {
    #[prost(uint64, tag = "1")]
    pub slot: u64,
    #[prost(bytes = "vec", tag = "2")]
    pub entries: Vec<u8>,
}

pub async fn subscribe(
    url: String,
    method: String,
    token: Option<String>,
    header: String,
) -> Result<Streaming<EntryMessage>> {
    let path: tonic::codegen::http::uri::PathAndQuery = method
        .parse()
        .map_err(|_| BenchmarkError::ConfigError("invalid Entry RPC path".into()))?;
    let mut endpoint = Endpoint::from_shared(url.clone())
        .map_err(|_| BenchmarkError::ConfigError("invalid Entry endpoint URI".into()))?
        .connect_timeout(Duration::from_secs(10))
        .tcp_nodelay(true)
        .http2_keep_alive_interval(Duration::from_secs(30))
        .keep_alive_while_idle(true);
    if url.starts_with("https://") {
        endpoint = endpoint
            .tls_config(ClientTlsConfig::new().with_native_roots())
            .map_err(|_| BenchmarkError::ConfigError("cannot configure Entry TLS".into()))?;
    }
    let channel: Channel = endpoint
        .connect()
        .await
        .map_err(|_| BenchmarkError::ConfigError("cannot connect to Entry endpoint".into()))?;
    let mut client = tonic::client::Grpc::new(channel).max_decoding_message_size(MAX_MESSAGE_BYTES);
    client
        .ready()
        .await
        .map_err(|_| BenchmarkError::ConfigError("Entry service not ready".into()))?;
    let mut request = Request::new(SubscribeEntriesRequest {});
    if let Some(token) = token.filter(|value| !value.is_empty()) {
        let key: tonic::metadata::MetadataKey<tonic::metadata::Ascii> =
            header.parse().map_err(|_| {
                BenchmarkError::ConfigError("invalid Entry authentication header".into())
            })?;
        let value = token.parse().map_err(|_| {
            BenchmarkError::ConfigError("invalid Entry authentication value".into())
        })?;
        request.metadata_mut().insert(key, value);
    }
    let response = client
        .server_streaming(
            request,
            path,
            ProstCodec::<SubscribeEntriesRequest, EntryMessage>::default(),
        )
        .await?;
    Ok(response.into_inner())
}

/// Bounded, canonical bincode Vec<Entry>. Do not accept a truncated vector or trailing bytes.
pub fn decode_entries(bytes: &[u8]) -> Result<Vec<Entry>> {
    if bytes.len() < 8 || bytes.len() > MAX_MESSAGE_BYTES {
        return Err(BenchmarkError::ConfigError(
            "invalid Entry payload size".into(),
        ));
    }
    let count = u64::from_le_bytes(bytes[..8].try_into().expect("length checked"));
    if count > 50_000 {
        return Err(BenchmarkError::ConfigError(
            "Entry vector count exceeds limit".into(),
        ));
    }
    bincode::options().with_fixint_encoding().with_limit(bytes.len() as u64)
        .reject_trailing_bytes().deserialize(bytes)
        .map_err(|_| BenchmarkError::ConfigError("cannot decode bincode Vec<Entry>; check provider wire format and transaction version".into()))
}
