#!/bin/bash

export FZSTREAM_SERVER_ADDRESS=64.130.37.188:2222
export AUTH_TOKEN=demo_token

export GRPC_URL=https://solana-yellowstone-grpc.publicnode.com:443
export GRPC_TOKEN=

RUST_LOG=info cargo run --bin grpc-vs-fzstream