use crate::error::{BenchmarkError, Result};
use crate::wire_entry::{self, BlockComponent, WireEntry, WireError};
use reed_solomon_erasure::galois_8::ReedSolomon;
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::net::SocketAddr;
use std::str::FromStr;
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;
use tokio::sync::mpsc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShredDecodeMode {
    Ledger,
    Compat,
}

impl FromStr for ShredDecodeMode {
    type Err = BenchmarkError;

    fn from_str(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "ledger" | "raw" | "agave" => Ok(Self::Ledger),
            "compat" | "sdk" | "shredstream-sdk" => Ok(Self::Compat),
            other => Err(BenchmarkError::ConfigError(format!(
                "unsupported shred decode mode: {other}"
            ))),
        }
    }
}

impl std::fmt::Display for ShredDecodeMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ledger => f.write_str("ledger"),
            Self::Compat => f.write_str("compat"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RawShredConfig {
    pub bind_addr: String,
    pub recv_buffer_bytes: usize,
    pub decode_mode: ShredDecodeMode,
    pub decode_entries: bool,
    /// Capture transaction signatures only for transaction comparisons.
    pub collect_signatures: bool,
    pub enable_fec: bool,
    pub udp_payload_prefix_skip: usize,
    pub max_tracked_slots: usize,
    pub reassembly_gap_timeout_ms: u64,
    pub stuck_batch_timeout_ms: u64,
    pub max_fec_sets_per_slot: usize,
    pub forward_slot_watermark: bool,
    pub shredstream_max_slot_age: u64,
}

impl Default for RawShredConfig {
    fn default() -> Self {
        Self {
            bind_addr: "0.0.0.0:20000".to_string(),
            recv_buffer_bytes: 128 * 1024 * 1024,
            decode_mode: ShredDecodeMode::Ledger,
            decode_entries: true,
            collect_signatures: false,
            enable_fec: true,
            udp_payload_prefix_skip: 0,
            max_tracked_slots: 128,
            reassembly_gap_timeout_ms: 500,
            stuck_batch_timeout_ms: 50,
            max_fec_sets_per_slot: 32,
            forward_slot_watermark: true,
            shredstream_max_slot_age: 128,
        }
    }
}

impl RawShredConfig {
    pub fn from_env() -> Result<Self> {
        let mut cfg = Self::default();

        if let Ok(value) = std::env::var("SHRED_BIND") {
            cfg.bind_addr = value;
        }
        if let Ok(value) = std::env::var("SHRED_RECV_BUFFER_BYTES") {
            cfg.recv_buffer_bytes = parse_env("SHRED_RECV_BUFFER_BYTES", &value)?;
        }
        if let Ok(value) = std::env::var("SHRED_DECODE_MODE") {
            cfg.decode_mode = value.parse()?;
        }
        if let Ok(value) = std::env::var("SHRED_DECODE_ENTRIES") {
            cfg.decode_entries = parse_bool_env("SHRED_DECODE_ENTRIES", &value)?;
        }
        if let Ok(value) = std::env::var("SHRED_ENABLE_FEC") {
            cfg.enable_fec = parse_bool_env("SHRED_ENABLE_FEC", &value)?;
        }
        if let Ok(value) = std::env::var("SHRED_UDP_PAYLOAD_PREFIX_SKIP") {
            cfg.udp_payload_prefix_skip = parse_env("SHRED_UDP_PAYLOAD_PREFIX_SKIP", &value)?;
        }
        if let Ok(value) = std::env::var("SHRED_MAX_TRACKED_SLOTS") {
            cfg.max_tracked_slots = parse_env("SHRED_MAX_TRACKED_SLOTS", &value)?;
        }
        if let Ok(value) = std::env::var("SHRED_REASSEMBLY_GAP_TIMEOUT_MS") {
            cfg.reassembly_gap_timeout_ms = parse_env("SHRED_REASSEMBLY_GAP_TIMEOUT_MS", &value)?;
        }
        if let Ok(value) = std::env::var("SHRED_STUCK_BATCH_TIMEOUT_MS") {
            cfg.stuck_batch_timeout_ms = parse_env("SHRED_STUCK_BATCH_TIMEOUT_MS", &value)?;
        }
        if let Ok(value) = std::env::var("SHRED_MAX_FEC_SETS_PER_SLOT") {
            cfg.max_fec_sets_per_slot = parse_env("SHRED_MAX_FEC_SETS_PER_SLOT", &value)?;
        }
        if let Ok(value) = std::env::var("SHRED_FORWARD_SLOT_WATERMARK") {
            cfg.forward_slot_watermark = parse_bool_env("SHRED_FORWARD_SLOT_WATERMARK", &value)?;
        }
        if let Ok(value) = std::env::var("SHREDSTREAM_MAX_SLOT_AGE") {
            cfg.shredstream_max_slot_age = parse_env("SHREDSTREAM_MAX_SLOT_AGE", &value)?;
        }

        Ok(cfg)
    }
}

fn parse_env<T>(name: &str, value: &str) -> Result<T>
where
    T: FromStr,
    T::Err: std::fmt::Display,
{
    value
        .parse()
        .map_err(|e| BenchmarkError::ConfigError(format!("invalid {name} value {value:?}: {e}")))
}

fn parse_bool_env(name: &str, value: &str) -> Result<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => Err(BenchmarkError::ConfigError(format!(
            "invalid {name} value {value:?}; expected true/false"
        ))),
    }
}

#[derive(Debug, Clone)]
pub struct ShredEvent {
    pub slot: u64,
    pub received_at: Instant,
    pub kind: ShredEventKind,
}

#[derive(Debug, Clone)]
pub enum ShredEventKind {
    FirstShred {
        shred_type: &'static str,
        index: u32,
    },
    DecodedEntries {
        entry_count: usize,
        transaction_count: usize,
    },
}

#[derive(Debug, Default, Clone)]
pub struct ShredRunStats {
    pub datagrams_received: u64,
    pub valid_shreds: u64,
    pub parse_errors: u64,
    pub unique_slots: u64,
    pub decoded_segments: u64,
    pub decoded_entries: u64,
    pub decoded_transactions: u64,
    pub decode_errors: u64,
    pub fec_recovered_data_shreds: u64,
}

#[derive(Debug, Default)]
pub(crate) struct ShredPacketOutcome {
    pub(crate) slot: Option<u64>,
    pub(crate) shred_type: Option<&'static str>,
    pub(crate) index: Option<u32>,
    decoded_segments: Vec<DecodedShredSegment>,
    pub(crate) signatures: Vec<[u8; 64]>,
    pub(crate) decode_errors: u64,
    pub(crate) fec_recovered_data_shreds: u64,
}

#[derive(Debug, Clone)]
struct DecodedShredSegment {
    slot: u64,
    entry_count: usize,
    transaction_count: usize,
}

