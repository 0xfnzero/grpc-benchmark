use clap::{parser::ValueSource, CommandFactory, FromArgMatches, Parser};
use grpc_benchmark::{
    comparison::{
        entries::DEFAULT_ENTRIES_METHOD,
        sources::{SourceConfig, SourceRunStats, Transport},
        stats::{Comparator, ComparisonConfig, ComparisonSummary},
        Mode,
    },
    shred_client::{RawShredConfig, ShredDecodeMode},
    BenchmarkError, Result,
};
use serde::Serialize;
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
use tokio::{sync::mpsc, task::JoinSet};

#[derive(Parser)]
#[command(
    about = "Compare two shred sources: slot visibility, identical UDP shreds, or decoded transactions"
)]
struct Args {
    /// Read SHRED_* settings from this file (default: ./shred-comparison.env if present).
    #[arg(long, env = "SHRED_COMPARISON_CONFIG", hide_env_values = true)]
    config: Option<PathBuf>,
    #[arg(
        long,
        env = "SHRED_COMPARISON_MODE",
        value_enum,
        default_value = "slot"
    )]
    mode: Mode,
    #[arg(long, env = "SHRED_SOURCE_1", value_enum, default_value = "udp")]
    source_1: Transport,
    #[arg(long, env = "SHRED_SOURCE_2", value_enum, default_value = "udp")]
    source_2: Transport,
    #[arg(long, env = "SHRED_BIND_1", default_value = "0.0.0.0:20000")]
    shred_bind_1: String,
    #[arg(long, env = "SHRED_BIND_2", default_value = "0.0.0.0:20001")]
    shred_bind_2: String,
    #[arg(long, env = "SHRED_NAME_1", default_value = "Shred_1")]
    shred_name_1: String,
    #[arg(long, env = "SHRED_NAME_2", default_value = "Shred_2")]
    shred_name_2: String,
    #[arg(long, env = "SHRED_URL_1", hide_env_values = true)]
    shred_url_1: Option<String>,
    #[arg(long, env = "SHRED_URL_2", hide_env_values = true)]
    shred_url_2: Option<String>,
    #[arg(long, env = "SHRED_TOKEN_1", hide_env_values = true)]
    shred_token_1: Option<String>,
    #[arg(long, env = "SHRED_TOKEN_2", hide_env_values = true)]
    shred_token_2: Option<String>,
    #[arg(long, env = "SHRED_TOKEN_HEADER_1", default_value = "x-token")]
    shred_token_header_1: String,
    #[arg(long, env = "SHRED_TOKEN_HEADER_2", default_value = "x-token")]
    shred_token_header_2: String,
    #[arg(long, env = "SHRED_ENTRIES_METHOD_1", default_value = DEFAULT_ENTRIES_METHOD)]
    entries_method_1: String,
    #[arg(long, env = "SHRED_ENTRIES_METHOD_2", default_value = DEFAULT_ENTRIES_METHOD)]
    entries_method_2: String,
    #[arg(long, env = "SHRED_DECODE_MODE_1", default_value = "ledger")]
    shred_decode_mode_1: ShredDecodeMode,
    #[arg(long, env = "SHRED_DECODE_MODE_2", default_value = "ledger")]
    shred_decode_mode_2: ShredDecodeMode,
    #[arg(long, env = "SHRED_UDP_PAYLOAD_PREFIX_SKIP_1", default_value_t = 0)]
    shred_udp_payload_prefix_skip_1: usize,
    #[arg(long, env = "SHRED_UDP_PAYLOAD_PREFIX_SKIP_2", default_value_t = 0)]
    shred_udp_payload_prefix_skip_2: usize,
    #[arg(long, env = "SHRED_COMPARISON_DURATION_SEC", default_value_t = 30, value_parser = clap::value_parser!(u64).range(1..=86400))]
    duration_sec: u64,
    #[arg(long, env = "SHRED_WARMUP_MS", default_value_t = 1000, value_parser = clap::value_parser!(u64).range(0..=60000))]
    warmup_ms: u64,
    #[arg(long, env = "SHRED_SETTLE_WINDOW_MS", default_value_t = 1000, value_parser = clap::value_parser!(u64).range(1..=60000))]
    settle_window_ms: u64,
    #[arg(long, env = "SHRED_RECV_BUFFER_BYTES", default_value_t = 128 * 1024 * 1024)]
    shred_recv_buffer_bytes: usize,
    #[arg(long, env = "SHRED_MAX_TRACKED_SLOTS", default_value_t = 128)]
    shred_max_tracked_slots: usize,
    #[arg(long, env = "SHRED_REASSEMBLY_GAP_TIMEOUT_MS", default_value_t = 500)]
    shred_reassembly_gap_timeout_ms: u64,
    /// Retain settled IDs for this many slots to deduplicate retransmits.
    #[arg(long, env = "SHRED_RETENTION_SLOTS", default_value_t = 128)]
    retention_slots: u64,
    #[arg(long, env = "SHRED_MAX_KEYS", default_value_t = 1_000_000)]
    max_keys: usize,
    #[arg(long, env = "SHRED_MAX_SAMPLES", default_value_t = 5_000_000)]
    max_samples: usize,
    /// Write a machine-readable report; endpoints and authentication values are omitted.
    #[arg(long, env = "SHRED_JSON_OUTPUT")]
    json_output: Option<PathBuf>,
}

