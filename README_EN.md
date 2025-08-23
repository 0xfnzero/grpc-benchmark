# gRPC Benchmark Tool - Quick Start Guide
[中文](https://github.com/0xfnzero/grpc-benchmark/blob/main/README.md) | [English](https://github.com/0xfnzero/grpc-benchmark/blob/main/README_EN.md) | [Telegram](https://t.me/fnzero_group)

> grpc-benchmark is a Rust version built on [ChainBuff/grpc-benchmark-ts](https://github.com/ChainBuff/grpc-benchmark-ts)
, offering higher performance and a better deployment experience. ✅

## Overview

This guide will help you quickly download and run the gRPC benchmark tool on Ubuntu servers without compilation, using pre-compiled binary files directly.

## Download Files

### 1. Create Directory and Download Files

```bash
# Create directory
mkdir -p ~/grpc-benchmark
cd ~/grpc-benchmark

# Download binary files
wget https://github.com/0xfnzero/grpc-benchmark/releases/download/v1.1/grpc-comparison
wget https://github.com/0xfnzero/grpc-benchmark/releases/download/v1.1/benchmark-jito
wget https://github.com/0xfnzero/grpc-benchmark/releases/download/v1.1/latency-test

# Download run scripts
wget https://github.com/0xfnzero/grpc-benchmark/releases/download/v1.1/run-grpc-comparison.sh
wget https://github.com/0xfnzero/grpc-benchmark/releases/download/v1.1/run-benchmark-jito.sh
wget https://github.com/0xfnzero/grpc-benchmark/releases/download/v1.1/run-latency-test.sh

# Set execution permissions
chmod +x grpc-comparison benchmark-jito latency-test
chmod +x run-*.sh
```

### 2. Verify Download

```bash
# Check if files exist
ls -la

# You should see the following files:
# - grpc-comparison (binary file)
# - benchmark-jito (binary file)
# - latency-test (binary file)
# - run-grpc-comparison.sh (script)
# - run-benchmark-jito.sh (script)
# - run-latency-test.sh (script)
```

## Configuration

### 1. Latency Test Configuration

Edit the `run-latency-test.sh` file:

```bash
vim run-latency-test.sh
```

**Configuration to modify:**
```bash
# Default gRPC endpoint configuration
export GRPC_URL="https://solana-yellowstone-grpc.publicnode.com:443"  # Change to your endpoint
export GRPC_TOKEN=""  # If authentication is required, enter your token

# Test parameters
export TOTAL_ROUNDS=10        # Number of test rounds
export PING_INTERVAL_MS=1000  # Ping interval (milliseconds)
export TEST_TIMEOUT=120       # Timeout (seconds)
```

### 2. gRPC Comparison Test Configuration

Edit the `run-grpc-comparison.sh` file:

```bash
vim run-grpc-comparison.sh
```

**Configuration to modify:**
```bash
# Endpoint 1 configuration
export GRPC_URL_1="https://solana-yellowstone-grpc.publicnode.com:443"  # Change to your endpoint1
export GRPC_NAME_1="Public_Node"  # Name of endpoint1
export GRPC_TOKEN_1=""  # Authentication token for endpoint1

# Endpoint 2 configuration
export GRPC_URL_2="http://64.130.32.158:10900"  # Change to your endpoint2
export GRPC_NAME_2="Custom_Node"  # Name of endpoint2
export GRPC_TOKEN_2=""  # Authentication token for endpoint2

# Test configuration
export GRPC_COMPARISON_DURATION_SEC=30  # Test duration (seconds)
export CONCURRENCY=10                   # Concurrency level
```

### 3. Jito Benchmark Test Configuration

Edit the `run-benchmark-jito.sh` file:

```bash
vim run-benchmark-jito.sh
```

**Configuration to modify:**
```bash
# Jito configuration
export JITO_URL="https://amsterdam.mainnet.block-engine.jito.wtf"  # Change to your Jito endpoint
export JITO_CONCURRENCY=10  # Concurrency level
```

## Running Tests

### 1. Latency Test

```bash
# Run latency test
./run-latency-test.sh

# Or run with parameters
./run-latency-test.sh --grpc-url "https://your-endpoint.com:443" --total-rounds 20
```

### 2. gRPC Endpoint Comparison Test

```bash
# Run gRPC endpoint comparison test
./run-grpc-comparison.sh

# Or run with parameters
./run-grpc-comparison.sh --duration 60 --concurrency 20
```

### 3. Jito Benchmark Test

```bash
# Run Jito benchmark test
./run-benchmark-jito.sh

# Or run with parameters
./run-benchmark-jito.sh --concurrency 15
```

## View Help Information

```bash
# View latency test help
./latency-test --help

# View gRPC comparison test help
./grpc-comparison --help

# View Jito benchmark test help
./benchmark-jito --help
```

## Common Configuration Examples

### 1. Using Custom Endpoints

```bash
# Modify run-latency-test.sh
export GRPC_URL="https://your-custom-endpoint.com:443"
export GRPC_TOKEN="your-auth-token"
```

### 2. Adjusting Test Parameters

```bash
# Modify run-grpc-comparison.sh
export GRPC_COMPARISON_DURATION_SEC=60  # Test for 1 minute
export CONCURRENCY=20                   # Increase concurrency
```

### 3. Adding More Endpoints

```bash
# Add endpoint3 in run-grpc-comparison.sh
export GRPC_URL_3="https://endpoint3.com:443"
export GRPC_NAME_3="Endpoint_3"
export GRPC_TOKEN_3=""
```

## Troubleshooting

### 1. Permission Issues

```bash
# If you encounter permission errors
chmod +x grpc-comparison benchmark-jito latency-test
chmod +x run-*.sh
```

### 2. Network Connection Issues

```bash
# Test network connection
curl -I https://solana-yellowstone-grpc.publicnode.com:443

# Check firewall settings
sudo ufw status
```

### 3. Endpoint Unavailable

```bash
# Test endpoint connection
telnet your-endpoint.com 443

# Or use curl to test
curl -v https://your-endpoint.com:443
```

## Output Description

### Latency Test Output
- Average latency time
- Minimum/maximum latency
- Latency distribution statistics
- Success rate

### gRPC Comparison Test Output
- Performance comparison of endpoints
- Throughput statistics
- Error rate statistics
- Latency comparison

### Jito Benchmark Test Output
- Block engine performance
- Transaction processing speed
- Error statistics

## Important Notes

1. **Network Requirements**: Ensure the server can access gRPC endpoints
2. **Authentication Tokens**: Some endpoints may require API tokens
3. **Test Duration**: Long-term tests may generate large amounts of data
4. **Resource Consumption**: High concurrency tests may consume more CPU and memory
5. **Endpoint Limits**: Pay attention to endpoint rate limits

## One-Click Download Script

If you want to download all files with one click, you can use the following script:

```bash
#!/bin/bash
# One-click download script

mkdir -p ~/grpc-benchmark
cd ~/grpc-benchmark

echo "Downloading binary files..."
wget https://github.com/0xfnzero/grpc-benchmark/releases/download/v1.1/grpc-comparison
wget https://github.com/0xfnzero/grpc-benchmark/releases/download/v1.1/benchmark-jito
wget https://github.com/0xfnzero/grpc-benchmark/releases/download/v1.1/latency-test

echo "Downloading run scripts..."
wget https://github.com/0xfnzero/grpc-benchmark/releases/download/v1.1/run-grpc-comparison.sh
wget https://github.com/0xfnzero/grpc-benchmark/releases/download/v1.1/run-benchmark-jito.sh
wget https://github.com/0xfnzero/grpc-benchmark/releases/download/v1.1/run-latency-test.sh

echo "Setting execution permissions..."
chmod +x grpc-comparison benchmark-jito latency-test
chmod +x run-*.sh

echo "Download complete!"
echo "Please edit the script files to configure your endpoint information."
```

Save this script as `download.sh` and run:

```bash
chmod +x download.sh
./download.sh
```
