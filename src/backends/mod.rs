//! Backend Capability Verification: distinguishes "a driver was detected"
//! (Stage 1's `hardware::gpu` — presence only) from "this backend was
//! actually exercised end to end" (binary present + hash-verified, process
//! launches, a real model loads, a real tiny benchmark completes, and the
//! benchmark's own output confirms the GPU path was actually used rather
//! than a silent CPU fallback).
//!
//! Never claims `verified` from detection alone, and never lets a silent
//! CPU fallback during a GPU verification run report as GPU success - see
//! `check_no_silent_fallback`.

use crate::profile::HardwareCapabilityProfile;
use crate::runtime::llama_cpp::{self, BenchRow};
use crate::runtime::{Backend, RuntimeConfig};
use serde::Serialize;
use std::path::Path;
use std::time::Duration;

/// Small, fixed values used only for the *verification* run - proving the
/// backend initializes and computes at all, not measuring real throughput
/// or testing whether the full model fits. Real capacity is what
/// `tuning::candidates`/the tuner itself determines afterward.
const VERIFICATION_GPU_LAYERS: u32 = 1;
const VERIFICATION_PROMPT_TOKENS: u32 = 16;
const VERIFICATION_GEN_TOKENS: u32 = 8;
const VERIFICATION_REPETITIONS: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BackendStatus {
    /// Every stage succeeded, including confirmation the backend was
    /// actually used (not a silent fallback).
    Verified,
    /// Driver/hardware detected, but verification wasn't attempted or
    /// couldn't get past detection (e.g. no binary directory given).
    DetectedOnly,
    BinaryMissing,
    LaunchFailed,
    ModelLoadFailed,
    BenchmarkFailed,
    /// Detected but not usable (e.g. CPU-only machine for a GPU backend).
    Unavailable,
    /// Detection itself was inconclusive.
    Unknown,
}

impl BackendStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            BackendStatus::Verified => "verified",
            BackendStatus::DetectedOnly => "detected_only",
            BackendStatus::BinaryMissing => "binary_missing",
            BackendStatus::LaunchFailed => "launch_failed",
            BackendStatus::ModelLoadFailed => "model_load_failed",
            BackendStatus::BenchmarkFailed => "benchmark_failed",
            BackendStatus::Unavailable => "unavailable",
            BackendStatus::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BackendVerification {
    pub backend: Backend,
    pub detected: bool,
    pub binary_available: bool,
    pub launch_verified: bool,
    pub model_load_verified: bool,
    pub benchmark_verified: bool,
    pub status: BackendStatus,
    pub failure_reason: Option<String>,
    /// Real evidence from the verification benchmark, when it ran -
    /// tokens/sec and the backend's own self-reported `gpu_info` string,
    /// kept distinct from any later real tuning measurement.
    pub verification_tokens_per_second: Option<f64>,
    pub reported_gpu_info: Option<String>,
}

fn detected(backend: Backend, profile: &HardwareCapabilityProfile) -> Option<bool> {
    match backend {
        Backend::Cpu => Some(true),
        Backend::Cuda => profile.backends.cuda.value,
        Backend::Vulkan => profile.backends.vulkan.value,
    }
}

