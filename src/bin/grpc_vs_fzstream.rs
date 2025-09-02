use grpc_benchmark::Result;
use fzstream_client::{FzStreamClient, StreamClientConfig};
use fzstream_common::EventTypeFilter;
use solana_streamer_sdk::streaming::event_parser::common::EventType;
use solana_streamer_sdk::match_event as solana_match_event;
use solana_streamer_sdk::streaming::event_parser::core::UnifiedEvent;
use solana_streamer_sdk::streaming::event_parser::protocols::BlockMetaEvent;
use solana_streamer_sdk::streaming::YellowstoneGrpc;
use solana_streamer_sdk::streaming::yellowstone_grpc::{TransactionFilter, AccountFilter};
use solana_streamer_sdk::streaming::event_parser::common::filter::EventTypeFilter as GrpcEventTypeFilter;
use solana_streamer_sdk::streaming::event_parser::common::types::EventType as GrpcEventType;
// 移除 tracing，直接使用 println!
use std::env;
use chrono::{DateTime, Local};
use colored::*;
use grpc_benchmark::output::ColoredOutput;
use std::time::{Duration, Instant};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::Mutex;
use tokio::time::{interval, sleep};


// Program IDs for major Solana DeFi protocols
#[allow(dead_code)]
const PUMPFUN_PROGRAM_ID: &str = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";
#[allow(dead_code)]
const PUMPSWAP_PROGRAM_ID: &str = "PSwpkKNJhTNm5CbhHTNNfCEEF7ZdA8fxh4Wj1S6GzPo";
#[allow(dead_code)]
const BONK_PROGRAM_ID: &str = "treaf4wWBBty3fHdyBpo35Mz84M8k3heKXmjmi9vFt5";
#[allow(dead_code)]
const RAYDIUM_CPMM_PROGRAM_ID: &str = "CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C";
#[allow(dead_code)]
const RAYDIUM_CLMM_PROGRAM_ID: &str = "CAMMCzo5YL8w4VFF8KVHrK22GGUQpMAS4ZnukSFGUvJ";
#[allow(dead_code)]
const RAYDIUM_AMM_V4_PROGRAM_ID: &str = "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8";

