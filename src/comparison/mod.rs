//! Shred comparisons share matching and statistics; transport adapters only emit observations.
pub mod entries;
pub mod sources;
pub mod stats;

use clap::ValueEnum;
use serde::Serialize;
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    Slot,
    Shred,
    Transaction,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SampleKey {
    Slot(u64),
    Shred { slot: u64, index: u32, coding: bool },
    Transaction { slot: u64, signature: [u8; 64] },
}

impl SampleKey {
    pub fn slot(&self) -> u64 {
        match self {
            Self::Slot(slot) | Self::Shred { slot, .. } | Self::Transaction { slot, .. } => *slot,
        }
    }
}

#[derive(Debug)]
pub struct Observation {
    pub source: usize,
    pub key: SampleKey,
    pub at: Instant,
}
