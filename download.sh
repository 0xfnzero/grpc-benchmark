#!/bin/bash

# gRPC 基准测试工具一键下载脚本

echo "=========================================="
echo "gRPC 基准测试工具一键下载"
echo "=========================================="

# 创建目录
echo "创建目录..."
mkdir -p ~/grpc-benchmark
cd ~/grpc-benchmark

# 下载二进制文件
echo "下载二进制文件..."
wget -q --show-progress https://github.com/0xfnzero/grpc-benchmark/releases/download/v1.1/grpc-comparison
wget -q --show-progress https://github.com/0xfnzero/grpc-benchmark/releases/download/v1.1/benchmark-jito
wget -q --show-progress https://github.com/0xfnzero/grpc-benchmark/releases/download/v1.1/latency-test

# 下载运行脚本
echo "下载运行脚本..."
wget -q --show-progress https://github.com/0xfnzero/grpc-benchmark/releases/download/v1.1/run-grpc-comparison.sh
wget -q --show-progress https://github.com/0xfnzero/grpc-benchmark/releases/download/v1.1/run-benchmark-jito.sh
wget -q --show-progress https://github.com/0xfnzero/grpc-benchmark/releases/download/v1.1/run-latency-test.sh

# 设置执行权限
echo "设置执行权限..."
chmod +x grpc-comparison benchmark-jito latency-test
chmod +x run-*.sh

# 验证下载
echo ""
echo "验证下载结果..."
ls -la

echo ""
echo "=========================================="
echo "下载完成！"
echo "=========================================="
echo ""
echo "下一步操作："
echo "1. 编辑脚本文件配置您的端点信息："
echo "   nano run-latency-test.sh"
echo "   nano run-grpc-comparison.sh"
echo "   nano run-benchmark-jito.sh"
echo ""
echo "2. 运行测试："
echo "   ./run-latency-test.sh"
echo "   ./run-grpc-comparison.sh"
echo "   ./run-benchmark-jito.sh"
echo ""
echo "3. 查看帮助信息："
echo "   ./latency-test --help"
echo "   ./grpc-comparison --help"
echo "   ./benchmark-jito --help"
echo ""
echo "详细说明请查看 README.md 文件"
