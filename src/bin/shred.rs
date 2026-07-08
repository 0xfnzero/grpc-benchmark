use clap::Parser;
use colored::*;
use grpc_benchmark::shred_client::{
    run_shred_stream, RawShredConfig, ShredDecodeMode, ShredEventKind,
};
use grpc_benchmark::Result;
use std::time::Duration;
use tokio::sync::mpsc;

#[derive(Debug, Parser)]
#[command(name = "shred")]
#[command(about = "Listen to raw Solana shred UDP and print decoded slot events")]
struct Args {
    #[arg(long, env = "SHRED_BIND", default_value = "0.0.0.0:20000")]
    bind: String,

    #[arg(long, env = "SHRED_DECODE_MODE", default_value = "ledger")]
    decode_mode: ShredDecodeMode,

    #[arg(long, env = "SHRED_DECODE_ENTRIES", default_value_t = true, action = clap::ArgAction::Set)]
    decode_entries: bool,

    #[arg(long, env = "SHRED_ENABLE_FEC", default_value_t = true, action = clap::ArgAction::Set)]
    enable_fec: bool,

    #[arg(long, env = "SHRED_DURATION_SEC", default_value_t = 30)]
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

    println!("{}", "🚀 Raw Shred Listener".bright_green().bold());
    println!("bind: {}", config.bind_addr);
    println!("decode mode: {}", config.decode_mode);
    println!("decode entries: {}", config.decode_entries);
    println!("fec: {}", config.enable_fec);
    println!("duration: {}s", args.duration_sec);
    println!("{}", "────────────────────────────────────────".dimmed());

    let (event_tx, mut event_rx) = mpsc::channel(4096);
    let handle = tokio::spawn(run_shred_stream(
        config,
        Duration::from_secs(args.duration_sec),
        event_tx,
    ));

    while let Some(event) = event_rx.recv().await {
        match event.kind {
            ShredEventKind::FirstShred { shred_type, index } => {
                println!(
                    "{} {} slot {} index {}",
                    "first".green(),
                    shred_type.yellow(),
                    event.slot.to_string().bright_white(),
                    index
                );
            }
            ShredEventKind::DecodedEntries {
                entry_count,
                transaction_count,
            } => {
                println!(
                    "{} slot {} entries {} txs {}",
                    "decoded".cyan(),
                    event.slot.to_string().bright_white(),
                    entry_count,
                    transaction_count
                );
            }
        }
    }

    let stats = handle
        .await
        .map_err(|e| grpc_benchmark::BenchmarkError::ConfigError(e.to_string()))??;

    println!("{}", "────────────────────────────────────────".dimmed());
    println!("{}", "Summary".bright_green().bold());
    println!("datagrams: {}", stats.datagrams_received);
    println!("valid shreds: {}", stats.valid_shreds);
    println!("parse errors: {}", stats.parse_errors);
    println!("unique slots: {}", stats.unique_slots);
    println!("decoded segments: {}", stats.decoded_segments);
    println!("decoded entries: {}", stats.decoded_entries);
    println!("decoded txs: {}", stats.decoded_transactions);
    println!("decode errors: {}", stats.decode_errors);

    Ok(())
}
