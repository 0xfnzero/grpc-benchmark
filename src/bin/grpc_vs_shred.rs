use chrono::{DateTime, Local};
use clap::Parser;
use colored::*;
use futures::StreamExt;
use grpc_benchmark::output::ColoredOutput;
use grpc_benchmark::shred_client::{
    run_shred_stream, RawShredConfig, ShredDecodeMode, ShredEventKind,
};
use grpc_benchmark::{BenchmarkError, Result};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::time::{interval, sleep_until};
use tonic::transport::ClientTlsConfig;
use yellowstone_grpc_client::GeyserGrpcClient;
use yellowstone_grpc_proto::prelude::{
    subscribe_update::UpdateOneof, CommitmentLevel, SubscribeRequest, SubscribeRequestFilterSlots,
};

#[derive(Debug, Parser)]
#[command(name = "grpc-vs-shred")]
#[command(about = "Compare Yellowstone gRPC slot updates with raw shred UDP slot visibility")]
struct Args {
    #[arg(
        long,
        env = "GRPC_URL",
        default_value = "https://solana-yellowstone-grpc.publicnode.com:443"
    )]
    grpc_url: String,

    #[arg(long, env = "GRPC_TOKEN", hide_env_values = true)]
    grpc_token: Option<String>,

    #[arg(long, env = "GRPC_NAME", default_value = "gRPC")]
    grpc_name: String,

    #[arg(long, env = "SHRED_BIND", default_value = "0.0.0.0:20000")]
    shred_bind: String,

    #[arg(long, env = "SHRED_NAME", default_value = "Shred")]
    shred_name: String,

    #[arg(long, env = "SHRED_DECODE_MODE", default_value = "ledger")]
    shred_decode_mode: ShredDecodeMode,

    #[arg(long, env = "TEST_DURATION", default_value_t = 30)]
    duration_sec: u64,

    #[arg(long, env = "SHRED_RECV_BUFFER_BYTES", default_value_t = 128 * 1024 * 1024)]
    shred_recv_buffer_bytes: usize,

    #[arg(long, env = "SHRED_UDP_PAYLOAD_PREFIX_SKIP", default_value_t = 0)]
    shred_udp_payload_prefix_skip: usize,

    #[arg(long, env = "SHRED_MAX_TRACKED_SLOTS", default_value_t = 128)]
    shred_max_tracked_slots: usize,

    #[arg(long, env = "SHRED_REASSEMBLY_GAP_TIMEOUT_MS", default_value_t = 500)]
    shred_reassembly_gap_timeout_ms: u64,
}

#[derive(Debug, Clone)]
struct SlotUpdate {
    endpoint: String,
    slot: u64,
    received_at: Instant,
}

#[derive(Debug, Clone)]
struct BlockData {
    endpoint: String,
    timestamp: Instant,
}

#[derive(Debug)]
struct EndpointStats {
    latencies: Vec<f64>,
    first_received: u64,
    total_received: u64,
    first_slot: Option<u64>,
    has_received_data: bool,
}

impl EndpointStats {
    fn new() -> Self {
        Self {
            latencies: Vec::new(),
            first_received: 0,
            total_received: 0,
            first_slot: None,
            has_received_data: false,
        }
    }
}

fn log_info(message: &str) {
    let now: DateTime<Local> = Local::now();
    println!(
        "{} {}",
        format!("[{}]", now.format("%H:%M:%S%.3f")).white(),
        message
    );
}

