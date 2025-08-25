#!/bin/bash

# gRPC vs FzStream 对比测试脚本
# gRPC vs FzStream Comparison Test Script

set -e

echo "🚀 Starting gRPC vs FzStream Performance Comparison..."
echo "=================================================="

# 默认配置 / Default Configuration
export FZSTREAM_SERVER_ADDRESS="${FZSTREAM_SERVER_ADDRESS:-64.130.37.195:2222}"
export AUTH_TOKEN="${AUTH_TOKEN:-demo_token_12345}"
export GRPC_URL="${GRPC_URL:-https://solana-yellowstone-grpc.publicnode.com:443}"
export GRPC_TOKEN="${GRPC_TOKEN:-}"
export TEST_DURATION="${TEST_DURATION:-30}"

# 显示配置信息 / Display configuration
echo "📋 Configuration:"
echo "  FzStream Server: $FZSTREAM_SERVER_ADDRESS"
echo "  Auth Token: ${AUTH_TOKEN:0:10}..."
echo "  gRPC URL: $GRPC_URL"
echo "  Test Duration: ${TEST_DURATION}s"
echo ""

# 检查二进制文件是否存在 / Check if binary exists
if [ ! -f "./grpc-vs-fzstream" ]; then
    echo "❌ Error: grpc-vs-fzstream binary not found!"
    echo "   Please make sure the binary is in the current directory."
    exit 1
fi

# 运行测试 / Run test
echo "🏃 Running comparison test..."
RUST_LOG=info \
FZSTREAM_SERVER_ADDRESS="$FZSTREAM_SERVER_ADDRESS" \
AUTH_TOKEN="$AUTH_TOKEN" \
GRPC_URL="$GRPC_URL" \
GRPC_TOKEN="$GRPC_TOKEN" \
./grpc-vs-fzstream

echo ""
echo "✅ Test completed!"
echo "=================================================="