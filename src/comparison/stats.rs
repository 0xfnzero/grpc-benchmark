use super::{Mode, Observation, SampleKey};
use crate::{BenchmarkError, Result};
use rustc_hash::FxHashMap;
use serde::Serialize;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

#[derive(Default)]
struct Sample {
    arrivals: [Option<Instant>; 2],
    settled: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SourceSummary {
    pub name: String,
    pub unique: u64,
    pub only: u64,
    pub wins: u64,
    pub duplicates: u64,
    pub late: u64,
}

#[derive(Debug, Serialize)]
pub struct Distribution {
    pub count: usize,
    pub mean: f64,
    pub p50: f64,
    pub p95: f64,
    pub p99: f64,
    pub min: f64,
    pub max: f64,
}

impl Distribution {
    fn new(values: &mut [f64]) -> Option<Self> {
        if values.is_empty() {
            return None;
        }
        values.sort_unstable_by(f64::total_cmp);
        let percentile =
            |p: f64| values[((values.len() as f64 * p).ceil() as usize).saturating_sub(1)];
        Some(Self {
            count: values.len(),
            mean: values.iter().sum::<f64>() / values.len() as f64,
            p50: percentile(0.5),
            p95: percentile(0.95),
            p99: percentile(0.99),
            min: values[0],
            max: values[values.len() - 1],
        })
    }
}

#[derive(Debug, Serialize)]
pub struct ComparisonSummary {
    pub mode: Mode,
    pub timestamp: &'static str,
    pub settle_window_ms: u64,
    pub sources: [SourceSummary; 2],
    pub matched: u64,
    pub ties: u64,
    /// Positive means source 1 arrived earlier; negative means source 2 arrived earlier.
    pub b_minus_a_us: Option<Distribution>,
    pub loser_gap_us: Option<Distribution>,
}

#[derive(Clone)]
pub struct ComparisonConfig {
    pub mode: Mode,
    pub duration: Duration,
    pub settle: Duration,
    pub max_keys: usize,
    pub max_samples: usize,
    pub retention_slots: u64,
}

/// Bounded, first-observation matching. A settled key remains a tombstone until its
/// slot leaves the retention window so retransmits cannot create fresh samples.
pub struct Comparator {
    samples: FxHashMap<SampleKey, Sample>,
    sources: [SourceSummary; 2],
    mode: Mode,
    start: Instant,
    end: Instant,
    settle: Duration,
    max_keys: usize,
    min_slot: u64,
    high_slot: u64,
    retention_slots: u64,
    diffs: Vec<f64>,
    max_samples: usize,
    matched: u64,
    ties: u64,
    pending: VecDeque<SampleKey>,
    pruned_at_slot: u64,
}

impl Comparator {
    pub fn new(config: ComparisonConfig, names: [String; 2], start: Instant) -> Self {
        let ComparisonConfig {
            mode,
            duration,
            settle,
            max_keys,
            max_samples,
            retention_slots,
        } = config;
        Self {
            samples: FxHashMap::default(),
            sources: names.map(|name| SourceSummary {
                name,
                unique: 0,
                only: 0,
                wins: 0,
                duplicates: 0,
                late: 0,
            }),
            mode,
            start,
            end: start + duration,
            settle,
            max_keys,
            max_samples,
            min_slot: 0,
            high_slot: 0,
            retention_slots,
            diffs: Vec::new(),
            matched: 0,
            ties: 0,
            pending: VecDeque::new(),
            pruned_at_slot: 0,
        }
    }

    pub fn record(&mut self, event: Observation) -> Result<()> {
        let source = event.source;
        if source > 1 {
            return Err(BenchmarkError::ConfigError("invalid source index".into()));
        }
        if event.at < self.start {
            return Ok(());
        }
        let prior = self.samples.get(&event.key);
        if event.key.slot() < self.min_slot && prior.is_none_or(|sample| sample.settled) {
            self.sources[source].late += 1;
            return Ok(());
        }
        let existing = prior.is_some();
        // During the grace period, only counterparts of measured samples are accepted.
        if !existing && event.at >= self.end {
            return Ok(());
        }
        if !existing && self.samples.len() >= self.max_keys {
            return Err(BenchmarkError::ConfigError(
                "comparison key capacity reached; increase --max-keys or reduce --retention-slots"
                    .into(),
            ));
        }
        self.high_slot = self.high_slot.max(event.key.slot());
        if !existing {
            self.pending.push_back(event.key.clone());
        }
        let sample = self.samples.entry(event.key).or_default();
        if let Some(previous) = sample.arrivals[source] {
            self.sources[source].duplicates += 1;
            if !sample.settled {
                sample.arrivals[source] = Some(previous.min(event.at));
            }
            return Ok(());
        }
        let first = sample.arrivals.iter().flatten().copied().min();
        if sample.settled
            || first.is_some_and(|at| {
                let gap = if event.at >= at {
                    event.at.duration_since(at)
                } else {
                    at.duration_since(event.at)
                };
                gap > self.settle
            })
        {
            self.sources[source].late += 1;
            return Ok(());
        }
        sample.arrivals[source] = Some(event.at);
        self.sources[source].unique += 1;
        Ok(())
    }