pub async fn run_shred_stream(
    config: RawShredConfig,
    duration: Duration,
    event_tx: mpsc::Sender<ShredEvent>,
) -> Result<ShredRunStats> {
    let addr: SocketAddr = config.bind_addr.parse().map_err(|e| {
        BenchmarkError::ConfigError(format!("invalid SHRED_BIND {:?}: {e}", config.bind_addr))
    })?;

    let socket = bind_shred_udp(addr, config.recv_buffer_bytes).await?;
    let mut engine = ShredEngine::new(&config);
    let mut stats = ShredRunStats::default();
    let mut seen_slots = HashSet::new();
    let seen_slot_window = (config.max_tracked_slots.max(1) as u64).saturating_mul(4);
    let mut buf = [0u8; 2048];

    let deadline = tokio::time::Instant::now() + duration;
    let mut stop = Box::pin(tokio::time::sleep_until(deadline));
    let mut ticker = tokio::time::interval(Duration::from_millis(
        config.reassembly_gap_timeout_ms.max(1),
    ));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = &mut stop => break,
            _ = ticker.tick() => {
                engine.evict_stale_slots(Instant::now());
            }
            received = socket.recv_from(&mut buf) => {
                let (n, _from) = received?;
                stats.datagrams_received += 1;
                let now = Instant::now();
                let outcome = engine.push_packet(&buf[..n], now);

                if let Some(slot) = outcome.slot {
                    stats.valid_shreds += 1;
                    if seen_slots.insert(slot) {
                        stats.unique_slots += 1;
                        if seen_slots.len() > seen_slot_window as usize {
                            let min_seen_slot = slot.saturating_sub(seen_slot_window);
                            seen_slots.retain(|seen_slot| *seen_slot >= min_seen_slot);
                        }
                        let _ = event_tx
                            .try_send(ShredEvent {
                                slot,
                                received_at: now,
                                kind: ShredEventKind::FirstShred {
                                    shred_type: outcome.shred_type.unwrap_or("unknown"),
                                    index: outcome.index.unwrap_or_default(),
                                },
                            });
                    }
                } else {
                    stats.parse_errors += 1;
                    continue;
                }

                stats.decode_errors += outcome.decode_errors;
                stats.fec_recovered_data_shreds += outcome.fec_recovered_data_shreds;

                for segment in outcome.decoded_segments {
                    stats.decoded_segments += 1;
                    stats.decoded_entries += segment.entry_count as u64;
                    stats.decoded_transactions += segment.transaction_count as u64;
                    let _ = event_tx
                        .try_send(ShredEvent {
                            slot: segment.slot,
                            received_at: now,
                            kind: ShredEventKind::DecodedEntries {
                                entry_count: segment.entry_count,
                                transaction_count: segment.transaction_count,
                            },
                        });
                }
            }
        }
    }

    Ok(stats)
}

pub(crate) async fn bind_shred_udp(
    addr: SocketAddr,
    recv_buffer_bytes: usize,
) -> Result<UdpSocket> {
    if recv_buffer_bytes == 0 {
        return UdpSocket::bind(addr).await.map_err(BenchmarkError::from);
    }

    let domain = match addr {
        SocketAddr::V4(_) => socket2::Domain::IPV4,
        SocketAddr::V6(_) => socket2::Domain::IPV6,
    };
    let socket = socket2::Socket::new(domain, socket2::Type::DGRAM, Some(socket2::Protocol::UDP))?;
    // A benchmark must exclusively own its port: sharing would split UDP traffic.
    let _ = socket.set_recv_buffer_size(recv_buffer_bytes);
    socket.set_nonblocking(true)?;
    socket.bind(&addr.into())?;
    let std_socket: std::net::UdpSocket = socket.into();
    UdpSocket::from_std(std_socket).map_err(BenchmarkError::from)
}

pub(crate) enum ShredEngine {
    Fast(FastShredParser),
    Ledger(DataShredReassembler),
    Compat(ShredstreamCompatReassembler),
}

impl ShredEngine {
    pub(crate) fn new(config: &RawShredConfig) -> Self {
        if !config.decode_entries {
            return Self::Fast(FastShredParser::new(
                config.decode_mode,
                config.udp_payload_prefix_skip,
            ));
        }

        match config.decode_mode {
            ShredDecodeMode::Ledger => Self::Ledger(DataShredReassembler::new(config)),
            ShredDecodeMode::Compat => Self::Compat(ShredstreamCompatReassembler::with_signatures(
                config.forward_slot_watermark,
                config.shredstream_max_slot_age,
                config.udp_payload_prefix_skip,
                config.collect_signatures,
            )),
        }
    }

    pub(crate) fn push_packet(&mut self, packet: &[u8], now: Instant) -> ShredPacketOutcome {
        match self {
            Self::Fast(engine) => engine.push_packet(packet),
            Self::Ledger(engine) => engine.push_packet(packet, now),
            Self::Compat(engine) => engine.push_packet(packet),
        }
    }

    pub(crate) fn evict_stale_slots(&mut self, now: Instant) -> usize {
        match self {
            Self::Fast(engine) => engine.evict_stale_slots(now),
            Self::Ledger(engine) => engine.evict_stale_slots(now),
            Self::Compat(engine) => engine.evict_stale_slots(now),
        }
    }
}

pub(crate) struct FastShredParser {
    decode_mode: ShredDecodeMode,
    udp_payload_prefix_skip: usize,
}

impl FastShredParser {
    fn new(decode_mode: ShredDecodeMode, udp_payload_prefix_skip: usize) -> Self {
        Self {
            decode_mode,
            udp_payload_prefix_skip,
        }
    }

    fn push_packet(&mut self, packet: &[u8]) -> ShredPacketOutcome {
        match self.decode_mode {
            ShredDecodeMode::Ledger => match parse_wire_shred(packet, self.udp_payload_prefix_skip)
            {
                Some(ParsedWireShred::Data(shred)) => ShredPacketOutcome {
                    slot: Some(shred.slot),
                    shred_type: Some("data"),
                    index: Some(shred.index),
                    ..Default::default()
                },
                Some(ParsedWireShred::Code(shred)) => ShredPacketOutcome {
                    slot: Some(shred.slot),
                    shred_type: Some("code"),
                    index: Some(shred.index),
                    ..Default::default()
                },
                None => ShredPacketOutcome::default(),
            },
            ShredDecodeMode::Compat => {
                match parse_compat_packet(packet, self.udp_payload_prefix_skip) {
                    Some(shred) => ShredPacketOutcome {
                        slot: Some(shred.slot),
                        shred_type: Some("data"),
                        index: Some(shred.index),
                        ..Default::default()
                    },
                    None => ShredPacketOutcome::default(),
                }
            }
        }
    }

    fn evict_stale_slots(&mut self, _now: Instant) -> usize {
        0
    }
}

const COMMON_HEADER_SIZE: usize = 83;
const DATA_HEADER_SIZE: usize = 88;
const CODE_HEADER_SIZE: usize = 89;
const SHRED_VARIANT_OFFSET: usize = 64;
const SLOT_OFFSET: usize = 65;
const INDEX_OFFSET: usize = 73;
const VERSION_OFFSET: usize = 77;
const FEC_SET_INDEX_OFFSET: usize = 79;
const DATA_FLAGS_OFFSET: usize = 85;
const DATA_SIZE_OFFSET: usize = 86;
const CODE_NUM_DATA_OFFSET: usize = 83;
const CODE_NUM_CODING_OFFSET: usize = 85;
const CODE_POSITION_OFFSET: usize = 87;
const FLAG_DATA_COMPLETE: u8 = 0x40;
const FLAG_LAST_IN_SLOT: u8 = 0xc0;
const MAX_DATA_SHRED_SIZE: usize = 1203;
const DATA_SHRED_PAYLOAD_LEN: usize = 1203;
const CODE_SHRED_PAYLOAD_LEN: usize = 1228;
const SIZE_OF_SIGNATURE: usize = 64;
const SIZE_OF_MERKLE_ROOT: usize = 32;
const SIZE_OF_MERKLE_PROOF_ENTRY: usize = 20;
const INITIAL_DECODER_BUFFER_CAPACITY: usize = 64 * 1024;

