use anyhow::{bail, Result};
use chrono::{DateTime, Local};
use clap::Parser;
use dotenvy::dotenv;
use futures::StreamExt;
use grpc_benchmark::output::{ColoredOutput, EndpointStatus};
use std::collections::{HashMap, HashSet};
use std::env;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tokio::time::{interval, sleep};
use tonic::transport::ClientTlsConfig;
use yellowstone_grpc_client::GeyserGrpcClient;
use yellowstone_grpc_proto::prelude::{
    subscribe_update::UpdateOneof, CommitmentLevel, SubscribeRequest, SubscribeRequestFilterSlots,
};

// Initialize rustls crypto provider
use rustls;

fn init_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(
        long,
        env = "GRPC_COMPARISON_DURATION_SEC",
        default_value_t = 30,
        value_parser = clap::value_parser!(u64).range(1..)
    )]
    duration: u64,

    #[arg(long)]
    grpc_url_1: Option<String>,

    #[arg(long)]
    grpc_name_1: Option<String>,

    #[arg(long)]
    grpc_token_1: Option<String>,

    #[arg(long)]
    grpc_url_2: Option<String>,

    #[arg(long)]
    grpc_name_2: Option<String>,

    #[arg(long)]
    grpc_token_2: Option<String>,
}

#[derive(Debug, Clone)]
struct GrpcEndpoint {
    name: String,
    url: String,
    token: Option<String>,
}

