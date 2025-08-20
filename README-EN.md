# gRPC Benchmark Tool (Rust Version)

A high-performance gRPC benchmark tool implemented in Rust for testing Solana Yellowstone gRPC endpoints and Jito Block Engine APIs.

📖 [中文版本](README-rust.md)

## Features

- **gRPC Comparison**: Compare multiple Solana Yellowstone gRPC endpoints for performance and reliability
- **Latency Testing**: Measure ping latencies with configurable concurrency levels
- **Jito Benchmark**: HTTP API benchmarking for Jito Block Engine endpoints
- **Real-time Statistics**: Comprehensive performance metrics including percentiles and standard deviations
- **Concurrent Testing**: Support for high-concurrency testing scenarios

## Installation

### Prerequisites

- Rust 1.70+ (install via [rustup](https://rustup.rs/))

### Build from Source

```bash
git clone <repository-url>
cd grpc-benchmark
cargo build --release
```

## Usage

### 1. gRPC Endpoint Comparison

Compare multiple gRPC endpoints to determine which provides the best performance:

```bash
# Using environment variables
export GRPC_URL_1="https://solana-yellowstone-grpc.publicnode.com:443"
export GRPC_NAME_1="Public_Node"
export GRPC_TOKEN_1="your-token-1"

export GRPC_URL_2="https://endpoint2.example.com:443"
export GRPC_NAME_2="Custom_Node"
export GRPC_TOKEN_2="your-token-2"

cargo run --bin grpc-comparison
```

### 2. Latency Testing

Test ping latencies to a single gRPC endpoint:

```bash
# Using environment variables
export GRPC_URL="https://solana-yellowstone-grpc.publicnode.com:443"
export TOTAL_ROUNDS=100

cargo run --bin latency-test
```

### 3. Jito Block Engine Benchmark

Benchmark Jito Block Engine HTTP API performance:

```bash
# Using environment variables
export JITO_URL="https://amsterdam.mainnet.block-engine.jito.wtf"
export JITO_CONCURRENCY=10

cargo run --bin benchmark-jito
```

## Configuration

### Environment Variables

Create a `.env` file based on `.env.example`:

```bash
cp .env.example .env
```

Key configuration options:

- `GRPC_URL`: Default gRPC endpoint URL
- `GRPC_TOKEN`: gRPC endpoint authentication token
- `GRPC_URL_*`, `GRPC_NAME_*`, `GRPC_TOKEN_*`: Multi-endpoint configuration
- `TOTAL_ROUNDS`: Number of ping requests (default: 50)
- `CONCURRENCY`: Concurrent request limit (default: 10)
- `GRPC_COMPARISON_DURATION_SEC`: Comparison test duration (default: 30 seconds)
- `JITO_URL`: Jito Block Engine endpoint
- `JITO_CONCURRENCY`: Jito API concurrency level

### Quick Start Examples

```bash
# 1. Configure environment variables
export JITO_URL="https://amsterdam.mainnet.block-engine.jito.wtf"
export JITO_CONCURRENCY=10

# 2. Run Jito benchmark
cargo run --bin benchmark-jito

# 3. Configure gRPC comparison test
export GRPC_URL_1="http://64.130.37.195:10900"
export GRPC_NAME_1="Custom_Node"
export GRPC_URL_2="https://solana-yellowstone-grpc.publicnode.com:443"
export GRPC_NAME_2="Public_Node"
export GRPC_COMPARISON_DURATION_SEC=30

# 4. Run gRPC comparison test
cargo run --bin grpc-comparison-real
```

## Output Examples

### gRPC Comparison Results

```
===== Endpoint Performance Comparison =====
Custom_Node     : First received  65.23%, Average lag when behind   8.45ms, Overall average latency   3.67ms
Public_Node     : First received  34.77%, Average lag when behind  15.91ms, Overall average latency   7.23ms
```

### Latency Test Results

```
Latency Statistics:
  Average latency: 45.67ms
  Minimum latency: 23.12ms
  Maximum latency: 89.34ms
  Standard deviation: 12.45ms
  Median (p50): 43.21ms
  Percentile (p90): 65.78ms
  Percentile (p99): 82.45ms
  Sample count: 100
```

### Jito Benchmark Results

```
Jito URL: https://amsterdam.mainnet.block-engine.jito.wtf
Request concurrency: 10/s
Statistics output every 10 seconds, please wait...
Statistics - Past 10 seconds: Total requests:  100, Successful responses:   85, Average per second:  8.5, 429 errors:  15
Statistics - Past 10 seconds: Total requests:  100, Successful responses:   92, Average per second:  9.2, 429 errors:   8
```

## Performance Features

- **Zero-copy Operations**: Efficient memory usage with minimal allocations
- **Async/Await**: Non-blocking I/O for maximum concurrency
- **Connection Pooling**: Reuse connections for better performance
- **Real-time Metrics**: Live statistics during long-running tests
- **Error Handling**: Robust error recovery and reporting

## Architecture

```
grpc-benchmark-rust/
├── src/
│   ├── lib.rs              # Core library and protobuf definitions
│   ├── config.rs           # Configuration management
│   ├── stats.rs            # Statistics calculation
│   ├── grpc_client.rs      # gRPC client wrapper
│   ├── error.rs            # Error type definitions
│   └── bin/
│       ├── grpc_comparison.rs        # Multi-endpoint comparison tool
│       ├── grpc_comparison_real.rs   # Real-world comparison tool
│       ├── latency_test.rs           # Latency testing tool
│       └── benchmark_jito.rs         # Jito API benchmark tool
├── Cargo.toml              # Rust dependencies
└── .env.example            # Environment variable configuration example
```

## Development

### Running Tests

```bash
cargo test
```

### Building for Production

```bash
cargo build --release
```

### Adding New Benchmarks

1. Create a new binary in `src/bin/`
2. Use the shared modules from `src/lib.rs`
3. Add configuration options to `src/config.rs`
4. Update `Cargo.toml` to include the new binary

## Comparison with TypeScript Version

This Rust implementation provides:

- **3-5x better performance** due to native compilation
- **Lower memory usage** with zero-copy operations
- **Better concurrency** handling with tokio async runtime
- **Type safety** with Rust's ownership model
- **Smaller binary size** for distribution

## Contributing

1. Fork the repository
2. Create a feature branch
3. Add tests for new functionality
4. Run `cargo fmt` and `cargo clippy`
5. Submit a pull request

## License

[Same as original project]