#[inline]
fn read_u16(raw: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        raw.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

#[inline]
fn read_u32(raw: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        raw.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

#[inline]
fn read_u64(raw: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_le_bytes(
        raw.get(offset..offset + 8)?.try_into().ok()?,
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VariantKind {
    DataLegacy,
    CodeLegacy,
    DataMerkle {
        proof_size: u8,
        chained: bool,
        resigned: bool,
    },
    CodeMerkle {
        proof_size: u8,
        chained: bool,
        resigned: bool,
    },
}

impl VariantKind {
    fn is_data(self) -> bool {
        matches!(self, Self::DataLegacy | Self::DataMerkle { .. })
    }

    fn is_code(self) -> bool {
        matches!(self, Self::CodeLegacy | Self::CodeMerkle { .. })
    }

    fn merkle_suffix(self) -> usize {
        match self {
            Self::DataLegacy | Self::CodeLegacy => 0,
            Self::DataMerkle {
                proof_size,
                chained,
                resigned,
            }
            | Self::CodeMerkle {
                proof_size,
                chained,
                resigned,
            } => {
                let chained_root = if chained { SIZE_OF_MERKLE_ROOT } else { 0 };
                let proof = usize::from(proof_size) * SIZE_OF_MERKLE_PROOF_ENTRY;
                let resigned_sig = if resigned { SIZE_OF_SIGNATURE } else { 0 };
                chained_root + proof + resigned_sig
            }
        }
    }

    fn coding_shard_size(self) -> Option<usize> {
        match self {
            Self::DataMerkle {
                proof_size,
                chained,
                resigned,
            }
            | Self::CodeMerkle {
                proof_size,
                chained,
                resigned,
            } => CODE_SHRED_PAYLOAD_LEN.checked_sub(
                CODE_HEADER_SIZE
                    + if chained { SIZE_OF_MERKLE_ROOT } else { 0 }
                    + usize::from(proof_size) * SIZE_OF_MERKLE_PROOF_ENTRY
                    + if resigned { SIZE_OF_SIGNATURE } else { 0 },
            ),
            _ => None,
        }
    }

    fn as_data(self) -> Self {
        match self {
            Self::CodeMerkle {
                proof_size,
                chained,
                resigned,
            } => Self::DataMerkle {
                proof_size,
                chained,
                resigned,
            },
            other => other,
        }
    }
}

#[inline]
fn classify_variant(value: u8) -> Option<VariantKind> {
    match value {
        0x5a => Some(VariantKind::CodeLegacy),
        0xa5 => Some(VariantKind::DataLegacy),
        value => {
            let proof_size = value & 0x0f;
            match value & 0xf0 {
                0x40 => Some(VariantKind::CodeMerkle {
                    proof_size,
                    chained: false,
                    resigned: false,
                }),
                0x60 => Some(VariantKind::CodeMerkle {
                    proof_size,
                    chained: true,
                    resigned: false,
                }),
                0x70 => Some(VariantKind::CodeMerkle {
                    proof_size,
                    chained: true,
                    resigned: true,
                }),
                0x80 => Some(VariantKind::DataMerkle {
                    proof_size,
                    chained: false,
                    resigned: false,
                }),
                0x90 => Some(VariantKind::DataMerkle {
                    proof_size,
                    chained: true,
                    resigned: false,
                }),
                0xb0 => Some(VariantKind::DataMerkle {
                    proof_size,
                    chained: true,
                    resigned: true,
                }),
                _ => None,
            }
        }
    }
}

#[derive(Debug)]
struct WireDataShred<'a> {
    slot: u64,
    index: u32,
    fec_set_index: u32,
    raw: &'a [u8],
    payload_end: u16,
    batch_complete: bool,
    last_in_slot: bool,
}

#[derive(Debug)]
struct WireCodeShred<'a> {
    slot: u64,
    index: u32,
    fec_set_index: u32,
    num_data_shreds: u16,
    num_coding_shreds: u16,
    position: u16,
    coded: &'a [u8],
    variant: VariantKind,
}

#[derive(Debug)]
enum ParsedWireShred<'a> {
    Data(WireDataShred<'a>),
    Code(WireCodeShred<'a>),
}

impl ParsedWireShred<'_> {
    fn slot(&self) -> u64 {
        match self {
            Self::Data(shred) => shred.slot,
            Self::Code(shred) => shred.slot,
        }
    }

    fn index(&self) -> u32 {
        match self {
            Self::Data(shred) => shred.index,
            Self::Code(shred) => shred.index,
        }
    }

    fn type_name(&self) -> &'static str {
        match self {
            Self::Data(_) => "data",
            Self::Code(_) => "code",
        }
    }
}

fn parse_wire_shred(packet: &[u8], cfg_prefix_skip: usize) -> Option<ParsedWireShred<'_>> {
    if cfg_prefix_skip > 0 {
        return packet.get(cfg_prefix_skip..).and_then(parse_wire_candidate);
    }

    if let Some(shred) = parse_wire_candidate(packet) {
        return Some(shred);
    }

    if packet.len() >= SIZE_OF_SIGNATURE + DATA_HEADER_SIZE
        && packet[..SIZE_OF_SIGNATURE].iter().all(|&byte| byte == 0)
    {
        return parse_wire_candidate(&packet[SIZE_OF_SIGNATURE..]);
    }

    None
}

fn parse_wire_candidate(raw: &[u8]) -> Option<ParsedWireShred<'_>> {
    if raw.len() < COMMON_HEADER_SIZE {
        return None;
    }

    let variant = classify_variant(raw[SHRED_VARIANT_OFFSET])?;
    let slot = read_u64(raw, SLOT_OFFSET)?;
    let index = read_u32(raw, INDEX_OFFSET)?;
    let _version = read_u16(raw, VERSION_OFFSET)?;
    let fec_set_index = read_u32(raw, FEC_SET_INDEX_OFFSET)?;

    if variant.is_data() {
        if raw.len() < DATA_HEADER_SIZE {
            return None;
        }
        let flags = raw[DATA_FLAGS_OFFSET];
        let size = read_u16(raw, DATA_SIZE_OFFSET)? as usize;
        if !(DATA_HEADER_SIZE..=raw.len()).contains(&size) || size > MAX_DATA_SHRED_SIZE {
            return None;
        }
        let _payload = raw.get(DATA_HEADER_SIZE..size)?;
        return Some(ParsedWireShred::Data(WireDataShred {
            slot,
            index,
            fec_set_index,
            raw,
            payload_end: size as u16,
            batch_complete: flags & FLAG_DATA_COMPLETE != 0,
            last_in_slot: flags & FLAG_LAST_IN_SLOT == FLAG_LAST_IN_SLOT,
        }));
    }

    if variant.is_code() {
        if raw.len() < CODE_HEADER_SIZE {
            return None;
        }
        let num_data_shreds = read_u16(raw, CODE_NUM_DATA_OFFSET)?;
        let num_coding_shreds = read_u16(raw, CODE_NUM_CODING_OFFSET)?;
        let position = read_u16(raw, CODE_POSITION_OFFSET)?;
        if num_data_shreds == 0 || num_coding_shreds == 0 || position >= num_coding_shreds {
            return None;
        }
        let suffix = variant.merkle_suffix();
        if suffix > raw.len() {
            return None;
        }
        let coded_end = raw.len().saturating_sub(suffix);
        if coded_end < CODE_HEADER_SIZE {
            return None;
        }
        return Some(ParsedWireShred::Code(WireCodeShred {
            slot,
            index,
            fec_set_index,
            num_data_shreds,
            num_coding_shreds,
            position,
            coded: &raw[..coded_end],
            variant,
        }));
    }

    None
}

#[derive(Default, Clone, Copy)]
struct DecodedCounts {
    entries: usize,
    transactions: usize,
}

impl DecodedCounts {
    fn is_empty(self) -> bool {
        self.entries == 0 && self.transactions == 0
    }

    fn add(&mut self, other: Self) {
        self.entries = self.entries.saturating_add(other.entries);
        self.transactions = self.transactions.saturating_add(other.transactions);
    }
}

/// Keeps the existing decoder deduplication and optionally exports newly decoded signatures.
/// Clearing a batch's dedup set must not discard observations decoded earlier in the same packet.
struct SignatureTracker {
    seen: FxHashSet<[u8; 64]>,
    collected: Option<Vec<[u8; 64]>>,
}

impl SignatureTracker {
    fn new(collect: bool) -> Self {
        Self {
            seen: FxHashSet::default(),
            collected: collect.then(Vec::new),
        }
    }
    fn insert(&mut self, sig: [u8; 64]) -> bool {
        let fresh = self.seen.insert(sig);
        if fresh {
            if let Some(collected) = &mut self.collected {
                collected.push(sig);
            }
        }
        fresh
    }
    fn clear(&mut self) {
        self.seen.clear();
    }
    fn take(&mut self) -> Vec<[u8; 64]> {
        self.collected
            .as_mut()
            .map(std::mem::take)
            .unwrap_or_default()
    }
}

fn count_entry(entry: &WireEntry, yielded_sigs: &mut SignatureTracker) -> DecodedCounts {
    let mut counts = DecodedCounts {
        entries: 1,
        transactions: 0,
    };

    for tx in &entry.transactions {
        match tx.first_signature {
            Some(signature) => {
                if yielded_sigs.insert(signature) {
                    counts.transactions += 1;
                }
            }
            None => {
                counts.transactions += 1;
            }
        }
    }

    counts
}

