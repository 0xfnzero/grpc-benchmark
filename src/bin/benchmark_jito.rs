use anyhow::Result;
use clap::Parser;
use reqwest::Client;
use serde_json::json;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::{interval, Instant};
use tracing::{error, info};

#[derive(Debug, Parser)]
#[command(name = "benchmark-jito")]
#[command(about = "Benchmark Jito Block Engine HTTP request throughput")]
struct Args {
    #[arg(
        long,
        env = "JITO_URL",
        default_value = "https://amsterdam.mainnet.block-engine.jito.wtf"
    )]
    jito_url: String,

    #[arg(
        long,
        env = "JITO_CONCURRENCY",
        default_value_t = 10,
        value_parser = clap::value_parser!(u64).range(1..=100_000)
    )]
    concurrency: u64,

    #[arg(long, env = "JITO_DURATION_SEC", default_value_t = 0)]
    duration_sec: u64,
}

#[derive(Clone)]
struct Statistics {
    total_requests: Arc<AtomicU64>,
    successful_requests: Arc<AtomicU64>,
    error_429_count: Arc<AtomicU64>,
}

impl Statistics {
    fn new() -> Self {
        Self {
            total_requests: Arc::new(AtomicU64::new(0)),
            successful_requests: Arc::new(AtomicU64::new(0)),
            error_429_count: Arc::new(AtomicU64::new(0)),
        }
    }

    fn increment_total(&self) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
    }

    fn increment_success(&self) {
        self.successful_requests.fetch_add(1, Ordering::Relaxed);
    }

    fn increment_429(&self) {
        self.error_429_count.fetch_add(1, Ordering::Relaxed);
    }

    fn reset(&self) -> (u64, u64, u64) {
        let total = self.total_requests.swap(0, Ordering::Relaxed);
        let success = self.successful_requests.swap(0, Ordering::Relaxed);
        let error_429 = self.error_429_count.swap(0, Ordering::Relaxed);
        (total, success, error_429)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let args = Args::parse();

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()))
        .init();

    info!("Jito URL: {}", args.jito_url);
    info!("请求并发量: {}/s", args.concurrency);
    if args.duration_sec > 0 {
        info!("运行时长: {}s", args.duration_sec);
    }
    info!("每 10 秒输出统计信息, 请稍后...");

    // Create statistics
    let stats = Statistics::new();
    let stats_clone = stats.clone();

    // Create HTTP client
    let client = Client::new();

    // Start statistics logging task
    let stats_task = {
        let stats = stats_clone;
        tokio::spawn(async move {
            let mut stats_interval = interval(Duration::from_secs(10));
            loop {
                stats_interval.tick().await;

                let (total, success, error_429) = stats.reset();

                info!(
                    "统计 - 过去 10 秒：发送请求总量: {:>3}, 成功响应量: {:>3}, 平均每秒成功: {:>4.1}, 429 错误次数: {:>3}",
                    total,
                    success,
                    success as f64 / 10.0,
                    error_429
                );
            }
        })
    };

    // Main request sending loop
    let request_task = {
        let client = client.clone();
        let jito_url = args.jito_url.clone();
        let concurrency = args.concurrency;
        let stats = stats.clone();

        tokio::spawn(async move {
            let interval_per_request = Duration::from_micros((1_000_000 / concurrency).max(1));

            loop {
                // Send requests with staggered timing
                let mut handles = Vec::new();

                for i in 0..concurrency {
                    let client = client.clone();
                    let url = jito_url.clone();
                    let stats = stats.clone();

                    let handle = tokio::spawn(async move {
                        // Stagger requests evenly across the second
                        tokio::time::sleep(interval_per_request * i as u32).await;
                        send_request(client, &url, stats).await;
                    });

                    handles.push(handle);
                }

                // Wait for all requests in this batch to complete
                for handle in handles {
                    if let Err(e) = handle.await {
                        error!("Request task failed: {}", e);
                    }
                }

                // Wait for the next second
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        })
    };

    if args.duration_sec > 0 {
        tokio::time::sleep(Duration::from_secs(args.duration_sec)).await;
        stats_task.abort();
        request_task.abort();
    } else {
        // Wait for both tasks (this will run indefinitely)
        tokio::select! {
            _ = stats_task => {},
            _ = request_task => {},
        }
    }

    Ok(())
}

async fn send_request(client: Client, url: &str, stats: Statistics) {
    let start = Instant::now();

    // Create the Jito bundle request payload matching the TypeScript version
    let bundle = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getTipAccounts",
        "params": []
    });

    stats.increment_total();

    match client
        .post(format!("{}/api/v1/bundles", url))
        .header("Content-Type", "application/json")
        .json(&bundle)
        .send()
        .await
    {
        Ok(response) => {
            let duration = start.elapsed();

            if response.status() == 200 {
                stats.increment_success();
                // Uncomment for detailed logging:
                // info!("200: 请求成功, 耗时: {}ms", duration.as_millis());
            } else if response.status() == 429 {
                stats.increment_429();
                // Uncomment for detailed logging:
                // error!("429: 请求过于频繁, 耗时: {}ms", duration.as_millis());
            } else {
                error!(
                    "请求失败, 状态码: {}, 耗时: {}ms",
                    response.status(),
                    duration.as_millis()
                );
            }
        }
        Err(e) => {
            let duration = start.elapsed();

            // Check if it's a reqwest error with status 429
            if let Some(status) = e.status() {
                if status == 429 {
                    stats.increment_429();
                    // Uncomment for detailed logging:
                    // error!("429: 请求过于频繁, 耗时: {}ms", duration.as_millis());
                    return;
                }
            }

            error!("请求失败, 耗时: {}ms, 错误: {}", duration.as_millis(), e);
        }
    }
}
