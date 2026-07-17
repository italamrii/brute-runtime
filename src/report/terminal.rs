//! Human-readable rendering of a `CapabilityReport`. Every value is printed
//! next to its confidence tag so a reader never mistakes a guess for a
//! measurement.

use super::CapabilityReport;
use crate::hardware::{Confidence, HardwareField};
use std::fmt::Write as _;

fn tag(c: Confidence) -> &'static str {
    match c {
        Confidence::Measured => "measured",
        Confidence::Detected => "detected",
        Confidence::Inferred => "inferred",
        Confidence::Unavailable => "unavailable",
    }
}

fn field_line<T: std::fmt::Display>(out: &mut String, label: &str, field: &HardwareField<T>) {
    match &field.value {
        Some(v) => {
            let _ = writeln!(
                out,
                "  {label}: {v}  [{}, source: {}]",
                tag(field.confidence),
                field.source
            );
        }
        None => {
            let _ = writeln!(out, "  {label}: (unavailable)  [{}]", field.source);
        }
    }
}

pub fn render(report: &CapabilityReport) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "BRUTE Runtime - Capability Report");
    let _ = writeln!(out, "Generated: {}", report.generated_at_rfc3339);
    let _ = writeln!(out);

    render_hardware(&mut out, report);

    if let Some(model) = &report.model {
        render_model(&mut out, model);
    }

    if let Some(bench) = &report.benchmark {
        render_benchmark(&mut out, bench);
    }

    if let Some(rec) = &report.recommendation {
        render_recommendation(&mut out, rec);
    }

    out
}

fn render_hardware(out: &mut String, report: &CapabilityReport) {
    let hw = &report.hardware;
    let _ = writeln!(out, "== Hardware ==");
    field_line(out, "OS product name", &hw.os.product_name);
    field_line(out, "OS display version", &hw.os.display_version);
    field_line(out, "OS build", &hw.os.build_number);
    field_line(out, "Process architecture", &hw.os.process_architecture);
    field_line(out, "Native architecture", &hw.os.native_architecture);
    field_line(out, "CPU vendor", &hw.cpu.vendor);
    field_line(out, "CPU brand", &hw.cpu.brand);
    field_line(out, "Physical cores", &hw.cpu.physical_cores);
    field_line(out, "Logical cores", &hw.cpu.logical_cores);
    match &hw.cpu.instruction_sets.value {
        Some(sets) if !sets.is_empty() => {
            let _ = writeln!(
                out,
                "  Instruction sets: {}  [{}]",
                sets.join(", "),
                tag(hw.cpu.instruction_sets.confidence)
            );
        }
        _ => {
            let _ = writeln!(out, "  Instruction sets: (unavailable)");
        }
    }
    field_line(out, "Total RAM (bytes)", &hw.memory.total_bytes);
    field_line(out, "Available RAM (bytes)", &hw.memory.available_bytes);

    match &hw.gpu.adapters.value {
        Some(adapters) if !adapters.is_empty() => {
            let _ = writeln!(out, "  GPU adapters: [{}]", tag(hw.gpu.adapters.confidence));
            for a in adapters {
                let vram = a
                    .dedicated_vram_bytes
                    .map(|v| format!("{v} bytes"))
                    .unwrap_or_else(|| "unknown".to_string());
                let driver = a
                    .driver_version
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string());
                let _ = writeln!(
                    out,
                    "    - {} ({:?}), VRAM: {vram}, driver: {driver}",
                    a.name, a.vendor
                );
            }
        }
        _ => {
            let _ = writeln!(out, "  GPU adapters: (unavailable)");
        }
    }
    field_line(out, "CUDA available", &hw.gpu.cuda_available);
    field_line(out, "Vulkan available", &hw.gpu.vulkan_available);

    if let Some(storage) = &report.hardware.storage {
        let _ = writeln!(out, "  Storage path queried: {}", storage.path_queried);
        field_line(out, "Storage free bytes", &storage.free_bytes);
        field_line(out, "Storage total bytes", &storage.total_bytes);
    }
    let _ = writeln!(out);
}