struct StreamingEntryDecoder {
    buffer: Vec<u8>,
    expected_count: Option<u64>,
    entries_yielded: u64,
    marker_complete: bool,
    cursor: usize,
}

impl StreamingEntryDecoder {
    fn new() -> Self {
        Self {
            buffer: Vec::with_capacity(INITIAL_DECODER_BUFFER_CAPACITY),
            expected_count: None,
            entries_yielded: 0,
            marker_complete: false,
            cursor: 0,
        }
    }

    fn reset(&mut self) {
        self.buffer.clear();
        self.expected_count = None;
        self.entries_yielded = 0;
        self.marker_complete = false;
        self.cursor = 0;
    }

    fn is_complete(&self) -> bool {
        match self.expected_count {
            Some(0) => self.marker_complete,
            Some(expected) => self.entries_yielded >= expected,
            None => false,
        }
    }

    fn push(
        &mut self,
        payload: &[u8],
        yielded_sigs: &mut SignatureTracker,
    ) -> std::result::Result<DecodedCounts, WireError> {
        self.buffer.extend_from_slice(payload);
        self.try_deserialize(yielded_sigs)
    }

    fn try_deserialize(
        &mut self,
        yielded_sigs: &mut SignatureTracker,
    ) -> std::result::Result<DecodedCounts, WireError> {
        if self.expected_count.is_none() && self.buffer.len() >= self.cursor + 8 {
            // A zero count starts a block marker, which carries no transactions.
            let count = wire_entry::parse_entry_count(&self.buffer[self.cursor..])?;
            self.cursor += 8;
            self.expected_count = Some(count);
        }

        let Some(expected) = self.expected_count else {
            return Ok(DecodedCounts::default());
        };

        let mut counts = DecodedCounts::default();

        if expected == 0 && !self.marker_complete {
            match wire_entry::parse_block_marker_body(&self.buffer[self.cursor..]) {
                Ok((_, _, len)) => {
                    self.cursor += len;
                    self.marker_complete = true;
                }
                Err(WireError::Incomplete) => return Ok(counts),
                Err(error) => return Err(error),
            }
        }

        while self.entries_yielded < expected {
            let remaining = &self.buffer[self.cursor..];
            if remaining.is_empty() {
                break;
            }

            match wire_entry::parse_entry(remaining) {
                Ok((entry, len)) => {
                    self.cursor += len;
                    self.entries_yielded += 1;
                    counts.add(count_entry(&entry, yielded_sigs));
                }
                Err(WireError::Incomplete) => break,
                Err(error) => return Err(error),
            }
        }

        if self.cursor > 128 * 1024 {
            self.buffer.drain(..self.cursor);
            self.cursor = 0;
        }

        Ok(counts)
    }
}

/// Decodes one DATA_COMPLETE range: an entry batch or a block marker, followed by optional padding.
fn decode_batch_counts(bytes: &[u8], yielded_sigs: &mut SignatureTracker) -> Option<DecodedCounts> {
    let mut counts = DecodedCounts::default();
    if let BlockComponent::EntryBatch(entries) = wire_entry::parse_block_component(bytes).ok()?.0 {
        for entry in &entries {
            counts.add(count_entry(entry, yielded_sigs));
        }
    }
    Some(counts)
}

/// Salvage path: decodes back-to-back block components until the bytes run out or stop parsing.
fn decode_concatenated_counts(bytes: &[u8], yielded_sigs: &mut SignatureTracker) -> DecodedCounts {
    let mut counts = DecodedCounts::default();
    let mut offset = 0usize;
    while offset + 8 <= bytes.len() {
        let remaining = &bytes[offset..];
        if remaining.iter().all(|&b| b == 0) {
            break; // zero padding to the end of the FEC set
        }
        let Ok((component, consumed)) = wire_entry::parse_block_component(remaining) else {
            break;
        };
        if let BlockComponent::EntryBatch(entries) = component {
            for entry in &entries {
                counts.add(count_entry(entry, yielded_sigs));
            }
        }
        offset += consumed;
    }
    counts
}

fn data_shard_slice(raw: &[u8], variant: VariantKind) -> Option<&[u8]> {
    let len = variant.coding_shard_size()?;
    let end = SIZE_OF_SIGNATURE.checked_add(len)?;
    raw.get(SIZE_OF_SIGNATURE..end)
}

fn code_shard_slice(raw: &[u8], variant: VariantKind) -> Option<&[u8]> {
    let len = variant.coding_shard_size()?;
    let end = CODE_HEADER_SIZE.checked_add(len)?;
    raw.get(CODE_HEADER_SIZE..end)
}

struct ReedSolomonCache {
    entries: Vec<((u16, u16), ReedSolomon)>,
}

impl ReedSolomonCache {
    fn new() -> Self {
        Self {
            entries: Vec::with_capacity(32),
        }
    }

    fn get_or_build(
        &mut self,
        data: u16,
        coding: u16,
    ) -> std::result::Result<&ReedSolomon, reed_solomon_erasure::Error> {
        if let Some(pos) = self
            .entries
            .iter()
            .position(|(key, _)| *key == (data, coding))
        {
            let entry = self.entries.swap_remove(pos);
            self.entries.push(entry);
            return Ok(&self.entries.last().unwrap().1);
        }

        let rs = ReedSolomon::new(data as usize, coding as usize)?;
        if self.entries.len() >= 32 {
            self.entries.remove(0);
        }
        self.entries.push(((data, coding), rs));
        Ok(&self.entries.last().unwrap().1)
    }
}

struct FecSetBuffer {
    num_data: u16,
    num_coding: u16,
    variant: Option<VariantKind>,
    shard_size: usize,
    code: BTreeMap<u16, Vec<u8>>,
}

impl FecSetBuffer {
    fn new() -> Self {
        Self {
            num_data: 0,
            num_coding: 0,
            variant: None,
            shard_size: 0,
            code: BTreeMap::new(),
        }
    }

    fn record_code(
        &mut self,
        position: u16,
        num_data: u16,
        num_coding: u16,
        coded: &[u8],
        variant: VariantKind,
    ) {
        if self.num_data == 0 {
            self.num_data = num_data;
        }
        if self.num_coding == 0 {
            self.num_coding = num_coding;
        }
        if self.variant.is_none() {
            self.variant = Some(variant);
        }
        if self.shard_size == 0 {
            self.shard_size = variant.coding_shard_size().unwrap_or_default();
        }
        self.code.entry(position).or_insert_with(|| coded.to_vec());
    }

    fn can_recover_with(&self, data_received: usize) -> bool {
        self.num_data > 0
            && self.shard_size > 0
            && data_received + self.code.len() >= self.num_data as usize
    }

    fn try_reconstruct(
        &self,
        data_shards: &[(u16, &[u8])],
        cache: &mut ReedSolomonCache,
    ) -> std::result::Result<Vec<(u16, Vec<u8>)>, reed_solomon_erasure::Error> {
        let Some(variant) = self.variant else {
            return Ok(Vec::new());
        };
        if self.num_data == 0 || self.num_coding == 0 || self.shard_size == 0 {
            return Ok(Vec::new());
        }

        let nd = self.num_data as usize;
        let nc = self.num_coding as usize;
        let total = nd + nc;
        let mut shards: Vec<Option<Vec<u8>>> = vec![None; total];

        for (&position, raw) in &self.code {
            let idx = nd + position as usize;
            if idx >= total {
                continue;
            }
            let Some(shard) = code_shard_slice(raw, variant) else {
                continue;
            };
            if shard.len() == self.shard_size {
                shards[idx] = Some(shard.to_vec());
            }
        }

        for (position, raw) in data_shards {
            let idx = *position as usize;
            if idx >= nd || shards[idx].is_some() {
                continue;
            }
            let Some(shard) = data_shard_slice(raw, variant.as_data()) else {
                continue;
            };
            if shard.len() == self.shard_size {
                shards[idx] = Some(shard.to_vec());
            }
        }

        let have = shards.iter().flatten().count();
        if have < nd {
            return Ok(Vec::new());
        }

        let rs = cache.get_or_build(self.num_data, self.num_coding)?;
        rs.reconstruct_data(&mut shards)?;

        let present: std::collections::BTreeSet<u16> =
            data_shards.iter().map(|(position, _)| *position).collect();
        let mut out = Vec::new();
        for position in 0..self.num_data {
            if present.contains(&position) {
                continue;
            }
            let Some(shard) = shards[position as usize].take() else {
                continue;
            };
            if shard.len() != self.shard_size {
                continue;
            }
            let mut raw = vec![0u8; DATA_SHRED_PAYLOAD_LEN];
            raw[SIZE_OF_SIGNATURE..SIZE_OF_SIGNATURE + self.shard_size].copy_from_slice(&shard);
            out.push((position, raw));
        }

        Ok(out)
    }
}

