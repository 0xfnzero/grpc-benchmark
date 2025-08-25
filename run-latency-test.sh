#!/bin/bash

# 延迟测试脚本
# 用于测试 gRPC 端点的延迟性能

# 默认 gRPC 端点配置
export GRPC_URL="https://solana-yellowstone-grpc.publicnode.com:443"
export GRPC_TOKEN=""

# 测试参数
export TOTAL_ROUNDS=10
export PING_INTERVAL_MS=1000
export TEST_TIMEOUT=120

# 运行延迟测试
echo "开始延迟测试..."
RUST_LOG=info cargo run --bin latency-test
