use thiserror::Error;

#[derive(Error, Debug)]
pub enum BenchmarkError {
    #[error("gRPC error: {0}")]
    GrpcError(#[from] tonic::Status),
    
    #[error("HTTP error: {0}")]
    HttpError(#[from] reqwest::Error),
    
    #[error("JSON parsing error: {0}")]
    JsonError(#[from] serde_json::Error),
    
    #[error("Configuration error: {0}")]
    ConfigError(String),
    
    #[error("Connection timeout")]
    Timeout,
    
    #[error("No data received")]
    NoData,
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, BenchmarkError>;