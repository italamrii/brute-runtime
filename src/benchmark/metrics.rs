//! Metric types shared between the benchmark runner and the report/
//! recommendation layers. Every numeric field states, via its containing
//! struct's doc comment or an adjacent `*_source` string, whether it came
//! from a direct measurement or a tool's self-reported log.

use crate::hardware::HardwareField;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct ThroughputMetric {
    pub avg_tokens_per_second: f64,
    pub stddev_tokens_per_second: f64,
    pub samples_tokens_per_second: Vec<f64>,
    pub coefficient_of_variation: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RamSample {
    pub elapsed_ms: u64,
    pub available_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryMetrics {
    pub system_total_bytes: HardwareField<u64>,
    pub system_available_before_bytes: HardwareField<u64>,
    pub system_available_min_during_bytes: HardwareField<u64>,
    pub system_available_after_bytes: HardwareField<u64>,
    pub process_peak_working_set_bytes: HardwareField<u64>,
    pub ram_series: Vec<RamSample>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TimingMetrics {
    /// Wall-clock time of the whole llama-cli process, measured by us via
    /// `runtime::process::run` (`Instant`-based).
    pub cli_wall_time_ms: HardwareField<f64>,
    /// llama.cpp's own self-reported model load time, parsed from its
    /// `llama_perf_context_print` stderr log.
    pub model_load_time_ms: HardwareField<f64>,
    /// Approximated as the prompt-eval time from the same log line - the
    /// time to process the prompt and become ready to emit the first
    /// generated token. Not an independently captured per-token timestamp.
    pub time_to_first_token_ms: HardwareField<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StabilityStatus {
    Stable,
    Marginal,
    Unstable,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunOutcome {
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub succeeded: bool,
}
