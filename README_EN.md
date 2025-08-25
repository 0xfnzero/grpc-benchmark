# gRPC Benchmark Tool - Quick Start Guide
[中文](https://github.com/0xfnzero/grpc-benchmark/blob/main/README.md) | [English](https://github.com/0xfnzero/grpc-benchmark/blob/main/README_EN.md) | [Telegram](https://t.me/fnzero_group)

> grpc-benchmark is a Rust version built on [ChainBuff/grpc-benchmark-ts](https://github.com/ChainBuff/grpc-benchmark-ts)
, offering higher performance and a better deployment experience. ✅

## Overview

This guide will help you quickly download and run the gRPC benchmark tool on Ubuntu servers without compilation, using pre-compiled binary files directly.

## Install Speed Test Tool

### 1. Download Installation Script

```bash
cd /root

# Get installation script
wget https://github.com/0xfnzero/grpc-benchmark/releases/download/v1.2/install.sh

# Execute installation script
sudo ./install.sh
```

### 2. Verify Download

```bash
# Check if files exist
ls -la

# You should see the following files:
# - grpc-comparison (binary file)
# - benchmark-jito (binary file)
# - latency-test (binary file)
# - grpc-vs-fzstream (binary file)
# - run-grpc-comparison.sh (script)
# - run-benchmark-jito.sh (script)
# - run-latency-test.sh (script)
# - run-grpc-vs-fzstream.sh (script)
```

## gRPC Comparison Test

**Edit the `run-grpc-comparison.sh` file:**

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

**Run gRPC Endpoint Comparison Test**
```bash
# Run gRPC endpoint comparison test
./run-grpc-comparison.sh
```

### 2. gRPC vs FzStream Comparison Test

**Edit the `run-grpc-vs-fzstream.sh` file:**

```bash
vim run-grpc-vs-fzstream.sh
```

**Configuration to modify:**
```bash
# FzStream configuration
export FZSTREAM_SERVER_ADDRESS="64.130.37.195:2222"  # FzStream server address
export AUTH_TOKEN="demo_token"  # FzStream authentication token

# gRPC configuration
export GRPC_URL="https://solana-yellowstone-grpc.publicnode.com:443"  # gRPC endpoint
export GRPC_TOKEN=""  # gRPC authentication token (optional)

# Test configuration
export TEST_DURATION=30  # Test duration (seconds)
```

**Run gRPC vs FzStream Comparison Test**
```bash
# Run gRPC vs FzStream comparison test
./run-grpc-vs-fzstream.sh
```

### 3. Latency Test

**Edit the `run-latency-test.sh` file:**

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

**Run Latency Test**
```bash
# Run latency test
./run-latency-test.sh
```

### 4. Jito Benchmark Test Configuration

**Edit the `run-benchmark-jito.sh` file:**

```bash
vim run-benchmark-jito.sh
```

**Configuration to modify:**
```bash
# Jito configuration
export JITO_URL="https://amsterdam.mainnet.block-engine.jito.wtf"  # Change to your Jito endpoint
export JITO_CONCURRENCY=10  # Concurrency level
```

**Run Jito Benchmark Test**
```bash
# Run Jito benchmark test
./run-benchmark-jito.sh
```

## Common Configuration Examples

### 1. Adjusting Test Parameters

```bash
# Modify run-grpc-comparison.sh
export GRPC_COMPARISON_DURATION_SEC=60  # Test for 1 minute
export CONCURRENCY=20                   # Increase concurrency
```

### 2. Configure FzStream vs gRPC Comparison

```bash
# Modify run-grpc-vs-fzstream.sh
export FZSTREAM_SERVER_ADDRESS="your-fzstream-server:2222"
export AUTH_TOKEN="your-fzstream-token"
export GRPC_URL="https://your-grpc-endpoint.com:443"
export TEST_DURATION=60  # Test for 60 seconds
```

### 3. Adding More Endpoints

```bash
# Add endpoint3 in run-grpc-comparison.sh
export GRPC_URL_3="https://endpoint3.com:443"
export GRPC_NAME_3="Endpoint_3"
export GRPC_TOKEN_3=""
```

### 4. Using Custom Endpoints

```bash
# Modify run-latency-test.sh
export GRPC_URL="https://your-custom-endpoint.com:443"
export GRPC_TOKEN="your-auth-token"
```

## Common Configuration Examples

### 1. Adjusting Test Parameters

```bash
# Modify run-grpc-comparison.sh
export GRPC_COMPARISON_DURATION_SEC=60  # Test for 1 minute
export CONCURRENCY=20                   # Increase concurrency
```

### 2. Configure FzStream vs gRPC Comparison

```bash
# Modify run-grpc-vs-fzstream.sh
export FZSTREAM_SERVER_ADDRESS="your-fzstream-server:2222"
export AUTH_TOKEN="your-fzstream-token"
export GRPC_URL="https://your-grpc-endpoint.com:443"
export TEST_DURATION=60  # Test for 60 seconds
```

### 3. Adding More Endpoints

```bash
# Add endpoint3 in run-grpc-comparison.sh
export GRPC_URL_3="https://endpoint3.com:443"
export GRPC_NAME_3="Endpoint_3"
export GRPC_TOKEN_3=""
```

### 4. Using Custom Endpoints

```bash
# Modify run-latency-test.sh
export GRPC_URL="https://your-custom-endpoint.com:443"
export GRPC_TOKEN="your-auth-token"
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

### gRPC vs FzStream Comparison Test Output
- Real-time slot reception comparison
- Latency statistics (FzStream vs gRPC)
- First reception statistics
- Performance analysis report
- Average latency comparison

**Output Example:**
```
[00:18:20.759] gRPC     接收 slot 362449176 : 首次接收
[00:18:20.759] FzStream 接收 slot 362449176 : 延迟   0.63ms (相对于 gRPC)
[00:18:21.763] gRPC     接收 slot 362449177 : 首次接收
[00:18:21.763] FzStream 接收 slot 362449177 : 延迟   1.91ms (相对于 gRPC)

📊 gRPC Performance Analysis
Total received blocks: 1250 blocks
First received blocks: 758 (60.64%) blocks
Delayed received blocks: 492 (39.36%) blocks

📊 FzStream Performance Analysis
Total received blocks: 1250 blocks
First received blocks: 492 (39.36%) blocks
Delayed received blocks: 758 (60.64%) blocks
Average latency: 1.25ms
```

## Important Notes

1. **Network Requirements**: Ensure the server can access gRPC endpoints
2. **Authentication Tokens**: Some endpoints may require API tokens
3. **Test Duration**: Long-term tests may generate large amounts of data
4. **Resource Consumption**: High concurrency tests may consume more CPU and memory
5. **Endpoint Limits**: Pay attention to endpoint rate limits
