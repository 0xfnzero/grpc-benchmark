#!/bin/bash

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Jito 区块引擎基准测试脚本
# 用于测试 Jito Block Engine API 性能

# Jito 配置
export JITO_URL="${JITO_URL:-https://amsterdam.mainnet.block-engine.jito.wtf}"
export JITO_CONCURRENCY="${JITO_CONCURRENCY:-10}"
export JITO_DURATION_SEC="${JITO_DURATION_SEC:-0}"

# 运行 Jito 基准测试
echo "开始 Jito 区块引擎基准测试..."
RUST_LOG="${RUST_LOG:-info}" cargo run --bin benchmark-jito
