#!/bin/bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"
# Copy configs/shred-udp-shred.env.example to shred-comparison.env and edit it.
# The binary loads the config; CLI and environment variables can override it.
exec cargo run --release --bin shred-comparison -- "$@"
