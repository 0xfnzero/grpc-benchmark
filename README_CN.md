# Solana Yellowstone gRPC / Raw Shred 基准测试工具

[中文](https://github.com/0xfnzero/grpc-benchmark/blob/main/README_CN.md) | [English](https://github.com/0xfnzero/grpc-benchmark/blob/main/README.md) | [Telegram](https://t.me/fnzero_group) | [Discord](https://discord.gg/vuazbGkqQE)

高性能 Rust CLI，用于 Solana 延迟测速：Yellowstone gRPC vs gRPC、raw shred vs raw shred、gRPC vs raw shred。支持 slot 首达延迟、UDP shred 吞吐、Entry/交易解码统计，以及 Jito Block Engine HTTP 性能测试，支持 macOS 和 Linux。

**关键词:** Solana benchmark, Yellowstone gRPC, Geyser gRPC, raw shred, ShredStream, shred latency, gRPC latency, validator infrastructure, Jito Block Engine.

## 二进制平台支持

Release 包支持 macOS 和 Linux，安装脚本会自动识别：

- `x86_64-apple-darwin`
- `aarch64-apple-darwin`
- `x86_64-unknown-linux-gnu`
- `aarch64-unknown-linux-gnu`

## 测速排序

对比测速是主要场景，所以在 README 和教程里排在前面。

| 二进制 | 脚本 | 测试内容 |
|--------|------|----------|
| `grpc-comparison` | `run-grpc-comparison.sh` | 多个 Yellowstone gRPC 端点的 slot 首达顺序和延迟 |
| `shred-comparison` | `run-shred-comparison.sh` | 两个 raw shred UDP 源之间的 slot 首达延迟 |
| `grpc-vs-shred` | `run-grpc-vs-shred.sh` | Yellowstone gRPC 和 raw shred 之间的 slot 首达延迟 |
| `latency-test` | `run-latency-test.sh` | 单个 gRPC 端点 ping/订阅延迟 |
| `shred` | `run-shred.sh` | 监听 raw shred UDP，打印首次看到的 slot 和解码片段 |
| `shred-benchmark` | `run-shred-benchmark.sh` | raw shred UDP 吞吐、Entry/交易解码统计 |
| `benchmark-jito` | `run-benchmark-jito.sh` | Jito Block Engine HTTP 请求性能 |

旧的私有流入口已移除。raw shred 解码不依赖 Jito ShredStream 服务。默认 `ledger` 模式直接解析 Solana wire header，按连续 data shred 流式解码 `bincode Vec<Entry>`，并可用 Merkle Reed-Solomon FEC 恢复缺失 data shred。`shred-comparison` 和 `grpc-vs-shred` 默认关闭 Entry 解码，只用首次合法 shred header 到达时间做最低延迟 slot 对比。

## 安装

macOS 和 Linux：

```bash
curl -fsSL -o install.sh https://github.com/0xfnzero/grpc-benchmark/releases/latest/download/install.sh
chmod +x install.sh
./install.sh
cd "$HOME/grpc-benchmark"
```

指定 release tag 或安装目录：

```bash
VERSION=v1.4 ./install.sh
INSTALL_DIR=/opt/grpc-benchmark sudo -E ./install.sh
```

如果 `INSTALL_DIR` 已存在且不是本安装脚本创建的目录，安装脚本会拒绝覆盖。确实要替换时设置 `FORCE_INSTALL=1`，旧目录会先移动到带时间戳的备份路径。

强制指定平台包：

```bash
TARGET=x86_64-unknown-linux-gnu VERSION=v1.4 ./install.sh
```

## 对比测速

### gRPC vs gRPC

编辑 `run-grpc-comparison.sh`：

```bash
export GRPC_URL_1="https://solana-yellowstone-grpc.publicnode.com:443"
export GRPC_NAME_1="Public_Node"
export GRPC_TOKEN_1=""

export GRPC_URL_2="https://your-grpc-endpoint.com:443"
export GRPC_NAME_2="Custom_Node"
export GRPC_TOKEN_2=""

export GRPC_COMPARISON_DURATION_SEC=30
./run-grpc-comparison.sh
```

### Shred vs Shred

把两个 raw shred 数据源分别转发到两个本地 UDP 端口：

```bash
export SHRED_BIND_1="0.0.0.0:20000"
export SHRED_NAME_1="Shred_A"
export SHRED_DECODE_MODE_1="ledger"

export SHRED_BIND_2="0.0.0.0:20001"
export SHRED_NAME_2="Shred_B"
export SHRED_DECODE_MODE_2="ledger"

export SHRED_COMPARISON_DURATION_SEC=30
./run-shred-comparison.sh
```

### gRPC vs Shred

```bash
export GRPC_URL="https://solana-yellowstone-grpc.publicnode.com:443"
export GRPC_TOKEN=""
export GRPC_NAME="gRPC"

export SHRED_BIND="0.0.0.0:20000"
export SHRED_NAME="Shred"
export SHRED_DECODE_MODE="ledger"
export SHRED_DECODE_ENTRIES=false

export TEST_DURATION=30
./run-grpc-vs-shred.sh
```

## 单源测速和工具

### gRPC 延迟

```bash
export GRPC_URL="https://solana-yellowstone-grpc.publicnode.com:443"
export GRPC_TOKEN=""
export TOTAL_ROUNDS=10
./run-latency-test.sh
```

### Shred 监听

```bash
export SHRED_BIND="0.0.0.0:20000"
export SHRED_DECODE_MODE="ledger"
export SHRED_DECODE_ENTRIES=true
export SHRED_ENABLE_FEC=true
export SHRED_DURATION_SEC=30
./run-shred.sh
```

如果数据源是 ShredStream SDK 固定偏移格式：

```bash
export SHRED_DECODE_MODE="compat"
./run-shred.sh
```

### Shred 吞吐和解码测速

```bash
export SHRED_BIND="0.0.0.0:20000"
export SHRED_DECODE_MODE="ledger"
export SHRED_DECODE_ENTRIES=true
export SHRED_ENABLE_FEC=true
export SHRED_BENCHMARK_DURATION_SEC=30
export SHRED_RECV_BUFFER_BYTES=134217728
./run-shred-benchmark.sh
```

### Jito Block Engine HTTP 测速

```bash
export JITO_URL="https://amsterdam.mainnet.block-engine.jito.wtf"
export JITO_CONCURRENCY=10
export JITO_DURATION_SEC=0   # 0 = 一直运行，直到手动停止
./run-benchmark-jito.sh
```

这个不是 Jito ShredStream。

## 常用 Shred 配置

```bash
export SHRED_DECODE_MODE="ledger"              # ledger/raw/agave 或 compat/sdk
export SHRED_DECODE_ENTRIES=true               # false = 只解析 header 的最快路径
export SHRED_ENABLE_FEC=true                   # ledger 模式下启用 Merkle Reed-Solomon 恢复
export SHRED_RECV_BUFFER_BYTES=134217728
export SHRED_UDP_PAYLOAD_PREFIX_SKIP=0
export SHRED_MAX_TRACKED_SLOTS=128
export SHRED_REASSEMBLY_GAP_TIMEOUT_MS=500
export SHRED_STUCK_BATCH_TIMEOUT_MS=50
export SHRED_MAX_FEC_SETS_PER_SLOT=32
```

## 构建 Release 产物

构建当前机器平台：

```bash
VERSION=v1.4 ./compile.sh
```

如果本机或 CI 已配置对应 Rust target 和 linker，可以一次构建 macOS/Linux 多平台包：

```bash
rustup target add x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu x86_64-apple-darwin aarch64-apple-darwin

VERSION=v1.4 TARGETS="x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu x86_64-apple-darwin aarch64-apple-darwin" ./compile.sh
```

产物在 `release/`：

```text
install.sh
grpc-benchmark-x86_64-unknown-linux-gnu.tar.gz
grpc-benchmark-aarch64-unknown-linux-gnu.tar.gz
grpc-benchmark-x86_64-apple-darwin.tar.gz
grpc-benchmark-aarch64-apple-darwin.tar.gz
SHA256SUMS
```

## 创建 Tag 和 GitHub Release

推荐流程：推送 `v*` tag。GitHub Actions 会自动构建并上传全部 macOS/Linux 二进制包：

```bash
git tag v1.4
git push origin v1.4
```

release workflow 会发布：

```text
install.sh
grpc-benchmark-x86_64-unknown-linux-gnu.tar.gz
grpc-benchmark-aarch64-unknown-linux-gnu.tar.gz
grpc-benchmark-x86_64-apple-darwin.tar.gz
grpc-benchmark-aarch64-apple-darwin.tar.gz
SHA256SUMS
```

手动兜底方式：

```bash
VERSION=v1.4 TARGETS="x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu x86_64-apple-darwin aarch64-apple-darwin" ./compile.sh

git tag v1.4
git push origin v1.4

gh release create v1.4 \
  release/install.sh \
  release/grpc-benchmark-*.tar.gz \
  release/SHA256SUMS \
  --title "v1.4" \
  --notes "gRPC/Shred 对比测速和 raw shred 解码更新"
```
