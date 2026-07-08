#!/bin/bash

set -euo pipefail

export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target}"

VERSION="${VERSION:-dev}"
PKG="${PKG:-}"
RELEASE_DIR="${RELEASE_DIR:-$PWD/release}"
TARGETS_VALUE="${TARGETS:-${TARGET:-$(rustc -vV | awk '/host:/ {print $2}')}}"
TARGETS_VALUE="${TARGETS_VALUE//,/ }"
read -r -a TARGET_LIST <<< "$TARGETS_VALUE"

BINS=(
  "grpc-comparison"
  "shred-comparison"
  "grpc-vs-shred"
  "latency-test"
  "shred"
  "shred-benchmark"
  "benchmark-jito"
)

generate_run_scripts() {
  local dir="$1"

  cat > "$dir/run-grpc-comparison.sh" << 'EOF'
#!/bin/bash

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

export GRPC_URL_1="${GRPC_URL_1:-https://solana-yellowstone-grpc.publicnode.com:443}"
export GRPC_NAME_1="${GRPC_NAME_1:-Public_Node}"
export GRPC_TOKEN_1="${GRPC_TOKEN_1:-}"

export GRPC_URL_2="${GRPC_URL_2:-https://your-grpc-endpoint.com:443}"
export GRPC_NAME_2="${GRPC_NAME_2:-Custom_Node}"
export GRPC_TOKEN_2="${GRPC_TOKEN_2:-}"

export GRPC_COMPARISON_DURATION_SEC="${GRPC_COMPARISON_DURATION_SEC:-30}"
export CONCURRENCY="${CONCURRENCY:-10}"

./grpc-comparison
EOF

  cat > "$dir/run-shred-comparison.sh" << 'EOF'
#!/bin/bash

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

export SHRED_BIND_1="${SHRED_BIND_1:-0.0.0.0:20000}"
export SHRED_NAME_1="${SHRED_NAME_1:-Shred_1}"
export SHRED_DECODE_MODE_1="${SHRED_DECODE_MODE_1:-ledger}"
export SHRED_UDP_PAYLOAD_PREFIX_SKIP_1="${SHRED_UDP_PAYLOAD_PREFIX_SKIP_1:-0}"

export SHRED_BIND_2="${SHRED_BIND_2:-0.0.0.0:20001}"
export SHRED_NAME_2="${SHRED_NAME_2:-Shred_2}"
export SHRED_DECODE_MODE_2="${SHRED_DECODE_MODE_2:-ledger}"
export SHRED_UDP_PAYLOAD_PREFIX_SKIP_2="${SHRED_UDP_PAYLOAD_PREFIX_SKIP_2:-0}"

export SHRED_RECV_BUFFER_BYTES="${SHRED_RECV_BUFFER_BYTES:-134217728}"
export SHRED_COMPARISON_DURATION_SEC="${SHRED_COMPARISON_DURATION_SEC:-30}"

./shred-comparison
EOF

  cat > "$dir/run-grpc-vs-shred.sh" << 'EOF'
#!/bin/bash

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

export GRPC_URL="${GRPC_URL:-https://solana-yellowstone-grpc.publicnode.com:443}"
export GRPC_TOKEN="${GRPC_TOKEN:-}"
export GRPC_NAME="${GRPC_NAME:-gRPC}"

export SHRED_BIND="${SHRED_BIND:-0.0.0.0:20000}"
export SHRED_NAME="${SHRED_NAME:-Shred}"
export SHRED_DECODE_MODE="${SHRED_DECODE_MODE:-ledger}"
export SHRED_DECODE_ENTRIES="${SHRED_DECODE_ENTRIES:-false}"
export SHRED_RECV_BUFFER_BYTES="${SHRED_RECV_BUFFER_BYTES:-134217728}"

export TEST_DURATION="${TEST_DURATION:-30}"

./grpc-vs-shred
EOF

  cat > "$dir/run-latency-test.sh" << 'EOF'
#!/bin/bash

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

export GRPC_URL="${GRPC_URL:-https://solana-yellowstone-grpc.publicnode.com:443}"
export GRPC_TOKEN="${GRPC_TOKEN:-}"
export TOTAL_ROUNDS="${TOTAL_ROUNDS:-10}"
export PING_INTERVAL_MS="${PING_INTERVAL_MS:-1000}"
export TEST_TIMEOUT="${TEST_TIMEOUT:-120}"

./latency-test
EOF

  cat > "$dir/run-shred.sh" << 'EOF'
#!/bin/bash

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

export SHRED_BIND="${SHRED_BIND:-0.0.0.0:20000}"
export SHRED_DECODE_MODE="${SHRED_DECODE_MODE:-ledger}"
export SHRED_DECODE_ENTRIES="${SHRED_DECODE_ENTRIES:-true}"
export SHRED_ENABLE_FEC="${SHRED_ENABLE_FEC:-true}"
export SHRED_DURATION_SEC="${SHRED_DURATION_SEC:-30}"
export SHRED_RECV_BUFFER_BYTES="${SHRED_RECV_BUFFER_BYTES:-134217728}"
export SHRED_STUCK_BATCH_TIMEOUT_MS="${SHRED_STUCK_BATCH_TIMEOUT_MS:-50}"
export SHRED_MAX_FEC_SETS_PER_SLOT="${SHRED_MAX_FEC_SETS_PER_SLOT:-32}"

./shred
EOF

  cat > "$dir/run-shred-benchmark.sh" << 'EOF'
#!/bin/bash

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

export SHRED_BIND="${SHRED_BIND:-0.0.0.0:20000}"
export SHRED_DECODE_MODE="${SHRED_DECODE_MODE:-ledger}"
export SHRED_DECODE_ENTRIES="${SHRED_DECODE_ENTRIES:-true}"
export SHRED_ENABLE_FEC="${SHRED_ENABLE_FEC:-true}"
export SHRED_BENCHMARK_DURATION_SEC="${SHRED_BENCHMARK_DURATION_SEC:-30}"
export SHRED_RECV_BUFFER_BYTES="${SHRED_RECV_BUFFER_BYTES:-134217728}"
export SHRED_STUCK_BATCH_TIMEOUT_MS="${SHRED_STUCK_BATCH_TIMEOUT_MS:-50}"
export SHRED_MAX_FEC_SETS_PER_SLOT="${SHRED_MAX_FEC_SETS_PER_SLOT:-32}"

./shred-benchmark
EOF

  cat > "$dir/run-benchmark-jito.sh" << 'EOF'
#!/bin/bash

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

export JITO_URL="${JITO_URL:-https://amsterdam.mainnet.block-engine.jito.wtf}"
export JITO_CONCURRENCY="${JITO_CONCURRENCY:-10}"
export JITO_DURATION_SEC="${JITO_DURATION_SEC:-0}"

./benchmark-jito
EOF

  chmod +x "$dir"/run-*.sh
}