async fn run_grpc_slot_stream(
    endpoint_name: String,
    url: String,
    token: Option<String>,
    duration: Duration,
    slot_tx: mpsc::Sender<SlotUpdate>,
) -> Result<()> {
    log_info(&format!("连接到 gRPC: {url}"));

    let mut builder = GeyserGrpcClient::build_from_shared(url.clone())
        .map_err(|e| BenchmarkError::ConfigError(format!("Invalid gRPC URL: {e}")))?;

    if let Some(token) = token.filter(|token| !token.trim().is_empty()) {
        builder = builder
            .x_token(Some(token))
            .map_err(|e| BenchmarkError::ConfigError(format!("gRPC token error: {e}")))?;
    }

    if url.starts_with("https://") {
        builder = builder
            .tls_config(ClientTlsConfig::new().with_native_roots())
            .map_err(|e| BenchmarkError::ConfigError(format!("gRPC TLS config error: {e}")))?;
    }

    builder = builder
        .max_decoding_message_size(64 * 1024 * 1024)
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .tcp_nodelay(true)
        .http2_keep_alive_interval(Duration::from_secs(30))
        .keep_alive_timeout(Duration::from_secs(5))
        .keep_alive_while_idle(true);

    let mut client = builder
        .connect()
        .await
        .map_err(|e| BenchmarkError::GrpcError(tonic::Status::unavailable(e.to_string())))?;

    let mut slots_filter = HashMap::new();
    slots_filter.insert(
        "slot".to_string(),
        SubscribeRequestFilterSlots {
            filter_by_commitment: Some(true),
            ..Default::default()
        },
    );
    let request = SubscribeRequest {
        slots: slots_filter,
        commitment: Some(CommitmentLevel::Processed as i32),
        ..Default::default()
    };

    let mut stream = client
        .subscribe_once(request)
        .await
        .map_err(|e| BenchmarkError::GrpcError(tonic::Status::unavailable(e.to_string())))?;
    log_info(&format!("{endpoint_name} 订阅成功"));

    let deadline = tokio::time::Instant::now() + duration;
    let mut stop = Box::pin(sleep_until(deadline));

    loop {
        tokio::select! {
            _ = &mut stop => break,
            message = stream.next() => {
                let Some(message) = message else {
                    break;
                };
                let update = message?;
                if let Some(UpdateOneof::Slot(slot_update)) = update.update_oneof {
                    let _ = slot_tx
                        .send(SlotUpdate {
                            endpoint: endpoint_name.clone(),
                            slot: slot_update.slot,
                            received_at: Instant::now(),
                        })
                        .await;
                }
            }
        }
    }

    Ok(())
}

async fn run_shred_slot_stream(
    endpoint_name: String,
    config: RawShredConfig,
    duration: Duration,
    slot_tx: mpsc::Sender<SlotUpdate>,
) -> Result<()> {
    log_info(&format!(
        "监听 Shred UDP: {} ({})",
        config.bind_addr, config.decode_mode
    ));

    let (event_tx, mut event_rx) = mpsc::channel(8192);
    let listener = tokio::spawn(run_shred_stream(config, duration, event_tx));

    while let Some(event) = event_rx.recv().await {
        if matches!(event.kind, ShredEventKind::FirstShred { .. }) {
            let _ = slot_tx
                .send(SlotUpdate {
                    endpoint: endpoint_name.clone(),
                    slot: event.slot,
                    received_at: event.received_at,
                })
                .await;
        }
    }

    let stats = listener
        .await
        .map_err(|e| BenchmarkError::ConfigError(e.to_string()))??;
    log_info(&format!(
        "{endpoint_name} UDP完成: datagrams={}, valid_shreds={}, unique_slots={}, parse_errors={}",
        stats.datagrams_received, stats.valid_shreds, stats.unique_slots, stats.parse_errors
    ));

    Ok(())
}

