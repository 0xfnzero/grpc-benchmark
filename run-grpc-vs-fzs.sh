#!/bin/bash

export FZSTREAM_SERVER_ADDRESS=127.0.0.1:2222
export AUTH_TOKEN=demo_token_12345

export GRPC_URL=https://solana-yellowstone-grpc.publicnode.com:443
export GRPC_TOKEN=

RUST_LOG=info cargo run --bin grpc-vs-fzstream