    pub fn settle(&mut self, now: Instant, finish: bool) -> Result<()> {
        while let Some(key) = self.pending.front() {
            let sample = self.samples.get_mut(key).expect("pending sample exists");
            let first = sample
                .arrivals
                .iter()
                .flatten()
                .copied()
                .min()
                .expect("sample has an arrival");
            if !finish && now.saturating_duration_since(first) <= self.settle {
                break;
            }
            self.pending.pop_front();
            sample.settled = true;
            match sample.arrivals {
                [Some(a), Some(b)] => {
                    if self.diffs.len() >= self.max_samples {
                        return Err(BenchmarkError::ConfigError("latency sample capacity reached; increase --max-samples or shorten the run".into()));
                    }
                    self.matched += 1;
                    let diff = if b >= a {
                        b.duration_since(a).as_secs_f64() * 1e6
                    } else {
                        -a.duration_since(b).as_secs_f64() * 1e6
                    };
                    if diff > 0.0 {
                        self.sources[0].wins += 1;
                    } else if diff < 0.0 {
                        self.sources[1].wins += 1;
                    } else {
                        self.ties += 1;
                    }
                    self.diffs.push(diff);
                }
                [Some(_), None] => self.sources[0].only += 1,
                [None, Some(_)] => self.sources[1].only += 1,
                _ => unreachable!(),
            }
        }
        // Prune only settled keys; unsettled keys stay eligible for their full wait window.
        if self.high_slot.saturating_sub(self.pruned_at_slot) >= 16 {
            self.min_slot = self.high_slot.saturating_sub(self.retention_slots);
            self.samples
                .retain(|key, sample| key.slot() >= self.min_slot || !sample.settled);
            self.pruned_at_slot = self.high_slot;
        }
        Ok(())
    }

    pub fn progress(&self) -> (u64, usize) {
        (self.matched, self.pending.len())
    }