#[derive(Clone, Copy)]
struct AccumulatorConfig {
    max_fec_sets_per_slot: usize,
    stuck_batch_timeout: Duration,
    collect_signatures: bool,
}

struct PendingDataShred {
    raw: Vec<u8>,
    payload_end: u16,
    batch_complete: bool,
    fec_set_index: u32,
}

impl PendingDataShred {
    fn payload(&self) -> Option<&[u8]> {
        let end = self.payload_end as usize;
        if end < DATA_HEADER_SIZE || end > self.raw.len() {
            return None;
        }
        self.raw.get(DATA_HEADER_SIZE..end)
    }
}

struct SlotAccumulator {
    cfg: AccumulatorConfig,
    pending: BTreeMap<u32, PendingDataShred>,
    fec_sets: BTreeMap<u32, FecSetBuffer>,
    batch_complete_indices: BTreeMap<u32, Instant>,
    streaming_cursor: u32,
    streaming_decoder: StreamingEntryDecoder,
    yielded_sigs: SignatureTracker,
    batch_start: u32,
    slot_complete: bool,
    decode_errors: u64,
    fec_recovered_data_shreds: u64,
    combined_scratch: Vec<u8>,
}

impl SlotAccumulator {
    fn new(cfg: AccumulatorConfig) -> Self {
        Self {
            cfg,
            pending: BTreeMap::new(),
            fec_sets: BTreeMap::new(),
            batch_complete_indices: BTreeMap::new(),
            streaming_cursor: 0,
            streaming_decoder: StreamingEntryDecoder::new(),
            yielded_sigs: SignatureTracker::new(cfg.collect_signatures),
            batch_start: 0,
            slot_complete: false,
            decode_errors: 0,
            fec_recovered_data_shreds: 0,
            combined_scratch: Vec::with_capacity(24 * 1024),
        }
    }

    fn push_data(
        &mut self,
        shred: &WireDataShred<'_>,
        enable_fec: bool,
        rs_cache: &mut ReedSolomonCache,
    ) -> DecodedCounts {
        if shred.index < self.batch_start {
            return DecodedCounts::default();
        }

        self.pending
            .entry(shred.index)
            .or_insert_with(|| PendingDataShred {
                raw: shred.raw.to_vec(),
                payload_end: shred.payload_end,
                batch_complete: shred.batch_complete,
                fec_set_index: shred.fec_set_index,
            });

        if shred.batch_complete {
            self.batch_complete_indices
                .entry(shred.index)
                .or_insert_with(Instant::now);
        }
        if shred.last_in_slot {
            self.slot_complete = true;
        }

        let mut counts = DecodedCounts::default();
        self.advance_streaming(&mut counts);

        // Parity can arrive before data. Advancing past this shred does not mean
        // that the rest of its FEC set (including a missing tail) is complete.
        if enable_fec && self.try_fec_fill(shred.fec_set_index, rs_cache) {
            self.advance_streaming(&mut counts);
        }

        self.try_flush_batches(enable_fec, rs_cache, &mut counts);
        counts
    }

    fn push_code(
        &mut self,
        shred: &WireCodeShred<'_>,
        rs_cache: &mut ReedSolomonCache,
    ) -> DecodedCounts {
        if self.fec_sets.len() >= self.cfg.max_fec_sets_per_slot
            && !self.fec_sets.contains_key(&shred.fec_set_index)
        {
            if let Some(oldest) = self.fec_sets.keys().copied().next() {
                if oldest < shred.fec_set_index {
                    self.fec_sets.remove(&oldest);
                }
            }
        }

        self.fec_sets
            .entry(shred.fec_set_index)
            .or_insert_with(FecSetBuffer::new)
            .record_code(
                shred.position,
                shred.num_data_shreds,
                shred.num_coding_shreds,
                shred.coded,
                shred.variant,
            );

        let mut counts = DecodedCounts::default();
        if self.try_fec_fill(shred.fec_set_index, rs_cache) {
            self.advance_streaming(&mut counts);
        }
        self.try_flush_batches(true, rs_cache, &mut counts);
        counts
    }

    fn advance_streaming(&mut self, counts: &mut DecodedCounts) {
        loop {
            let Some(pending) = self.pending.get(&self.streaming_cursor) else {
                break;
            };
            let batch_complete = pending.batch_complete;
            let payload = match pending.payload() {
                Some(payload) => payload,
                None => {
                    let dropped = self.streaming_cursor;
                    self.pending.remove(&dropped);
                    if batch_complete {
                        self.batch_complete_indices.remove(&dropped);
                    }
                    self.streaming_cursor = self.streaming_cursor.saturating_add(1);
                    self.decode_errors = self.decode_errors.saturating_add(1);
                    continue;
                }
            };

            let push_result = self.streaming_decoder.push(payload, &mut self.yielded_sigs);
            self.streaming_cursor = self.streaming_cursor.saturating_add(1);
            match push_result {
                Ok(decoded) => counts.add(decoded),
                Err(_) => {
                    self.decode_errors = self.decode_errors.saturating_add(1);
                    self.streaming_decoder.reset();
                    return;
                }
            }

            if batch_complete {
                return;
            }
        }
    }

    fn try_fec_fill(&mut self, hint: u32, rs_cache: &mut ReedSolomonCache) -> bool {
        let Some(set_idx) = self.select_recoverable_set(hint) else {
            return false;
        };

        let recovered = {
            let Some(fec) = self.fec_sets.get(&set_idx) else {
                return false;
            };
            let nd = fec.num_data as u32;
            if nd == 0 {
                return false;
            }
            let data_refs: Vec<(u16, &[u8])> = self
                .pending
                .range(set_idx..set_idx.saturating_add(nd))
                .filter(|(&idx, pending)| {
                    idx >= set_idx && (idx - set_idx) < nd && pending.fec_set_index == set_idx
                })
                .map(|(&idx, pending)| ((idx - set_idx) as u16, pending.raw.as_slice()))
                .collect();

            match fec.try_reconstruct(&data_refs, rs_cache) {
                Ok(recovered) => recovered,
                Err(_) => return false,
            }
        };

        if recovered.is_empty() {
            return false;
        }

        let mut inserted = 0u64;
        for (position_in_set, raw) in recovered {
            let global_index = set_idx + u32::from(position_in_set);
            if global_index < self.batch_start || self.pending.contains_key(&global_index) {
                continue;
            }

            let Some(ParsedWireShred::Data(recovered_shred)) = parse_wire_candidate(&raw) else {
                continue;
            };
            let payload_end = recovered_shred.payload_end;
            let batch_complete = recovered_shred.batch_complete;
            let last_in_slot = recovered_shred.last_in_slot;

            if batch_complete {
                self.batch_complete_indices
                    .entry(global_index)
                    .or_insert_with(Instant::now);
            }
            if last_in_slot {
                self.slot_complete = true;
            }

            self.pending.insert(
                global_index,
                PendingDataShred {
                    raw,
                    payload_end,
                    batch_complete,
                    fec_set_index: set_idx,
                },
            );
            inserted = inserted.saturating_add(1);
        }

        if inserted > 0 {
            self.fec_recovered_data_shreds =
                self.fec_recovered_data_shreds.saturating_add(inserted);
            self.fec_sets.remove(&set_idx);
            return true;
        }

        false
    }

