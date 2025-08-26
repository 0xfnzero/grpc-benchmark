use crate::error::{BenchmarkError, Result};
use std::time::Duration;
use tracing::{debug, info};

use fzstream_client::{FzStreamClient, StreamClientConfig};
use fzstream_common::{EventType, EventTypeFilter};

pub struct FzsClient {
    endpoint_name: String,
    endpoint: String,
    auth_token: Option<String>,
}

impl FzsClient {
    pub async fn connect(endpoint: &str, auth_token: Option<&str>, endpoint_name: String) -> Result<Self> {
        info!("Connecting to {}: {}", endpoint_name, endpoint);

        // Test connection by creating a client and connecting
        let config = StreamClientConfig {
            endpoint: endpoint.to_string(),
            auth_token: auth_token.map(|s| s.to_string()),
            ..Default::default()
        };

        let mut test_client = FzStreamClient::with_config(config);
        
        // Test the connection
        test_client.connect().await
            .map_err(|e| BenchmarkError::ConfigError(format!("FzStream connection failed: {}", e)))?;

        // Connection successful, store the parameters
        Ok(Self {
            endpoint_name,
            endpoint: endpoint.to_string(),
            auth_token: auth_token.map(|s| s.to_string()),
        })
    }

    pub fn get_endpoint_name(&self) -> &str {
        &self.endpoint_name
    }

    pub fn get_endpoint(&self) -> &str {
        &self.endpoint
    }

    pub fn get_auth_token(&self) -> Option<&str> {
        self.auth_token.as_deref()
    }

    pub fn create_event_filter() -> EventTypeFilter {
        EventTypeFilter::include_only(vec![
            EventType::PumpSwapBuy,
            EventType::PumpSwapSell,
            EventType::PumpSwapCreate,
        ])
    }

    pub async fn create_client(&self) -> Result<FzStreamClient> {
        let config = StreamClientConfig {
            endpoint: self.endpoint.clone(),
            auth_token: self.auth_token.clone(),
            ..Default::default()
        };

        let mut client = FzStreamClient::with_config(config);
        client.connect().await
            .map_err(|e| BenchmarkError::ConfigError(format!("FzStream client creation failed: {}", e)))?;
        
        Ok(client)
    }
}