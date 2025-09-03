#! /bin/bash

set -euo pipefail

# 方式 1：统一把 target 放到当前包（如果没有设置则使用默认）
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target}"

# ==== 配置区 ====
TARGET=${TARGET:-x86_64-unknown-linux-gnu}
PKG=${PKG:-}  # 如果在 workspace 根，可写实际包名，例如 PKG=my-crate
BINS=("grpc-vs-fzstream" "latency-test" "benchmark-jito" "grpc-comparison")
# ===============

# 解析 target 输出目录（考虑 CARGO_TARGET_DIR）
TARGET_DIR="${CARGO_TARGET_DIR:-target}"
BINARY_DIR="${TARGET_DIR}/${TARGET}/release"

echo "使用目标: $TARGET"
echo "产物目录: $BINARY_DIR"
mkdir -p lib

# 构建所有 bin（逐个指定，确保 cargo 真正去编它们）
for BIN in "${BINS[@]}"; do
  echo "==> 构建 $BIN ..."
  if [[ -n "$PKG" ]]; then
    cargo build --release --target "$TARGET" --package "$PKG" --bin "$BIN"
  else
    cargo build --release --target "$TARGET" --bin "$BIN"
  fi

  # 优先找正式产物，其次兜底到 deps 目录（某些情况下会带 hash 名）
  SRC_PATH="$BINARY_DIR/$BIN"
  if [[ ! -f "$SRC_PATH" ]]; then
    # 兜底：deps 下可能叫 $BIN-<hash>，取最新的可执行文件
    CANDIDATE=$(ls -t "$BINARY_DIR"/deps/"$BIN"-* 2>/dev/null | head -n1 || true)
    if [[ -n "$CANDIDATE" && -x "$CANDIDATE" ]]; then
      SRC_PATH="$CANDIDATE"
    fi
  fi

  if [[ -f "$SRC_PATH" ]]; then
    cp "$SRC_PATH" "lib/$BIN"
    chmod +x "lib/$BIN"
    echo "✓ 已复制: lib/$BIN"
  else
    echo "✗ 未找到二进制: 期望位置 $BINARY_DIR/$BIN"
    echo "  帮你列一下目录，方便排查："
    echo "  ls -l $BINARY_DIR"
    ls -l "$BINARY_DIR" || true
    echo "  ls -l $BINARY_DIR/deps"
    ls -l "$BINARY_DIR/deps" || true
    exit 1
  fi
done

echo ""
echo "生成执行脚本..."

# 生成 grpc-comparison.sh
cat > lib/run-grpc-comparison.sh << 'EOF'
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
./grpc-comparison
EOF

# 生成 benchmark-jito.sh
cat > lib/run-benchmark-jito.sh << 'EOF'
#!/bin/bash

# Jito 区块引擎基准测试脚本
# 用于测试 Jito Block Engine API 性能

# Jito 配置
export JITO_URL="https://amsterdam.mainnet.block-engine.jito.wtf"
export JITO_CONCURRENCY=10

# 运行 Jito 基准测试
echo "开始 Jito 区块引擎基准测试..."
./benchmark-jito
EOF

# 生成 latency-test.sh
cat > lib/run-latency-test.sh << 'EOF'
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
./latency-test
EOF

# 生成 grpc-vs-fzstream.sh
cat > lib/run-grpc-vs-fzstream.sh << 'EOF'
#!/bin/bash

# gRPC vs FzStream 性能对比测试脚本
# 用于对比 gRPC 和 FzStream 在接收 PumpFunTradeEvent 事件方面的性能

# FzStream 配置
export FZSTREAM_SERVER_ADDRESS=127.0.0.1:2222
export AUTH_TOKEN=demo_token

# gRPC 配置
export GRPC_URL=https://solana-yellowstone-grpc.publicnode.com:443
export GRPC_TOKEN=

# 运行性能对比测试
./grpc-vs-fzstream
EOF

# 设置所有 shell 脚本的执行权限
chmod +x lib/*.sh

echo ""
echo "=========================================="
echo "编译完成！"
echo "=========================================="
echo ""
echo "生成的文件："
echo "二进制文件："
for BIN in "${BINS[@]}"; do
  echo "- lib/$BIN"
done
echo ""
echo "Shell 脚本："
echo "- lib/run-grpc-comparison.sh"
echo "- lib/run-benchmark-jito.sh"
echo "- lib/run-latency-test.sh"
echo "- lib/run-grpc-vs-fzstream.sh"
echo ""
echo "使用方法："
echo "1. gRPC 端点性能比较:"
echo "   ./lib/run-grpc-comparison.sh"
echo ""
echo "2. Jito Block Engine 基准测试:"
echo "   ./lib/run-benchmark-jito.sh"
echo ""
echo "3. 延迟测试:"
echo "   ./lib/run-latency-test.sh"
echo ""
echo "4. gRPC vs FzStream 性能对比:"
echo "   ./lib/run-grpc-vs-fzstream.sh"
echo ""
echo "自定义配置示例："
echo "FZSTREAM_SERVER_ADDRESS=\"your-server:8080\" \\"
echo "GRPC_URL=\"https://your-grpc-endpoint.com\" \\"
echo "AUTH_TOKEN=\"your_token\" \\"
echo "GRPC_TOKEN=\"your_grpc_token\" \\"
echo "./lib/run-grpc-vs-fzstream.sh"
echo ""