    fn select_recoverable_set(&self, hint: u32) -> Option<u32> {
        let data_count_for = |set_idx: u32, fec: &FecSetBuffer| -> usize {
            let nd = fec.num_data as u32;
            if nd == 0 {
                return 0;
            }
            self.pending
                .range(set_idx..set_idx.saturating_add(nd))
                .filter(|(_, pending)| pending.fec_set_index == set_idx)
                .count()
        };

        if let Some(fec) = self.fec_sets.get(&hint) {
            if fec.can_recover_with(data_count_for(hint, fec)) {
                return Some(hint);
            }
        }

        for (&set_idx, fec) in &self.fec_sets {
            let nd = fec.num_data as u32;
            if nd == 0 {
                continue;
            }
            if set_idx <= self.streaming_cursor
                && self.streaming_cursor < set_idx.saturating_add(nd)
                && fec.can_recover_with(data_count_for(set_idx, fec))
            {
                return Some(set_idx);
            }
        }

        None
    }

    fn try_flush_batches(
        &mut self,
        enable_fec: bool,
        rs_cache: &mut ReedSolomonCache,
        counts: &mut DecodedCounts,
    ) {
        loop {
            let (batch_complete_idx, first_seen) =
                match self.batch_complete_indices.range(self.batch_start..).next() {
                    Some((&idx, &seen)) => (idx, seen),
                    None => return,
                };
            let start = self.batch_start;

            if self.streaming_cursor > batch_complete_idx && self.streaming_decoder.is_complete() {
                self.complete_batch(batch_complete_idx);
                continue;
            }

            let expected = (batch_complete_idx - start + 1) as usize;
            let mut present = self.pending.range(start..=batch_complete_idx).count();
            if enable_fec && present < expected {
                let hints: Vec<u32> = self
                    .fec_sets
                    .iter()
                    .filter(|(&fec_set_index, fec)| {
                        let nd = fec.num_data as u32;
                        nd != 0
                            && fec_set_index.saturating_add(nd) > start
                            && fec_set_index <= batch_complete_idx
                    })
                    .map(|(&fec_set_index, _)| fec_set_index)
                    .collect();
                for hint in hints {
                    self.try_fec_fill(hint, rs_cache);
                }
                present = self.pending.range(start..=batch_complete_idx).count();
            }

            if present < expected {
                if first_seen.elapsed() >= self.cfg.stuck_batch_timeout {
                    counts.add(self.salvage_contiguous_runs_in(start, batch_complete_idx));
                    self.complete_batch(batch_complete_idx);
                    continue;
                }
                return;
            }

            self.combined_scratch.clear();
            for index in start..=batch_complete_idx {
                if let Some(pending) = self.pending.get(&index) {
                    if let Some(payload) = pending.payload() {
                        self.combined_scratch.extend_from_slice(payload);
                    }
                }
            }

            match decode_batch_counts(&self.combined_scratch, &mut self.yielded_sigs) {
                Some(decoded) => counts.add(decoded),
                None => {
                    self.decode_errors = self.decode_errors.saturating_add(1);
                }
            }

            self.complete_batch(batch_complete_idx);
        }
    }

    fn complete_batch(&mut self, batch_complete_idx: u32) {
        let next_start = batch_complete_idx.saturating_add(1);
        self.pending.retain(|&idx, _| idx >= next_start);
        self.batch_complete_indices
            .retain(|&idx, _| idx >= next_start);
        self.fec_sets.retain(|&set_idx, fec| {
            let nd = fec.num_data as u32;
            !(nd > 0 && set_idx.saturating_add(nd) <= next_start)
        });
        self.batch_start = next_start;
        self.streaming_cursor = self.streaming_cursor.max(next_start);
        self.streaming_decoder.reset();
        self.yielded_sigs.clear();
    }

    fn try_harvest_tail(
        &mut self,
        enable_fec: bool,
        rs_cache: &mut ReedSolomonCache,
    ) -> DecodedCounts {
        let mut counts = DecodedCounts::default();

        if enable_fec {
            let hints: Vec<u32> = self.fec_sets.keys().copied().collect();
            for hint in hints {
                self.try_fec_fill(hint, rs_cache);
            }
        }

        let boundaries: Vec<u32> = self
            .pending
            .iter()
            .filter(|(_, pending)| pending.batch_complete)
            .map(|(&idx, _)| idx)
            .filter(|&idx| idx >= self.batch_start)
            .collect();

        for batch_complete_idx in boundaries {
            let start = self.batch_start;
            if batch_complete_idx < start {
                continue;
            }
            if (start..=batch_complete_idx).all(|idx| self.pending.contains_key(&idx)) {
                self.combined_scratch.clear();
                for idx in start..=batch_complete_idx {
                    if let Some(pending) = self.pending.get(&idx) {
                        if let Some(payload) = pending.payload() {
                            self.combined_scratch.extend_from_slice(payload);
                        }
                    }
                }
                if let Some(decoded) =
                    decode_batch_counts(&self.combined_scratch, &mut self.yielded_sigs)
                {
                    counts.add(decoded);
                } else {
                    self.decode_errors = self.decode_errors.saturating_add(1);
                }
            } else {
                counts.add(self.salvage_contiguous_runs_in(start, batch_complete_idx));
            }
            self.complete_batch(batch_complete_idx);
        }

        if !self.pending.is_empty() {
            let indices: Vec<u32> = self.pending.keys().copied().collect();
            let mut i = 0usize;
            while i < indices.len() {
                self.combined_scratch.clear();
                let mut expected = indices[i];
                while i < indices.len() && indices[i] == expected {
                    if let Some(pending) = self.pending.get(&indices[i]) {
                        if let Some(payload) = pending.payload() {
                            self.combined_scratch.extend_from_slice(payload);
                        }
                    }
                    expected = indices[i].saturating_add(1);
                    i += 1;
                }
                if !self.combined_scratch.is_empty() {
                    counts.add(decode_concatenated_counts(
                        &self.combined_scratch,
                        &mut self.yielded_sigs,
                    ));
                }
            }
        }

        counts
    }

    fn salvage_contiguous_runs_in(&mut self, start: u32, end: u32) -> DecodedCounts {
        let mut counts = DecodedCounts::default();
        let mut cursor = start;

        while cursor <= end {
            while cursor <= end && !self.pending.contains_key(&cursor) {
                cursor = cursor.saturating_add(1);
            }
            if cursor > end {
                break;
            }

            self.combined_scratch.clear();
            while cursor <= end && self.pending.contains_key(&cursor) {
                if let Some(pending) = self.pending.get(&cursor) {
                    if let Some(payload) = pending.payload() {
                        self.combined_scratch.extend_from_slice(payload);
                    }
                }
                cursor = cursor.saturating_add(1);
            }

            if !self.combined_scratch.is_empty() {
                counts.add(decode_concatenated_counts(
                    &self.combined_scratch,
                    &mut self.yielded_sigs,
                ));
            }
        }

        counts
    }
}

struct SlotState {
    accumulator: SlotAccumulator,
    last_activity: Instant,
}

pub struct DataShredReassembler {
    max_tracked_slots: usize,
    gap_timeout: Duration,
    forward_slot_watermark: bool,
    udp_payload_prefix_skip: usize,
    enable_fec: bool,
    accumulator_cfg: AccumulatorConfig,
    forward_watermark: Option<u64>,
    slots: FxHashMap<u64, SlotState>,
    rs_cache: ReedSolomonCache,
}

impl DataShredReassembler {
    pub fn new(config: &RawShredConfig) -> Self {
        Self {
            max_tracked_slots: config.max_tracked_slots.max(1),
            gap_timeout: Duration::from_millis(config.reassembly_gap_timeout_ms.max(1)),
            forward_slot_watermark: config.forward_slot_watermark,
            udp_payload_prefix_skip: config.udp_payload_prefix_skip,
            enable_fec: config.enable_fec,
            accumulator_cfg: AccumulatorConfig {
                max_fec_sets_per_slot: config.max_fec_sets_per_slot.max(1),
                stuck_batch_timeout: Duration::from_millis(config.stuck_batch_timeout_ms.max(1)),
                collect_signatures: config.collect_signatures,
            },
            forward_watermark: None,
            slots: FxHashMap::default(),
            rs_cache: ReedSolomonCache::new(),
        }
    }

