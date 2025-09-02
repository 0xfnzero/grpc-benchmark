use clap::Parser;
use futures::StreamExt;
use anyhow::Result;
use yellowstone_grpc_client::GeyserGrpcClient;
use yellowstone_grpc_proto::prelude::{
    subscribe_update::UpdateOneof, SubscribeRequest, SubscribeRequestPing,
};

use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::time::{sleep, timeout, interval};
use tracing::{error, info, warn};
use tonic::transport::ClientTlsConfig;

#[derive(Parser)]
#[command(name = "latency-test")]
#[command(about = "Ping/Pong latency test tool for gRPC endpoints")]
struct Args {
    /// gRPC service URL
    #[arg(long, env = "GRPC_URL")]
    grpc_url: Option<String>,

    /// gRPC authentication token (X-Token)
    #[arg(long, env = "GRPC_TOKEN")]
    grpc_token: Option<String>,

    /// Total number of ping requests to send
    #[arg(long, env = "TOTAL_ROUNDS", default_value = "10")]
    total_rounds: usize,

    /// Interval between pings in milliseconds
    #[arg(long, default_value = "1000")]
    ping_interval_ms: u64,

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,

    /// Test timeout in seconds
    #[arg(long, default_value = "120")]
    timeout: u64,
}

#[allow(dead_code)]
#[derive(Clone)]
struct PingInfo {
    id: i32,
    send_time: Instant,
}

#[derive(Default)]
struct LatencyStats {
    latencies: Vec<f64>,
    count: usize,
    mean: f64,
    min: f64,
    max: f64,
    std_dev: f64,
    median: f64,
    p90: f64,
    p99: f64,
}

impl LatencyStats {
    fn new() -> Self {
        Default::default()
    }

    fn add_latency(&mut self, latency: f64) {
        self.latencies.push(latency);
        self.count += 1;
    }

    fn calculate(&mut self) {
        if self.latencies.is_empty() {
            return;
        }

        self.latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());

        self.min = self.latencies[0];
        self.max = self.latencies[self.latencies.len() - 1];
        self.mean = self.latencies.iter().sum::<f64>() / self.latencies.len() as f64;

        let variance = self.latencies.iter()
            .map(|&x| (x - self.mean).powi(2))
            .sum::<f64>() / self.latencies.len() as f64;
        self.std_dev = variance.sqrt();

        self.median = self.percentile(50.0);
        self.p90 = self.percentile(90.0);
        self.p99 = self.percentile(99.0);
    }

    fn percentile(&self, p: f64) -> f64 {
        if self.latencies.is_empty() {
            return 0.0;
        }
        let index = (p / 100.0 * (self.latencies.len() - 1) as f64).round() as usize;
        self.latencies[index.min(self.latencies.len() - 1)]
    }

    fn display(&self) {
        println!("\n延迟统计:");
        println!("  平均延迟: {:.2}ms", self.mean);
        println!("  最小延迟: {:.2}ms", self.min);
        println!("  最大延迟: {:.2}ms", self.max);
        println!("  标准差: {:.2}ms", self.std_dev);
        println!("  中位数 (p50): {:.2}ms", self.median);
        println!("  百分位数 (p90): {:.2}ms", self.p90);
        println!("  百分位数 (p99): {:.2}ms", self.p99);
        println!("  样本数量: {}", self.count);
    }
}

fn create_ping_request(id: i32) -> SubscribeRequest {
    SubscribeRequest {
        ping: Some(SubscribeRequestPing { id }),
        ..Default::default()
    }
}

