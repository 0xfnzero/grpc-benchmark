use clap::Parser;
use serde::{Deserialize, Serialize};

#[derive(Parser, Debug, Clone)]
#[command(name = "grpc-benchmark")]
#[command(about = "A gRPC benchmark tool for Solana networks")]
pub struct Config {
    /// gRPC service URL
    #[arg(long, env = "GRPC_URL")]
    pub grpc_url: Option<String>,

    /// gRPC authentication token (X-Token)
    #[arg(long, env = "GRPC_TOKEN")]
    pub grpc_token: Option<String>,

    /// Total number of ping requests to send
    #[arg(long, env = "TOTAL_ROUNDS", default_value = "50")]
    pub total_rounds: usize,

    /// Concurrency level (simultaneous requests)
    #[arg(long, env = "CONCURRENCY", default_value = "10")]
    pub concurrency: usize,

    /// Test duration in seconds (for comparison tests)
    #[arg(long, env = "GRPC_COMPARISON_DURATION_SEC", default_value = "30")]
    pub test_duration_sec: u64,

    /// Jito URL for block engine tests
    #[arg(long, env = "JITO_URL", default_value = "https://amsterdam.mainnet.block-engine.jito.wtf")]
    pub jito_url: String,

    /// Jito concurrency level
    #[arg(long, env = "JITO_CONCURRENCY", default_value = "10")]
    pub jito_concurrency: usize,

    /// Enable verbose logging
    #[arg(short, long)]
    pub verbose: bool,
}

impl Config {
    pub fn new() -> Self {
        dotenvy::dotenv().ok();
        Config::parse()
    }

    pub fn get_grpc_url(&self) -> String {
        self.grpc_url
            .clone()
            .unwrap_or_else(|| "https://solana-yellowstone-grpc.publicnode.com:443".to_string())
    }

    pub fn get_grpc_endpoints(&self) -> Vec<GrpcEndpoint> {
        let mut endpoints = Vec::new();
        
        // Check for environment variables with pattern GRPC_URL_*
        for (key, value) in std::env::vars() {
            if key.starts_with("GRPC_URL_") {
                let suffix = key.strip_prefix("GRPC_URL_").unwrap();
                let name = std::env::var(format!("GRPC_NAME_{}", suffix))
                    .unwrap_or_else(|_| format!("GRPC_{}", suffix));
                let token = std::env::var(format!("GRPC_TOKEN_{}", suffix)).ok();
                
                endpoints.push(GrpcEndpoint {
                    name,
                    url: value,
                    token,
                });
            }
        }

        // If no endpoints configured, use defaults
        if endpoints.is_empty() {
            endpoints.push(GrpcEndpoint {
                name: "GRPC_1".to_string(),
                url: "https://solana-yellowstone-grpc.publicnode.com:443".to_string(),
                token: None,
            });
            endpoints.push(GrpcEndpoint {
                name: "GRPC_2".to_string(),
                url: "https://solana-yellowstone-grpc.publicnode.com:443".to_string(),
                token: None,
            });
        }

        endpoints
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrpcEndpoint {
    pub name: String,
    pub url: String,
    pub token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JitoBundleRequest {
    pub jsonrpc: String,
    pub id: u32,
    pub method: String,
    pub params: Vec<serde_json::Value>,
}

impl Default for JitoBundleRequest {
    fn default() -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: 1,
            method: "getTipAccounts".to_string(),
            params: vec![],
        }
    }
}