fn render_model(out: &mut String, model: &crate::models::ModelReport) {
    let _ = writeln!(out, "== Model ==");
    let _ = writeln!(out, "  Path: {}", model.path.display());
    let _ = writeln!(out, "  File size: {} bytes", model.file_size_bytes);
    let _ = writeln!(out, "  SHA-256: {}", model.sha256);
    let _ = writeln!(out, "  GGUF version: {}", model.gguf_version);
    let _ = writeln!(out, "  Tensor count: {}", model.tensor_count);
    let _ = writeln!(out, "  KV count: {}", model.kv_count);
    let _ = writeln!(
        out,
        "  Architecture: {}",
        model.architecture.as_deref().unwrap_or("(unknown)")
    );
    let _ = writeln!(
        out,
        "  Name: {}",
        model.name.as_deref().unwrap_or("(unknown)")
    );
    let _ = writeln!(
        out,
        "  Quantization: {}",
        model.quantization.as_deref().unwrap_or("(unknown)")
    );
    let _ = writeln!(
        out,
        "  Parameter count: {}",
        model
            .parameter_count
            .map(|p| p.to_string())
            .unwrap_or_else(|| "(unavailable)".to_string())
    );
    let _ = writeln!(
        out,
        "  Tensor-data size consistency check: {}",
        if model.size_consistency_checked {
            "passed"
        } else {
            "not performed (unknown tensor type present)"
        }
    );
    let _ = writeln!(out);
}

fn render_benchmark(out: &mut String, bench: &crate::benchmark::BenchmarkReport) {
    let _ = writeln!(out, "== Benchmark ==");
    let _ = writeln!(out, "  Backend: {}", bench.config.backend.as_str());
    let _ = writeln!(out, "  Threads: {}", bench.config.threads);
    let _ = writeln!(out, "  Context size: {}", bench.config.context_size);
    let _ = writeln!(out, "  Batch size: {}", bench.config.batch_size);
    let _ = writeln!(out, "  Repetitions: {}", bench.config.repetitions);
    let _ = writeln!(out, "  Stability: {:?}", bench.stability);
    let _ = writeln!(
        out,
        "  CLI run: exit={:?} timed_out={}",
        bench.cli_outcome.exit_code, bench.cli_outcome.timed_out
    );
    let _ = writeln!(
        out,
        "  Bench run: exit={:?} timed_out={}",
        bench.bench_outcome.exit_code, bench.bench_outcome.timed_out
    );
    field_line(
        out,
        "Model load time (ms)",
        &bench.timing.model_load_time_ms,
    );
    field_line(
        out,
        "Time to first token (ms, approx.)",
        &bench.timing.time_to_first_token_ms,
    );
    let _ = writeln!(
        out,
        "  Prompt processing: {:.2} tok/s (stddev {:.2}, CV {:.3})",
        bench.prompt_processing.avg_tokens_per_second,
        bench.prompt_processing.stddev_tokens_per_second,
        bench.prompt_processing.coefficient_of_variation
    );
    let _ = writeln!(
        out,
        "  Generation: {:.2} tok/s (stddev {:.2}, CV {:.3})",
        bench.generation.avg_tokens_per_second,
        bench.generation.stddev_tokens_per_second,
        bench.generation.coefficient_of_variation
    );
    field_line(
        out,
        "Peak process memory (bytes)",
        &bench.memory.process_peak_working_set_bytes,
    );
    field_line(
        out,
        "System RAM available before (bytes)",
        &bench.memory.system_available_before_bytes,
    );
    field_line(
        out,
        "System RAM available min-during (bytes)",
        &bench.memory.system_available_min_during_bytes,
    );
    field_line(
        out,
        "System RAM available after (bytes)",
        &bench.memory.system_available_after_bytes,
    );
    let _ = writeln!(out);
}

fn render_recommendation(out: &mut String, rec: &super::Recommendation) {
    let _ = writeln!(out, "== Recommended profile (from this run only) ==");
    let _ = writeln!(out, "  backend: {}", rec.backend.as_str());
    let _ = writeln!(out, "  thread count: {}", rec.thread_count);
    let _ = writeln!(out, "  gpu layers: {}", rec.gpu_layers);
    let _ = writeln!(out, "  context size tested: {}", rec.context_size_tested);
    let _ = writeln!(out, "  batch size tested: {}", rec.batch_size_tested);
    let _ = writeln!(
        out,
        "  expected prompt processing speed: {:.2}-{:.2} tok/s",
        rec.prompt_processing_tokens_per_second_range.0,
        rec.prompt_processing_tokens_per_second_range.1
    );
    let _ = writeln!(
        out,
        "  expected generation speed: {:.2}-{:.2} tok/s",
        rec.generation_tokens_per_second_range.0, rec.generation_tokens_per_second_range.1
    );
    let _ = writeln!(
        out,
        "  observed peak memory: {}",
        rec.observed_peak_memory_bytes
            .map(|v| format!("{v} bytes"))
            .unwrap_or_else(|| "unavailable".to_string())
    );
    let _ = writeln!(out, "  stability confidence: {:?}", rec.stability);
    let _ = writeln!(out, "  basis: {}", rec.basis);
}