async fn test_grpc_latency_serial(
    url: &str,
    total_rounds: usize,
    ping_interval_ms: u64,
    token: Option<&str>,
    test_timeout: Duration,
) -> Result<()> {
    info!("开始串行延迟测试...");
    info!("总轮数: {}, 间隔: {}ms", total_rounds, ping_interval_ms);

    let mut latencies = Vec::new();
    let mut ping_id = 1;

    let result = timeout(test_timeout, async {
        for round in 1..=total_rounds {
            // Create client for each ping (simpler approach)
            let mut builder = GeyserGrpcClient::build_from_shared(url.to_string())?;

            if let Some(token) = token {
                builder = builder.x_token(Some(token.to_string()))?;
            }

            if url.starts_with("https://") {
                let tls_config = ClientTlsConfig::new().with_native_roots();
                builder = builder.tls_config(tls_config)?;
            }

            builder = builder
                .max_decoding_message_size(64 * 1024 * 1024)
                .connect_timeout(Duration::from_secs(10))
                .timeout(Duration::from_secs(30))
                .tcp_nodelay(true)
                .http2_keep_alive_interval(Duration::from_secs(30))
                .keep_alive_timeout(Duration::from_secs(5))
                .keep_alive_while_idle(true);

            let mut client = builder.connect().await?;

            let ping_request = create_ping_request(ping_id);
            let send_time = Instant::now();

            info!("发送 ping {}", ping_id);
            let mut stream = client.subscribe_once(ping_request).await?;

            // Wait for pong response with timeout
            let pong_timeout = Duration::from_secs(5);
            let pong_result = timeout(pong_timeout, async {
                while let Some(message) = stream.next().await {
                    match message {
                        Ok(update) => {
                            if let Some(UpdateOneof::Pong(pong)) = update.update_oneof {
                                if pong.id == ping_id {
                                    let pong_time = Instant::now();
                                    let latency = pong_time.duration_since(send_time).as_secs_f64() * 1000.0;
                                    return Ok(latency);
                                } else {
                                    warn!("收到错误ID的pong: 期望 {}, 收到 {}", ping_id, pong.id);
                                }
                            }
                        }
                        Err(e) => {
                            error!("Stream error: {}", e);
                            return Err(anyhow::anyhow!("Stream error: {}", e));
                        }
                    }
                }
                Err(anyhow::anyhow!("No pong received"))
            }).await;

            match pong_result {
                Ok(Ok(latency)) => {
                    latencies.push(latency);
                    info!("轮次 {}: {:.2}ms", round, latency);
                }
                Ok(Err(e)) => {
                    error!("轮次 {} 失败: {}", round, e);
                }
                Err(_) => {
                    error!("轮次 {} 超时", round);
                }
            }

            ping_id += 1;

            // Wait before next ping (except for the last one)
            if round < total_rounds {
                sleep(Duration::from_millis(ping_interval_ms)).await;
            }
        }

        Ok::<(), anyhow::Error>(())
    }).await;

    match result {
        Ok(_) => info!("测试完成"),
        Err(_) => {
            warn!("测试超时");
            return Err(anyhow::anyhow!("测试超时"));
        }
    }

    // 显示统计结果
    if latencies.is_empty() {
        warn!("没有收集到任何延迟数据");
        return Ok(());
    }

    let mut stats = LatencyStats::new();
    for latency in &latencies {
        stats.add_latency(*latency);
    }
    stats.calculate();
    stats.display();

    Ok(())
}