    fn push_packet(&mut self, packet: &[u8], now: Instant) -> ShredPacketOutcome {
        let Some(shred) = parse_wire_shred(packet, self.udp_payload_prefix_skip) else {
            return ShredPacketOutcome::default();
        };

        if self.forward_slot_watermark {
            if let Some(watermark) = self.forward_watermark {
                if shred.slot() < watermark {
                    return ShredPacketOutcome {
                        slot: Some(shred.slot()),
                        shred_type: Some(shred.type_name()),
                        index: Some(shred.index()),
                        ..Default::default()
                    };
                }
            }
        }

        self.evict_excess_slots(shred.slot());

        let mut decoded_counts = DecodedCounts::default();
        let mut decode_errors = 0u64;
        let mut recovered = 0u64;
        let mut slot_complete = false;

        match &shred {
            ParsedWireShred::Data(data) => {
                let state = self.slots.entry(data.slot).or_insert_with(|| SlotState {
                    accumulator: SlotAccumulator::new(self.accumulator_cfg),
                    last_activity: now,
                });
                state.last_activity = now;
                let errors_before = state.accumulator.decode_errors;
                let recovered_before = state.accumulator.fec_recovered_data_shreds;
                decoded_counts.add(state.accumulator.push_data(
                    data,
                    self.enable_fec,
                    &mut self.rs_cache,
                ));
                slot_complete = state.accumulator.slot_complete;
                decode_errors = state
                    .accumulator
                    .decode_errors
                    .saturating_sub(errors_before);
                recovered = state
                    .accumulator
                    .fec_recovered_data_shreds
                    .saturating_sub(recovered_before);
            }
            ParsedWireShred::Code(code) => {
                if self.enable_fec {
                    let state = self.slots.entry(code.slot).or_insert_with(|| SlotState {
                        accumulator: SlotAccumulator::new(self.accumulator_cfg),
                        last_activity: now,
                    });
                    state.last_activity = now;
                    let errors_before = state.accumulator.decode_errors;
                    let recovered_before = state.accumulator.fec_recovered_data_shreds;
                    decoded_counts.add(state.accumulator.push_code(code, &mut self.rs_cache));
                    slot_complete = state.accumulator.slot_complete;
                    decode_errors = state
                        .accumulator
                        .decode_errors
                        .saturating_sub(errors_before);
                    recovered = state
                        .accumulator
                        .fec_recovered_data_shreds
                        .saturating_sub(recovered_before);
                }
            }
        }

        let mut signatures = Vec::new();
        if slot_complete {
            if let Some(mut state) = self.slots.remove(&shred.slot()) {
                let errors_before = state.accumulator.decode_errors;
                let recovered_before = state.accumulator.fec_recovered_data_shreds;
                decoded_counts.add(
                    state
                        .accumulator
                        .try_harvest_tail(self.enable_fec, &mut self.rs_cache),
                );
                decode_errors = decode_errors.saturating_add(
                    state
                        .accumulator
                        .decode_errors
                        .saturating_sub(errors_before),
                );
                recovered = recovered.saturating_add(
                    state
                        .accumulator
                        .fec_recovered_data_shreds
                        .saturating_sub(recovered_before),
                );
                signatures = state.accumulator.yielded_sigs.take();
            }
        } else if let Some(state) = self.slots.get_mut(&shred.slot()) {
            signatures = state.accumulator.yielded_sigs.take();
        }

        let mut decoded_segments = Vec::new();
        if !decoded_counts.is_empty() {
            decoded_segments.push(DecodedShredSegment {
                slot: shred.slot(),
                entry_count: decoded_counts.entries,
                transaction_count: decoded_counts.transactions,
            });
        }
        self.note_emitted(&decoded_segments);

        ShredPacketOutcome {
            slot: Some(shred.slot()),
            shred_type: Some(shred.type_name()),
            index: Some(shred.index()),
            decoded_segments,
            decode_errors,
            fec_recovered_data_shreds: recovered,
            signatures,
        }
    }

    pub fn evict_stale_slots(&mut self, now: Instant) -> usize {
        let timeout = self.gap_timeout;
        let before = self.slots.len();
        self.slots
            .retain(|_, state| now.duration_since(state.last_activity) <= timeout);
        before.saturating_sub(self.slots.len())
    }

    fn note_emitted(&mut self, segments: &[DecodedShredSegment]) {
        if !self.forward_slot_watermark || segments.is_empty() {
            return;
        }
        let max_slot = segments.iter().map(|segment| segment.slot).max().unwrap();
        self.forward_watermark = Some(self.forward_watermark.map_or(max_slot, |w| w.max(max_slot)));
    }

    fn evict_excess_slots(&mut self, touch_slot: u64) {
        while self.slots.len() >= self.max_tracked_slots {
            let Some(oldest) = self.slots.keys().copied().min() else {
                break;
            };
            if oldest == touch_slot && self.slots.len() == 1 {
                break;
            }
            self.slots.remove(&oldest);
        }
    }
}

struct ParsedCompatShred<'a> {
    slot: u64,
    index: u32,
    payload: &'a [u8],
    batch_complete: bool,
    last_in_slot: bool,
}

fn parse_compat_shred(raw: &[u8]) -> Option<ParsedCompatShred<'_>> {
    if raw.len() < DATA_HEADER_SIZE {
        return None;
    }
    let slot = read_u64(raw, SLOT_OFFSET)?;
    let index = read_u32(raw, INDEX_OFFSET)?;
    let flags = raw[DATA_FLAGS_OFFSET];
    let size = read_u16(raw, DATA_SIZE_OFFSET)? as usize;
    if size > raw.len() {
        return None;
    }
    let payload = if size > DATA_HEADER_SIZE {
        raw.get(DATA_HEADER_SIZE..size)?
    } else {
        &[]
    };
    Some(ParsedCompatShred {
        slot,
        index,
        payload,
        batch_complete: flags & FLAG_DATA_COMPLETE != 0,
        last_in_slot: flags & FLAG_LAST_IN_SLOT == FLAG_LAST_IN_SLOT,
    })
}

fn parse_compat_packet(packet: &[u8], cfg_prefix_skip: usize) -> Option<ParsedCompatShred<'_>> {
    if cfg_prefix_skip > 0 {
        return packet.get(cfg_prefix_skip..).and_then(parse_compat_shred);
    }

    parse_compat_shred(packet)
}

struct CompatBatchDecoder {
    decoder: StreamingEntryDecoder,
    yielded_sigs: SignatureTracker,
}

impl CompatBatchDecoder {
    fn new(collect_signatures: bool) -> Self {
        Self {
            decoder: StreamingEntryDecoder::new(),
            yielded_sigs: SignatureTracker::new(collect_signatures),
        }
    }

    fn reset(&mut self) {
        self.decoder.reset();
        self.yielded_sigs.clear();
    }

    fn push(&mut self, payload: &[u8]) -> std::result::Result<DecodedCounts, WireError> {
        self.decoder.push(payload, &mut self.yielded_sigs)
    }

    fn is_complete(&self) -> bool {
        self.decoder.is_complete()
    }
}

struct CompatSlotAccumulator {
    pending: BTreeMap<u32, (Vec<u8>, bool, bool)>,
    decoder: CompatBatchDecoder,
    next_drain: u32,
    slot_complete: bool,
    stall_count: u32,
    decode_errors: u32,
}

impl CompatSlotAccumulator {
    fn new(collect_signatures: bool) -> Self {
        Self {
            pending: BTreeMap::new(),
            decoder: CompatBatchDecoder::new(collect_signatures),
            next_drain: 0,
            slot_complete: false,
            stall_count: 0,
            decode_errors: 0,
        }
    }

    fn push(
        &mut self,
        index: u32,
        payload: Vec<u8>,
        batch_complete: bool,
        last_in_slot: bool,
    ) -> DecodedCounts {
        if index < self.next_drain || self.pending.contains_key(&index) {
            return DecodedCounts::default();
        }
        self.pending
            .insert(index, (payload, batch_complete, last_in_slot));

        let mut counts = DecodedCounts::default();
        let drain_start = self.next_drain;
        let errors_before = self.decode_errors;
        self.drain_contiguous(&mut counts);
        if self.decode_errors > errors_before {
            return counts;
        }
        if self.next_drain == drain_start {
            self.stall_count += 1;
        } else {
            self.stall_count = 0;
        }
        if self.stall_count >= 5 && !self.pending.is_empty() {
            if let Some(&lowest) = self.pending.keys().next() {
                self.next_drain = lowest;
                self.stall_count = 0;
                self.decoder.reset();
                self.drain_contiguous(&mut counts);
            }
        }
        counts
    }