impl Args {
    fn load() -> Result<Self> {
        let argv: Vec<_> = std::env::args_os().collect();
        let command = Self::command();
        // Parse first so --help/--version and CLI errors never require a config file.
        let matches = command.clone().get_matches_from(&argv);
        let explicit = matches.get_one::<PathBuf>("config");
        let default = PathBuf::from("shred-comparison.env");
        let path = match explicit {
            Some(path) => path,
            None if default.exists() => &default,
            None => return Ok(Self::from_arg_matches(&matches).expect("validated CLI")),
        };
        let settings = dotenvy::from_path_iter(path).map_err(|_| {
            BenchmarkError::ConfigError("cannot read shred comparison config file".into())
        })?;
        let mut merged = argv.clone();
        let mut seen = std::collections::HashSet::new();
        for setting in settings {
            // Never include parser errors: they can contain authentication values.
            let (key, value) = setting.map_err(|_| {
                BenchmarkError::ConfigError(
                    "invalid dotenv syntax in shred comparison config".into(),
                )
            })?;
            let arg = command
                .get_arguments()
                .find(|arg| {
                    arg.get_id() != "config" && arg.get_env() == Some(std::ffi::OsStr::new(&key))
                })
                .ok_or_else(|| {
                    BenchmarkError::ConfigError(
                        "unknown setting in shred comparison config; use the supplied templates"
                            .into(),
                    )
                })?;
            if !seen.insert(key) {
                return Err(BenchmarkError::ConfigError(
                    "duplicate setting in shred comparison config".into(),
                ));
            }
            if matches.value_source(arg.get_id().as_str()).is_none()
                || matches.value_source(arg.get_id().as_str()) == Some(ValueSource::DefaultValue)
            {
                merged.push(
                    format!("--{}={value}", arg.get_long().expect("long config option")).into(),
                );
            }
        }
        let merged = command.try_get_matches_from(merged).map_err(|_| {
            BenchmarkError::ConfigError(
                "invalid setting value in shred comparison config; check types and ranges".into(),
            )
        })?;
        Ok(Self::from_arg_matches(&merged).expect("validated config"))
    }

    fn validate(&self) -> Result<()> {
        if self.shred_name_1.trim().is_empty()
            || self.shred_name_2.trim().is_empty()
            || self.shred_name_1.trim() == self.shred_name_2.trim()
        {
            return Err(BenchmarkError::ConfigError(
                "source names must be nonempty and distinct".into(),
            ));
        }
        if self.max_keys == 0
            || self.max_samples == 0
            || self.retention_slots == 0
            || self.shred_max_tracked_slots == 0
        {
            return Err(BenchmarkError::ConfigError(
                "capacity and retention settings must be positive".into(),
            ));
        }
        if self.mode == Mode::Shred
            && (self.source_1 != Transport::Udp || self.source_2 != Transport::Udp)
        {
            return Err(BenchmarkError::ConfigError("--mode shred requires two UDP sources; use --mode transaction for Entry or mixed feeds".into()));
        }
        if self.source_1 == Transport::Udp && self.source_2 == Transport::Udp {
            let a: std::net::SocketAddr = self
                .shred_bind_1
                .parse()
                .map_err(|_| BenchmarkError::ConfigError("invalid --shred-bind-1".into()))?;
            let b: std::net::SocketAddr = self
                .shred_bind_2
                .parse()
                .map_err(|_| BenchmarkError::ConfigError("invalid --shred-bind-2".into()))?;
            if a.port() == b.port()
                && (a.ip() == b.ip() || a.ip().is_unspecified() || b.ip().is_unspecified())
            {
                return Err(BenchmarkError::ConfigError(
                    "two UDP sources must bind distinct addresses/ports".into(),
                ));
            }
        }
        Ok(())
    }