/// Runs the full verification pipeline for one backend. `binary_dir` is
/// required for anything past detection - without it, verification stops
/// at `DetectedOnly`/`Unavailable`/`Unknown` honestly rather than guessing.
pub fn verify_backend(
    backend: Backend,
    binary_dir: Option<&Path>,
    model: &Path,
    profile: &HardwareCapabilityProfile,
    allow_unverified_binary: bool,
    timeout: Duration,
) -> BackendVerification {
    let mut result = BackendVerification {
        backend,
        detected: false,
        binary_available: false,
        launch_verified: false,
        model_load_verified: false,
        benchmark_verified: false,
        status: BackendStatus::Unknown,
        failure_reason: None,
        verification_tokens_per_second: None,
        reported_gpu_info: None,
    };

    match detected(backend, profile) {
        None => {
            result.status = BackendStatus::Unknown;
            result.failure_reason =
                Some("backend availability could not be determined on this machine".to_string());
            return result;
        }
        Some(false) => {
            result.status = BackendStatus::Unavailable;
            result.failure_reason = Some(format!("{backend:?} is not available on this machine"));
            return result;
        }
        Some(true) => {
            result.detected = true;
        }
    }

    let Some(binary_dir) = binary_dir else {
        result.status = BackendStatus::DetectedOnly;
        result.failure_reason =
            Some("no llama.cpp binary directory given - detection only, not verified".to_string());
        return result;
    };

    let cli_check = llama_cpp::verify_llama_binary(
        &llama_cpp::llama_cli_path(binary_dir),
        allow_unverified_binary,
    );
    let bench_check = llama_cpp::verify_llama_binary(
        &llama_cpp::llama_bench_path(binary_dir),
        allow_unverified_binary,
    );
    if let (Err(e), _) | (_, Err(e)) = (&cli_check, &bench_check) {
        result.status = BackendStatus::BinaryMissing;
        result.failure_reason = Some(e.to_string());
        return result;
    }
    result.binary_available = true;

    if let Err(e) = llama_cpp::run_cli_version(binary_dir, allow_unverified_binary) {
        result.status = BackendStatus::LaunchFailed;
        result.failure_reason = Some(e.to_string());
        return result;
    }
    result.launch_verified = true;

    let config = RuntimeConfig {
        backend,
        binary_dir: binary_dir.to_path_buf(),
        threads: std::thread::available_parallelism()
            .map(|n| n.get() as u32)
            .unwrap_or(1),
        gpu_layers: VERIFICATION_GPU_LAYERS,
        context_size: 512,
        batch_size: 128,
        prompt_tokens: VERIFICATION_PROMPT_TOKENS,
        gen_tokens: VERIFICATION_GEN_TOKENS,
        repetitions: VERIFICATION_REPETITIONS,
        timeout_secs: timeout.as_secs(),
    };

    match llama_cpp::run_bench(binary_dir, model, &config, allow_unverified_binary, |_| {
        crate::runtime::process::TickAction::Continue
    }) {
        Err(e) => {
            // llama-bench failing to even start/exit cleanly means the
            // model never loaded under this backend.
            result.status = BackendStatus::ModelLoadFailed;
            result.failure_reason = Some(e.to_string());
        }
        Ok((rows, _run)) => {
            result.model_load_verified = true;
            check_no_silent_fallback(backend, &rows, &mut result);
        }
    }

    result
}

/// Decides which single GPU backend (if any) tuning is allowed to
/// generate GPU-offload candidates for - `None` unless a real
/// end-to-end verification (not mere driver detection) passed. When the
/// caller pins `Backend::Cpu`, no GPU backend is even attempted; when
/// they pin a specific GPU backend, only that one is checked; otherwise
/// both CUDA and Vulkan are opportunistically verified and the first
/// verified one wins. Shared by `brute tune run` and the desktop
/// auto-tune workflow so this selection policy exists exactly once.
pub fn determine_verified_gpu_backend(
    model: &Path,
    llama_bin: &Path,
    profile: &HardwareCapabilityProfile,
    requested_backend: Option<Backend>,
    allow_unverified_binary: bool,
    timeout: Duration,
) -> (Option<Backend>, Vec<BackendVerification>) {
    let to_check: Vec<Backend> = match requested_backend {
        Some(Backend::Cpu) => vec![],
        Some(b) => vec![b],
        None => vec![Backend::Cuda, Backend::Vulkan],
    };

    let mut verifications = Vec::new();
    let mut verified_gpu = None;
    for b in to_check {
        let v = verify_backend(
            b,
            Some(llama_bin),
            model,
            profile,
            allow_unverified_binary,
            timeout,
        );
        if v.status == BackendStatus::Verified && verified_gpu.is_none() {
            verified_gpu = Some(b);
        }
        verifications.push(v);
    }
    (verified_gpu, verifications)
}