    fn drain_contiguous(&mut self, counts: &mut DecodedCounts) {
        while let Some((payload, batch_complete, last_in_slot)) =
            self.pending.remove(&self.next_drain)
        {
            self.next_drain += 1;
            match self.decoder.push(&payload) {
                Ok(decoded) => counts.add(decoded),
                Err(_) => {
                    self.decode_errors += 1;
                    self.decoder.reset();
                    return;
                }
            }
            if last_in_slot {
                self.slot_complete = true;
            }
            if batch_complete {
                if !self.decoder.is_complete() {
                    self.decode_errors += 1;
                }
                self.decoder.reset();
            }
        }
    }
}

pub struct ShredstreamCompatReassembler {
    slots: HashMap<u64, CompatSlotAccumulator>,
    max_age: u64,
    last_slot: u64,
    forward_slot_watermark: bool,
    forward_watermark: Option<u64>,
    udp_payload_prefix_skip: usize,
    collect_signatures: bool,
}

impl ShredstreamCompatReassembler {
    pub fn new(
        forward_slot_watermark: bool,
        max_slot_age: u64,
        udp_payload_prefix_skip: usize,
    ) -> Self {
        Self::with_signatures(
            forward_slot_watermark,
            max_slot_age,
            udp_payload_prefix_skip,
            false,
        )
    }

    fn with_signatures(
        forward_slot_watermark: bool,
        max_slot_age: u64,
        udp_payload_prefix_skip: usize,
        collect_signatures: bool,
    ) -> Self {
        Self {
            collect_signatures,
            slots: HashMap::new(),
            max_age: max_slot_age.max(1),
            last_slot: 0,
            forward_slot_watermark,
            forward_watermark: None,
            udp_payload_prefix_skip,
        }
    }

    fn push_packet(&mut self, packet: &[u8]) -> ShredPacketOutcome {
        let Some(shred) = parse_compat_packet(packet, self.udp_payload_prefix_skip) else {
            return ShredPacketOutcome::default();
        };

        if self.forward_slot_watermark {
            if let Some(watermark) = self.forward_watermark {
                if shred.slot < watermark {
                    return ShredPacketOutcome {
                        slot: Some(shred.slot),
                        shred_type: Some("data"),
                        index: Some(shred.index),
                        ..Default::default()
                    };
                }
            }
        }

        if shred.slot > self.last_slot {
            self.last_slot = shred.slot;
            self.evict_old_slots();
        }

        let acc = self
            .slots
            .entry(shred.slot)
            .or_insert_with(|| CompatSlotAccumulator::new(self.collect_signatures));
        let errors_before = acc.decode_errors;
        let counts = acc.push(
            shred.index,
            shred.payload.to_vec(),
            shred.batch_complete,
            shred.last_in_slot,
        );
        let decode_errors = u64::from(acc.decode_errors.saturating_sub(errors_before));
        let slot_complete = acc.slot_complete;
        let signatures = acc.decoder.yielded_sigs.take();
        if decode_errors > 0 {
            self.slots.remove(&shred.slot);
        }

        let mut decoded_segments = Vec::new();
        if !counts.is_empty() {
            decoded_segments.push(DecodedShredSegment {
                slot: shred.slot,
                entry_count: counts.entries,
                transaction_count: counts.transactions,
            });
            self.note_emitted(shred.slot);
        }
        if slot_complete {
            self.slots.remove(&shred.slot);
        }

        ShredPacketOutcome {
            slot: Some(shred.slot),
            shred_type: Some("data"),
            index: Some(shred.index),
            decoded_segments,
            decode_errors,
            fec_recovered_data_shreds: 0,
            signatures,
        }
    }

    fn note_emitted(&mut self, slot: u64) {
        if self.forward_slot_watermark {
            self.forward_watermark = Some(self.forward_watermark.map_or(slot, |w| w.max(slot)));
        }
    }

    fn evict_old_slots(&mut self) {
        if self.last_slot <= self.max_age {
            return;
        }
        let min = self.last_slot - self.max_age;
        self.slots.retain(|slot, _| *slot >= min);
    }

    pub fn evict_stale_slots(&mut self, _now: Instant) -> usize {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block_marker() -> Vec<u8> {
        let mut bytes = 0u64.to_le_bytes().to_vec();
        bytes.extend(1u16.to_le_bytes());
        bytes.push(1);
        bytes.extend(41u16.to_le_bytes());
        bytes.extend([1u8; 41]);
        bytes
    }

    #[test]
    fn streaming_marker_requires_the_entire_body() {
        let marker = block_marker();
        for cut in 0..marker.len() {
            let mut decoder = StreamingEntryDecoder::new();
            let mut signatures = SignatureTracker::new(true);
            decoder.push(&marker[..cut], &mut signatures).unwrap();
            assert!(!decoder.is_complete(), "marker cut {cut}");
            decoder.push(&marker[cut..], &mut signatures).unwrap();
            assert!(decoder.is_complete());
            assert!(signatures.take().is_empty());
        }
    }

    #[test]
    fn compat_rejects_truncated_and_unknown_markers() {
        let marker = block_marker();
        for cut in 8..marker.len() {
            let mut slot = CompatSlotAccumulator::new(true);
            slot.push(0, marker[..cut].to_vec(), true, false);
            assert!(slot.decode_errors > 0, "marker cut {cut}");
        }
        let mut invalid = marker;
        invalid[8..10].copy_from_slice(&2u16.to_le_bytes());
        let mut slot = CompatSlotAccumulator::new(true);
        slot.push(0, invalid, true, false);
        assert!(slot.decode_errors > 0);
    }

    fn write_u16(raw: &mut [u8], offset: usize, value: u16) {
        raw[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }

    fn write_u32(raw: &mut [u8], offset: usize, value: u32) {
        raw[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn write_u64(raw: &mut [u8], offset: usize, value: u64) {
        raw[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    }

    #[test]
    fn parses_wire_shred_after_udp_prefix_skip() {
        let mut raw = vec![0u8; DATA_HEADER_SIZE];
        raw[SHRED_VARIANT_OFFSET] = 0xa5;
        write_u64(&mut raw, SLOT_OFFSET, 123);
        write_u32(&mut raw, INDEX_OFFSET, 7);
        write_u16(&mut raw, VERSION_OFFSET, 1);
        write_u32(&mut raw, FEC_SET_INDEX_OFFSET, 7);
        raw[DATA_FLAGS_OFFSET] = FLAG_DATA_COMPLETE;
        write_u16(&mut raw, DATA_SIZE_OFFSET, DATA_HEADER_SIZE as u16);

        let mut packet = vec![0xaa, 0xbb, 0xcc];
        packet.extend_from_slice(&raw);

        let parsed = parse_wire_shred(&packet, 3).expect("prefixed wire shred should parse");
        assert_eq!(parsed.slot(), 123);
        assert_eq!(parsed.index(), 7);
        assert_eq!(parsed.type_name(), "data");
    }

    #[test]
    fn parses_compat_shred_after_udp_prefix_skip() {
        let mut raw = vec![0u8; DATA_HEADER_SIZE + 8];
        write_u64(&mut raw, SLOT_OFFSET, 456);
        write_u32(&mut raw, INDEX_OFFSET, 9);
        raw[DATA_FLAGS_OFFSET] = FLAG_DATA_COMPLETE;
        write_u16(&mut raw, DATA_SIZE_OFFSET, (DATA_HEADER_SIZE + 8) as u16);

        let mut packet = vec![0x11, 0x22];
        packet.extend_from_slice(&raw);

        let parsed = parse_compat_packet(&packet, 2).expect("prefixed compat shred should parse");
        assert_eq!(parsed.slot, 456);
        assert_eq!(parsed.index, 9);
        assert_eq!(parsed.payload.len(), 8);
        assert!(parsed.batch_complete);
    }
}
