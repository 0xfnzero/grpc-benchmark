use crate::error::{BenchmarkError, Result};
use std::collections::HashMap;
use std::time::Duration;
use tonic::transport::ClientTlsConfig;
use tracing::{debug, info};

use yellowstone_grpc_client::GeyserGrpcClient;
use yellowstone_grpc_proto::prelude::{
    CommitmentLevel, SubscribeRequest, SubscribeRequestFilterSlots, SubscribeUpdate,
};

pub struct GrpcClient {
    endpoint_name: String,
    url: String,
    token: Option<String>,
}

impl GrpcClient {
    pub async fn connect(url: &str, token: Option<&str>, endpoint_name: String) -> Result<Self> {
        info!("Connecting to {}: {}", endpoint_name, url);

        // Test connection by creating a client and connecting
        let mut builder = GeyserGrpcClient::build_from_shared(url.to_string())
            .map_err(|e| BenchmarkError::ConfigError(format!("Invalid URL: {}", e)))?;

        // Configure authentication token
        if let Some(token) = token {
            builder = builder.x_token(Some(token.to_string()))
                .map_err(|e| BenchmarkError::ConfigError(format!("Token error: {}", e)))?;
        }

        // Configure TLS for HTTPS endpoints
        if url.starts_with("https://") {
            let tls_config = ClientTlsConfig::new().with_native_roots();
            builder = builder.tls_config(tls_config)
                .map_err(|e| BenchmarkError::ConfigError(format!("TLS config error: {}", e)))?;
        }

        // Set performance optimization parameters
        builder = builder
            .max_decoding_message_size(64 * 1024 * 1024) // 64MB
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .tcp_nodelay(true)
            .http2_keep_alive_interval(Duration::from_secs(30))
            .keep_alive_timeout(Duration::from_secs(5))
            .keep_alive_while_idle(true);

        // Test the connection
        let _test_client = builder.connect().await
            .map_err(|e| BenchmarkError::GrpcError(tonic::Status::unavailable(e.to_string())))?;

        // Connection successful, store the parameters
        Ok(Self {
            endpoint_name,
            url: url.to_string(),
            token: token.map(|s| s.to_string()),
        })
    }

    pub fn get_endpoint_name(&self) -> &str {
        &self.endpoint_name
    }

    pub fn get_url(&self) -> &str {
        &self.url
    }

    pub fn get_token(&self) -> Option<&str> {
        self.token.as_deref()
    }

    pub fn create_slot_subscription_request() -> SubscribeRequest {
        let mut slots_filter = HashMap::new();
        slots_filter.insert("slot".to_string(), SubscribeRequestFilterSlots {
            filter_by_commitment: Some(true),
            ..Default::default()
        });

        SubscribeRequest {
            slots: slots_filter,
            commitment: Some(CommitmentLevel::Processed as i32),
            ..Default::default()
        }
    }
}