// 添加格式化日志函数，参考 grpc_comparison.rs
fn log_info(msg: &str) {
    let now: DateTime<Local> = Local::now();
    
    // 解析消息格式: "ENDPOINT_NAME 接收 slot XXX: 信息"
    if let Some(captures) = extract_log_parts(msg) {
        println!("{} {} {} {} {}", 
                format!("[{}]", now.format("%H:%M:%S%.3f")).white(),
                format!("{:width$}", captures.grpc_name, width = get_max_name_length()).bright_white(),
                captures.action.green(),
                captures.slot_info.yellow(),
                format!(": {}", captures.timing).magenta()
        );
    } else {
        // 如果不匹配模式，使用默认格式
        println!("{} {}", 
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
    *MAX_NAME_LENGTH.get().unwrap_or(&10)
}

struct LogParts {
    grpc_name: String,
    action: String,
    slot_info: String,
    timing: String,
}

fn extract_log_parts(msg: &str) -> Option<LogParts> {
    // 匹配格式: "ENDPOINT_NAME 接收 slot XXX: 信息"
    if let Some(pos) = msg.find("接收") {
        let grpc_name = msg[..pos].trim().to_string();
        let rest = &msg[pos..];
        
        if let Some(colon_pos) = rest.find(':') {
            let slot_part = rest[..colon_pos].trim().to_string();
            let timing_part = rest[colon_pos + 1..].trim().to_string();
            
            // 正确处理中文字符：去掉"接收"（2个中文字符）
            let slot_info = if slot_part.starts_with("接收") {
                slot_part.chars().skip(2).collect::<String>().trim().to_string()
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

#[derive(Debug, Clone)]
struct BlockData {
    endpoint: String,
    #[allow(dead_code)]
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
    #[allow(dead_code)]
    processed_slots: HashSet<u64>, // 记录已处理的slot，避免重复
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
            processed_slots: HashSet::new(), // 初始化已处理slots集合
        }
    }
}

#[derive(Debug, Clone)]
struct Endpoint {
    name: String,
    endpoint_type: EndpointType,
}

#[derive(Debug, Clone)]
enum EndpointType {
    FzStream { address: String, auth_token: String },
    Grpc { url: String, token: Option<String> },
}

async fn compare_endpoints(endpoints: Vec<Endpoint>, test_duration_sec: u64) -> Result<()> {
    // 计算最大端点名称长度用于对齐输出
    let max_name_length = endpoints.iter().map(|e| e.name.len()).max().unwrap_or(0);
    set_max_name_length(max_name_length);
    
    println!("🚀 Solana gRPC vs FzStream Benchmark Tool");
    println!("===============================\n");
    
    println!("📋 Configured Endpoints");
    println!("-------------------------");
    for endpoint in &endpoints {
        match &endpoint.endpoint_type {
            EndpointType::FzStream { address, .. } => {
                println!("🟡 {} - {}", endpoint.name, address);
            },
            EndpointType::Grpc { url, .. } => {
                println!("🟡 {} - {}", endpoint.name, url);
            }
        }
    }
    println!("ℹ Test duration: {} seconds", test_duration_sec);
    println!("────────────────────────────────────────────────────────────────────────────────");
    
    log_info("开始对比 gRPC vs FzStream 性能...");
    log_info(&format!("测试持续时间: {}秒", test_duration_sec));
    log_info(&format!(
        "测试端点: {}",
        endpoints.iter().map(|e| e.name.as_str()).collect::<Vec<_>>().join(", ")
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
    
    // 用于去重的同步Mutex，避免竞态条件
    let dedup_mutex = Arc::new(StdMutex::new(HashMap::<String, HashSet<u64>>::new()));

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
    let processed_slots = Arc::new(Mutex::new(HashSet::<u64>::new())); // 跟踪已处理的slot

    // 为每个端点创建连接和订阅
    let mut tasks = Vec::new();

    for endpoint in endpoints.clone() {
        let endpoint_name = endpoint.name.clone();
        let block_data_by_slot = block_data_by_slot.clone();
        let endpoint_stats = endpoint_stats.clone();
        let first_slot_received = first_slot_received.clone();
        let active_endpoints = active_endpoints.clone();
        let first_received_slots = first_received_slots.clone();
        let started_formal_stats = started_formal_stats.clone();
        let processed_slots = processed_slots.clone();
        let dedup_mutex = dedup_mutex.clone();
        let _test_end_time = end_time;

        match endpoint.endpoint_type {
            EndpointType::FzStream { address, auth_token } => {
                let task = tokio::spawn(async move {
                    log_info(&format!("连接到 FzStream: {}", address));

                    let mut client = FzStreamClient::builder()
                        .server_address(&address)
                        .auth_token(&auth_token)
                        .connection_timeout(Duration::from_secs(5))
                        .build()
                        .expect("Failed to create client");
                    
                    if let Err(e) = client.connect().await {
                        log_info(&format!("连接 {} 失败: {:?}", endpoint_name, e));
                        let mut stats = endpoint_stats.lock().await;
                        if let Some(stat) = stats.get_mut(&endpoint_name) {
                            stat.is_available = false;
                        }
                        let mut active = active_endpoints.lock().await;
                        active.remove(&endpoint_name);
                        return;
                    }

                    log_info(&format!("{} 连接成功", endpoint_name));
                    
                    // 创建事件回调 - 参考 basic_quic_test.rs 的简单模式
                    let endpoint_name_for_callback = endpoint_name.clone();
                    let block_data_by_slot_for_callback = block_data_by_slot.clone();
                    let endpoint_stats_for_callback = endpoint_stats.clone();
                    let first_slot_received_for_callback = first_slot_received.clone();
                    let active_endpoints_for_callback = active_endpoints.clone();
                    let first_received_slots_for_callback = first_received_slots.clone();
                    let started_formal_stats_for_callback = started_formal_stats.clone();
                    let processed_slots_for_callback = processed_slots.clone();
                    let dedup_mutex_for_callback = dedup_mutex.clone();
                    
                    let event_callback = move |event: Box<dyn UnifiedEvent>| {
                        
                        // 只处理 BlockMetaEvent
                        solana_match_event!(event, {
                            BlockMetaEvent => |e: BlockMetaEvent| {
                                let current_slot = e.slot;
                                let timestamp = Instant::now();

                                // 在同步回调中进行去重检查，避免竞态条件
                                let endpoint_name_clone = endpoint_name_for_callback.clone();
                                let endpoint_stats_clone = endpoint_stats_for_callback.clone();
                                
                                // 使用同步mutex进行去重检查，避免竞态条件
                                let should_process = {
                                    let mut dedup_map = dedup_mutex_for_callback.lock().unwrap();
                                    let endpoint_slots = dedup_map.entry(endpoint_name_clone.clone()).or_insert_with(HashSet::new);
                                    if endpoint_slots.contains(&current_slot) {
                                        false // 已处理过
                                    } else {
                                        endpoint_slots.insert(current_slot);
                                        true // 可以处理
                                    }
                                };
                                
                                if should_process {
                                    let block_data_by_slot_clone = block_data_by_slot_for_callback.clone();
                                    let endpoint_stats_clone = endpoint_stats_for_callback.clone();
                                    let first_slot_received_clone = first_slot_received_for_callback.clone();
                                    let active_endpoints_clone = active_endpoints_for_callback.clone();
                                    let first_received_slots_clone = first_received_slots_for_callback.clone();
                                    let started_formal_stats_clone = started_formal_stats_for_callback.clone();
                                    let processed_slots_clone = processed_slots_for_callback.clone();
                                    
                                    tokio::spawn(async move {
                                        handle_block_event(
                                            endpoint_name_clone,
                                            current_slot,
                                            timestamp,
                                            block_data_by_slot_clone,
                                            endpoint_stats_clone,
                                            first_slot_received_clone,
                                            active_endpoints_clone,
                                            first_received_slots_clone,
                                            started_formal_stats_clone,
                                            processed_slots_clone,
                                        ).await;
                                    });
                                }
                            },
                        });
                    };
                    
                    // 设置事件过滤器
                    let event_filter = EventTypeFilter::allow_only(vec![
                        EventType::BlockMeta,
                    ]);
                
                    let _handle = client.subscribe_with_filter(event_filter, event_callback).await.unwrap();   

                    // 给事件流一些时间来建立连接，参考 basic_quic_test.rs
                    sleep(Duration::from_millis(5000)).await;
                    
                    // 保持任务运行
                    let _ = _handle.await;

                    println!("================✅ 事件订阅成功==================");     
                });
                tasks.push(task);
            },

            EndpointType::Grpc { url, token } => {
                let task = tokio::spawn(async move {
                    log_info(&format!("连接到 gRPC: {}", url));
                    
                    let grpc_client = match YellowstoneGrpc::new(
                        url.clone(),
                        token.clone(),
                    ) {
                        Ok(client) => client,
                        Err(e) => {
                            log_info(&format!("连接 {} 失败: {:?}", endpoint_name, e));
                            let mut stats = endpoint_stats.lock().await;
                            if let Some(stat) = stats.get_mut(&endpoint_name) {
                                stat.is_available = false;
                            }
                            let mut active = active_endpoints.lock().await;
                            active.remove(&endpoint_name);
                            return;
                        }
                    };

                    // gRPC 连接逻辑 - 监听 BlockMeta 事件
                    // BlockMeta事件不需要特定的协议过滤
                    let protocols = vec![];

                    // BlockMeta 不需要特定的账户过滤
                    let transaction_filter = TransactionFilter {
                        account_include: vec![],
                        account_exclude: vec![],
                        account_required: vec![],
                    };
                    
                    let account_filter = AccountFilter {
                        account: vec![],
                        owner: vec![],
                    };
                    
                    log_info("🚀 Starting to listen for BlockMeta events...");
                    log_info("📡 Starting subscription...");

                    // 使用正确的EventType和事件回调
                    let event_type_filter = GrpcEventTypeFilter{
                        include: vec![GrpcEventType::BlockMeta],
                    };
                    
                    let endpoint_name_for_callback = endpoint_name.clone();
                    let block_data_by_slot_for_callback = block_data_by_slot.clone();
                    let endpoint_stats_for_callback = endpoint_stats.clone();
                    let first_slot_received_for_callback = first_slot_received.clone();
                    let active_endpoints_for_callback = active_endpoints.clone();
                    let first_received_slots_for_callback = first_received_slots.clone();
                    let started_formal_stats_for_callback = started_formal_stats.clone();
                    let processed_slots_for_callback = processed_slots.clone();
                    let dedup_mutex_for_callback = dedup_mutex.clone();
                    
                    let grpc_event_callback = move |event: Box<dyn UnifiedEvent>| {
                        let mut handled = false;
                        
                        // 只监听 BlockMetaEvent 与 FzStream 进行比较
                        solana_match_event!(event, {
                            BlockMetaEvent => |e: BlockMetaEvent| {
                                let current_slot = e.slot;
                                let timestamp = Instant::now();

                                // 在同步回调中进行去重检查，避免竞态条件
                                let endpoint_name_clone = endpoint_name_for_callback.clone();
                                let endpoint_stats_clone = endpoint_stats_for_callback.clone();
                                
                                // 使用同步mutex进行去重检查，避免竞态条件
                                let should_process = {
                                    let mut dedup_map = dedup_mutex_for_callback.lock().unwrap();
                                    let endpoint_slots = dedup_map.entry(endpoint_name_clone.clone()).or_insert_with(HashSet::new);
                                    if endpoint_slots.contains(&current_slot) {
                                        false // 已处理过
                                    } else {
                                        endpoint_slots.insert(current_slot);
                                        true // 可以处理
                                    }
                                };
                                
                                if should_process {
                                    let block_data_by_slot_clone = block_data_by_slot_for_callback.clone();
                                    let endpoint_stats_clone = endpoint_stats_for_callback.clone();
                                    let first_slot_received_clone = first_slot_received_for_callback.clone();
                                    let active_endpoints_clone = active_endpoints_for_callback.clone();
                                    let first_received_slots_clone = first_received_slots_for_callback.clone();
                                    let started_formal_stats_clone = started_formal_stats_for_callback.clone();
                                    let processed_slots_clone = processed_slots_for_callback.clone();
                                    
                                    // 使用单独的线程和 Tokio 运行时来执行异步任务
                                    std::thread::spawn(move || {
                                        let rt = tokio::runtime::Builder::new_current_thread()
                                            .enable_time()
                                            .enable_io()
                                            .build()
                                            .unwrap();
                                        rt.block_on(async move {
                                            handle_block_event(
                                                endpoint_name_clone,
                                                current_slot,
                                                timestamp,
                                                block_data_by_slot_clone,
                                                endpoint_stats_clone,
                                                first_slot_received_clone,
                                                active_endpoints_clone,
                                                first_received_slots_clone,
                                                started_formal_stats_clone,
                                                processed_slots_clone,
                                            ).await;
                                        });
                                    });
                                }
                                handled = true;
                            },
                        });
                        
                        if !handled {
                            // 其他事件类型暂不处理
                        }
                    };
                    
                    if let Err(e) = grpc_client.subscribe_events_immediate(
                        protocols,
                        None,
                        transaction_filter,
                        account_filter,
                        Some(event_type_filter),
                        None,
                        grpc_event_callback,
                    )
                    .await {
                        log_info(&format!("{} gRPC订阅失败: {:?}", endpoint_name, e));
                        return;
                    };
                    
                    log_info(&format!("{} 连接成功", endpoint_name));

                    // Keep connection alive
                    loop {
                        sleep(Duration::from_millis(100)).await;
                    }
                });
                tasks.push(task);
            }
        }
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
    
    // 生成统计报告
    let stats = endpoint_stats.lock().await;
    let mut endpoint_results = Vec::new();
    
    for (endpoint_name, stat) in stats.iter() {
        let total_received = stat.total_received;
        let first_received_count = stat.first_received;
        let behind_count = total_received - first_received_count;
        let first_percentage = if total_received > 0 { 
            (first_received_count as f64 / total_received as f64) * 100.0 
        } else { 0.0 };
        let behind_percentage = if total_received > 0 { 
            (behind_count as f64 / total_received as f64) * 100.0 
        } else { 0.0 };
        
        let avg_latency = if !stat.latencies.is_empty() {
            stat.latencies.iter().sum::<f64>() / stat.latencies.len() as f64
        } else { 0.0 };
        
        let min_latency = stat.latencies.iter().copied().fold(f64::INFINITY, f64::min);
        let max_latency = stat.latencies.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        
        let overall_avg_latency = if total_received > 0 {
            stat.total_latency / total_received as f64
        } else { 0.0 };

        output.subheader(&format!("📊 {} 性能分析", endpoint_name));
        output.metric("总接收区块数", &total_received.to_string(), "blocks");
        output.metric("首先接收区块数", &format!("{} ({:.2}%)", first_received_count, first_percentage), "blocks");
        output.metric("落后接收区块数", &format!("{} ({:.2}%)", behind_count, behind_percentage), "blocks");
        
        if !stat.latencies.is_empty() {
            output.info("ℹ 延迟统计 (相对于最快端点):");
            output.metric("  平均延迟", &format!("{:.2}", avg_latency), "ms");
            output.metric("  最小延迟", &format!("{:.2}", min_latency), "ms");
            output.metric("  最大延迟", &format!("{:.2}", max_latency), "ms");
        } else {
            output.success("该端点始终是最快的，没有延迟数据");
        }
        
        output.separator();
        
        endpoint_results.push((endpoint_name.clone(), first_percentage, avg_latency, overall_avg_latency, total_received));
    }
    
    let title = "🏆 端点性能对比";
    println!("{}", title.yellow().bold());
    println!("{}", "-".repeat(28).yellow());
    
    for (endpoint_name, first_percentage, avg_behind_latency, overall_avg_latency, _total_received) in endpoint_results {
        println!("{:width$} : 首先接收 {:6.2}%, 落后时平均延迟 {:7.2}ms, 总体平均延迟 {:7.2}ms", 
                endpoint_name, first_percentage, avg_behind_latency, overall_avg_latency,
                width = get_max_name_length());
    }
    
    output.separator();
    output.success("✓ 测试完成，正在关闭连接...");
    output.success("✓ 所有连接已关闭，测试结束");
    
    Ok(())
}

async fn handle_block_event(
    endpoint_name: String,
    current_slot: u64,
    timestamp: Instant,
    block_data_by_slot: Arc<Mutex<HashMap<u64, Vec<BlockData>>>>,
    endpoint_stats: Arc<Mutex<HashMap<String, EndpointStats>>>,
    first_slot_received: Arc<Mutex<HashMap<String, bool>>>,
    active_endpoints: Arc<Mutex<HashSet<String>>>,
    first_received_slots: Arc<Mutex<HashMap<String, u64>>>,
    started_formal_stats: Arc<Mutex<bool>>,
    processed_slots: Arc<Mutex<HashSet<u64>>>,
) {
    // 在handle_block_event中进行原子去重检查和数据添加，避免竞态条件
    {
        let mut block_data = block_data_by_slot.lock().await;
        if let Some(existing_data) = block_data.get(&current_slot) {
            if existing_data.iter().any(|bd| bd.endpoint == endpoint_name) {
                return;
            }
        }
        
        // 原子操作：检查通过后立即添加数据，避免并发竞态
        block_data
            .entry(current_slot)
            .or_insert_with(Vec::new)
            .push(BlockData {
                endpoint: endpoint_name.clone(),
                slot: current_slot,
                timestamp,
            });
    }

    // 标记此端点已收到数据
    let mut first_received = first_slot_received.lock().await;
    let already_received = first_received.get(&endpoint_name).copied().unwrap_or(false);
    
    if !already_received {
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
            endpoint_name,
            current_slot
        ));

        // 检查是否可以开始正式统计
        let started = *started_formal_stats.lock().await;
        if !started {
            let first_slots = first_received_slots.lock().await;
            let active = active_endpoints.lock().await;
            let received_data_count = first_slots.len();
            let total_active_endpoints = active.len();
            
            // 修改条件：只要有2个或以上端点接收到数据就开始统计
            // 原条件太严格，要求所有端点都接收到数据
            if received_data_count >= 2 {
                drop(first_slots);
                drop(active);

                let mut started = started_formal_stats.lock().await;
                *started = true;
                drop(started);

                let active = active_endpoints.lock().await;
                log_info(&format!("有{}个有效端点, 开始正式统计...", active.len()));
                drop(active);
            }
        }
    }

    let started = *started_formal_stats.lock().await;
    if !started {
        return;
    }

    // 等待其他端点数据以进行比较 - 增加等待时间以适应高延迟场景
    tokio::time::sleep(Duration::from_millis(500)).await;

    // 检查这个slot是否已经被处理过（避免重复输出）
    {
        let mut processed = processed_slots.lock().await;
        if processed.contains(&current_slot) {
            return;
        }
        processed.insert(current_slot);
    }

    // 进行实时统计 - 改为检查实际收到数据的端点数
    let active = active_endpoints.lock().await;

    let block_data = block_data_by_slot.lock().await;
    let block_data_list = match block_data.get(&current_slot) {
        Some(list) => list.clone(),
        None => return,
    };
    drop(block_data);

    let received_endpoints: HashSet<_> = block_data_list
        .iter()
        .map(|bd| bd.endpoint.clone())
        .collect();

    // 只有当有2个或以上端点收到同一个slot的数据时才进行比较
    // 这样才能计算延迟差异
    
    if received_endpoints.len() >= 2 {  // 至少有2个端点有数据才能比较
        let mut stats = endpoint_stats.lock().await;
        for endpoint in active.iter() {
            if block_data_list
                .iter()
                .any(|bd| &bd.endpoint == endpoint)
            {
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

        // 按时间戳排序，确保稳定的排序结果
        let mut sorted_data = active_endpoint_data.clone();
        sorted_data.sort_by(|a, b| {
            a.timestamp.cmp(&b.timestamp)
                .then_with(|| a.endpoint.cmp(&b.endpoint))
        });

        // 找到真正最早的端点（第一个）
        let first_endpoint = sorted_data
            .first()
            .unwrap();
        

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
                let latency = bd.timestamp.duration_since(first_endpoint.timestamp).as_nanos() as f64 / 1_000_000.0;
                if latency >= 0.01 { // 只显示大于0.01ms的延迟
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

#[tokio::main] 
async fn main() -> Result<()> {
    // 不初始化 tracing 来避免多余的日志输出
    
    let fzstream_address = env::var("FZSTREAM_SERVER_ADDRESS")
        .unwrap_or_else(|_| "64.130.37.195:2222".to_string());
    let grpc_url = env::var("GRPC_URL")
        .unwrap_or_else(|_| "https://solana-yellowstone-grpc.publicnode.com:443".to_string());
    let auth_token = env::var("AUTH_TOKEN").unwrap_or_else(|_| "demo_token_12345".to_string());
    let grpc_token = env::var("GRPC_TOKEN").ok();
    
    let test_duration = Duration::from_secs(30);

    let endpoints = vec![
        Endpoint {
            name: "FzStream".to_string(),
            endpoint_type: EndpointType::FzStream { 
                address: fzstream_address, 
                auth_token 
            },
        },
        Endpoint {
            name: "gRPC".to_string(),
            endpoint_type: EndpointType::Grpc { 
                url: grpc_url, 
                token: grpc_token 
            },
        },
    ];

    compare_endpoints(endpoints, test_duration.as_secs()).await
}