/// The core anti-fabrication check: a GPU backend's verification run must
/// show real evidence the GPU path was used.
///
/// This trusts **only** llama-bench's own `gpu_info` string being
/// non-empty - not `n_gpu_layers`. That field was dropped from the check
/// after live testing against this repo's pinned CPU-only b10064 binary:
/// pointing `brute backends verify --backend cuda` at the CPU-only
/// binary directory returned `n_gpu_layers: 1` in the JSON output (an
/// echo of the requested `-ngl` flag) even though `gpu_info` was empty
/// and no GPU was ever touched - a real silent-fallback false positive,
/// not a hypothetical one. `gpu_info` is populated by llama.cpp only when
/// a device backend actually initialized, so it is the one signal this
/// check relies on. If llama-bench ran and exited cleanly but `gpu_info`
/// is empty for a GPU backend, that is a silent-fallback failure, not a
/// success - `benchmark_verified` stays `false` and `status` is
/// `BenchmarkFailed`, never `Verified`.
fn check_no_silent_fallback(backend: Backend, rows: &[BenchRow], result: &mut BackendVerification) {
    if rows.is_empty() {
        result.status = BackendStatus::BenchmarkFailed;
        result.failure_reason = Some("llama-bench produced no result rows".to_string());
        return;
    }

    let generation_row = rows
        .iter()
        .find(|r| r.test == "tg")
        .or_else(|| rows.first());
    result.verification_tokens_per_second = generation_row.map(|r| r.avg_ts);
    result.reported_gpu_info = generation_row.and_then(|r| r.gpu_info.clone());

    if backend == Backend::Cpu {
        result.benchmark_verified = true;
        result.status = BackendStatus::Verified;
        return;
    }

    let gpu_info_present = result
        .reported_gpu_info
        .as_deref()
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);

    if gpu_info_present {
        result.benchmark_verified = true;
        result.status = BackendStatus::Verified;
    } else {
        result.benchmark_verified = false;
        result.status = BackendStatus::BenchmarkFailed;
        result.failure_reason = Some(format!(
            "{backend:?} verification ran and exited cleanly, but llama-bench reported no GPU device in use (gpu_info empty) - refusing to report GPU success on a possible silent CPU fallback (n_gpu_layers alone is not trusted as evidence: it can echo the requested flag even on a binary with no {backend:?} support compiled in)"
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware::HardwareField;

    fn profile_with(cuda: Option<bool>, vulkan: Option<bool>) -> HardwareCapabilityProfile {
        let hw = crate::hardware::inspect(None);
        let mut profile = crate::profile::build_profile(&hw, "2026-01-01T00:00:00Z".to_string(), 0);
        profile.backends.cuda = match cuda {
            Some(v) => HardwareField::measured(v, "test"),
            None => HardwareField::unavailable("test"),
        };
        profile.backends.vulkan = match vulkan {
            Some(v) => HardwareField::measured(v, "test"),
            None => HardwareField::unavailable("test"),
        };
        profile
    }

    #[test]
    fn cpu_is_always_detected() {
        let profile = profile_with(None, None);
        let result = verify_backend(
            Backend::Cpu,
            None,
            Path::new("model.gguf"),
            &profile,
            true,
            Duration::from_secs(1),
        );
        assert!(result.detected);
    }

    #[test]
    fn undetected_gpu_backend_is_unavailable_not_unknown() {
        let profile = profile_with(Some(false), Some(false));
        let result = verify_backend(
            Backend::Cuda,
            None,
            Path::new("model.gguf"),
            &profile,
            true,
            Duration::from_secs(1),
        );
        assert_eq!(result.status, BackendStatus::Unavailable);
        assert!(!result.detected);
    }

    #[test]
    fn unknown_gpu_detection_state_is_unknown_not_unavailable() {
        let profile = profile_with(None, None);
        let result = verify_backend(
            Backend::Cuda,
            None,
            Path::new("model.gguf"),
            &profile,
            true,
            Duration::from_secs(1),
        );
        assert_eq!(result.status, BackendStatus::Unknown);
    }

    #[test]
    fn detected_but_no_binary_dir_stops_at_detected_only() {
        let profile = profile_with(Some(true), None);
        let result = verify_backend(
            Backend::Cuda,
            None,
            Path::new("model.gguf"),
            &profile,
            true,
            Duration::from_secs(1),
        );
        assert_eq!(result.status, BackendStatus::DetectedOnly);
        assert!(result.detected);
        assert!(!result.binary_available);
    }

    #[test]
    fn missing_binary_directory_reports_binary_missing() {
        let profile = profile_with(Some(true), None);
        let missing_dir = Path::new(r"C:\nonexistent\llama-bin-dir");
        let result = verify_backend(
            Backend::Cuda,
            Some(missing_dir),
            Path::new("model.gguf"),
            &profile,
            true,
            Duration::from_secs(1),
        );
        assert_eq!(result.status, BackendStatus::BinaryMissing);
        assert!(!result.binary_available);
    }

    #[test]
    fn undetected_vulkan_backend_is_unavailable() {
        let profile = profile_with(Some(false), Some(false));
        let result = verify_backend(
            Backend::Vulkan,
            None,
            Path::new("model.gguf"),
            &profile,
            true,
            Duration::from_secs(1),
        );
        assert_eq!(result.status, BackendStatus::Unavailable);
        assert!(!result.detected);
    }

    /// A real directory that exists but doesn't contain llama-cli.exe/
    /// llama-bench.exe (distinct from `missing_binary_directory_reports_binary_missing`,
    /// which covers the directory itself not existing at all).
    #[test]
    fn binary_directory_exists_but_files_are_absent_reports_binary_missing() {
        let dir = std::env::temp_dir().join(format!(
            "brute-backends-test-empty-dir-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();

        let profile = profile_with(Some(true), None);
        let result = verify_backend(
            Backend::Cuda,
            Some(&dir),
            Path::new("model.gguf"),
            &profile,
            true,
            Duration::from_secs(1),
        );

        assert_eq!(result.status, BackendStatus::BinaryMissing);
        assert!(!result.binary_available);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn cpu_backend_bypasses_the_gpu_fallback_check() {
        let mut result = BackendVerification {
            backend: Backend::Cpu,
            detected: true,
            binary_available: true,
            launch_verified: true,
            model_load_verified: true,
            benchmark_verified: false,
            status: BackendStatus::Unknown,
            failure_reason: None,
            verification_tokens_per_second: None,
            reported_gpu_info: None,
        };
        let rows = vec![BenchRow {
            test: "tg".to_string(),
            n_prompt: 0,
            n_gen: 8,
            avg_ts: 40.0,
            stddev_ts: 1.0,
            samples_ts: vec![40.0],
            model_n_params: None,
            model_size_bytes: None,
            cpu_info: None,
            gpu_info: None,
            n_threads: None,
            n_gpu_layers: Some(0),
        }];
        check_no_silent_fallback(Backend::Cpu, &rows, &mut result);
        assert_eq!(result.status, BackendStatus::Verified);
        assert!(result.benchmark_verified);
    }

    #[test]
    fn gpu_backend_with_empty_gpu_info_is_never_reported_as_verified() {
        let mut result = BackendVerification {
            backend: Backend::Cuda,
            detected: true,
            binary_available: true,
            launch_verified: true,
            model_load_verified: true,
            benchmark_verified: false,
            status: BackendStatus::Unknown,
            failure_reason: None,
            verification_tokens_per_second: None,
            reported_gpu_info: None,
        };
        let rows = vec![BenchRow {
            test: "tg".to_string(),
            n_prompt: 0,
            n_gen: 8,
            avg_ts: 40.0,
            stddev_ts: 1.0,
            samples_ts: vec![40.0],
            model_n_params: None,
            model_size_bytes: None,
            cpu_info: None,
            gpu_info: Some(String::new()), // reported, but empty - no GPU actually used
            n_threads: None,
            n_gpu_layers: Some(0),
        }];
        check_no_silent_fallback(Backend::Cuda, &rows, &mut result);
        assert_ne!(result.status, BackendStatus::Verified);
        assert!(!result.benchmark_verified);
        assert!(result.failure_reason.is_some());
    }

    /// Regression test for a real false positive found via live testing:
    /// `brute backends verify --backend cuda` against this repo's pinned
    /// CPU-only b10064 binary returned `n_gpu_layers: 1` (an echo of the
    /// requested `-ngl 1` flag) with `gpu_info` empty - `n_gpu_layers`
    /// alone must never be trusted as GPU-use evidence.
    #[test]
    fn gpu_backend_with_echoed_n_gpu_layers_but_empty_gpu_info_is_not_verified() {
        let mut result = BackendVerification {
            backend: Backend::Cuda,
            detected: true,
            binary_available: true,
            launch_verified: true,
            model_load_verified: true,
            benchmark_verified: false,
            status: BackendStatus::Unknown,
            failure_reason: None,
            verification_tokens_per_second: None,
            reported_gpu_info: None,
        };
        let rows = vec![BenchRow {
            test: "tg".to_string(),
            n_prompt: 0,
            n_gen: 8,
            avg_ts: 74.69,
            stddev_ts: 1.0,
            samples_ts: vec![74.69],
            model_n_params: None,
            model_size_bytes: None,
            cpu_info: None,
            gpu_info: Some(String::new()),
            n_threads: None,
            // The requested -ngl value echoed back, not confirmed usage.
            n_gpu_layers: Some(1),
        }];
        check_no_silent_fallback(Backend::Cuda, &rows, &mut result);
        assert_ne!(result.status, BackendStatus::Verified);
        assert!(!result.benchmark_verified);
        assert!(result.failure_reason.is_some());
    }

    #[test]
    fn gpu_backend_with_real_gpu_info_is_verified() {
        let mut result = BackendVerification {
            backend: Backend::Cuda,
            detected: true,
            binary_available: true,
            launch_verified: true,
            model_load_verified: true,
            benchmark_verified: false,
            status: BackendStatus::Unknown,
            failure_reason: None,
            verification_tokens_per_second: None,
            reported_gpu_info: None,
        };
        let rows = vec![BenchRow {
            test: "tg".to_string(),
            n_prompt: 0,
            n_gen: 8,
            avg_ts: 90.0,
            stddev_ts: 2.0,
            samples_ts: vec![90.0],
            model_n_params: None,
            model_size_bytes: None,
            cpu_info: None,
            gpu_info: Some("NVIDIA GeForce RTX 5050 Laptop GPU".to_string()),
            n_threads: None,
            n_gpu_layers: Some(1),
        }];
        check_no_silent_fallback(Backend::Cuda, &rows, &mut result);
        assert_eq!(result.status, BackendStatus::Verified);
        assert!(result.benchmark_verified);
        assert_eq!(result.verification_tokens_per_second, Some(90.0));
    }

    #[test]
    fn empty_bench_rows_is_a_benchmark_failure_not_a_panic() {
        let mut result = BackendVerification {
            backend: Backend::Cpu,
            detected: true,
            binary_available: true,
            launch_verified: true,
            model_load_verified: true,
            benchmark_verified: false,
            status: BackendStatus::Unknown,
            failure_reason: None,
            verification_tokens_per_second: None,
            reported_gpu_info: None,
        };
        check_no_silent_fallback(Backend::Cpu, &[], &mut result);
        assert_eq!(result.status, BackendStatus::BenchmarkFailed);
    }
}
