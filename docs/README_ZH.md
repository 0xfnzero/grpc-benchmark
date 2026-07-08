# gRPC / Shred 基准测试工具

本教程与根目录 README 保持一致，并把对比测速排在最前面。

## 平台二进制

Release 包支持 macOS 和 Linux：

- `x86_64-apple-darwin`
- `aarch64-apple-darwin`
- `x86_64-unknown-linux-gnu`
- `aarch64-unknown-linux-gnu`

安装：

```bash
curl -fsSL -o install.sh https://github.com/0xfnzero/grpc-benchmark/releases/latest/download/install.sh
chmod +x install.sh
./install.sh
cd "$HOME/grpc-benchmark"
```

指定 tag：

```bash
VERSION=v1.4 ./install.sh
```

如果安装目录已存在且不是本安装脚本创建的目录，请换一个 `INSTALL_DIR`，或设置 `FORCE_INSTALL=1` 先备份再替换。

## 对比测速

### 1. gRPC vs gRPC

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

### 2. Shred vs Shred

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

### 3. gRPC vs Shred

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

```bash
./run-latency-test.sh
./run-shred.sh
./run-shred-benchmark.sh
./run-benchmark-jito.sh
```

raw shred 解码不依赖 Jito ShredStream 服务。`ledger` 模式直接解析 Solana wire header，并可用 Merkle Reed-Solomon FEC。只有供应商发送 ShredStream SDK 固定偏移 payload 时才使用 `SHRED_DECODE_MODE=compat`。

## Release 和 Tag

推荐流程：推送 `v*` tag，由 GitHub Actions 自动构建并上传全部 macOS/Linux 包：

```bash
git tag v1.4
git push origin v1.4
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
