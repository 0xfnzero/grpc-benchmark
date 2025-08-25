#!/bin/bash

# 编译脚本
# 只负责编译 Rust 项目，生成二进制文件

set -e

echo "开始编译 gRPC 基准测试工具..."

# 创建 lib 目录
mkdir -p lib

# 检查是否支持交叉编译
echo "检查交叉编译环境..."

# 编译项目，生成 Ubuntu 可运行的二进制文件
echo "编译二进制文件（Ubuntu x86_64）..."
if cargo build --target x86_64-unknown-linux-gnu --release; then
    echo "✓ 交叉编译成功！"
    BINARY_DIR="target/x86_64-unknown-linux-gnu/release"
    echo "✓ 生成了 Ubuntu 可运行的二进制文件"
else
    echo "✗ 交叉编译失败！"
    echo ""
    echo "可能的解决方案："
    echo "1. 安装交叉编译目标：rustup target add x86_64-unknown-linux-gnu"
    echo "2. 安装 OpenSSL 开发包和交叉编译工具链"
    echo "3. 配置交叉编译环境变量"
    echo ""
    echo "Ubuntu 交叉编译环境配置："
    echo "- 安装必要的包：apt-get install gcc-x86-64-linux-gnu"
    echo "- 设置环境变量：export CC_x86_64_unknown_linux_gnu=x86_64-linux-gnu-gcc"
    echo ""
    exit 1
fi

# 复制二进制文件到 lib 目录
echo "复制二进制文件到 lib 目录..."
cp "$BINARY_DIR/grpc-comparison" lib/
cp "$BINARY_DIR/benchmark-jito" lib/
cp "$BINARY_DIR/latency-test" lib/
cp "$BINARY_DIR/grpc-vs-fzstream" lib/

# 设置二进制文件执行权限
chmod +x lib/grpc-comparison
chmod +x lib/benchmark-jito
chmod +x lib/latency-test
chmod +x lib/grpc-vs-fzstream

echo "生成 shell 脚本..."

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
echo "- lib/grpc-comparison"
echo "- lib/benchmark-jito"
echo "- lib/latency-test"
echo "- lib/grpc-vs-fzstream"
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