fn process_slot_if_ready(
    slot: u64,
    endpoint_names: &[String],
    block_data_by_slot: &HashMap<u64, Vec<BlockData>>,
    processed_slots: &mut HashSet<u64>,
    endpoint_stats: &mut HashMap<String, EndpointStats>,
) {
    if processed_slots.contains(&slot) {
        return;
    }

    let Some(block_data_list) = block_data_by_slot.get(&slot) else {
        return;
    };
    let received_endpoints: HashSet<_> = block_data_list
        .iter()
        .map(|block| block.endpoint.as_str())
        .collect();
    if !endpoint_names
        .iter()
        .all(|endpoint| received_endpoints.contains(endpoint.as_str()))
    {
        return;
    }

    processed_slots.insert(slot);

    for endpoint in endpoint_names {
        if let Some(stats) = endpoint_stats.get_mut(endpoint) {
            stats.total_received += 1;
        }
    }

    let mut sorted = block_data_list.clone();
    sorted.sort_by(|a, b| {
        a.timestamp
            .cmp(&b.timestamp)
            .then_with(|| a.endpoint.cmp(&b.endpoint))
    });
    let first = sorted.first().unwrap().clone();

    if let Some(stats) = endpoint_stats.get_mut(&first.endpoint) {
        stats.first_received += 1;
    }
    log_info(&format!(
        "{:width$} 接收 slot {}: 首次接收",
        first.endpoint,
        slot,
        width = max_endpoint_name_len(endpoint_names)
    ));

    for block in sorted.iter().skip(1) {
        let latency_ms =
            block.timestamp.duration_since(first.timestamp).as_nanos() as f64 / 1_000_000.0;
        if let Some(stats) = endpoint_stats.get_mut(&block.endpoint) {
            stats.latencies.push(latency_ms);
        }
        log_info(&format!(
            "{:width$} 接收 slot {}: 延迟 {:>6.2}ms (相对于 {})",
            block.endpoint,
            slot,
            latency_ms,
            first.endpoint,
            width = max_endpoint_name_len(endpoint_names)
        ));
    }
}

fn max_endpoint_name_len(endpoint_names: &[String]) -> usize {
    endpoint_names
        .iter()
        .map(|name| name.len())
        .max()
        .unwrap_or(8)
}

fn prune_old_slots(
    min_keep_slot: u64,
    dedup: &mut HashMap<String, HashSet<u64>>,
    block_data_by_slot: &mut HashMap<u64, Vec<BlockData>>,
    processed_slots: &mut HashSet<u64>,
) {
    block_data_by_slot.retain(|slot, _| *slot >= min_keep_slot);
    processed_slots.retain(|slot| *slot >= min_keep_slot);
    for slots in dedup.values_mut() {
        slots.retain(|slot| *slot >= min_keep_slot);
    }
}