build_target() {
  local target="$1"
  local target_dir="${CARGO_TARGET_DIR}/${target}/release"
  local staging_root="${RELEASE_DIR}/staging/${target}"
  local package_dir="${staging_root}/grpc-benchmark"
  local archive="${RELEASE_DIR}/grpc-benchmark-${target}.tar.gz"

  echo "==> Target: $target"
  rm -rf "$staging_root"
  mkdir -p "$package_dir"

  for bin in "${BINS[@]}"; do
    echo "   build $bin"
    if [[ -n "$PKG" ]]; then
      cargo build --release --target "$target" --package "$PKG" --bin "$bin"
    else
      cargo build --release --target "$target" --bin "$bin"
    fi

    local src_path="${target_dir}/${bin}"
    if [[ ! -f "$src_path" ]]; then
      local candidate
      candidate=$(ls -t "${target_dir}/deps/${bin}"-* 2>/dev/null | head -n1 || true)
      if [[ -n "$candidate" && -x "$candidate" ]]; then
        src_path="$candidate"
      fi
    fi

    if [[ ! -f "$src_path" ]]; then
      echo "missing binary: ${target_dir}/${bin}" >&2
      exit 1
    fi

    cp "$src_path" "${package_dir}/${bin}"
    chmod +x "${package_dir}/${bin}"
  done

  generate_run_scripts "$package_dir"
  cp README.md README_CN.md LICENSE "$package_dir/"
  touch "$package_dir/.grpc-benchmark-install"

  tar -C "$staging_root" -czf "$archive" grpc-benchmark
  echo "   packaged $archive"

  if [[ ! -d lib ]]; then
    cp -R "$package_dir" lib
  fi
}

if [[ "${#TARGET_LIST[@]}" -eq 0 ]]; then
  echo "No targets specified" >&2
  exit 1
fi

rm -rf "$RELEASE_DIR"
rm -rf lib
mkdir -p "$RELEASE_DIR"

for target in "${TARGET_LIST[@]}"; do
  build_target "$target"
done

cp install.sh "${RELEASE_DIR}/install.sh"
chmod +x "${RELEASE_DIR}/install.sh"

(
  cd "$RELEASE_DIR"
  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 grpc-benchmark-*.tar.gz > SHA256SUMS
  elif command -v sha256sum >/dev/null 2>&1; then
    sha256sum grpc-benchmark-*.tar.gz > SHA256SUMS
  fi
)

echo ""
echo "Release assets generated in: $RELEASE_DIR"
echo "Targets:"
for target in "${TARGET_LIST[@]}"; do
  echo "- grpc-benchmark-${target}.tar.gz"
done
echo "- install.sh"
[[ -f "${RELEASE_DIR}/SHA256SUMS" ]] && echo "- SHA256SUMS"
echo ""
echo "Comparison benchmarks first:"
echo "1. ./lib/run-grpc-comparison.sh"
echo "2. ./lib/run-shred-comparison.sh"
echo "3. ./lib/run-grpc-vs-shred.sh"