    fn sources(&self) -> [SourceConfig; 2] {
        let udp = |bind_addr, decode_mode, udp_payload_prefix_skip| RawShredConfig {
            bind_addr,
            decode_mode,
            udp_payload_prefix_skip,
            recv_buffer_bytes: self.shred_recv_buffer_bytes,
            max_tracked_slots: self.shred_max_tracked_slots,
            reassembly_gap_timeout_ms: self.shred_reassembly_gap_timeout_ms,
            ..Default::default()
        };
        [
            SourceConfig {
                transport: self.source_1,
                udp: udp(
                    self.shred_bind_1.clone(),
                    self.shred_decode_mode_1,
                    self.shred_udp_payload_prefix_skip_1,
                ),
                url: self.shred_url_1.clone(),
                method: self.entries_method_1.clone(),
                token: self.shred_token_1.clone(),
                token_header: self.shred_token_header_1.clone(),
            },
            SourceConfig {
                transport: self.source_2,
                udp: udp(
                    self.shred_bind_2.clone(),
                    self.shred_decode_mode_2,
                    self.shred_udp_payload_prefix_skip_2,
                ),
                url: self.shred_url_2.clone(),
                method: self.entries_method_2.clone(),
                token: self.shred_token_2.clone(),
                token_header: self.shred_token_header_2.clone(),
            },
        ]
    }
}

#[derive(Serialize)]
struct Report {
    schema_version: u32,
    valid: bool,
    invalid_reasons: Vec<String>,
    duration_sec: u64,
    warmup_ms: u64,
    transports: [Transport; 2],
    comparison: ComparisonSummary,
    receive: [SourceRunStats; 2],
    settings: serde_json::Value,
}