fn ensure_unique_endpoint_names(endpoint_names: &[String]) -> Result<()> {
    let mut seen = HashSet::new();
    for name in endpoint_names {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(BenchmarkError::ConfigError(
                "endpoint names must not be empty".to_string(),
            ));
        }
        if !seen.insert(trimmed.to_string()) {
            return Err(BenchmarkError::ConfigError(format!(
                "duplicate endpoint name: {trimmed}"
            )));
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let _ = dotenvy::dotenv();
    let args = Args::parse();
    let duration = Duration::from_secs(args.duration_sec);
    let endpoint_names = vec![args.grpc_name.clone(), args.shred_name.clone()];
    ensure_unique_endpoint_names(&endpoint_names)?;

    println!(
        "{}",
        "🚀 Solana gRPC vs Shred Benchmark Tool"
            .bright_green()
            .bold()
    );
    println!("{}", "=====================================".bright_green());
    println!("gRPC:  {} - {}", args.grpc_name, args.grpc_url);
    println!("Shred: {} - {}", args.shred_name, args.shred_bind);
    println!("Duration: {}s", args.duration_sec);
    println!(
        "{}",
        "────────────────────────────────────────────────────────────────".dimmed()
    );

    let shred_config = RawShredConfig {
        bind_addr: args.shred_bind,
        recv_buffer_bytes: args.shred_recv_buffer_bytes,
        decode_mode: args.shred_decode_mode,
        decode_entries: false,
        udp_payload_prefix_skip: args.shred_udp_payload_prefix_skip,
        max_tracked_slots: args.shred_max_tracked_slots,
        reassembly_gap_timeout_ms: args.shred_reassembly_gap_timeout_ms,
        ..RawShredConfig::default()
    };

    let (slot_tx, mut slot_rx) = mpsc::channel(8192);

    let grpc_handle = tokio::spawn(run_grpc_slot_stream(
        args.grpc_name.clone(),
        args.grpc_url,
        args.grpc_token,
        duration,
        slot_tx.clone(),
    ));
    let shred_handle = tokio::spawn(run_shred_slot_stream(
        args.shred_name.clone(),
        shred_config,
        duration,
        slot_tx.clone(),
    ));
    drop(slot_tx);

    let mut endpoint_stats: HashMap<String, EndpointStats> = endpoint_names
        .iter()
        .map(|name| (name.clone(), EndpointStats::new()))
        .collect();
    let mut first_seen = HashSet::<String>::new();
    let mut dedup = HashMap::<String, HashSet<u64>>::new();
    let mut block_data_by_slot = HashMap::<u64, Vec<BlockData>>::new();
    let mut processed_slots = HashSet::<u64>::new();
    let mut started_stats = false;
    let started_at = Instant::now();
    let mut progress = interval(Duration::from_secs(5));

    loop {
        tokio::select! {
            update = slot_rx.recv() => {
                let Some(update) = update else {
                    break;
                };

                let endpoint_slots = dedup.entry(update.endpoint.clone()).or_default();
                if !endpoint_slots.insert(update.slot) {
                    continue;
                }

                block_data_by_slot
                    .entry(update.slot)
                    .or_default()
                    .push(BlockData {
                        endpoint: update.endpoint.clone(),
                        timestamp: update.received_at,
                    });

                if first_seen.insert(update.endpoint.clone()) {
                    if let Some(stats) = endpoint_stats.get_mut(&update.endpoint) {
                        stats.has_received_data = true;
                        stats.first_slot = Some(update.slot);
                    }
                    log_info(&format!(
                        "{} 成功接收到第一个 slot {}, 确认为可用端点",
                        update.endpoint, update.slot
                    ));
                }

                if !started_stats && first_seen.len() >= 2 {
                    started_stats = true;
                    log_info("两个端点都已收到 slot，开始正式统计...");
                }

                if started_stats {
                    process_slot_if_ready(
                        update.slot,
                        &endpoint_names,
                        &block_data_by_slot,
                        &mut processed_slots,
                        &mut endpoint_stats,
                    );
                }

                let min_keep_slot = update.slot.saturating_sub(200);
                prune_old_slots(
                    min_keep_slot,
                    &mut dedup,
                    &mut block_data_by_slot,
                    &mut processed_slots,
                );
            }
            _ = progress.tick() => {
                let elapsed = started_at.elapsed().as_secs();
                let remaining = args.duration_sec.saturating_sub(elapsed);
                log_info(&format!(
                    "===== 测试进度: {}/{}秒 - 剩余: {}秒 - 已对比slot: {} =====",
                    elapsed.min(args.duration_sec),
                    args.duration_sec,
                    remaining,
                    processed_slots.len()
                ));
            }
        }
    }

    for handle in [grpc_handle, shred_handle] {
        match handle.await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => log_info(&format!("任务结束时出错: {e}")),
            Err(e) => log_info(&format!("任务 join 失败: {e}")),
        }
    }

    let output = ColoredOutput::new();
    output.separator();
    output.success("测试完成，正在分析结果...");

    for endpoint in &endpoint_names {
        let stats = endpoint_stats
            .get(endpoint)
            .expect("endpoint stats should exist");
        let behind_count = stats.total_received.saturating_sub(stats.first_received);
        let first_percentage = if stats.total_received > 0 {
            (stats.first_received as f64 / stats.total_received as f64) * 100.0
        } else {
            0.0
        };
        let avg_latency = if stats.latencies.is_empty() {
            0.0
        } else {
            stats.latencies.iter().sum::<f64>() / stats.latencies.len() as f64
        };

        output.subheader(&format!("📊 {} 性能分析", endpoint));
        output.metric("总对齐 slot 数", &stats.total_received.to_string(), "slots");
        output.metric(
            "首先接收 slot 数",
            &format!("{} ({:.2}%)", stats.first_received, first_percentage),
            "slots",
        );
        output.metric("落后接收 slot 数", &behind_count.to_string(), "slots");
        if stats.latencies.is_empty() && stats.first_received > 0 {
            output.success("该端点没有落后样本");
        } else {
            output.metric("落后时平均延迟", &format!("{avg_latency:.2}"), "ms");
        }
    }

    output.separator();
    output.success("✓ 测试结束");

    Ok(())
}
