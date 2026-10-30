# Shred vs Shred: choosing a comparison

One command supports three measurement modes and independently selected input transports.
There is no dependency on the `jito-shredstream-proxy` crate, executable, authentication service,
or deployment. An Entry provider may expose the historically named `ShredstreamProxy` RPC;
that is only a wire protocol, consumed directly by this tool.

| What you want to compare | Mode | Source 1 / Source 2 |
| --- | --- | --- |
| First visibility of each slot | `slot` | UDP, Entry gRPC, or one of each |
| Delivery of identical raw shreds | `shred` | UDP / UDP |
| When identical transactions become readable | `transaction` | UDP, Entry gRPC, or one of each |

`slot` stays the default for compatibility. Use `shred` for packet-delivery comparisons and
`transaction` for end-to-end transaction usability. These metrics answer different questions;
do not combine their numbers into one ranking.

## Configuration files (recommended)

From the repository or downloaded bundle root:

```bash
cp configs/shred-udp-shred.env.example shred-comparison.env
# Edit the file, then run without benchmark arguments:
./run-shred-comparison.sh
```

The [README template table](../README.md#shred-vs-shred) covers raw UDP shreds, decoded UDP
transactions, Entry/Entry, UDP/Entry, and slot visibility. Settings use the existing `SHRED_*`
variable names; capacity controls also accept `SHRED_RETENTION_SLOTS`, `SHRED_MAX_KEYS`, and
`SHRED_MAX_SAMPLES`. Select another file with `--config PATH` or `SHRED_COMPARISON_CONFIG`.
Recursive includes are unsupported. Relative paths, including JSON output, resolve against the
working directory; launchers first change to their own directory.
Precedence: CLI > process environment > `.env` > comparison config > defaults.
Unknown/duplicate keys, syntax errors, invalid values, and explicitly missing files fail;
a missing default file preserves previous behavior. Existing `.env` loading is retained:
remove conflicting `SHRED_*` settings there if you want the comparison file to control them.
Keep credentials in a personal file or environment, outside Git. Leave commented authentication
settings alone when authentication is unnecessary; otherwise fill in a nonempty value.
Files use dotenv quoting and variable expansion, never shell execution; single-quote literal credentials containing `$`.

The CLI examples below remain supported and can temporarily override config settings.


## Quick starts

Source 1 is **A**, source 2 is **B**. Run `cargo build --release --bin shred-comparison` first;
all commands below also work through `./run-shred-comparison.sh`, which builds in release mode
and forwards arguments. Downloaded bundles use the same launcher without Cargo.

### 1. UDP vs UDP: identical shreds

Configure the providers to push raw Solana shreds to two distinct UDP ports on the **same host**.
This tool listens locally; it does not register your destination with a provider.

```bash
./target/release/shred-comparison --mode shred \
  --shred-bind-1 0.0.0.0:20000 --shred-name-1 Provider_A \
  --shred-bind-2 0.0.0.0:20001 --shred-name-2 Provider_B \
  --duration-sec 60 --json-output raw-shreds.json
```

The matching key is `(slot, index, data/code type)`. Repeated copies from one source keep
only its earliest timestamp. There is no Entry decoding or RPC in this mode.

### 2. UDP vs UDP: decoded transaction usability

```bash
./target/release/shred-comparison --mode transaction \
  --shred-bind-1 0.0.0.0:20000 --shred-name-1 Provider_A \
  --shred-bind-2 0.0.0.0:20001 --shred-name-2 Provider_B \
  --duration-sec 60 --json-output udp-transactions.json
```

Each input independently performs local reassembly, available Merkle FEC recovery and Entry
decoding. Transactions are matched by `(slot, first signature)`, independent of Entry batching.
The timestamp is **after local decoding**, so this includes decoder cost and missing-shred recovery.
It does not measure execution, confirmation, or transaction landing.

### 3. Entry gRPC vs Entry gRPC

Use environment variables for endpoints and credentials; they are not included in reports.
Omit tokens for providers that do not require authentication.

```bash
export SHRED_URL_1="https://provider-a.example:443"
export SHRED_URL_2="https://provider-b.example:443"
export SHRED_TOKEN_1="your-token-a"
export SHRED_TOKEN_2="your-token-b"
./target/release/shred-comparison --mode transaction \
  --source-1 entries-grpc --source-2 entries-grpc \
  --shred-name-1 Entry_A --shred-name-2 Entry_B \
  --duration-sec 60 --json-output entries.json
```

By default authentication uses `x-token`. For `x-api-key`, pass
`--shred-token-header-1 x-api-key` (and/or `-2`). For bearer auth, select `authorization` and
put the complete `Bearer ...` value in `SHRED_TOKEN_N`. HTTPS uses native trust roots.

### 4. UDP vs Entry gRPC

```bash
export SHRED_URL_2="https://provider-b.example:443"
export SHRED_TOKEN_2="your-token-b"
./target/release/shred-comparison --mode transaction \
  --source-1 udp --shred-bind-1 0.0.0.0:20000 --shred-name-1 Raw_A \
  --source-2 entries-grpc --shred-name-2 Entry_B \
  --duration-sec 60 --json-output mixed.json
```

The UDP side includes local recovery/decoding; the Entry side includes provider preprocessing,
gRPC transport and local Entry deserialization. This is deliberately an **application usability**
comparison. It is not a pure UDP-versus-TCP network latency measurement.

### 5. Slot first visibility

Use `--mode slot` with any of the above transport combinations. For UDP it timestamps the first
parseable data or coding shred of a slot; for Entry gRPC it timestamps the first valid Entry message
of that slot at receive time. The inputs represent different processing stages, and neither needs
to contain the same shred or transaction.

## Entry protocol compatibility

The default server-streaming method is:

```text
/shredstream.ShredstreamProxy/SubscribeEntries
```

Request: empty protobuf message. Response wire schema:

```protobuf
message Entry {
  uint64 slot = 1;
  bytes entries = 2; // canonical bincode Vec<solana_entry::entry::Entry>
}
```

Override the path independently using `--entries-method-1 /package.Service/Method` or
`SHRED_ENTRIES_METHOD_1` (and `-2`). A custom path must retain the same request/response schema.
This does **not** make arbitrary JSON, Yellowstone transaction streams, WebSockets, or different
protobuf schemas compatible. Header names, TLS, and RPC path are configurable without a proxy.

The wire decoder supports legacy, v0 (including address lookup tables), and v1 transactions.
V1 uses the `0x81` prefix and places signatures after the message; UDP and Entry gRPC therefore
match its first signature in the same way. UDP also supports Agave 4.x block markers between
DATA_COMPLETE boundaries, including split markers and zero padding; markers contain no transactions.
Entry gRPC must still send a complete `Vec<Entry>`, not a raw block marker or an arbitrary stream format. Malformed,
truncated, trailing-byte, excessive-count and oversized messages are rejected and counted;
check `decode_errors`/`invalid` rather than interpreting zero matches as a fast provider.
Message size is capped at 64 MiB and Entry count at 50,000 per message.

The parser only reads boundaries and signatures. It does not perform cryptographic verification
or transaction sanitization/execution, and does not imply support for future transaction versions.
Wire layouts are documented by the upstream [v1 message](https://github.com/anza-xyz/solana-sdk/blob/master/message/src/versions/v1/message.rs),
[versioned transaction](https://github.com/anza-xyz/solana-sdk/blob/master/transaction/src/versioned/mod.rs),
and [Agave block component](https://github.com/anza-xyz/agave/blob/master/entry/src/block_component.rs) implementations.

For UDP, `--shred-decode-mode-N ledger` is the default raw Solana format. `compat` is the
existing fixed-offset compatibility decoder. `--shred-udp-payload-prefix-skip-N N` supports
known transport prefixes; it is not a general framing decoder. Entry boundaries may differ
between sources; transaction matching is unaffected.

## Statistics and timing

- Both inputs bind/connect/subscribe before a shared warmup (`--warmup-ms`, default 1000).
  Both continue receiving during warmup; its observations are excluded.
- The measurement interval is `--duration-sec` (default 30). Afterwards, sources continue for
  one settle window to allow counterparts of already measured samples. New keys are excluded.
- `--settle-window-ms` (default 1000) is the maximum inter-source arrival skew accepted for a
  match. A counterpart beyond that limit is counted as late; its original sample remains single-sided.
- `matched` counts samples received by both inputs inside the wait window. Wins and ties use
  **matched samples only**; a single-sided sample is not automatically a speed win.
- `only` means observed only on that input within the wait window. It is not an independently
  established network packet-loss rate: a provider may not publish the same coverage.
- `b_minus_a_us` is B arrival minus A arrival: **positive means A earlier**, negative means B
  earlier. Signed P50/P95/P99 use ascending nearest-rank percentiles. Also inspect
  `loser_gap_us`, the distribution of absolute differences (including zero ties).
- All timestamps use the same host's monotonic **application** clock. UDP timestamps are taken
  just after `recv_from`; transaction timestamps after decode. No kernel/hardware timestamping
  is claimed. Scheduling, socket backlog and decoding affect results.
- Header-valid shreds and decoded transactions are not proof of canonical inclusion. The raw
  path does not validate leader signatures, the matching key does not distinguish equivocations,
  and no RPC or funded transactions are used by the benchmark.

## Output, errors and resource limits

Console output includes per-input message counts, invalid payloads, decode errors, observations
lost from the local queue and FEC recovered-shred counts. `--json-output path.json` writes the
same results with `schema_version: 1`, input transport types and measurement settings; credentials
and endpoint URLs are omitted. Zero matches, an observation queue overflow, or Entry payload decode failures set `valid: false` and `invalid_reasons`
and returns a nonzero exit status. One-sided sources remain visible in the report. Bind,
subscription/authentication, early stream termination and capacity errors also return nonzero;
they must not be interpreted as a completed comparison.

The observation channel is bounded and uses nonblocking sends. Watch `queue_dropped`; invalid
runs need a less loaded host or a lower-input-cost setup. Header errors and decode errors are
reported separately. Receive counters include warmup and the final grace period; comparison statistics apply to the measured window. UDP ports must be exclusively owned, not shared with an existing receiver.

The requested UDP socket buffer is 128 MiB by default, but the OS can clamp
it; this tool does not claim to detect kernel or NIC drops. Watch host UDP/NIC counters separately.

Settled identities are retained for `--retention-slots` (default 128), then pruned in slot batches.
Older arrivals are ignored and counted as late. Pending samples retain their complete wait window.
State is capped by `--max-keys` (default 1,000,000 retained IDs) and `--max-samples` (default
5,000,000 matched latency samples). Reaching a cap **fails explicitly**, without silently sampling.
For long/high-volume runs, increase limits according to available memory or use shorter runs.
UDP decoder state also respects `--shred-max-tracked-slots` and
`--shred-reassembly-gap-timeout-ms`; increase the latter if recovery spans longer gaps.

Use `--help` for CLI flags. All previous `SHRED_BIND_N`, `SHRED_NAME_N`, decode-mode/prefix,
buffer and duration environment variables remain supported. New settings include
`SHRED_COMPARISON_MODE`, `SHRED_SOURCE_N`, `SHRED_URL_N`, `SHRED_TOKEN_N`,
`SHRED_TOKEN_HEADER_N`, `SHRED_ENTRIES_METHOD_N`, `SHRED_WARMUP_MS`,
`SHRED_SETTLE_WINDOW_MS`, and `SHRED_JSON_OUTPUT`. The binary loads a local `.env`; CLI flags
have priority over environment values.

## Validation

`cargo test --all-targets` covers sample alignment, signed differences, missing/duplicate/late
samples, warmup/grace boundaries and hard capacity limits. Local UDP tests use genuinely signed
legacy/v0 (including ALT) transactions embedded in synthetic data-shred headers, including split payloads, reordering,
prefixes and retransmission in both decoder modes. A local authenticated Entry gRPC fixture tests
the actual streaming transport, a custom RPC path, malformed payloads, and the same signed
transaction arriving through UDP and Entry gRPC. Tests also cover missing-data Reed-Solomon recovery with either parity/data arrival order and all five CLI combinations. These are local correctness tests, not production
provider performance measurements.

Real legacy/v0/v1 fixtures are additionally verified and matched through Entry gRPC and both UDP
decoder modes with a padded, split block marker. Regression tests reject noncanonical/overflowing
compact-u16 lengths and truncated/unknown-version markers; every truncation of each wire transaction
fixture is checked. The contributor's mainnet replay numbers are not reproduced by these local tests.