#[allow(dead_code)]
async fn test_grpc_latency_continuous(
    url: &str,
    total_rounds: usize,
    ping_interval_ms: u64,
    token: Option<&str>,
    test_timeout: Duration,
) -> Result<()> {
    info!("开始连续延迟测试 (单一连接)...");
    info!("总轮数: {}, 间隔: {}ms", total_rounds, ping_interval_ms);

    // Create single client connection
    let mut builder = GeyserGrpcClient::build_from_shared(url.to_string())?;

    if let Some(token) = token {
        builder = builder.x_token(Some(token.to_string()))?;
    }

    if url.starts_with("https://") {
        let tls_config = ClientTlsConfig::new().with_native_roots();
        builder = builder.tls_config(tls_config)?;
    }

    builder = builder
        .max_decoding_message_size(64 * 1024 * 1024)
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .tcp_nodelay(true)
        .http2_keep_alive_interval(Duration::from_secs(30))
        .keep_alive_timeout(Duration::from_secs(5))
        .keep_alive_while_idle(true);

    let mut client = builder.connect().await?;

    let mut latencies = Vec::new();
    let mut ping_timer = interval(Duration::from_millis(ping_interval_ms));
    let mut ping_id = 1;
    let mut pending_pings: HashMap<i32, PingInfo> = HashMap::new();

    // Start with an empty subscription to listen for pongs
    let initial_request = SubscribeRequest::default();
    let mut stream = client.subscribe_once(initial_request).await?;

    let result = timeout(test_timeout, async {
        loop {
            tokio::select! {
                // Send ping at regular intervals
                _ = ping_timer.tick() => {
                    if ping_id <= total_rounds {
                        info!("发送 ping {}", ping_id);

                        // For this test, we'll use a different approach
                        // Send individual ping requests
                        let ping_request = create_ping_request(ping_id as i32);

                        // Create a new client for each ping (workaround)
                        let mut ping_builder = GeyserGrpcClient::build_from_shared(url.to_string())?;
                        if let Some(token) = token {
                            ping_builder = ping_builder.x_token(Some(token.to_string()))?;
                        }
                        if url.starts_with("https://") {
                            let tls_config = ClientTlsConfig::new().with_native_roots();
                            ping_builder = ping_builder.tls_config(tls_config)?;
                        }
                        let mut ping_client = ping_builder.connect().await?;

                        let send_time = Instant::now();
                        pending_pings.insert(ping_id as i32, PingInfo {
                            id: ping_id as i32,
                            send_time,
                        });

                        // Subscribe with ping - this will eventually get a pong
                        let _ping_stream = ping_client.subscribe_once(ping_request).await?;

                        ping_id += 1;
                    }
                }

                // Listen for responses
                message = stream.next() => {
                    if let Some(message) = message {
                        match message {
                            Ok(update) => {
                                if let Some(UpdateOneof::Pong(pong)) = update.update_oneof {
                                    let pong_time = Instant::now();
                                    let pong_id = pong.id;

                                    if let Some(ping_info) = pending_pings.remove(&pong_id) {
                                        let latency = pong_time.duration_since(ping_info.send_time).as_secs_f64() * 1000.0;
                                        latencies.push(latency);
                                        info!("轮次 {}: {:.2}ms", latencies.len(), latency);

                                        if latencies.len() >= total_rounds {
                                            break;
                                        }
                                    } else {
                                        warn!("收到未知id的pong: {}", pong_id);
                                    }
                                }
                            }
                            Err(e) => {
                                error!("Stream error: {}", e);
                                break;
                            }
                        }
                    }
                }
            }
        }

        Ok::<(), anyhow::Error>(())
    }).await;

    match result {
        Ok(_) => info!("测试完成"),
        Err(_) => {
            warn!("测试超时");
            return Err(anyhow::anyhow!("测试超时"));
        }
    }

    // 显示统计结果
    if latencies.is_empty() {
        warn!("没有收集到任何延迟数据");
        return Ok(());
    }

    let mut stats = LatencyStats::new();
    for latency in &latencies {
        stats.add_latency(*latency);
    }
    stats.calculate();
    stats.display();

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize tracing
    let level = if args.verbose { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_env_filter(format!("latency_test={}", level))
        .init();

    let grpc_url = args.grpc_url
        .unwrap_or_else(|| "https://solana-yellowstone-grpc.publicnode.com:443".to_string());

    info!("gRPC URL: {}", grpc_url);
    if args.grpc_token.is_some() {
        info!("gRPC Token: 已配置");
    }
    info!("总轮数: {}", args.total_rounds);
    info!("Ping间隔: {}ms", args.ping_interval_ms);

    let test_timeout = Duration::from_secs(args.timeout);

    // Use serial approach for simplicity and reliability
    test_grpc_latency_serial(&grpc_url, args.total_rounds, args.ping_interval_ms, args.grpc_token.as_deref(), test_timeout).await?;

    Ok(())
}
