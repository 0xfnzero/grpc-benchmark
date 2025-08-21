# gRPC 基准测试工具 - 快速使用指南
[中文](https://github.com/0xfnzero/grpc-benchmark/blob/main/README.md) | [English](https://github.com/0xfnzero/grpc-benchmark/blob/main/README-EN.md) | [Telegram](https://t.me/fnzero_group)

## 概述

本指南将帮助您在 Ubuntu 服务器上快速下载和运行 gRPC 基准测试工具，无需编译，直接使用预编译的二进制文件。

## 下载文件

### 1. 创建目录并下载文件

```bash
# 创建目录
mkdir -p ~/grpc-benchmark
cd ~/grpc-benchmark

# 下载二进制文件
wget https://github.com/0xfnzero/grpc-benchmark/releases/download/v1.1/grpc-comparison
wget https://github.com/0xfnzero/grpc-benchmark/releases/download/v1.1/benchmark-jito
wget https://github.com/0xfnzero/grpc-benchmark/releases/download/v1.1/latency-test

# 下载运行脚本
wget https://github.com/0xfnzero/grpc-benchmark/releases/download/v1.1/run-grpc-comparison.sh
wget https://github.com/0xfnzero/grpc-benchmark/releases/download/v1.1/run-benchmark-jito.sh
wget https://github.com/0xfnzero/grpc-benchmark/releases/download/v1.1/run-latency-test.sh

# 设置执行权限
chmod +x grpc-comparison benchmark-jito latency-test
chmod +x run-*.sh
```

### 2. 验证下载

```bash
# 检查文件是否存在
ls -la

# 应该看到以下文件：
# - grpc-comparison (二进制文件)
# - benchmark-jito (二进制文件)
# - latency-test (二进制文件)
# - run-grpc-comparison.sh (脚本)
# - run-benchmark-jito.sh (脚本)
# - run-latency-test.sh (脚本)
```

## 配置修改

### 1. 延迟测试配置

编辑 `run-latency-test.sh` 文件：

```bash
nano run-latency-test.sh
```

**需要修改的配置：**
```bash
# 默认 gRPC 端点配置
export GRPC_URL="https://solana-yellowstone-grpc.publicnode.com:443"  # 修改为您的端点
export GRPC_TOKEN=""  # 如果需要认证，填入您的令牌

# 测试参数
export TOTAL_ROUNDS=10        # 测试轮数
export PING_INTERVAL_MS=1000  # ping 间隔（毫秒）
export TEST_TIMEOUT=120       # 超时时间（秒）
```

### 2. gRPC 比较测试配置

编辑 `run-grpc-comparison.sh` 文件：

```bash
nano run-grpc-comparison.sh
```

**需要修改的配置：**
```bash
# 端点 1 配置
export GRPC_URL_1="https://solana-yellowstone-grpc.publicnode.com:443"  # 修改为您的端点1
export GRPC_NAME_1="Public_Node"  # 端点1的名称
export GRPC_TOKEN_1=""  # 端点1的认证令牌

# 端点 2 配置
export GRPC_URL_2="http://64.130.32.158:10900"  # 修改为您的端点2
export GRPC_NAME_2="Custom_Node"  # 端点2的名称
export GRPC_TOKEN_2=""  # 端点2的认证令牌

# 测试配置
export GRPC_COMPARISON_DURATION_SEC=30  # 测试持续时间（秒）
export CONCURRENCY=10                   # 并发级别
```

### 3. Jito 基准测试配置

编辑 `run-benchmark-jito.sh` 文件：

```bash
nano run-benchmark-jito.sh
```

**需要修改的配置：**
```bash
# Jito 配置
export JITO_URL="https://amsterdam.mainnet.block-engine.jito.wtf"  # 修改为您的 Jito 端点
export JITO_CONCURRENCY=10  # 并发级别
```

## 运行测试

### 1. 延迟测试

```bash
# 运行延迟测试
./run-latency-test.sh

# 或者带参数运行
./run-latency-test.sh --grpc-url "https://your-endpoint.com:443" --total-rounds 20
```

### 2. gRPC 端点比较测试

```bash
# 运行 gRPC 端点比较测试
./run-grpc-comparison.sh

# 或者带参数运行
./run-grpc-comparison.sh --duration 60 --concurrency 20
```

### 3. Jito 基准测试

```bash
# 运行 Jito 基准测试
./run-benchmark-jito.sh

# 或者带参数运行
./run-benchmark-jito.sh --concurrency 15
```

## 查看帮助信息

```bash
# 查看延迟测试帮助
./latency-test --help

# 查看 gRPC 比较测试帮助
./grpc-comparison --help

# 查看 Jito 基准测试帮助
./benchmark-jito --help
```

## 常见配置示例

### 1. 使用自定义端点

```bash
# 修改 run-latency-test.sh
export GRPC_URL="https://your-custom-endpoint.com:443"
export GRPC_TOKEN="your-auth-token"
```

### 2. 调整测试参数

```bash
# 修改 run-grpc-comparison.sh
export GRPC_COMPARISON_DURATION_SEC=60  # 测试1分钟
export CONCURRENCY=20                   # 增加并发
```

### 3. 添加更多端点

```bash
# 在 run-grpc-comparison.sh 中添加端点3
export GRPC_URL_3="https://endpoint3.com:443"
export GRPC_NAME_3="Endpoint_3"
export GRPC_TOKEN_3=""
```

## 故障排除

### 1. 权限问题

```bash
# 如果遇到权限错误
chmod +x grpc-comparison benchmark-jito latency-test
chmod +x run-*.sh
```

### 2. 网络连接问题

```bash
# 测试网络连接
curl -I https://solana-yellowstone-grpc.publicnode.com:443

# 检查防火墙设置
sudo ufw status
```

### 3. 端点不可用

```bash
# 测试端点连接
telnet your-endpoint.com 443

# 或者使用 curl 测试
curl -v https://your-endpoint.com:443
```

## 输出说明

### 延迟测试输出
- 平均延迟时间
- 最小/最大延迟
- 延迟分布统计
- 成功率

### gRPC 比较测试输出
- 各端点的性能对比
- 吞吐量统计
- 错误率统计
- 延迟对比

### Jito 基准测试输出
- 区块引擎性能
- 交易处理速度
- 错误统计

## 注意事项

1. **网络要求**：确保服务器能访问 gRPC 端点
2. **认证令牌**：某些端点可能需要 API 令牌
3. **测试时间**：长时间测试可能产生大量数据
4. **资源消耗**：高并发测试可能消耗较多 CPU 和内存
5. **端点限制**：注意端点的速率限制

## 一键下载脚本

如果您想要一键下载所有文件，可以使用以下脚本：

```bash
#!/bin/bash
# 一键下载脚本

mkdir -p ~/grpc-benchmark
cd ~/grpc-benchmark

echo "下载二进制文件..."
wget https://github.com/0xfnzero/grpc-benchmark/releases/download/v1.1/grpc-comparison
wget https://github.com/0xfnzero/grpc-benchmark/releases/download/v1.1/benchmark-jito
wget https://github.com/0xfnzero/grpc-benchmark/releases/download/v1.1/latency-test

echo "下载运行脚本..."
wget https://github.com/0xfnzero/grpc-benchmark/releases/download/v1.1/run-grpc-comparison.sh
wget https://github.com/0xfnzero/grpc-benchmark/releases/download/v1.1/run-benchmark-jito.sh
wget https://github.com/0xfnzero/grpc-benchmark/releases/download/v1.1/run-latency-test.sh

echo "设置执行权限..."
chmod +x grpc-comparison benchmark-jito latency-test
chmod +x run-*.sh

echo "下载完成！"
echo "请编辑脚本文件配置您的端点信息。"
```

将此脚本保存为 `download.sh` 并运行：

```bash
chmod +x download.sh
./download.sh
```