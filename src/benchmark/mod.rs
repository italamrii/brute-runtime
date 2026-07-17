pub mod metrics;
pub mod runner;
pub mod scoring;

use crate::runtime::RuntimeConfig;
use crate::runtime::llama_cpp::BenchRow;
use metrics::{MemoryMetrics, RunOutcome, StabilityStatus, ThroughputMetric, TimingMetrics};
use serde::Serialize;
use std::path::PathBuf;

/// Short, deterministic prompt used for the single `llama-cli` timing run.
/// Kept fixed so load-time/TTFT numbers are comparable run to run; the
/// throughput numbers (which matter far more) come from `llama-bench`'s own
/// synthetic token stream, not this prompt.
pub const DEFAULT_PROMPT: &str = "The quick brown fox jumps over the lazy dog.";

#[derive(Debug, Clone, Serialize)]
pub struct BenchmarkReport {
    pub config: RuntimeConfig,
    pub model_path: PathBuf,
    pub timing: TimingMetrics,
    pub memory: MemoryMetrics,
    pub prompt_processing: ThroughputMetric,
    pub generation: ThroughputMetric,
    pub cli_outcome: RunOutcome,
    pub bench_outcome: RunOutcome,
    pub stability: StabilityStatus,
    pub llama_bench_raw_rows: Vec<BenchRow>,
}

pub use runner::run;
