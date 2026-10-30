# Solana Yellowstone gRPC / Raw Shred Benchmark

Version v1.6 supports legacy/v0/v1 wire transactions and Agave 4.x UDP block markers. See the [complete comparison guide](shred-comparison.md#entry-protocol-compatibility) for protocol requirements and measurement limits.

This guide mirrors the root README and keeps comparison benchmarks first. The tool targets Solana Yellowstone gRPC, Geyser gRPC, raw shred/ShredStream latency, UDP shred throughput, and Jito Block Engine HTTP benchmarking.

## Platform Binaries

Release packages support macOS and Linux:

- `x86_64-apple-darwin`
- `aarch64-apple-darwin`
- `x86_64-unknown-linux-gnu`
- `aarch64-unknown-linux-gnu`

Install:

```bash
curl -fsSL -o install.sh https://github.com/0xfnzero/grpc-benchmark/releases/latest/download/install.sh
chmod +x install.sh
./install.sh
cd "$HOME/grpc-benchmark"
```

Use a specific tag:

```bash
VERSION=v1.6 ./install.sh
```

If the install directory already exists and was not created by this installer, use a new `INSTALL_DIR` or set `FORCE_INSTALL=1` to back it up and replace it.

## Comparison Benchmarks

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

## Single-Source Benchmarks

```bash
./run-latency-test.sh
./run-shred.sh
./run-shred-benchmark.sh
./run-benchmark-jito.sh
```

Raw shred decoding does not depend on Jito ShredStream services. `ledger` mode parses Solana wire headers directly and can use Merkle Reed-Solomon FEC. Use `SHRED_DECODE_MODE=compat` only for ShredStream SDK fixed-offset payloads.

## Release And Tag

With GitHub Actions enabled, push a `v*` tag to build/upload all macOS/Linux packages. If Actions is disabled, use the manual fallback below:

```bash
git tag v1.6
git push origin v1.6
```

Manual fallback:

```bash
VERSION=v1.6 TARGETS="x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu x86_64-apple-darwin aarch64-apple-darwin" ./compile.sh

git tag v1.6
git push origin v1.6

gh release create v1.6 \
  release/install.sh \
  release/grpc-benchmark-*.tar.gz \
  release/SHA256SUMS \
  --title "v1.6" \
  --notes "gRPC/Shred comparison benchmarks and raw shred decoding updates"
```
