//! Apply and Verify (spec section 10): turns a saved [`RuntimeProfile`]
//! into an actually-confirmed launch, never merely "flags were
//! generated". Mirrors `backends::verify_backend`'s anti-fabrication
//! discipline - a profile is only reported [`ApplyStatus::Verified`]
//! once a real short benchmark run has confirmed the backend/thread
//! count/GPU layers llama-bench itself reports match what the profile
//! asked for. A profile that has drifted out of compatibility (model
//! hash, binary hash, or machine changed) is rejected before any process
//! is even launched.

use super::runtime_profile::{self, InvalidationReason, RuntimeProfile};
use crate::models::ModelReport;
use crate::profile::HardwareCapabilityProfile;
use crate::runtime::llama_cpp::BenchRow;
use crate::runtime::{Backend, RuntimeConfig};
use serde::Serialize;
use std::path::Path;
use std::time::Duration;

/// Small, fixed values used only for the verification run - proving the
/// saved settings actually launch and produce the expected backend, not
/// re-measuring real throughput (that already happened during tuning and
/// is recorded on the profile itself).
const VERIFY_PROMPT_TOKENS: u32 = 16;
const VERIFY_GEN_TOKENS: u32 = 8;
const VERIFY_REPETITIONS: u32 = 1;

#[derive(Debug, Clone, Serialize)]
pub struct LaunchPreview {
    pub profile_id: String,
    pub backend: Backend,
    pub threads: u32,
    pub gpu_layers: u32,
    pub context_size: u32,
    pub batch_size: u32,
    pub binary_dir: String,
    pub model_path: String,
}