fn ensure_unique_endpoint_names(endpoints: &[GrpcEndpoint]) -> Result<()> {
    let mut seen = HashSet::new();
    for endpoint in endpoints {
        let name = endpoint.name.trim();
        if name.is_empty() {
            bail!("endpoint names must not be empty");
        }
        if !seen.insert(name.to_string()) {
            bail!("duplicate endpoint name: {name}");
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct BlockData {
    endpoint: String,
    slot: u64,
    timestamp: Instant,
}

#[derive(Debug)]
struct EndpointStats {
    total_latency: f64,
    latencies: Vec<f64>,
    first_received: u64,
    total_received: u64,
    is_available: bool,
    has_received_data: bool,
    first_slot: Option<u64>,
}

impl EndpointStats {
    fn new() -> Self {
        Self {
            total_latency: 0.0,
            latencies: Vec::new(),
            first_received: 0,
            total_received: 0,
            is_available: true,
            has_received_data: false,
            first_slot: None,
        }
    }
}

fn log_info(msg: &str) {
    let now: DateTime<Local> = Local::now();
    use colored::*;

    // 解析消息格式: "GRPC_NAME 接收 slot XXX: 延迟/首次接收"
    if let Some(captures) = extract_log_parts(msg) {
        println!(
            "{} {} {} {} {}",
            format!("[{}]", now.format("%H:%M:%S%.3f")).white(),
            format!(
                "{:width$}",
                captures.grpc_name,
                width = get_max_name_length()
            )
            .bright_white(),
            captures.action.green(),
            captures.slot_info.yellow(),
            format!(": {}", captures.timing).magenta()
        );
    } else {
        // 如果不匹配模式，使用默认格式
        println!(
            "{} {}",
            format!("[{}]", now.format("%H:%M:%S%.3f")).white(),
            msg.green()
        );
    }
}

// 全局变量来存储最大名称长度
use std::sync::OnceLock;
static MAX_NAME_LENGTH: OnceLock<usize> = OnceLock::new();

fn set_max_name_length(length: usize) {
    let _ = MAX_NAME_LENGTH.set(length);
}

fn get_max_name_length() -> usize {
    *MAX_NAME_LENGTH.get().unwrap_or(&0)
}

struct LogParts {
    grpc_name: String,
    action: String,
    slot_info: String,
    timing: String,
}

fn extract_log_parts(msg: &str) -> Option<LogParts> {
    // 匹配格式: "GRPC_NAME 接收 slot XXX: 延迟/首次接收信息"
    if let Some(pos) = msg.find("接收") {
        let grpc_name = msg[..pos].trim().to_string();
        let rest = &msg[pos..];

        if let Some(colon_pos) = rest.find(':') {
            let slot_part = rest[..colon_pos].trim().to_string();
            let timing_part = rest[colon_pos + 1..].trim().to_string();

            // 正确处理中文字符：去掉"接收"（2个中文字符）
            let slot_info = if slot_part.starts_with("接收") {
                slot_part
                    .chars()
                    .skip(2)
                    .collect::<String>()
                    .trim()
                    .to_string()
            } else {
                slot_part
            };

            return Some(LogParts {
                grpc_name,
                action: "接收".to_string(),
                slot_info,
                timing: timing_part,
            });
        }
    }
    None
}

async fn compare_grpc_endpoints(
    endpoints: Vec<GrpcEndpoint>,
    test_duration_sec: u64,
) -> Result<()> {
    // 计算最大端点名称长度用于对齐输出
    let max_name_length = endpoints.iter().map(|e| e.name.len()).max().unwrap_or(0);
    set_max_name_length(max_name_length);

    log_info("开始对比多个 GRPC 服务性能...");
    log_info(&format!("测试持续时间: {}秒", test_duration_sec));
    log_info(&format!(
        "测试端点: {}",
        endpoints
            .iter()
            .map(|e| e.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    ));

    let start_time = Instant::now();
    let end_time = start_time + Duration::from_secs(test_duration_sec);

    let block_data_by_slot = Arc::new(Mutex::new(HashMap::<u64, Vec<BlockData>>::new()));
    let endpoint_stats = Arc::new(Mutex::new({
        let mut stats = HashMap::new();
        for endpoint in &endpoints {
            stats.insert(endpoint.name.clone(), EndpointStats::new());
        }
        stats
    }));

    let first_slot_received = Arc::new(Mutex::new({
        let mut received = HashMap::new();
        for endpoint in &endpoints {
            received.insert(endpoint.name.clone(), false);
        }
        received
    }));

    let active_endpoints = Arc::new(Mutex::new({
        let mut active = HashSet::new();
        for endpoint in &endpoints {
            active.insert(endpoint.name.clone());
        }
        active
    }));

    let first_received_slots = Arc::new(Mutex::new(HashMap::<String, u64>::new()));
    let started_formal_stats = Arc::new(Mutex::new(false));
    let pending_block_data = Arc::new(Mutex::new(HashMap::<u64, Vec<BlockData>>::new()));

    // 检查所有slot对齐的函数
    async fn check_slots_alignment(
        first_received_slots: Arc<Mutex<HashMap<String, u64>>>,
        endpoint_stats: Arc<Mutex<HashMap<String, EndpointStats>>>,
        active_endpoints: Arc<Mutex<HashSet<String>>>,
    ) -> bool {
        let max_slot_difference = 10u64;

        let first_slots = first_received_slots.lock().await;
        let mut stats = endpoint_stats.lock().await;
        let mut active = active_endpoints.lock().await;

        if first_slots.len() < 2 {
            return false;
        }

        let slot_values: Vec<u64> = first_slots.values().copied().collect();
        let max_slot = *slot_values.iter().max().unwrap();
        log_info(&format!(
            "使用最大slot {} 作为基准进行对比 (最新区块)",
            max_slot
        ));

        let mut invalid_endpoints = Vec::new();

        for (endpoint, slot) in first_slots.iter() {
            let difference = max_slot.saturating_sub(*slot);

            if difference > max_slot_difference {
                log_info(&format!(
                    "{} 的第一个slot ({}) 比基准值旧 (基准值: {}, 落后: {}个区块)",
                    endpoint, slot, max_slot, difference
                ));
                log_info(&format!("{} 被标记为异常端点，将不参与性能比较", endpoint));
                if let Some(stat) = stats.get_mut(endpoint) {
                    stat.is_available = false;
                }
                active.remove(endpoint);
                invalid_endpoints.push(endpoint.clone());
            }
        }

        if !invalid_endpoints.is_empty() {
            log_info(&format!(
                "以下端点因slot落后过多被排除: {}",
                invalid_endpoints.join(", ")
            ));

            if active.len() < 2 {
                log_info(&format!(
                    "剩余可用端点不足两个 (当前{}个), 无法进行对比分析",
                    active.len()
                ));
                return false;
            }
        }

        true
    }

    // 简化处理函数
    async fn process_collected_data_fn(
        pending_block_data: Arc<Mutex<HashMap<u64, Vec<BlockData>>>>,
        block_data_by_slot: Arc<Mutex<HashMap<u64, Vec<BlockData>>>>,
        active_endpoints: Arc<Mutex<HashSet<String>>>,
        endpoint_stats: Arc<Mutex<HashMap<String, EndpointStats>>>,
    ) {
        let mut pending = pending_block_data.lock().await;
        let block_data_by_slot_ref = block_data_by_slot.clone();
        let block_data = block_data_by_slot_ref.lock().await;
        let active = active_endpoints.lock().await;
        let mut stats = endpoint_stats.lock().await;

        for (slot, block_data_list) in pending.drain() {
            if block_data.contains_key(&slot) {
                continue;
            }

            let active_endpoint_data: Vec<_> = block_data_list
                .iter()
                .filter(|data| {
                    active.contains(&data.endpoint)
                        && stats
                            .get(&data.endpoint)
                            .map_or(false, |s| s.has_received_data)
                })
                .cloned()
                .collect();

            if active_endpoint_data.len() >= 2 {
                // 注意：total_received在主任务循环中计算，这里不重复计算

                let _earliest_timestamp = active_endpoint_data
                    .iter()
                    .map(|bd| bd.timestamp)
                    .min()
                    .unwrap();

                // 按时间戳排序，确保稳定的排序结果
                let mut sorted_data = active_endpoint_data.clone();
                sorted_data.sort_by(|a, b| {
                    a.timestamp
                        .cmp(&b.timestamp)
                        .then_with(|| a.endpoint.cmp(&b.endpoint))
                });

                // 找到真正最早的端点（第一个）
                let first_endpoint = sorted_data.first().unwrap();

                // 首先输出首次接收的端点
                if let Some(stat) = stats.get_mut(&first_endpoint.endpoint) {
                    stat.first_received += 1;
                    log_info(&format!(
                        "{:width$} 接收 slot {}: 首次接收",
                        first_endpoint.endpoint,
                        first_endpoint.slot,
                        width = get_max_name_length()
                    ));
                }

                // 然后按排序顺序输出延迟的端点
                for bd in &sorted_data {
                    if bd.endpoint != first_endpoint.endpoint {
                        let latency = bd
                            .timestamp
                            .duration_since(first_endpoint.timestamp)
                            .as_nanos() as f64
                            / 1_000_000.0;
                        if latency >= 0.01 {
                            // 只显示大于0.01ms的延迟
                            if let Some(stat) = stats.get_mut(&bd.endpoint) {
                                stat.latencies.push(latency);
                                stat.total_latency += latency;
                                log_info(&format!(
                                    "{:width$} 接收 slot {}: 延迟 {:>6.2}ms (相对于 {})",
                                    bd.endpoint,
                                    bd.slot,
                                    latency,
                                    first_endpoint.endpoint,
                                    width = get_max_name_length()
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    // 为每个端点创建连接和订阅
    let mut tasks = Vec::new();

    for endpoint in endpoints.clone() {
        log_info(&format!("连接到 {}: {}", endpoint.name, endpoint.url));

        // 使用官方示例的连接方法，明确配置TLS
        let mut client = match (async {
            let mut builder = GeyserGrpcClient::build_from_shared(endpoint.url.clone())?;

            // 配置认证token
            if let Some(token) = &endpoint.token {
                builder = builder.x_token(Some(token.clone()))?;
            }

            // 对于HTTPS端点，配置TLS
            if endpoint.url.starts_with("https://") {
                let tls_config = ClientTlsConfig::new().with_native_roots();
                builder = builder.tls_config(tls_config)?;
            }

            log_info(&format!("尝试连接到 {}...", endpoint.name));
            builder.connect().await.map_err(anyhow::Error::from)
        })
        .await
        {
            Ok(client) => client,
            Err(e) => {
                log_info(&format!("连接 {} 失败: {:?}", endpoint.name, e));
                let mut stats = endpoint_stats.lock().await;
                if let Some(stat) = stats.get_mut(&endpoint.name) {
                    stat.is_available = false;
                }
                let mut active = active_endpoints.lock().await;
                active.remove(&endpoint.name);
                continue;
            }
        };

        let endpoint_name = endpoint.name.clone();
        let block_data_by_slot = block_data_by_slot.clone();
        let endpoint_stats = endpoint_stats.clone();
        let first_slot_received = first_slot_received.clone();
        let active_endpoints = active_endpoints.clone();
        let first_received_slots = first_received_slots.clone();
        let started_formal_stats = started_formal_stats.clone();
        let pending_block_data = pending_block_data.clone();
        let test_end_time = end_time;

        let task = tokio::spawn(async move {
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

            let mut stream = match client.subscribe_once(request).await {
                Ok(stream) => stream,
                Err(e) => {
                    log_info(&format!("{} 订阅失败: {}", endpoint_name, e));
                    let mut stats = endpoint_stats.lock().await;
                    if let Some(stat) = stats.get_mut(&endpoint_name) {
                        stat.is_available = false;
                    }
                    let mut active = active_endpoints.lock().await;
                    active.remove(&endpoint_name);
                    return;
                }
            };

            while let Some(message) = stream.next().await {
                if Instant::now() >= test_end_time {
                    break;
                }

                match message {
                    Ok(update) => {
                        if let Some(UpdateOneof::Pong(_)) = update.update_oneof {
                            continue;
                        }

                        if let Some(UpdateOneof::Slot(slot_update)) = update.update_oneof {
                            let current_slot = slot_update.slot;
                            let timestamp = Instant::now();

                            // 标记此端点已收到数据
                            let mut first_received = first_slot_received.lock().await;
                            if !first_received.get(&endpoint_name).copied().unwrap_or(false) {
                                first_received.insert(endpoint_name.clone(), true);
                                drop(first_received);

                                let mut stats = endpoint_stats.lock().await;
                                if let Some(stat) = stats.get_mut(&endpoint_name) {
                                    stat.has_received_data = true;
                                    stat.first_slot = Some(current_slot);
                                }
                                drop(stats);

                                let mut first_slots = first_received_slots.lock().await;
                                first_slots.insert(endpoint_name.clone(), current_slot);
                                drop(first_slots);

                                log_info(&format!(
                                    "{} 成功接收到第一个 slot {}, 确认为可用端点",
                                    endpoint_name, current_slot
                                ));

                                // 检查是否可以开始正式统计
                                let started = *started_formal_stats.lock().await;
                                if !started {
                                    let first_slots = first_received_slots.lock().await;
                                    let active = active_endpoints.lock().await;
                                    let received_data_count = first_slots.len();
                                    let total_active_endpoints = active.len();

                                    if received_data_count == total_active_endpoints
                                        && received_data_count >= 2
                                    {
                                        drop(first_slots);
                                        drop(active);

                                        log_info(
                                            "所有活跃端点都已收到第一个slot，开始检查slot差异...",
                                        );

                                        if check_slots_alignment(
                                            first_received_slots.clone(),
                                            endpoint_stats.clone(),
                                            active_endpoints.clone(),
                                        )
                                        .await
                                        {
                                            let mut started = started_formal_stats.lock().await;
                                            *started = true;
                                            drop(started);

                                            let active = active_endpoints.lock().await;
                                            log_info(&format!(
                                                "有{}个有效端点, 开始正式统计...",
                                                active.len()
                                            ));
                                            drop(active);

                                            process_collected_data_fn(
                                                pending_block_data.clone(),
                                                block_data_by_slot.clone(),
                                                active_endpoints.clone(),
                                                endpoint_stats.clone(),
                                            )
                                            .await;
                                        }
                                    }
                                }
                            }

                            // 记录区块数据
                            let mut block_data = block_data_by_slot.lock().await;
                            block_data
                                .entry(current_slot)
                                .or_insert_with(Vec::new)
                                .push(BlockData {
                                    endpoint: endpoint_name.clone(),
                                    slot: current_slot,
                                    timestamp,
                                });
                            drop(block_data);

                            // 如果尚未开始正式统计，先保存数据
                            let started = *started_formal_stats.lock().await;
                            if !started {
                                let mut pending = pending_block_data.lock().await;
                                pending.entry(current_slot).or_insert_with(Vec::new).push(
                                    BlockData {
                                        endpoint: endpoint_name.clone(),
                                        slot: current_slot,
                                        timestamp,
                                    },
                                );
                                drop(pending);
                                continue;
                            }

                            // 等待其他端点数据以进行比较
                            sleep(Duration::from_millis(50)).await;

                            // 进行实时统计
                            let active = active_endpoints.lock().await;
                            if active.len() < 2 {
                                drop(active);
                                continue;
                            }

                            let block_data = block_data_by_slot.lock().await;
                            let block_data_list = match block_data.get(&current_slot) {
                                Some(list) => list.clone(),
                                None => continue,
                            };
                            drop(block_data);

                            let received_endpoints: HashSet<_> = block_data_list
                                .iter()
                                .map(|bd| bd.endpoint.clone())
                                .collect();

                            let all_active_endpoints_received =
                                active.iter().all(|ep| received_endpoints.contains(ep));

                            if all_active_endpoints_received {
                                let mut stats = endpoint_stats.lock().await;
                                for endpoint in active.iter() {
                                    if block_data_list.iter().any(|bd| &bd.endpoint == endpoint) {
                                        if let Some(stat) = stats.get_mut(endpoint) {
                                            stat.total_received += 1;
                                        }
                                    }
                                }
                                drop(stats);

                                let active_endpoint_data: Vec<_> = block_data_list
                                    .iter()
                                    .filter(|bd| active.contains(&bd.endpoint))
                                    .cloned()
                                    .collect();

                                let _earliest_timestamp = active_endpoint_data
                                    .iter()
                                    .map(|bd| bd.timestamp)
                                    .min()
                                    .unwrap();

                                // 按时间戳排序，确保稳定的排序结果
                                let mut sorted_data = active_endpoint_data.clone();
                                sorted_data.sort_by(|a, b| {
                                    a.timestamp
                                        .cmp(&b.timestamp)
                                        .then_with(|| a.endpoint.cmp(&b.endpoint))
                                });

                                // 找到真正最早的端点（第一个）
                                let first_endpoint = sorted_data.first().unwrap();

                                let mut stats = endpoint_stats.lock().await;

                                // 首先输出首次接收的端点
                                if let Some(stat) = stats.get_mut(&first_endpoint.endpoint) {
                                    stat.first_received += 1;
                                    log_info(&format!(
                                        "{:width$} 接收 slot {}: 首次接收",
                                        first_endpoint.endpoint,
                                        current_slot,
                                        width = get_max_name_length()
                                    ));
                                }

                                // 然后按排序顺序输出延迟的端点
                                for bd in &sorted_data {
                                    if bd.endpoint != first_endpoint.endpoint {
                                        let latency = bd
                                            .timestamp
                                            .duration_since(first_endpoint.timestamp)
                                            .as_nanos()
                                            as f64
                                            / 1_000_000.0;
                                        if latency >= 0.01 {
                                            // 只显示大于0.01ms的延迟
                                            if let Some(stat) = stats.get_mut(&bd.endpoint) {
                                                stat.latencies.push(latency);
                                                stat.total_latency += latency;
                                                log_info(&format!(
                                                    "{:width$} 接收 slot {}: 延迟 {:>6.2}ms (相对于 {})",
                                                    bd.endpoint,
                                                    current_slot,
                                                    latency,
                                                    first_endpoint.endpoint,
                                                    width = get_max_name_length()
                                                ));
                                            }
                                        }
                                    }
                                }
                                drop(stats);

                                // 清理旧数据
                                let mut block_data = block_data_by_slot.lock().await;
                                let old_slots: Vec<_> = block_data
                                    .keys()
                                    .filter(|&&slot| slot < current_slot.saturating_sub(100))
                                    .copied()
                                    .collect();
                                for slot in old_slots {
                                    block_data.remove(&slot);
                                }
                                drop(block_data);
                            }
                            drop(active);
                        }
                    }
                    Err(error) => {
                        log_info(&format!("{} GRPC 流错误: {}", endpoint_name, error));
                        let mut stats = endpoint_stats.lock().await;
                        if let Some(stat) = stats.get_mut(&endpoint_name) {
                            stat.is_available = false;
                        }
                        let mut active = active_endpoints.lock().await;
                        active.remove(&endpoint_name);

                        if active.len() < 2 {
                            log_info(&format!(
                                "由于 {} 出错, 活跃端点不足两个 (当前{}个), 无法进行对比分析",
                                endpoint_name,
                                active.len()
                            ));
                        }
                        break;
                    }
                }
            }
        });

        tasks.push(task);
    }

    // 进度监控
    let progress_task = tokio::spawn(async move {
        let mut progress_interval = interval(Duration::from_secs(5));
        loop {
            progress_interval.tick().await;

            let elapsed_sec = start_time.elapsed().as_secs();
            let remaining_sec = test_duration_sec.saturating_sub(elapsed_sec);
            let progress_percent = (elapsed_sec * 100) / test_duration_sec;

            if elapsed_sec > 0 && elapsed_sec % 5 == 0 {
                log_info(&format!(
                    "===== 测试进度: {}% [{}/{}秒] - 剩余时间: {}秒 =====",
                    progress_percent, elapsed_sec, test_duration_sec, remaining_sec
                ));
            }

            if Instant::now() >= end_time {
                break;
            }
        }
    });

    // 等待测试结束
    tokio::select! {
        _ = progress_task => {},
        _ = sleep(Duration::from_secs(test_duration_sec)) => {},
    }

    // 取消所有任务
    for task in tasks {
        task.abort();
    }

    let output = ColoredOutput::new();
    output.success("测试完成，正在分析结果...");
    output.separator();

    // 分析和输出结果
    let stats = endpoint_stats.lock().await;

    for endpoint in &endpoints {
        if let Some(stat) = stats.get(&endpoint.name) {
            if stat.total_received > 0 {
                let avg_latency = if stat.latencies.is_empty() {
                    0.0
                } else {
                    stat.total_latency / stat.latencies.len() as f64
                };

                let min_latency = stat.latencies.iter().fold(f64::INFINITY, |a, &b| a.min(b));
                let max_latency = stat
                    .latencies
                    .iter()
                    .fold(f64::NEG_INFINITY, |a, &b| a.max(b));

                output.subheader(&format!("📊 {} 性能分析", endpoint.name));
                output.metric("总接收区块数", &stat.total_received.to_string(), "blocks");

                let first_percent =
                    (stat.first_received as f64 / stat.total_received as f64) * 100.0;
                output.metric(
                    "首先接收区块数",
                    &format!("{} ({:.2}%)", stat.first_received, first_percent),
                    "blocks",
                );

                let delayed_count = stat.latencies.len();
                let delayed_percent = (delayed_count as f64 / stat.total_received as f64) * 100.0;
                output.metric(
                    "落后接收区块数",
                    &format!("{} ({:.2}%)", delayed_count, delayed_percent),
                    "blocks",
                );

                if !stat.latencies.is_empty() {
                    output.info("延迟统计 (相对于最快端点):");
                    output.metric("  平均延迟", &format!("{:.2}", avg_latency), "ms");
                    output.metric("  最小延迟", &format!("{:.2}", min_latency), "ms");
                    output.metric("  最大延迟", &format!("{:.2}", max_latency), "ms");
                } else {
                    output.success("该端点始终是最快的，没有延迟数据");
                }

                output.separator();
            } else {
                output.warning(&format!("{}: 没有收集到数据", endpoint.name));
            }
        }
    }

    // 性能对比
    use colored::*;
    let title = "🏆 端点性能对比";
    println!("{}", title.yellow().bold());
    println!("{}", "-".repeat(28).yellow());

    let mut sorted_endpoints: Vec<_> = endpoints
        .iter()
        .filter_map(|e| {
            stats.get(&e.name).and_then(|stat| {
                if stat.total_received > 0 {
                    Some((
                        &e.name,
                        stat,
                        stat.first_received as f64 / stat.total_received as f64,
                    ))
                } else {
                    None
                }
            })
        })
        .collect();

    sorted_endpoints.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

    if sorted_endpoints.len() >= 2 {
        for (name, stat, first_percent) in sorted_endpoints {
            let avg_latency_when_slower = if stat.latencies.is_empty() {
                0.0
            } else {
                stat.total_latency / stat.latencies.len() as f64
            };

            let avg_latency_total = if stat.total_received > 0 {
                stat.total_latency / stat.total_received as f64
            } else {
                0.0
            };

            println!(
                "{:12}: 首先接收 {:>6.2}%, 落后时平均延迟 {:>6.2}ms, 总体平均延迟 {:>6.2}ms",
                name,
                first_percent * 100.0,
                avg_latency_when_slower,
                avg_latency_total
            );
        }
    } else if sorted_endpoints.len() == 1 {
        output.warning(&format!(
            "只有一个可用端点 {}, 无法进行对比分析",
            sorted_endpoints[0].0
        ));
    } else {
        output.error("没有任何可用端点收集到数据，无法进行对比分析");
    }

    output.separator();
    output.success("测试完成，正在关闭连接...");
    output.success("所有连接已关闭，测试结束");

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize crypto provider first
    init_crypto_provider();

    // 加载环境变量
    dotenv().ok();

    let output = ColoredOutput::new();
    output.header("🚀 Solana gRPC Benchmark Tool");

    let args = Args::parse();

    // 收集所有GRPC端点
    let mut endpoints = Vec::new();

    // 从环境变量读取端点
    let mut env_vars: Vec<_> = env::vars()
        .filter(|(key, _)| key.starts_with("GRPC_URL_"))
        .collect();
    env_vars.sort_by(|(left, _), (right, _)| left.cmp(right));

    for (key, url) in env_vars {
        let index = key.strip_prefix("GRPC_URL_").unwrap();
        let name =
            env::var(format!("GRPC_NAME_{}", index)).unwrap_or_else(|_| format!("GRPC_{}", index));
        let token = env::var(format!("GRPC_TOKEN_{}", index)).ok();

        endpoints.push(GrpcEndpoint { name, url, token });
    }

    // 从命令行参数添加端点
    if let Some(url) = args.grpc_url_1 {
        endpoints.push(GrpcEndpoint {
            name: args.grpc_name_1.unwrap_or_else(|| "GRPC_1".to_string()),
            url,
            token: args.grpc_token_1,
        });
    }

    if let Some(url) = args.grpc_url_2 {
        endpoints.push(GrpcEndpoint {
            name: args.grpc_name_2.unwrap_or_else(|| "GRPC_2".to_string()),
            url,
            token: args.grpc_token_2,
        });
    }

    // 如果没有配置任何端点，使用默认值
    if endpoints.is_empty() {
        output.warning("No endpoints configured, using default endpoints");
        endpoints.push(GrpcEndpoint {
            name: "PublicNode_1".to_string(),
            url: "https://solana-yellowstone-grpc.publicnode.com:443".to_string(),
            token: None,
        });
        endpoints.push(GrpcEndpoint {
            name: "PublicNode_2".to_string(),
            url: "https://solana-yellowstone-grpc.publicnode.com:443".to_string(),
            token: None,
        });
    }

    ensure_unique_endpoint_names(&endpoints)?;

    output.subheader("📋 Configured Endpoints");
    for endpoint in &endpoints {
        output.endpoint_status(
            &format!("{} - {}", endpoint.name, endpoint.url),
            EndpointStatus::Connecting,
        );
    }

    output.info(&format!("Test duration: {} seconds", args.duration));
    output.separator();

    compare_grpc_endpoints(endpoints, args.duration).await
}
