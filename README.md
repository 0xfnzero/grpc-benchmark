# gRPC 基准测试工具 (Rust 版本)

一个用 Rust 实现的高性能 gRPC 基准测试工具，用于测试 Solana Yellowstone gRPC 端点和 Jito Block Engine API。

📖 [English Version](README-english.md)

## 功能特性

- **gRPC 对比测试**: 比较多个 Solana Yellowstone gRPC 端点的性能和可靠性
- **延迟测试**: 测量 ping 延迟，支持可配置的并发级别
- **Jito 基准测试**: 对 Jito Block Engine 端点进行 HTTP API 基准测试
- **实时统计**: 全面的性能指标，包括百分位数和标准差
- **并发测试**: 支持高并发测试场景

## 安装

### 前置要求

- Rust 1.70+ (通过 [rustup](https://rustup.rs/) 安装)
- Protocol Buffers 编译器 (`protoc`)

### 从源码构建

```bash
git clone https://github.com/0xfnzero/grpc-benchmark
cd grpc-benchmark
cargo build --release
```

## 使用方法

### 1. gRPC 端点对比测试

比较多个 gRPC 端点以确定哪个提供最佳性能：

```bash
# 使用环境变量
export GRPC_URL_1="https://solana-yellowstone-grpc.publicnode.com:443"
export GRPC_NAME_1="端点1"
export GRPC_TOKEN_1="your-token-1"

export GRPC_URL_2="https://endpoint2.example.com:443"
export GRPC_NAME_2="端点2"
export GRPC_TOKEN_2="your-token-2"

cargo run --bin grpc-comparison-
```

### 2. 延迟测试

测试单个 gRPC 端点的 ping 延迟：

```bash
# 使用环境变量
export GRPC_URL="https://solana-yellowstone-grpc.publicnode.com:443"
export TOTAL_ROUNDS=100

cargo run --bin latency-test
```

### 3. Jito Block Engine 基准测试

对 Jito Block Engine HTTP API 进行性能基准测试：

```bash
# 使用环境变量
export JITO_URL="https://amsterdam.mainnet.block-engine.jito.wtf"
export JITO_CONCURRENCY=10

cargo run --bin benchmark-jito
```

## 配置

### 环境变量

基于 `.env.example` 创建 `.env` 文件：

```bash
cp .env.example .env
```

主要配置选项：

- `GRPC_URL`: 默认 gRPC 端点 URL
- `GRPC_TOKEN`: gRPC 端点认证令牌
- `GRPC_URL_*`, `GRPC_NAME_*`, `GRPC_TOKEN_*`: 多端点配置
- `TOTAL_ROUNDS`: ping 请求次数 (默认: 50)
- `CONCURRENCY`: 并发请求限制 (默认: 10)
- `GRPC_COMPARISON_DURATION_SEC`: 对比测试持续时间 (默认: 30 秒)
- `JITO_URL`: Jito Block Engine 端点
- `JITO_CONCURRENCY`: Jito API 并发级别

### 快速开始示例

```bash
# 1. 配置环境变量
export JITO_URL="https://amsterdam.mainnet.block-engine.jito.wtf"
export JITO_CONCURRENCY=10

# 2. 运行 Jito 基准测试
cargo run --bin benchmark-jito

# 3. 配置 gRPC 对比测试
export GRPC_URL_1="http://64.130.32.158:10900"
export GRPC_NAME_1="Custom_Node"
export GRPC_URL_2="https://solana-yellowstone-grpc.publicnode.com:443"
export GRPC_NAME_2="Public_Node"
export GRPC_COMPARISON_DURATION_SEC=30

# 4. 运行 gRPC 对比测试
cargo run --bin grpc-comparison
```

## 输出示例

### gRPC 对比测试结果

```
===== 端点性能对比 =====
自定义节点      : 首先接收  65.23%, 落后时平均延迟   8.45ms, 总体平均延迟   3.67ms
公共节点        : 首先接收  34.77%, 落后时平均延迟  15.91ms, 总体平均延迟   7.23ms
```

### 延迟测试结果

```
延迟统计:
  平均延迟: 45.67ms
  最小延迟: 23.12ms
  最大延迟: 89.34ms
  标准差: 12.45ms
  中位数 (p50): 43.21ms
  百分位数 (p90): 65.78ms
  百分位数 (p99): 82.45ms
  样本数量: 100
```

### Jito 基准测试结果

```
Jito URL: https://amsterdam.mainnet.block-engine.jito.wtf
请求并发量: 10/s
每 10 秒输出统计信息, 请稍后...
统计 - 过去 10 秒：发送请求总量:  100, 成功响应量:   85, 平均每秒成功:  8.5, 429 错误次数:  15
统计 - 过去 10 秒：发送请求总量:  100, 成功响应量:   92, 平均每秒成功:  9.2, 429 错误次数:   8
```

## 性能特性

- **零拷贝操作**: 高效的内存使用，最小化内存分配
- **异步/等待**: 非阻塞 I/O 实现最大并发性
- **连接池**: 复用连接以获得更好的性能
- **实时指标**: 长时间运行测试期间的实时统计
- **错误处理**: 健壮的错误恢复和报告机制

## 项目架构

```
grpc-benchmark-rust/
├── src/
│   ├── lib.rs              # 核心库和 protobuf 定义
│   ├── config.rs           # 配置管理
│   ├── stats.rs            # 统计计算
│   ├── grpc_client.rs      # gRPC 客户端封装
│   ├── error.rs            # 错误类型定义
│   └── bin/
│       ├── grpc_comparison.rs        # 多端点对比工具
│       ├── latency_test.rs           # 延迟测试工具
│       └── benchmark_jito.rs         # Jito API 基准测试工具
├── Cargo.toml              # Rust 依赖项
└── .env.example            # 环境变量配置示例
```

## 开发

### 运行测试

```bash
cargo test
```

### 生产环境构建

```bash
cargo build --release
```

### 添加新的基准测试

1. 在 `src/bin/` 中创建新的二进制文件
2. 使用 `src/lib.rs` 中的共享模块
3. 在 `src/config.rs` 中添加配置选项
4. 更新 `Cargo.toml` 以包含新的二进制文件

## 贡献

1. Fork 仓库
2. 创建功能分支
3. 为新功能添加测试
4. 运行 `cargo fmt` 和 `cargo clippy`
5. 提交 pull request
