#!/bin/bash

# gRPC 端点比较测试脚本
# 用于比较多个 gRPC 端点的性能

# 环境变量配置
export GRPC_URL_1="https://solana-yellowstone-grpc.publicnode.com:443"
export GRPC_NAME_1="Public_Node"
export GRPC_TOKEN_1=""

export GRPC_URL_2="http://64.130.32.158:10900"
export GRPC_NAME_2="Custom_Node"
export GRPC_TOKEN_2=""

# 测试配置
export GRPC_COMPARISON_DURATION_SEC=30
export CONCURRENCY=10

# 运行 gRPC 比较测试
echo "开始 gRPC 端点比较测试..."
RUST_LOG=info cargo run --bin grpc-comparison