    pub fn summary(mut self) -> Result<ComparisonSummary> {
        self.settle(self.end + self.settle, true)?;
        let mut gaps: Vec<_> = self.diffs.iter().map(|v| v.abs()).collect();
        Ok(ComparisonSummary {
            mode: self.mode,
            timestamp: "application receive; UDP transaction mode includes local decode",
            settle_window_ms: self.settle.as_millis() as u64,
            sources: self.sources,
            matched: self.matched,
            ties: self.ties,
            b_minus_a_us: Distribution::new(&mut self.diffs),
            loser_gap_us: Distribution::new(&mut gaps),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn comparator(mode: Mode, start: Instant) -> Comparator {
        Comparator::new(
            ComparisonConfig {
                mode,
                duration: Duration::from_secs(1),
                settle: Duration::from_millis(100),
                max_keys: 100,
                max_samples: 100,
                retention_slots: 128,
            },
            ["A".into(), "B".into()],
            start,
        )
    }
    fn event(source: usize, key: SampleKey, at: Instant) -> Observation {
        Observation { source, key, at }
    }

    #[test]
    fn signed_differences_ties_duplicates_and_single_sided_samples() {
        let start = Instant::now();
        let mut cmp = comparator(Mode::Slot, start);
        for (slot, a, b) in [(1, 0, 20), (2, 30, 10), (3, 40, 40)] {
            cmp.record(event(
                0,
                SampleKey::Slot(slot),
                start + Duration::from_micros(a),
            ))
            .unwrap();
            cmp.record(event(
                1,
                SampleKey::Slot(slot),
                start + Duration::from_micros(b),
            ))
            .unwrap();
        }
        cmp.record(event(
            0,
            SampleKey::Slot(1),
            start + Duration::from_micros(99),
        ))
        .unwrap();
        cmp.record(event(0, SampleKey::Slot(4), start)).unwrap();
        cmp.record(event(1, SampleKey::Slot(5), start)).unwrap();
        let result = cmp.summary().unwrap();
        assert_eq!((result.matched, result.ties), (3, 1));
        assert_eq!((result.sources[0].wins, result.sources[1].wins), (1, 1));
        assert_eq!((result.sources[0].only, result.sources[1].only), (1, 1));
        assert_eq!(result.sources[0].duplicates, 1);
        let diff = result.b_minus_a_us.unwrap();
        assert_eq!(
            (diff.min, diff.p50, diff.p95, diff.max),
            (-20.0, 0.0, 20.0, 20.0)
        );
    }

    #[test]
    fn same_slot_and_index_but_different_shred_types_do_not_match() {
        let start = Instant::now();
        let mut cmp = comparator(Mode::Shred, start);
        cmp.record(event(
            0,
            SampleKey::Shred {
                slot: 1,
                index: 0,
                coding: false,
            },
            start,
        ))
        .unwrap();
        cmp.record(event(
            1,
            SampleKey::Shred {
                slot: 1,
                index: 0,
                coding: true,
            },
            start,
        ))
        .unwrap();
        let result = cmp.summary().unwrap();
        assert_eq!(result.matched, 0);
        assert_eq!((result.sources[0].only, result.sources[1].only), (1, 1));
    }

    #[test]
    fn transaction_matches_signature_and_slot_not_batch_boundaries() {
        let start = Instant::now();
        let mut cmp = comparator(Mode::Transaction, start);
        let key = SampleKey::Transaction {
            slot: 1,
            signature: [3; 64],
        };
        cmp.record(event(0, key.clone(), start)).unwrap();
        cmp.record(event(
            1,
            SampleKey::Transaction {
                slot: 2,
                signature: [3; 64],
            },
            start,
        ))
        .unwrap();
        cmp.record(event(1, key, start)).unwrap();
        let result = cmp.summary().unwrap();
        assert_eq!(result.matched, 1);
        assert_eq!(result.sources[1].only, 1);
    }

    #[test]
    fn warmup_excluded_and_end_grace_accepts_only_counterparts_within_window() {
        let start = Instant::now();
        let mut cmp = comparator(Mode::Slot, start);
        cmp.record(event(
            0,
            SampleKey::Slot(1),
            start - Duration::from_nanos(1),
        ))
        .unwrap();
        cmp.record(event(
            0,
            SampleKey::Slot(2),
            start + Duration::from_millis(990),
        ))
        .unwrap();
        cmp.record(event(
            1,
            SampleKey::Slot(2),
            start + Duration::from_millis(1050),
        ))
        .unwrap();
        cmp.record(event(
            0,
            SampleKey::Slot(3),
            start + Duration::from_millis(1000),
        ))
        .unwrap();
        cmp.record(event(0, SampleKey::Slot(4), start)).unwrap();
        cmp.record(event(
            1,
            SampleKey::Slot(4),
            start + Duration::from_millis(101),
        ))
        .unwrap();
        let result = cmp.summary().unwrap();
        assert_eq!(result.matched, 1);
        assert_eq!(result.sources[0].only, 1);
        assert_eq!(result.sources[1].late, 1);
    }

    #[test]
    fn settled_retransmits_and_evicted_old_slots_cannot_be_scored_again() {
        let start = Instant::now();
        let mut cmp = comparator(Mode::Slot, start);
        cmp.record(event(0, SampleKey::Slot(1), start)).unwrap();
        cmp.settle(start + Duration::from_millis(110), false)
            .unwrap();
        cmp.record(event(
            1,
            SampleKey::Slot(1),
            start + Duration::from_millis(120),
        ))
        .unwrap();
        cmp.record(event(
            0,
            SampleKey::Slot(200),
            start + Duration::from_millis(130),
        ))
        .unwrap();
        cmp.settle(start + Duration::from_millis(240), false)
            .unwrap();
        cmp.record(event(
            0,
            SampleKey::Slot(1),
            start + Duration::from_millis(250),
        ))
        .unwrap();
        let result = cmp.summary().unwrap();
        assert_eq!(result.sources[0].only, 2);
        assert_eq!(result.sources[0].late, 1);
        assert_eq!(result.sources[1].late, 1);
    }

    #[test]
    fn limits_fail_instead_of_silently_biasing_statistics() {
        let start = Instant::now();
        let mut cmp = comparator(Mode::Slot, start);
        cmp.max_keys = 1;
        cmp.record(event(0, SampleKey::Slot(1), start)).unwrap();
        assert!(cmp.record(event(0, SampleKey::Slot(2), start)).is_err());
        let mut cmp = comparator(Mode::Slot, start);
        cmp.max_samples = 1;
        for slot in 1..=2 {
            for source in 0..=1 {
                cmp.record(event(source, SampleKey::Slot(slot), start))
                    .unwrap();
            }
        }
        assert!(cmp.summary().is_err());
    }
    #[test]
    fn out_of_order_source_delivery_still_enforces_absolute_wait_window() {
        let start = Instant::now();
        let mut cmp = comparator(Mode::Slot, start);
        cmp.record(event(
            0,
            SampleKey::Slot(1),
            start + Duration::from_millis(200),
        ))
        .unwrap();
        cmp.record(event(1, SampleKey::Slot(1), start)).unwrap();
        let result = cmp.summary().unwrap();
        assert_eq!(result.matched, 0);
        assert_eq!(result.sources[1].late, 1);
    }

    #[test]
    fn pending_old_slot_keeps_full_wait_window_despite_pruning_watermark() {
        let start = Instant::now();
        let mut cmp = comparator(Mode::Slot, start);
        cmp.record(event(0, SampleKey::Slot(1), start)).unwrap();
        cmp.record(event(
            0,
            SampleKey::Slot(200),
            start + Duration::from_millis(10),
        ))
        .unwrap();
        cmp.settle(start + Duration::from_millis(20), false)
            .unwrap();
        cmp.record(event(
            1,
            SampleKey::Slot(1),
            start + Duration::from_millis(50),
        ))
        .unwrap();
        assert_eq!(cmp.summary().unwrap().matched, 1);
    }
}
