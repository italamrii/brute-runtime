//! Orchestrates a benchmark: one `llama-cli` run (for load time / TTFT
//! approximation) and one `llama-bench` run (for repeated-sample
//! throughput), sampling system RAM throughout both.

use super::metrics::{MemoryMetrics, RamSample, RunOutcome, ThroughputMetric, TimingMetrics};
use super::scoring::{classify_stability, coefficient_of_variation};
use super::{BenchmarkReport, DEFAULT_PROMPT};
use crate::errors::BruteError;
use crate::hardware::{Confidence, HardwareField, memory as hw_memory};
use crate::runtime::RuntimeConfig;
use crate::runtime::llama_cpp::{BenchRow, run_bench, run_cli_once};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub fn run(
    model: &Path,
    binary_dir: &Path,
    config: &RuntimeConfig,
    allow_unverified: bool,
) -> Result<BenchmarkReport, BruteError> {
    let before = hw_memory::sample_bytes();
    let series: Arc<Mutex<Vec<RamSample>>> = Arc::new(Mutex::new(Vec::new()));
    let start = Instant::now();

    let series_for_cli = series.clone();
    let (cli_metrics, cli_run) = run_cli_once(
        binary_dir,
        model,
        config,
        DEFAULT_PROMPT,
        allow_unverified,
        move |_child| {
            if let Some((_, avail)) = hw_memory::sample_bytes() {
                series_for_cli.lock().unwrap().push(RamSample {
                    elapsed_ms: start.elapsed().as_millis() as u64,
                    available_bytes: avail,
                });
            }
        },
    )?;

    let series_for_bench = series.clone();
    let (bench_rows, bench_run) =
        run_bench(binary_dir, model, config, allow_unverified, move |_child| {
            if let Some((_, avail)) = hw_memory::sample_bytes() {
                series_for_bench.lock().unwrap().push(RamSample {
                    elapsed_ms: start.elapsed().as_millis() as u64,
                    available_bytes: avail,
                });
            }
        })?;

    let after = hw_memory::sample_bytes();
    let ram_series = Arc::try_unwrap(series)
        .map(|m| m.into_inner().unwrap())
        .unwrap_or_default();

    let memory = build_memory_metrics(before, after, &ram_series, &bench_run);
    let timing = build_timing_metrics(&cli_run, &cli_metrics);
    let (prompt_processing, generation) = build_throughput(&bench_rows);

    let cli_outcome = RunOutcome {
        exit_code: cli_run.exit_code,
        timed_out: cli_run.timed_out,
        succeeded: cli_run.succeeded(),
    };
    let bench_outcome = RunOutcome {
        exit_code: bench_run.exit_code,
        timed_out: bench_run.timed_out,
        succeeded: bench_run.succeeded(),
    };

    let stability = classify_stability(
        &prompt_processing,
        &generation,
        &cli_outcome,
        &bench_outcome,
    );

    Ok(BenchmarkReport {
        config: config.clone(),
        model_path: model.to_path_buf(),
        timing,
        memory,
        prompt_processing,
        generation,
        cli_outcome,
        bench_outcome,
        stability,
        llama_bench_raw_rows: bench_rows,
    })
}

fn build_memory_metrics(
    before: Option<(u64, u64)>,
    after: Option<(u64, u64)>,
    series: &[RamSample],
    bench_run: &crate::runtime::process::ProcessRun,
) -> MemoryMetrics {
    let min_during = series.iter().map(|s| s.available_bytes).min();

    let total = before.map(|(t, _)| t).or_else(|| after.map(|(t, _)| t));

    MemoryMetrics {
        system_total_bytes: field_u64(total, "Win32 GlobalMemoryStatusEx"),
        system_available_before_bytes: field_u64(
            before.map(|(_, a)| a),
            "Win32 GlobalMemoryStatusEx (before run)",
        ),
        system_available_min_during_bytes: match min_during {
            Some(v) => HardwareField::measured(v, "Win32 GlobalMemoryStatusEx (polled during run)"),
            None => HardwareField::unavailable(
                "no polling samples were collected (run completed faster than one poll interval)",
            ),
        },
        system_available_after_bytes: field_u64(
            after.map(|(_, a)| a),
            "Win32 GlobalMemoryStatusEx (after run)",
        ),
        process_peak_working_set_bytes: match bench_run.peak_working_set_bytes {
            Some(v) => HardwareField::measured(
                v,
                "Win32 GetProcessMemoryInfo.PeakWorkingSetSize (llama-bench process)",
            ),
            None => HardwareField::unavailable(
                "GetProcessMemoryInfo failed or process exited before it could be queried",
            ),
        },
        ram_series: series.to_vec(),
    }
}

fn field_u64(value: Option<u64>, source: &str) -> HardwareField<u64> {
    match value {
        Some(v) => HardwareField::measured(v, source),
        None => HardwareField::unavailable(format!("{source} call failed")),
    }
}

fn build_timing_metrics(
    cli_run: &crate::runtime::process::ProcessRun,
    cli_metrics: &crate::runtime::llama_cpp::CliPerfMetrics,
) -> TimingMetrics {
    TimingMetrics {
        cli_wall_time_ms: HardwareField::measured(
            cli_run.wall_time.as_secs_f64() * 1000.0,
            "runtime::process wall-clock (Instant) around the llama-cli process",
        ),
        model_load_time_ms: match cli_metrics.load_time_ms {
            Some(v) => HardwareField {
                value: Some(v),
                confidence: Confidence::Detected,
                source: "parsed from llama-cli's llama_perf_context_print stderr log".to_string(),
            },
            None => HardwareField::unavailable("llama-cli did not print a load time line"),
        },
        time_to_first_token_ms: match cli_metrics.prompt_eval_ms {
            Some(v) => HardwareField {
                value: Some(v),
                confidence: Confidence::Inferred,
                source: "approximated as llama-cli's prompt-eval time; not an independently captured per-token timestamp".to_string(),
            },
            None => HardwareField::unavailable("llama-cli did not print a prompt eval time line"),
        },
    }
}

fn build_throughput(rows: &[BenchRow]) -> (ThroughputMetric, ThroughputMetric) {
    let pp = rows.iter().find(|r| r.test == "pp");
    let tg = rows.iter().find(|r| r.test == "tg");
    (to_throughput(pp), to_throughput(tg))
}

fn to_throughput(row: Option<&BenchRow>) -> ThroughputMetric {
    match row {
        Some(r) => ThroughputMetric {
            avg_tokens_per_second: r.avg_ts,
            stddev_tokens_per_second: r.stddev_ts,
            samples_tokens_per_second: r.samples_ts.clone(),
            coefficient_of_variation: coefficient_of_variation(r.avg_ts, r.stddev_ts),
        },
        None => ThroughputMetric {
            avg_tokens_per_second: 0.0,
            stddev_tokens_per_second: 0.0,
            samples_tokens_per_second: vec![],
            coefficient_of_variation: 0.0,
        },
    }
}
