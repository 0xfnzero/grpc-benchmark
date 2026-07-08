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

RUST_LOG="${RUST_LOG:-info}" cargo run --bin grpc-vs-shred
