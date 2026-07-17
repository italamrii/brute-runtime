pub mod json;
pub mod terminal;

use crate::benchmark::BenchmarkReport;
use crate::benchmark::metrics::StabilityStatus;
use crate::hardware::HardwareReport;
use crate::models::ModelReport;
use crate::runtime::Backend;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct CapabilityReport {
    pub generated_at_rfc3339: String,
    pub hardware: HardwareReport,
    pub model: Option<ModelReport>,
    pub benchmark: Option<BenchmarkReport>,
    pub recommendation: Option<Recommendation>,
}

/// A profile derived strictly from a completed benchmark's own measured
/// samples - no universal capability score, no data from models that were
/// never actually run.
#[derive(Debug, Clone, Serialize)]
pub struct Recommendation {
    pub backend: Backend,
    pub thread_count: u32,
    pub gpu_layers: u32,
    pub context_size_tested: u32,
    pub batch_size_tested: u32,
    pub prompt_processing_tokens_per_second_range: (f64, f64),
    pub generation_tokens_per_second_range: (f64, f64),
    pub observed_peak_memory_bytes: Option<u64>,
    pub stability: StabilityStatus,
    pub basis: String,
}

/// Derives a recommendation from one completed benchmark run. This is
/// deliberately a description of *what was observed*, not a prediction for
/// configurations that were never tested.
pub fn build_recommendation(benchmark: &BenchmarkReport) -> Recommendation {
    let pp_range =
        sample_range(&benchmark.prompt_processing.samples_tokens_per_second).unwrap_or((
            benchmark.prompt_processing.avg_tokens_per_second,
            benchmark.prompt_processing.avg_tokens_per_second,
        ));
    let gen_range = sample_range(&benchmark.generation.samples_tokens_per_second).unwrap_or((
        benchmark.generation.avg_tokens_per_second,
        benchmark.generation.avg_tokens_per_second,
    ));

    Recommendation {
        backend: benchmark.config.backend,
        thread_count: benchmark.config.threads,
        gpu_layers: benchmark.config.gpu_layers,
        context_size_tested: benchmark.config.context_size,
        batch_size_tested: benchmark.config.batch_size,
        prompt_processing_tokens_per_second_range: pp_range,
        generation_tokens_per_second_range: gen_range,
        observed_peak_memory_bytes: benchmark.memory.process_peak_working_set_bytes.value,
        stability: benchmark.stability,
        basis: format!(
            "{} repetition(s) via llama-bench on the exact model/config recorded above; not extrapolated to other models or settings",
            benchmark.config.repetitions
        ),
    }
}

fn sample_range(samples: &[f64]) -> Option<(f64, f64)> {
    if samples.is_empty() {
        return None;
    }
    let min = samples.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = samples.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    Some((min, max))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::benchmark::metrics::{MemoryMetrics, RunOutcome, ThroughputMetric, TimingMetrics};
    use crate::hardware::HardwareField;
    use crate::runtime::RuntimeConfig;
    use std::path::PathBuf;

    fn synthetic_benchmark(samples: Vec<f64>, avg: f64, stddev: f64) -> BenchmarkReport {
        let throughput = ThroughputMetric {
            avg_tokens_per_second: avg,
            stddev_tokens_per_second: stddev,
            samples_tokens_per_second: samples,
            coefficient_of_variation: stddev / avg,
        };

        BenchmarkReport {
            config: RuntimeConfig {
                backend: Backend::Cpu,
                binary_dir: PathBuf::from(r"C:\tools\llama.cpp"),
                threads: 8,
                gpu_layers: 0,
                context_size: 2048,
                batch_size: 512,
                prompt_tokens: 512,
                gen_tokens: 128,
                repetitions: 3,
                timeout_secs: 120,
            },
            model_path: PathBuf::from(r"C:\models\test.gguf"),
            timing: TimingMetrics {
                cli_wall_time_ms: HardwareField::measured(1000.0, "test"),
                model_load_time_ms: HardwareField::measured(500.0, "test"),
                time_to_first_token_ms: HardwareField::measured(50.0, "test"),
            },
            memory: MemoryMetrics {
                system_total_bytes: HardwareField::measured(16_000_000_000, "test"),
                system_available_before_bytes: HardwareField::measured(8_000_000_000, "test"),
                system_available_min_during_bytes: HardwareField::measured(6_000_000_000, "test"),
                system_available_after_bytes: HardwareField::measured(7_500_000_000, "test"),
                process_peak_working_set_bytes: HardwareField::measured(2_000_000_000, "test"),
                ram_series: vec![],
            },
            prompt_processing: throughput.clone(),
            generation: throughput,
            cli_outcome: RunOutcome {
                exit_code: Some(0),
                timed_out: false,
                succeeded: true,
            },
            bench_outcome: RunOutcome {
                exit_code: Some(0),
                timed_out: false,
                succeeded: true,
            },
            stability: crate::benchmark::metrics::StabilityStatus::Stable,
            llama_bench_raw_rows: vec![],
        }
    }

    #[test]
    fn recommendation_range_reflects_actual_sample_spread() {
        let bench = synthetic_benchmark(vec![28.0, 30.0, 32.0], 30.0, 2.0);
        let rec = build_recommendation(&bench);
        assert_eq!(rec.generation_tokens_per_second_range, (28.0, 32.0));
        assert_eq!(rec.prompt_processing_tokens_per_second_range, (28.0, 32.0));
    }

    #[test]
    fn recommendation_falls_back_to_avg_when_no_samples_recorded() {
        let bench = synthetic_benchmark(vec![], 30.0, 0.0);
        let rec = build_recommendation(&bench);
        assert_eq!(rec.generation_tokens_per_second_range, (30.0, 30.0));
    }

    #[test]
    fn recommendation_carries_the_exact_config_used_not_a_guess() {
        let bench = synthetic_benchmark(vec![10.0], 10.0, 0.0);
        let rec = build_recommendation(&bench);
        assert_eq!(rec.thread_count, 8);
        assert_eq!(rec.context_size_tested, 2048);
        assert_eq!(rec.batch_size_tested, 512);
        assert_eq!(rec.backend, Backend::Cpu);
        assert_eq!(rec.observed_peak_memory_bytes, Some(2_000_000_000));
    }
}
