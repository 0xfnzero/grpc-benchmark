# Solana Yellowstone gRPC / Raw Shred Benchmark

[中文](https://github.com/0xfnzero/grpc-benchmark/blob/main/README_CN.md) | [English](https://github.com/0xfnzero/grpc-benchmark/blob/main/README.md) | [Telegram](https://t.me/fnzero_group) | [Discord](https://discord.gg/vuazbGkqQE)

High-performance Rust CLI for Solana latency benchmarking: Yellowstone gRPC vs gRPC, raw shred vs raw shred, and gRPC vs raw shred. It measures first-slot arrival latency, UDP shred throughput, Entry/transaction decode stats, and Jito Block Engine HTTP performance on macOS and Linux.

**Keywords:** Solana benchmark, Yellowstone gRPC, Geyser gRPC, raw shred, ShredStream, shred latency, gRPC latency, validator infrastructure, Jito Block Engine.

## Supported Binaries

Release packages support macOS and Linux. The installer auto-detects:

- `x86_64-apple-darwin`
- `aarch64-apple-darwin`
- `x86_64-unknown-linux-gnu`
- `aarch64-unknown-linux-gnu`

## Benchmark Order

Comparison benchmarks are listed first because they are the main latency-comparison workflows.

| Binary | Script | Measures |
|--------|--------|----------|
| `grpc-comparison` | `run-grpc-comparison.sh` | First-slot order and latency across Yellowstone gRPC endpoints |
| `shred-comparison` | `run-shred-comparison.sh` | Slot, identical shred, or transaction arrival; UDP / Entry gRPC / mixed sources |
| `grpc-vs-shred` | `run-grpc-vs-shred.sh` | First-slot latency between Yellowstone gRPC and raw shred |
| `latency-test` | `run-latency-test.sh` | Single gRPC endpoint ping/subscription latency |
| `shred` | `run-shred.sh` | Raw shred UDP listener with first-slot and decoded-segment logs |
| `shred-benchmark` | `run-shred-benchmark.sh` | Raw shred UDP throughput and Entry/transaction decode stats |
| `benchmark-jito` | `run-benchmark-jito.sh` | Jito Block Engine HTTP request performance |

The old private stream entrypoint has been removed. Raw shred decoding does not depend on Jito ShredStream services. The default `ledger` mode parses Solana wire headers directly, streams contiguous data shred batches into `bincode Vec<Entry>`, and can use Merkle Reed-Solomon FEC to recover missing data shreds. `shred-comparison` supports `slot`, `shred`, and `transaction` modes with UDP and provider-hosted Entry gRPC adapters; only transaction mode decodes UDP Entries. `grpc-vs-shred` retains first-slot header comparison.

## Install

macOS and Linux:

```bash
curl -fsSL -o install.sh https://github.com/0xfnzero/grpc-benchmark/releases/latest/download/install.sh
chmod +x install.sh
./install.sh
cd "$HOME/grpc-benchmark"
```

Install a specific release tag or directory:

```bash
VERSION=v1.5 ./install.sh
INSTALL_DIR=/opt/grpc-benchmark sudo -E ./install.sh
```

If `INSTALL_DIR` already exists and was not created by this installer, the installer refuses to replace it. To intentionally replace it, set `FORCE_INSTALL=1`; the previous directory is moved to a timestamped backup first.

If you need to force a platform asset:

```bash
TARGET=x86_64-unknown-linux-gnu VERSION=v1.5 ./install.sh
```

## Comparison Benchmarks

### gRPC vs gRPC

Edit `run-grpc-comparison.sh`:

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

Choose the metric independently of each source's transport:

| Mode | Matching key | Inputs |
| --- | --- | --- |
| `slot` (compatible default) | Slot first visibility | UDP / Entry gRPC / mixed |
| `shred` | Same slot + index + data/code type | UDP vs UDP |
| `transaction` | Same slot + first transaction signature | UDP / Entry gRPC / mixed |

Use a configuration file to avoid entering benchmark flags each time. Copy a template,
edit provider names, UDP bind addresses or Entry endpoints/authentication, then run:

```bash
cp configs/shred-udp-shred.env.example shred-comparison.env
# Edit shred-comparison.env
./run-shred-comparison.sh
```

| Scenario | Configuration template |
| --- | --- |
| UDP vs UDP: identical shreds | [shred-udp-shred.env.example](configs/shred-udp-shred.env.example) |
| UDP vs UDP: decoded transactions | [shred-udp-transaction.env.example](configs/shred-udp-transaction.env.example) |
| Entry gRPC vs Entry gRPC | [shred-entries-transaction.env.example](configs/shred-entries-transaction.env.example) |
| UDP vs Entry gRPC | [shred-mixed-transaction.env.example](configs/shred-mixed-transaction.env.example) |
| First slot visibility | [shred-slot.env.example](configs/shred-slot.env.example) |

The binary reads `shred-comparison.env` from the working directory by default; launchers
first change to their own directory. Select another file with `--config PATH` or
`SHRED_COMPARISON_CONFIG`. Precedence: **CLI > environment (including `.env`) > comparison
config file > built-in defaults**. A missing default file preserves the previous behavior;
an explicitly missing file, unknown key or invalid value fails. Files are parsed as dotenv,
never executed as shell scripts. Keep personal credentials out of Git (the default filename is ignored).
UDP providers must push to separate ports on this host; Entry providers must support the documented protocol.

CLI flags remain available, including temporary overrides:

```bash
./run-shred-comparison.sh --duration-sec 120
./run-shred-comparison.sh --config configs/shred-mixed-transaction.env.example
```

Replace placeholder endpoints before the second command, or copy the template to a personal file first.
The source launcher builds with Cargo; downloaded bundles run the binary directly with the same configuration.

Reports include matched and single-sided samples, duplicates, late arrivals, wins/ties,
signed B−A differences and P50/P95/P99 gaps. No matched samples, Entry decode failures or local queue overflow
returns an error. Both sources prepare before a shared warmup and measurement window.

See the [complete setup guide](docs/shred-comparison.md) for all five combinations,
authentication, custom RPC paths, timing semantics, resource limits, and format compatibility.
No Jito proxy process or Jito subscription is required.

The installer preserves `.env` and `shred-comparison.env` when upgrading a managed installation.

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

## Single-Source Benchmarks

### gRPC Latency

```bash
export GRPC_URL="https://solana-yellowstone-grpc.publicnode.com:443"
export GRPC_TOKEN=""
export TOTAL_ROUNDS=10
./run-latency-test.sh
```

### Shred Listener

```bash
export SHRED_BIND="0.0.0.0:20000"
export SHRED_DECODE_MODE="ledger"
export SHRED_DECODE_ENTRIES=true
export SHRED_ENABLE_FEC=true
export SHRED_DURATION_SEC=30
./run-shred.sh
```

For ShredStream SDK fixed-offset payloads:

```bash
export SHRED_DECODE_MODE="compat"
./run-shred.sh
```

### Shred Throughput And Decode Benchmark

```bash
export SHRED_BIND="0.0.0.0:20000"
export SHRED_DECODE_MODE="ledger"
export SHRED_DECODE_ENTRIES=true
export SHRED_ENABLE_FEC=true
export SHRED_BENCHMARK_DURATION_SEC=30
export SHRED_RECV_BUFFER_BYTES=134217728
./run-shred-benchmark.sh
```

### Jito Block Engine HTTP Benchmark

```bash
export JITO_URL="https://amsterdam.mainnet.block-engine.jito.wtf"
export JITO_CONCURRENCY=10
export JITO_DURATION_SEC=0   # 0 = run until stopped
./run-benchmark-jito.sh
```

This is not Jito ShredStream.

## Common Shred Settings

```bash
export SHRED_DECODE_MODE="ledger"              # ledger/raw/agave or compat/sdk
export SHRED_DECODE_ENTRIES=true               # false = header-only fast path
export SHRED_ENABLE_FEC=true                   # Merkle Reed-Solomon recovery in ledger mode
export SHRED_RECV_BUFFER_BYTES=134217728
export SHRED_UDP_PAYLOAD_PREFIX_SKIP=0
export SHRED_MAX_TRACKED_SLOTS=128
export SHRED_REASSEMBLY_GAP_TIMEOUT_MS=500
export SHRED_STUCK_BATCH_TIMEOUT_MS=50
export SHRED_MAX_FEC_SETS_PER_SLOT=32
```

## Build Release Assets

Build the host platform package:

```bash
VERSION=v1.5 ./compile.sh
```

Build multiple macOS/Linux packages when the required Rust targets and linkers are available:

```bash
rustup target add x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu x86_64-apple-darwin aarch64-apple-darwin

VERSION=v1.5 TARGETS="x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu x86_64-apple-darwin aarch64-apple-darwin" ./compile.sh
```

Release assets are generated under `release/`:

```text
install.sh
grpc-benchmark-x86_64-unknown-linux-gnu.tar.gz
grpc-benchmark-aarch64-unknown-linux-gnu.tar.gz
grpc-benchmark-x86_64-apple-darwin.tar.gz
grpc-benchmark-aarch64-apple-darwin.tar.gz
SHA256SUMS
```

## Create Tag And GitHub Release

Recommended release flow: push a `v*` tag. GitHub Actions builds and uploads all macOS/Linux binary packages automatically:

```bash
git tag v1.5
git push origin v1.5
```

The release workflow publishes:

```text
install.sh
grpc-benchmark-x86_64-unknown-linux-gnu.tar.gz
grpc-benchmark-aarch64-unknown-linux-gnu.tar.gz
grpc-benchmark-x86_64-apple-darwin.tar.gz
grpc-benchmark-aarch64-apple-darwin.tar.gz
SHA256SUMS
```

Manual fallback:

```bash
VERSION=v1.5 TARGETS="x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu x86_64-apple-darwin aarch64-apple-darwin" ./compile.sh

git tag v1.5
git push origin v1.5

gh release create v1.5 \
  release/install.sh \
  release/grpc-benchmark-*.tar.gz \
  release/SHA256SUMS \
  --title "v1.5" \
  --notes "gRPC/Shred comparison benchmarks and raw shred decoding updates"
```
