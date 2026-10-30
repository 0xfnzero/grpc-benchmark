<div align="center">
    <h1>Solana Yellowstone gRPC / Raw Shred 基准测试工具</h1>
    <h3><em>对比 Solana gRPC 与原始 Shred 延迟</em></h3>
</div>

<p align="center">
    <strong>高性能 Rust CLI，用于 Solana 延迟测速：Yellowstone gRPC vs gRPC、raw shred vs raw shred、gRPC vs raw shred。支持 slot 首达延迟、UDP shred 吞吐、Entry/交易解码统计，以及 Jito Block Engine HTTP 性能测试，支持 macOS 和 Linux。</strong>
</p>

<p align="center">
    <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License"></a>
    <a href="https://github.com/0xfnzero/grpc-benchmark"><img src="https://img.shields.io/github/stars/0xfnzero/grpc-benchmark?style=social" alt="GitHub stars"></a>
    <a href="https://github.com/0xfnzero/grpc-benchmark/network"><img src="https://img.shields.io/github/forks/0xfnzero/grpc-benchmark?style=social" alt="GitHub forks"></a>
</p>

<p align="center">
    <img src="https://img.shields.io/badge/Rust-000000?style=for-the-badge&amp;logo=rust&amp;logoColor=white" alt="Rust">
    <img src="https://img.shields.io/badge/Solana-9945FF?style=for-the-badge&amp;logo=solana&amp;logoColor=white" alt="Solana">
    <img src="https://img.shields.io/badge/gRPC-4285F4?style=for-the-badge&amp;logo=grpc&amp;logoColor=white" alt="gRPC">
</p>

<p align="center">
    <a href="https://github.com/0xfnzero/grpc-benchmark/blob/main/README_CN.md">中文</a> |
    <a href="https://github.com/0xfnzero/grpc-benchmark/blob/main/README.md">English</a> |
    <a href="https://fnzero.dev/">Website</a> |
    <a href="https://t.me/fnzero_group">Telegram</a> |
    <a href="https://discord.gg/vuazbGkqQE">Discord</a>
</p>

**关键词:** Solana benchmark, Yellowstone gRPC, Geyser gRPC, raw shred, ShredStream, shred latency, gRPC latency, validator infrastructure, Jito Block Engine.

## v1.6：legacy、v0 和 v1 交易解码

raw UDP shred 解码和 Entry gRPC 交易对比现在支持 legacy、v0、v1 交易。
wire 解析器直接读取交易边界和首个签名，不再依赖 Solana 2.3 的交易反序列化类型。
raw UDP 还支持 Agave 4.x 的 block marker，包含跨 shred 分片的 marker；marker 本身不计为交易。