async fn run(args: Args) -> Result<()> {
    args.validate()?;
    let [config_a, config_b] = args.sources();
    println!(
        "Preparing sources ({:?}, {:?}); mode={:?}",
        args.source_1, args.source_2, args.mode
    );
    let (a, b) = tokio::time::timeout(Duration::from_secs(15), async {
        tokio::try_join!(config_a.prepare(args.mode), config_b.prepare(args.mode))
    })
    .await
    .map_err(|_| BenchmarkError::Timeout)??;
    let start = Instant::now() + Duration::from_millis(args.warmup_ms);
    let duration = Duration::from_secs(args.duration_sec);
    let settle = Duration::from_millis(args.settle_window_ms);
    let stop_at = start + duration + settle;
    let names = [args.shred_name_1.clone(), args.shred_name_2.clone()];
    let mut comparator = Comparator::new(
        ComparisonConfig {
            mode: args.mode,
            duration,
            settle,
            max_keys: args.max_keys,
            max_samples: args.max_samples,
            retention_slots: args.retention_slots,
        },
        names.clone(),
        start,
    );
    let (tx, mut rx) = mpsc::channel(65536);
    let mut tasks = JoinSet::new();
    for (i, source) in [a, b].into_iter().enumerate() {
        let tx = tx.clone();
        let mode = args.mode;
        tasks.spawn(async move {
            source
                .run(i, mode, stop_at, tx)
                .await
                .map(|stats| (i, stats))
        });
    }
    drop(tx);
    let mut stats = [SourceRunStats::default(), SourceRunStats::default()];
    let mut closed = false;
    let mut tick = tokio::time::interval(Duration::from_millis(100));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    println!(
        "Both sources ready. Warmup={}ms, measurement={}s, counterpart wait={}ms",
        args.warmup_ms, args.duration_sec, args.settle_window_ms
    );
    let mut last_progress = Instant::now();
    while !closed || !tasks.is_empty() {
        tokio::select! {
            result = tasks.join_next(), if !tasks.is_empty() => {
                let (i, result) = result.expect("nonempty task set")
                    .map_err(|_| BenchmarkError::ConfigError("source task failed".into()))??;
                stats[i] = result;
            }
            event = rx.recv(), if !closed => {
                match event { Some(event) => comparator.record(event)?, None => closed = true }
            }
            _ = tick.tick() => {
                if rx.is_empty() { comparator.settle(Instant::now(), false)?; }
                if last_progress.elapsed() >= Duration::from_secs(5) {
                    let (matched, pending) = comparator.progress();
                    let elapsed = Instant::now().saturating_duration_since(start).as_secs();
                    println!("Progress: {}/{}s matched={} pending={}", elapsed.min(args.duration_sec), args.duration_sec, matched, pending);
                    last_progress = Instant::now();
                }
            },
        }
    }
    let summary = comparator.summary()?;
    let mut invalid_reasons = Vec::new();
    if summary.matched == 0 {
        invalid_reasons.push("no matched samples".to_string());
    }
    for (i, (receive, transport)) in stats.iter().zip([args.source_1, args.source_2]).enumerate() {
        if receive.queue_dropped > 0 {
            invalid_reasons.push(format!("source {} observation queue overflow", i + 1));
        }
        if transport == Transport::EntriesGrpc && receive.invalid > 0 {
            invalid_reasons.push(format!("source {} Entry payload decode failures; verify provider format/transaction version", i + 1));
        }
    }
    let valid = invalid_reasons.is_empty();
    println!(
        "\nMatched={} Ties={} (wins use matched samples only)",
        summary.matched, summary.ties
    );
    for source in &summary.sources {
        let percent = if summary.matched > 0 {
            source.wins as f64 * 100.0 / summary.matched as f64
        } else {
            0.0
        };
        println!(
            "{}: unique={} wins={} ({:.2}%) only={} duplicates={} late={}",
            source.name,
            source.unique,
            source.wins,
            percent,
            source.only,
            source.duplicates,
            source.late
        );
    }
    if let Some(diff) = &summary.b_minus_a_us {
        println!(
            "B-A (us; positive=A earlier): mean={:.2} p50={:.2} p95={:.2} p99={:.2}",
            diff.mean, diff.p50, diff.p95, diff.p99
        );
    }
    if let Some(gap) = &summary.loser_gap_us {
        println!(
            "Absolute gap (us): p50={:.2} p95={:.2} p99={:.2}",
            gap.p50, gap.p95, gap.p99
        );
    }
    for (i, receive) in stats.iter().enumerate() {
        println!(
            "{}: messages={} invalid={} decode_errors={} queue_dropped={} fec_recovered={}",
            names[i],
            receive.packets_or_messages,
            receive.invalid,
            receive.decode_errors,
            receive.queue_dropped,
            receive.fec_recovered
        );
    }
    for reason in &invalid_reasons {
        eprintln!("Invalid benchmark: {reason}");
    }
    if let Some(path) = &args.json_output {
        let report = Report {
            schema_version: 1,
            valid,
            invalid_reasons,
            duration_sec: args.duration_sec,
            warmup_ms: args.warmup_ms,
            transports: [args.source_1, args.source_2],
            comparison: summary,
            receive: stats,
            settings: serde_json::json!({
                "retention_slots": args.retention_slots,
                "max_keys": args.max_keys,
                "max_samples": args.max_samples,
                "udp_receive_buffer_requested_bytes": args.shred_recv_buffer_bytes,
                "decode_modes": [args.shred_decode_mode_1.to_string(), args.shred_decode_mode_2.to_string()],
                "udp_prefix_skip_bytes": [args.shred_udp_payload_prefix_skip_1, args.shred_udp_payload_prefix_skip_2],
                "decoder_max_tracked_slots": args.shred_max_tracked_slots,
                "decoder_gap_timeout_ms": args.shred_reassembly_gap_timeout_ms,
            }),
        };
        std::fs::write(path, serde_json::to_vec_pretty(&report)?)?;
    }
    if !valid {
        return Err(BenchmarkError::ConfigError(
            "invalid benchmark: inspect reported reasons, source counters and formats".into(),
        ));
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let _ = dotenvy::dotenv();
    run(Args::load()?).await
}