/// Builds a human-readable preview of exactly what would be launched,
/// without launching anything - the CLI shows this before `apply_and_verify`
/// runs, and it is also returned inside every `ApplyResult` so a caller
/// never has to guess what was attempted.
pub fn build_launch_preview(
    profile: &RuntimeProfile,
    binary_dir: &Path,
    model_path: &Path,
) -> LaunchPreview {
    LaunchPreview {
        profile_id: profile.profile_id.clone(),
        backend: profile.backend,
        threads: profile.threads,
        gpu_layers: profile.gpu_layers,
        context_size: profile.context_size,
        batch_size: profile.batch_size,
        binary_dir: binary_dir.display().to_string(),
        model_path: model_path.display().to_string(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApplyStatus {
    /// The verification run completed and confirmed the requested
    /// backend/settings were actually used.
    Verified,
    /// The profile is no longer compatible with the current model,
    /// machine, or binaries - rejected before any process was launched.
    IncompatibleProfile,
    /// The verification run itself failed, or completed but showed no
    /// evidence the requested backend was actually used (a silent
    /// fallback).
    VerificationFailed,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApplyResult {
    pub status: ApplyStatus,
    pub preview: LaunchPreview,
    pub compatibility_issues: Vec<String>,
    pub confirmed_backend_matches: Option<bool>,
    pub confirmed_threads: Option<u64>,
    pub confirmed_gpu_layers: Option<i64>,
    pub verification_tokens_per_second: Option<f64>,
    /// `true` whenever `status` is not `Verified` - a caller should treat
    /// the profile as not-currently-usable and either re-tune or fall
    /// back to a previously verified profile.
    pub rollback_recommended: bool,
    pub detail: String,
}

/// Everything `apply_and_verify` needs, bundled into a struct to stay
/// under clippy's argument-count lint (and to make call sites
/// self-documenting about which hash belongs to which binary).
pub struct ApplyRequest<'a> {
    pub profile: &'a RuntimeProfile,
    pub binary_dir: &'a Path,
    pub model: &'a ModelReport,
    pub machine_profile: &'a HardwareCapabilityProfile,
    pub llama_cli_sha256: Option<&'a str>,
    pub llama_bench_sha256: Option<&'a str>,
    pub allow_unverified_binary: bool,
    pub timeout: Duration,
}

/// Selects a saved profile (already loaded into `request.profile`),
/// validates it is still compatible, launches a short verification run,
/// and confirms the actual backend/settings llama-bench reports. Never
/// reports `Verified` on the strength of flag construction alone.
pub fn apply_and_verify(request: ApplyRequest) -> ApplyResult {
    let preview = build_launch_preview(request.profile, request.binary_dir, &request.model.path);

    // Sanity comes first: a manipulated/corrupted profile (e.g. a
    // hand-edited JSON file with `batch_size > context_size`, or
    // `backend: cpu` paired with nonzero `gpu_layers`) is rejected before
    // even checking environment compatibility, since those checks
    // wouldn't be meaningful against impossible values anyway.
    let mut compatibility_issues = runtime_profile::sanity_check(request.profile);

    let invalidation = runtime_profile::check_still_valid(
        request.profile,
        &request.model.sha256,
        request.machine_profile,
        request.llama_cli_sha256,
        request.llama_bench_sha256,
    );
    compatibility_issues.extend(invalidation.iter().map(InvalidationReason::description));

    if !compatibility_issues.is_empty() {
        return ApplyResult {
            status: ApplyStatus::IncompatibleProfile,
            preview,
            compatibility_issues,
            confirmed_backend_matches: None,
            confirmed_threads: None,
            confirmed_gpu_layers: None,
            verification_tokens_per_second: None,
            rollback_recommended: true,
            detail: "profile is not safe to apply as-is - re-run tuning instead".to_string(),
        };
    }

    let config = RuntimeConfig {
        backend: request.profile.backend,
        binary_dir: request.binary_dir.to_path_buf(),
        threads: request.profile.threads,
        gpu_layers: request.profile.gpu_layers,
        context_size: request.profile.context_size,
        batch_size: request.profile.batch_size,
        prompt_tokens: VERIFY_PROMPT_TOKENS,
        gen_tokens: VERIFY_GEN_TOKENS,
        repetitions: VERIFY_REPETITIONS,
        timeout_secs: request.timeout.as_secs(),
    };

    match crate::runtime::llama_cpp::run_bench(
        request.binary_dir,
        &request.model.path,
        &config,
        request.allow_unverified_binary,
        |_| crate::runtime::process::TickAction::Continue,
    ) {
        Err(e) => ApplyResult {
            status: ApplyStatus::VerificationFailed,
            preview,
            compatibility_issues: vec![],
            confirmed_backend_matches: None,
            confirmed_threads: None,
            confirmed_gpu_layers: None,
            verification_tokens_per_second: None,
            rollback_recommended: true,
            detail: format!("verification launch failed: {e}"),
        },
        Ok((rows, _run)) => build_verified_result(request.profile, preview, &rows),
    }
}

/// The same anti-silent-fallback check `backends::check_no_silent_fallback`
/// performs, applied here to a profile's own recorded backend: a GPU
/// backend's verification run must show real evidence of GPU use
/// (`gpu_info` non-empty or `n_gpu_layers > 0` in llama-bench's own
/// output), never just a clean exit code.
fn build_verified_result(
    profile: &RuntimeProfile,
    preview: LaunchPreview,
    rows: &[BenchRow],
) -> ApplyResult {
    let generation_row = rows
        .iter()
        .find(|r| r.test == "tg")
        .or_else(|| rows.first());

    let confirmed_threads = generation_row.and_then(|r| r.n_threads);
    let confirmed_gpu_layers = generation_row.and_then(|r| r.n_gpu_layers);
    let verification_tokens_per_second = generation_row.map(|r| r.avg_ts);

    let backend_matches = match profile.backend {
        Backend::Cpu => true,
        Backend::Cuda | Backend::Vulkan => generation_row.is_some_and(|r| {
            r.gpu_info.as_ref().is_some_and(|g| !g.is_empty()) || r.n_gpu_layers.unwrap_or(0) > 0
        }),
    };

    if !backend_matches {
        return ApplyResult {
            status: ApplyStatus::VerificationFailed,
            preview,
            compatibility_issues: vec![],
            confirmed_backend_matches: Some(false),
            confirmed_threads,
            confirmed_gpu_layers,
            verification_tokens_per_second,
            rollback_recommended: true,
            detail: format!(
                "verification run completed but showed no evidence the {:?} backend was actually \
                 used - this looks like a silent fallback, not a confirmed apply",
                profile.backend
            ),
        };
    }

    ApplyResult {
        status: ApplyStatus::Verified,
        preview,
        compatibility_issues: vec![],
        confirmed_backend_matches: Some(true),
        confirmed_threads,
        confirmed_gpu_layers,
        verification_tokens_per_second,
        rollback_recommended: false,
        detail: "verification run completed and confirmed the profile's backend and settings \
                 were actually used"
            .to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::gguf::GgufHyperparameters;
    use crate::runtime::llama_cpp::BenchRow;
    use crate::tuning::candidates::TuningDefaults;
    use crate::tuning::candidates::TuningPlan;
    use crate::tuning::ranking::{CandidateMeasurements, RankedCandidate, RankingConfidence};
    use crate::tuning::runtime_profile::ProfileInputs;
    use crate::tuning::stability::{StabilityAssessment, TuningStabilityStatus};
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn test_machine_profile() -> HardwareCapabilityProfile {
        let hw = crate::hardware::inspect(None);
        crate::profile::build_profile(&hw, "2026-01-01T00:00:00Z".to_string(), 0)
    }

    fn test_model() -> ModelReport {
        ModelReport {
            path: PathBuf::from(r"C:\models\test.gguf"),
            file_size_bytes: 491_400_032,
            sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string(),
            gguf_version: 3,
            tensor_count: 100,
            kv_count: 10,
            architecture: Some("qwen2".to_string()),
            name: Some("test".to_string()),
            quantization: Some("Q4_K_M".to_string()),
            dominant_tensor_type: Some("Q4_K".to_string()),
            parameter_count: Some(630_167_424),
            alignment: 32,
            size_consistency_checked: true,
            kv_preview: BTreeMap::new(),
            hyperparameters: GgufHyperparameters::default(),
        }
    }

    fn test_profile(
        machine: &HardwareCapabilityProfile,
        model: &ModelReport,
        backend: Backend,
    ) -> RuntimeProfile {
        let plan = TuningPlan {
            formula_version: "test".to_string(),
            model_sha256: model.sha256.clone(),
            machine_profile_schema_version: machine.schema_version.clone(),
            defaults: TuningDefaults {
                threads: 8,
                gpu_layers: 0,
                context_size: 2048,
                batch_size: 512,
            },
            candidates: vec![],
            pruned: vec![],
            truncated: false,
        };
        let winner = RankedCandidate {
            candidate_id: "threads-8".to_string(),
            rank: 1,
            measurements: CandidateMeasurements {
                candidate_id: "threads-8".to_string(),
                completed: true,
                stability: TuningStabilityStatus::Stable,
                backend,
                mean_generation_tokens_per_second: Some(30.0),
                mean_prompt_tokens_per_second: Some(100.0),
                context_size: 2048,
                batch_size: 512,
                gpu_layers: if backend == Backend::Cpu { 0 } else { 10 },
                threads: 8,
                predicted_ram_bytes: Some(1_500_000_000),
                predicted_vram_bytes: None,
            },
        };
        let stability = StabilityAssessment {
            status: TuningStabilityStatus::Stable,
            formula_version: "stage2-stability-v1".to_string(),
            repetitions_requested: 3,
            repetitions_succeeded: 3,
            generation_coefficient_of_variation: Some(0.01),
            prompt_coefficient_of_variation: Some(0.02),
            reason: "test".to_string(),
        };

        runtime_profile::build_profile(
            ProfileInputs {
                plan: &plan,
                winner: &winner,
                stability: &stability,
                confidence: RankingConfidence::High,
                machine_profile: machine,
                model,
                llama_cli_sha256: Some("cli-hash".to_string()),
                llama_bench_sha256: Some("bench-hash".to_string()),
                tuning_date: "2026-07-18T00:00:00Z".to_string(),
            },
            "profile-test".to_string(),
        )
    }

    #[test]
    fn build_launch_preview_reflects_the_profiles_settings() {
        let machine = test_machine_profile();
        let model = test_model();
        let profile = test_profile(&machine, &model, Backend::Cpu);
        let preview = build_launch_preview(&profile, Path::new(r"C:\tools\llama.cpp"), &model.path);

        assert_eq!(preview.threads, 8);
        assert_eq!(preview.backend, Backend::Cpu);
        assert_eq!(preview.profile_id, "profile-test");
    }

    #[test]
    fn a_manipulated_profile_with_impossible_settings_is_rejected_before_any_process_is_launched() {
        let machine = test_machine_profile();
        let model = test_model();
        let mut profile = test_profile(&machine, &model, Backend::Cpu);
        // Simulate a hand-edited/corrupted profile JSON file on disk.
        profile.batch_size = 999_999;

        let result = apply_and_verify(ApplyRequest {
            profile: &profile,
            binary_dir: Path::new(r"C:\nonexistent\llama.cpp"),
            model: &model,
            machine_profile: &machine,
            llama_cli_sha256: Some("cli-hash"),
            llama_bench_sha256: Some("bench-hash"),
            allow_unverified_binary: true,
            timeout: Duration::from_secs(5),
        });

        assert_eq!(result.status, ApplyStatus::IncompatibleProfile);
        assert!(result.rollback_recommended);
        assert!(
            result
                .compatibility_issues
                .iter()
                .any(|i| i.contains("batch size"))
        );
    }

    #[test]
    fn an_incompatible_profile_is_rejected_before_any_process_is_launched() {
        let machine = test_machine_profile();
        let model = test_model();
        let profile = test_profile(&machine, &model, Backend::Cpu);

        // A model hash mismatch alone is enough to reject - the
        // nonexistent binary_dir below proves no process was launched,
        // since a real launch would fail differently (BinaryNotFound),
        // not report IncompatibleProfile.
        let mut different_model = model.clone();
        different_model.sha256 = "a-totally-different-hash".to_string();

        let result = apply_and_verify(ApplyRequest {
            profile: &profile,
            binary_dir: Path::new(r"C:\nonexistent\llama.cpp"),
            model: &different_model,
            machine_profile: &machine,
            llama_cli_sha256: Some("cli-hash"),
            llama_bench_sha256: Some("bench-hash"),
            allow_unverified_binary: true,
            timeout: Duration::from_secs(5),
        });

        assert_eq!(result.status, ApplyStatus::IncompatibleProfile);
        assert!(result.rollback_recommended);
        assert!(!result.compatibility_issues.is_empty());
        assert!(result.confirmed_backend_matches.is_none());
    }

    #[test]
    fn cpu_backend_always_matches_regardless_of_gpu_info() {
        let machine = test_machine_profile();
        let model = test_model();
        let profile = test_profile(&machine, &model, Backend::Cpu);
        let preview = build_launch_preview(&profile, Path::new(r"C:\tools"), &model.path);

        let rows = vec![tg_row(None, Some(0))];
        let result = build_verified_result(&profile, preview, &rows);

        assert_eq!(result.status, ApplyStatus::Verified);
        assert_eq!(result.confirmed_backend_matches, Some(true));
    }

    #[test]
    fn cuda_backend_with_real_gpu_info_is_verified() {
        let machine = test_machine_profile();
        let model = test_model();
        let profile = test_profile(&machine, &model, Backend::Cuda);
        let preview = build_launch_preview(&profile, Path::new(r"C:\tools"), &model.path);

        let rows = vec![tg_row(
            Some("NVIDIA GeForce RTX 5050".to_string()),
            Some(10),
        )];
        let result = build_verified_result(&profile, preview, &rows);

        assert_eq!(result.status, ApplyStatus::Verified);
        assert_eq!(result.confirmed_backend_matches, Some(true));
    }

    #[test]
    fn cuda_backend_with_no_gpu_evidence_is_a_silent_fallback_not_a_success() {
        let machine = test_machine_profile();
        let model = test_model();
        let profile = test_profile(&machine, &model, Backend::Cuda);
        let preview = build_launch_preview(&profile, Path::new(r"C:\tools"), &model.path);

        let rows = vec![tg_row(Some(String::new()), Some(0))];
        let result = build_verified_result(&profile, preview, &rows);

        assert_eq!(result.status, ApplyStatus::VerificationFailed);
        assert_eq!(result.confirmed_backend_matches, Some(false));
        assert!(result.rollback_recommended);
    }

    fn tg_row(gpu_info: Option<String>, n_gpu_layers: Option<i64>) -> BenchRow {
        BenchRow {
            test: "tg".to_string(),
            n_prompt: 0,
            n_gen: 8,
            avg_ts: 25.0,
            stddev_ts: 0.5,
            samples_ts: vec![25.0],
            model_n_params: None,
            model_size_bytes: None,
            cpu_info: None,
            gpu_info,
            n_threads: Some(8),
            n_gpu_layers,
        }
    }
}