Entry gRPC 仍须使用约定的 `Vec<Entry>` payload，截断或带有尾随数据的消息会被拒绝。
这是用于测量的解析器，不负责验签、执行交易或判断确认状态。
详见[协议兼容说明](docs/shred-comparison_CN.md#entry-grpc-协议要求)。

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
| `shred-comparison` | `run-shred-comparison.sh` | slot、同一个 shred 或同一笔交易首达；支持 UDP、Entry gRPC 和混合输入 |
| `grpc-vs-shred` | `run-grpc-vs-shred.sh` | Yellowstone gRPC 和 raw shred 之间的 slot 首达延迟 |
| `latency-test` | `run-latency-test.sh` | 单个 gRPC 端点 ping/订阅延迟 |
| `shred` | `run-shred.sh` | 监听 raw shred UDP，打印首次看到的 slot 和解码片段 |
| `shred-benchmark` | `run-shred-benchmark.sh` | raw shred UDP 吞吐、Entry/交易解码统计 |
| `benchmark-jito` | `run-benchmark-jito.sh` | Jito Block Engine HTTP 请求性能 |

旧的私有流入口已移除。raw shred 解码不依赖 Jito ShredStream 服务。默认 `ledger` 模式直接解析 Solana wire header，按连续 data shred 流式解析 legacy/v0/v1 交易，并可用 Merkle Reed-Solomon FEC 恢复缺失 data shred。`shred-comparison` 提供 `slot`、`shred`、`transaction` 三种模式，支持 UDP 和服务商 Entry gRPC；只有交易模式对 UDP 做 Entry 解码。`grpc-vs-shred` 保留 slot header 首达对比。

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
VERSION=v1.6 ./install.sh
INSTALL_DIR=/opt/grpc-benchmark sudo -E ./install.sh
```

如果 `INSTALL_DIR` 已存在且不是本安装脚本创建的目录，安装脚本会拒绝覆盖。确实要替换时设置 `FORCE_INSTALL=1`，旧目录会先移动到带时间戳的备份路径。

强制指定平台包：

```bash
TARGET=x86_64-unknown-linux-gnu VERSION=v1.6 ./install.sh
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

测量口径和每端输入方式分别选择：

| 模式 | 对齐依据 | 支持输入 |
| --- | --- | --- |
| `slot`（兼容原默认值） | 同 slot 首达 | UDP / Entry gRPC / 混合 |
| `shred` | 同 slot + index + data/code 类型 | UDP vs UDP |
| `transaction` | 同 slot + 交易首签名 | UDP / Entry gRPC / 混合 |

推荐使用配置文件，无需每次输入测速参数。先复制模板，编辑其中的服务商名称、UDP 监听地址或 Entry 地址和认证，再运行：

```bash
cp configs/shred-udp-shred.env.example shred-comparison.env
# 编辑 shred-comparison.env
./run-shred-comparison.sh
```

| 场景 | 配置模板 |
| --- | --- |
| UDP 对 UDP：逐 shred | [shred-udp-shred.env.example](configs/shred-udp-shred.env.example) |
| UDP 对 UDP：解码交易 | [shred-udp-transaction.env.example](configs/shred-udp-transaction.env.example) |
| Entry gRPC 对 Entry gRPC | [shred-entries-transaction.env.example](configs/shred-entries-transaction.env.example) |
| UDP 对 Entry gRPC | [shred-mixed-transaction.env.example](configs/shred-mixed-transaction.env.example) |
| slot 首达 | [shred-slot.env.example](configs/shred-slot.env.example) |

默认读取当前目录的 `shred-comparison.env`；启动脚本会先进入脚本所在目录。
也可用 `--config 路径` 或 `SHRED_COMPARISON_CONFIG` 选择其他文件。
优先级：**命令行 > 环境变量（含 `.env`）> 测速配置文件 > 内置默认值**。
未找到默认文件时保持原行为；显式指定不存在的文件、未知配置项或无效值会报错。
配置文件不会作为 shell 执行；含认证的个人文件不要提交到 Git（默认文件名已忽略）。
UDP 仍需在服务商控制台配置推送到本机两个不同端口；Entry gRPC 必须符合文档中的协议。

命令行方式仍可使用，也可临时覆盖配置，例如：

```bash
./run-shred-comparison.sh --duration-sec 120
./run-shred-comparison.sh --config configs/shred-mixed-transaction.env.example
```

第二个命令请先修改模板中的占位地址，或复制到个人配置文件再修改。
源码版脚本使用 Cargo 编译运行；下载包中的同名脚本直接运行二进制，配置方式相同。

统一输出匹配、单边、重复、迟到、领先/平局、带正负号的 B−A 差值及 P50/P95/P99。
两端准备完成后使用共同预热及测量窗口；无匹配样本、Entry 解码失败或本地观测队列溢出会返回错误。

[完整使用说明](docs/shred-comparison_CN.md)包含五种组合、认证、自定义 RPC 路径、
时间戳口径、资源限制和格式兼容性。无需运行 Jito proxy，也不需要 Jito 订阅。

安装脚本升级已托管的安装目录时，会保留 `.env` 和 `shred-comparison.env`。

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
VERSION=v1.6 ./compile.sh
```

如果本机或 CI 已配置对应 Rust target 和 linker，可以一次构建 macOS/Linux 多平台包：

```bash
rustup target add x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu x86_64-apple-darwin aarch64-apple-darwin

VERSION=v1.6 TARGETS="x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu x86_64-apple-darwin aarch64-apple-darwin" ./compile.sh
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
git tag v1.6
git push origin v1.6
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
VERSION=v1.6 TARGETS="x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu x86_64-apple-darwin aarch64-apple-darwin" ./compile.sh

git tag v1.6
git push origin v1.6

gh release create v1.6 \
  release/install.sh \
  release/grpc-benchmark-*.tar.gz \
  release/SHA256SUMS \
  --title "v1.6" \
  --notes "gRPC/Shred 对比测速和 raw shred 解码更新"
```
