#!/bin/bash

# Jito 区块引擎基准测试脚本
# 用于测试 Jito Block Engine API 性能

# Jito 配置
export JITO_URL="https://amsterdam.mainnet.block-engine.jito.wtf"
export JITO_CONCURRENCY=10

# 运行 Jito 基准测试
echo "开始 Jito 区块引擎基准测试..."
RUST_LOG=info cargo run --bin benchmark-jito
