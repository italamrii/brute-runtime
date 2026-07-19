//! Local Runtime Profiles (spec section 9): what a `tuning::ranking`
//! winning configuration becomes once saved to disk, plus the
//! invalidation logic that decides when a saved profile can no longer be
//! trusted without re-verification.
//!
//! Profiles live under `%LOCALAPPDATA%\BruteRuntime\profiles\<id>.json` -
//! outside the git-tracked repository, never uploaded anywhere (see
//! `docs/privacy-model.md`). A profile identifies the model by its
//! content hash (never its file path - no personal path is ever stored),
//! and identifies the llama.cpp binaries by their own sha256 (not a
//! free-text `--version` string, which varies across builds and forks
//! and is not a reliable equality check). Any of the model hash, the
//! binary hashes, the machine profile's schema version, the machine
//! identity, or this schema's own version changing invalidates the
//! profile - see [`check_still_valid`].

use crate::models::ModelReport;
use crate::profile::HardwareCapabilityProfile;
use crate::runtime::Backend;
use crate::tuning::candidates::TuningPlan;
use crate::tuning::ranking::{RankedCandidate, RankingConfidence};
use crate::tuning::stability::{StabilityAssessment, TuningStabilityStatus};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub const PROFILE_SCHEMA_VERSION: &str = "stage2-profile-v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeProfile {
    pub schema_version: String,
    pub profile_id: String,
    pub tuning_date: String,

    pub model_sha256: String,
    pub model_architecture: Option<String>,
    pub model_quantization: Option<String>,
    pub model_parameter_count: Option<u64>,

    pub machine_profile_schema_version: String,
    /// The coarse Stage 1 `machine_id` - present here for local
    /// invalidation checks only. [`sanitize_for_export`] replaces this
    /// with [`crate::profile::REDACTED_MACHINE_ID_PLACEHOLDER`] before
    /// the profile is ever written to a shareable export.
    pub machine_id: String,

    pub backend: Backend,
    pub llama_cli_sha256: Option<String>,
    pub llama_bench_sha256: Option<String>,

    pub threads: u32,
    pub gpu_layers: u32,
    pub context_size: u32,
    pub batch_size: u32,

    pub mean_generation_tokens_per_second: Option<f64>,
    pub mean_prompt_tokens_per_second: Option<f64>,
    pub predicted_ram_bytes: Option<u64>,
    pub predicted_vram_bytes: Option<u64>,

    pub stability: TuningStabilityStatus,
    pub stability_formula_version: String,
    pub ranking_formula_version: String,
    pub tuning_formula_version: String,
    pub confidence: RankingConfidence,

    /// How many of the source repetitions behind this profile's winning
    /// candidate actually succeeded, out of how many were requested - so
    /// a profile never implies more measurement happened than it did.
    pub source_repetitions_succeeded: u32,
    pub source_repetitions_requested: u32,
}

/// Everything needed to build a [`RuntimeProfile`] from a completed
/// tuning run's winning candidate. Bundled into a struct rather than
/// passed as individual parameters both for readability and to keep the
/// constructor under clippy's argument-count lint.
pub struct ProfileInputs<'a> {
    pub plan: &'a TuningPlan,
    pub winner: &'a RankedCandidate,
    pub stability: &'a StabilityAssessment,
    pub confidence: RankingConfidence,
    pub machine_profile: &'a HardwareCapabilityProfile,
    pub model: &'a ModelReport,
    pub llama_cli_sha256: Option<String>,
    pub llama_bench_sha256: Option<String>,
    pub tuning_date: String,
}

pub fn build_profile(inputs: ProfileInputs, profile_id: String) -> RuntimeProfile {
    let m = &inputs.winner.measurements;
    RuntimeProfile {
        schema_version: PROFILE_SCHEMA_VERSION.to_string(),
        profile_id,
        tuning_date: inputs.tuning_date,
        model_sha256: inputs.model.sha256.clone(),
        model_architecture: inputs.model.architecture.clone(),
        model_quantization: inputs.model.quantization.clone(),
        model_parameter_count: inputs.model.parameter_count,
        machine_profile_schema_version: inputs.machine_profile.schema_version.clone(),
        machine_id: inputs.machine_profile.machine_id.clone(),
        backend: m.backend,
        llama_cli_sha256: inputs.llama_cli_sha256,
        llama_bench_sha256: inputs.llama_bench_sha256,
        threads: m.threads,
        gpu_layers: m.gpu_layers,
        context_size: m.context_size,
        batch_size: m.batch_size,
        mean_generation_tokens_per_second: m.mean_generation_tokens_per_second,
        mean_prompt_tokens_per_second: m.mean_prompt_tokens_per_second,
        predicted_ram_bytes: m.predicted_ram_bytes,
        predicted_vram_bytes: m.predicted_vram_bytes,
        stability: inputs.stability.status,
        stability_formula_version: inputs.stability.formula_version.clone(),
        ranking_formula_version: crate::tuning::ranking::TUNING_RANKING_FORMULA_VERSION.to_string(),
        tuning_formula_version: inputs.plan.formula_version.clone(),
        confidence: inputs.confidence,
        source_repetitions_succeeded: inputs.stability.repetitions_succeeded,
        source_repetitions_requested: inputs.stability.repetitions_requested,
    }
}

/// Returns a copy of `profile` safe to write to a shareable export - the
/// only change is redacting `machine_id`, mirroring
/// `profile::redact_machine_id_for_export`. Nothing else in
/// `RuntimeProfile` is personally identifying: no file paths, no
/// usernames, only a content hash for the model and sha256 hashes for
/// the binaries.
pub fn sanitize_for_export(profile: &RuntimeProfile) -> RuntimeProfile {
    let mut sanitized = profile.clone();
    sanitized.machine_id = crate::profile::REDACTED_MACHINE_ID_PLACEHOLDER.to_string();
    sanitized
}

#[derive(Debug, Clone, Serialize)]
pub enum InvalidationReason {
    ModelHashMismatch {
        expected: String,
        actual: String,
    },
    BinaryHashMismatch {
        which: String,
        expected: String,
        actual: String,
    },
    MachineProfileSchemaMismatch {
        expected: String,
        actual: String,
    },
    MachineIdMismatch {
        expected: String,
        actual: String,
    },
    ProfileSchemaOutdated {
        expected: String,
        actual: String,
    },
}

impl InvalidationReason {
    pub fn description(&self) -> String {
        match self {
            InvalidationReason::ModelHashMismatch { expected, actual } => format!(
                "model file hash changed - profile was tuned for {expected}, current model is {actual}"
            ),
            InvalidationReason::BinaryHashMismatch {
                which,
                expected,
                actual,
            } => format!(
                "{which} changed - profile was tuned against sha256 {expected}, current binary is {actual}"
            ),
            InvalidationReason::MachineProfileSchemaMismatch { expected, actual } => {
                format!("machine profile schema changed from {expected} to {actual}")
            }
            InvalidationReason::MachineIdMismatch { expected, actual } => format!(
                "machine identity changed (coarse hardware hash {expected} -> {actual}) - this profile was tuned on a different machine"
            ),
            InvalidationReason::ProfileSchemaOutdated { expected, actual } => {
                format!("runtime profile schema changed from {expected} to {actual}")
            }
        }
    }
}

/// Checks whether a saved profile is still trustworthy for the given
/// current model/machine/binaries. An empty result means the profile is
/// still valid; any entry means it must be re-verified (or re-tuned)
/// before being applied - see `tuning::apply`.
pub fn check_still_valid(
    profile: &RuntimeProfile,
    current_model_sha256: &str,
    current_machine_profile: &HardwareCapabilityProfile,
    current_llama_cli_sha256: Option<&str>,
    current_llama_bench_sha256: Option<&str>,
) -> Vec<InvalidationReason> {
    let mut reasons = Vec::new();

    if profile.schema_version != PROFILE_SCHEMA_VERSION {
        reasons.push(InvalidationReason::ProfileSchemaOutdated {
            expected: PROFILE_SCHEMA_VERSION.to_string(),
            actual: profile.schema_version.clone(),
        });
    }

    if profile.model_sha256 != current_model_sha256 {
        reasons.push(InvalidationReason::ModelHashMismatch {
            expected: profile.model_sha256.clone(),
            actual: current_model_sha256.to_string(),
        });
    }

    if profile.machine_profile_schema_version != current_machine_profile.schema_version {
        reasons.push(InvalidationReason::MachineProfileSchemaMismatch {
            expected: profile.machine_profile_schema_version.clone(),
            actual: current_machine_profile.schema_version.clone(),
        });
    }

    if profile.machine_id != current_machine_profile.machine_id {
        reasons.push(InvalidationReason::MachineIdMismatch {
            expected: profile.machine_id.clone(),
            actual: current_machine_profile.machine_id.clone(),
        });
    }

    if let (Some(expected), Some(actual)) = (&profile.llama_cli_sha256, current_llama_cli_sha256)
        && expected != actual
    {
        reasons.push(InvalidationReason::BinaryHashMismatch {
            which: format!("llama-cli{}", std::env::consts::EXE_SUFFIX),
            expected: expected.clone(),
            actual: actual.to_string(),
        });
    }

    if let (Some(expected), Some(actual)) =
        (&profile.llama_bench_sha256, current_llama_bench_sha256)
        && expected != actual
    {
        reasons.push(InvalidationReason::BinaryHashMismatch {
            which: format!("llama-bench{}", std::env::consts::EXE_SUFFIX),
            expected: expected.clone(),
            actual: actual.to_string(),
        });
    }

    reasons
}

/// A generous but real ceiling - no supported machine has anywhere near
/// this many logical threads; a profile claiming more is either
/// corrupted or hand-edited, not a value this codebase ever produced.
const MAX_PLAUSIBLE_THREADS: u32 = 4096;

/// Rejects a profile with numerically impossible or internally
/// inconsistent settings (spec section 13: "reject manipulated numeric
/// values/impossible settings"). This defends `profiles show`/
/// `profiles verify` against a hand-edited or corrupted profile JSON
/// file on disk - distinct from [`check_still_valid`], which checks
/// *compatibility* with the current environment, not internal sanity. An
/// empty result means the profile's values are internally plausible;
/// it says nothing about whether they will actually perform well.
pub fn sanity_check(profile: &RuntimeProfile) -> Vec<String> {
    let mut issues = Vec::new();

    if profile.threads == 0 {
        issues.push("thread count is zero".to_string());
    }
    if profile.threads > MAX_PLAUSIBLE_THREADS {
        issues.push(format!(
            "thread count {} exceeds a plausible maximum of {MAX_PLAUSIBLE_THREADS}",
            profile.threads
        ));
    }
    if profile.context_size == 0 {
        issues.push("context size is zero".to_string());
    }
    if profile.batch_size == 0 {
        issues.push("batch size is zero".to_string());
    }
    if profile.batch_size > profile.context_size {
        issues.push(format!(
            "batch size {} exceeds context size {} - not a configuration this codebase ever produces",
            profile.batch_size, profile.context_size
        ));
    }
    if profile.backend == Backend::Cpu && profile.gpu_layers != 0 {
        issues.push(format!(
            "backend is cpu but gpu_layers is {} - impossible combination",
            profile.gpu_layers
        ));
    }
    if profile.model_sha256.len() != 64
        || !profile.model_sha256.chars().all(|c| c.is_ascii_hexdigit())
    {
        issues.push("model_sha256 is not a well-formed 64-character hex sha256".to_string());
    }

    issues
}

/// `%LOCALAPPDATA%\BruteRuntime\profiles\` - where saved runtime profiles
/// live. See `identity::default_local_state_dir` for the parent.
pub fn default_profiles_dir() -> PathBuf {
    crate::identity::default_local_state_dir().join("profiles")
}

/// Generates a fresh random profile ID - reuses the same OS-entropy
/// generator as the local instance ID, since a profile ID is likewise a
/// non-identifying local label, not a content hash.
pub fn generate_profile_id() -> String {
    format!("profile-{}", crate::identity::generate_random_id())
}

fn profile_path(dir: &Path, profile_id: &str) -> PathBuf {
    dir.join(format!("{profile_id}.json"))
}

pub fn save_profile_to(dir: &Path, profile: &RuntimeProfile) -> io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let path = profile_path(dir, &profile.profile_id);
    let json = serde_json::to_string_pretty(profile)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    fs::write(&path, json)?;
    Ok(path)
}

pub fn load_profile_from(dir: &Path, profile_id: &str) -> io::Result<RuntimeProfile> {
    let path = profile_path(dir, profile_id);
    let contents = fs::read_to_string(path)?;
    serde_json::from_str(&contents).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Deletes only the local profile metadata file - never touches the
/// model or any runtime binary. Mirrors `library::forget`'s "metadata
/// only, never a file the user brought in" boundary.
pub fn delete_profile_from(dir: &Path, profile_id: &str) -> io::Result<()> {
    fs::remove_file(profile_path(dir, profile_id))
}

/// Lists saved profile IDs, sorted for deterministic output. An empty
/// (or not-yet-created) directory is not an error - it just means no
/// profiles have been saved yet.
pub fn list_profile_ids_in(dir: &Path) -> io::Result<Vec<String>> {
    if !dir.is_dir() {
        return Ok(vec![]);
    }
    let mut ids = Vec::new();
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) == Some("json")
            && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
        {
            ids.push(stem.to_string());
        }
    }
    ids.sort();
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::gguf::GgufHyperparameters;
    use crate::runtime::Backend;
    use crate::tuning::candidates::TuningDefaults;
    use crate::tuning::ranking::{CandidateMeasurements, RankedCandidate};
    use std::collections::BTreeMap;

    fn tmp_dir(name: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "brute-profile-test-{name}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

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

    fn test_plan(model: &ModelReport, machine: &HardwareCapabilityProfile) -> TuningPlan {
        TuningPlan {
            formula_version: "test-tuning-v1".to_string(),
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
        }
    }

    fn test_winner() -> RankedCandidate {
        RankedCandidate {
            candidate_id: "threads-8".to_string(),
            rank: 1,
            measurements: CandidateMeasurements {
                candidate_id: "threads-8".to_string(),
                completed: true,
                stability: TuningStabilityStatus::Stable,
                backend: Backend::Cpu,
                mean_generation_tokens_per_second: Some(30.0),
                mean_prompt_tokens_per_second: Some(100.0),
                context_size: 2048,
                batch_size: 512,
                gpu_layers: 0,
                threads: 8,
                predicted_ram_bytes: Some(1_500_000_000),
                predicted_vram_bytes: None,
            },
        }
    }

    fn test_stability() -> StabilityAssessment {
        StabilityAssessment {
            status: TuningStabilityStatus::Stable,
            formula_version: "stage2-stability-v1".to_string(),
            repetitions_requested: 3,
            repetitions_succeeded: 3,
            generation_coefficient_of_variation: Some(0.01),
            prompt_coefficient_of_variation: Some(0.02),
            reason: "test".to_string(),
        }
    }

    #[test]
    fn build_profile_carries_over_the_winners_measurements() {
        let model = test_model();
        let machine = test_machine_profile();
        let plan = test_plan(&model, &machine);
        let winner = test_winner();
        let stability = test_stability();

        let profile = build_profile(
            ProfileInputs {
                plan: &plan,
                winner: &winner,
                stability: &stability,
                confidence: RankingConfidence::High,
                machine_profile: &machine,
                model: &model,
                llama_cli_sha256: Some("cli-hash".to_string()),
                llama_bench_sha256: Some("bench-hash".to_string()),
                tuning_date: "2026-07-18T00:00:00Z".to_string(),
            },
            "profile-test".to_string(),
        );

        assert_eq!(
            profile.model_sha256,
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        );
        assert_eq!(profile.threads, 8);
        assert_eq!(profile.backend, Backend::Cpu);
        assert_eq!(profile.stability, TuningStabilityStatus::Stable);
        assert_eq!(profile.mean_generation_tokens_per_second, Some(30.0));
        assert_eq!(profile.schema_version, PROFILE_SCHEMA_VERSION);
    }

    #[test]
    fn sanitize_for_export_redacts_machine_id_and_nothing_else() {
        let model = test_model();
        let machine = test_machine_profile();
        let plan = test_plan(&model, &machine);
        let winner = test_winner();
        let stability = test_stability();
        let profile = build_profile(
            ProfileInputs {
                plan: &plan,
                winner: &winner,
                stability: &stability,
                confidence: RankingConfidence::High,
                machine_profile: &machine,
                model: &model,
                llama_cli_sha256: None,
                llama_bench_sha256: None,
                tuning_date: "2026-07-18T00:00:00Z".to_string(),
            },
            "profile-test".to_string(),
        );

        let real_machine_id = profile.machine_id.clone();
        let sanitized = sanitize_for_export(&profile);

        assert_eq!(
            sanitized.machine_id,
            crate::profile::REDACTED_MACHINE_ID_PLACEHOLDER
        );
        assert_ne!(sanitized.machine_id, real_machine_id);
        assert_eq!(sanitized.model_sha256, profile.model_sha256);
        assert_eq!(sanitized.threads, profile.threads);
    }

    #[test]
    fn check_still_valid_reports_no_reasons_when_nothing_changed() {
        let model = test_model();
        let machine = test_machine_profile();
        let plan = test_plan(&model, &machine);
        let winner = test_winner();
        let stability = test_stability();
        let profile = build_profile(
            ProfileInputs {
                plan: &plan,
                winner: &winner,
                stability: &stability,
                confidence: RankingConfidence::High,
                machine_profile: &machine,
                model: &model,
                llama_cli_sha256: Some("cli-hash".to_string()),
                llama_bench_sha256: Some("bench-hash".to_string()),
                tuning_date: "2026-07-18T00:00:00Z".to_string(),
            },
            "profile-test".to_string(),
        );

        let reasons = check_still_valid(
            &profile,
            &model.sha256,
            &machine,
            Some("cli-hash"),
            Some("bench-hash"),
        );
        assert!(reasons.is_empty());
    }

    #[test]
    fn check_still_valid_flags_a_changed_model_hash() {
        let model = test_model();
        let machine = test_machine_profile();
        let plan = test_plan(&model, &machine);
        let winner = test_winner();
        let stability = test_stability();
        let profile = build_profile(
            ProfileInputs {
                plan: &plan,
                winner: &winner,
                stability: &stability,
                confidence: RankingConfidence::High,
                machine_profile: &machine,
                model: &model,
                llama_cli_sha256: None,
                llama_bench_sha256: None,
                tuning_date: "2026-07-18T00:00:00Z".to_string(),
            },
            "profile-test".to_string(),
        );

        let reasons = check_still_valid(&profile, "a-different-hash", &machine, None, None);
        assert!(matches!(
            reasons[0],
            InvalidationReason::ModelHashMismatch { .. }
        ));
    }

    #[test]
    fn check_still_valid_flags_a_changed_binary_hash() {
        let model = test_model();
        let machine = test_machine_profile();
        let plan = test_plan(&model, &machine);
        let winner = test_winner();
        let stability = test_stability();
        let profile = build_profile(
            ProfileInputs {
                plan: &plan,
                winner: &winner,
                stability: &stability,
                confidence: RankingConfidence::High,
                machine_profile: &machine,
                model: &model,
                llama_cli_sha256: Some("cli-hash-old".to_string()),
                llama_bench_sha256: Some("bench-hash".to_string()),
                tuning_date: "2026-07-18T00:00:00Z".to_string(),
            },
            "profile-test".to_string(),
        );

        let reasons = check_still_valid(
            &profile,
            &model.sha256,
            &machine,
            Some("cli-hash-new"),
            Some("bench-hash"),
        );
        assert_eq!(reasons.len(), 1);
        assert!(matches!(
            reasons[0],
            InvalidationReason::BinaryHashMismatch { .. }
        ));
    }

    #[test]
    fn check_still_valid_flags_a_changed_machine_id() {
        let model = test_model();
        let machine = test_machine_profile();
        let plan = test_plan(&model, &machine);
        let winner = test_winner();
        let stability = test_stability();
        let profile = build_profile(
            ProfileInputs {
                plan: &plan,
                winner: &winner,
                stability: &stability,
                confidence: RankingConfidence::High,
                machine_profile: &machine,
                model: &model,
                llama_cli_sha256: None,
                llama_bench_sha256: None,
                tuning_date: "2026-07-18T00:00:00Z".to_string(),
            },
            "profile-test".to_string(),
        );

        let mut different_machine = machine.clone();
        different_machine.machine_id = "totally-different-machine".to_string();

        let reasons = check_still_valid(&profile, &model.sha256, &different_machine, None, None);
        assert!(
            reasons
                .iter()
                .any(|r| matches!(r, InvalidationReason::MachineIdMismatch { .. }))
        );
    }

    #[test]
    fn check_still_valid_flags_a_stale_schema_version() {
        let model = test_model();
        let machine = test_machine_profile();
        let plan = test_plan(&model, &machine);
        let winner = test_winner();
        let stability = test_stability();
        let mut profile = build_profile(
            ProfileInputs {
                plan: &plan,
                winner: &winner,
                stability: &stability,
                confidence: RankingConfidence::High,
                machine_profile: &machine,
                model: &model,
                llama_cli_sha256: None,
                llama_bench_sha256: None,
                tuning_date: "2026-07-18T00:00:00Z".to_string(),
            },
            "profile-test".to_string(),
        );
        profile.schema_version = "stage2-profile-v0-ancient".to_string();

        let reasons = check_still_valid(&profile, &model.sha256, &machine, None, None);
        assert!(
            reasons
                .iter()
                .any(|r| matches!(r, InvalidationReason::ProfileSchemaOutdated { .. }))
        );
    }

    #[test]
    fn sanity_check_passes_a_profile_this_codebase_actually_produces() {
        let model = test_model();
        let machine = test_machine_profile();
        let plan = test_plan(&model, &machine);
        let winner = test_winner();
        let stability = test_stability();
        let profile = build_profile(
            ProfileInputs {
                plan: &plan,
                winner: &winner,
                stability: &stability,
                confidence: RankingConfidence::High,
                machine_profile: &machine,
                model: &model,
                llama_cli_sha256: None,
                llama_bench_sha256: None,
                tuning_date: "2026-07-18T00:00:00Z".to_string(),
            },
            "profile-test".to_string(),
        );
        assert!(sanity_check(&profile).is_empty());
    }

    #[test]
    fn sanity_check_rejects_zero_threads() {
        let model = test_model();
        let machine = test_machine_profile();
        let plan = test_plan(&model, &machine);
        let winner = test_winner();
        let stability = test_stability();
        let mut profile = build_profile(
            ProfileInputs {
                plan: &plan,
                winner: &winner,
                stability: &stability,
                confidence: RankingConfidence::High,
                machine_profile: &machine,
                model: &model,
                llama_cli_sha256: None,
                llama_bench_sha256: None,
                tuning_date: "2026-07-18T00:00:00Z".to_string(),
            },
            "profile-test".to_string(),
        );
        profile.threads = 0;
        let issues = sanity_check(&profile);
        assert!(issues.iter().any(|i| i.contains("thread")));
    }

    #[test]
    fn sanity_check_rejects_batch_larger_than_context() {
        let model = test_model();
        let machine = test_machine_profile();
        let plan = test_plan(&model, &machine);
        let winner = test_winner();
        let stability = test_stability();
        let mut profile = build_profile(
            ProfileInputs {
                plan: &plan,
                winner: &winner,
                stability: &stability,
                confidence: RankingConfidence::High,
                machine_profile: &machine,
                model: &model,
                llama_cli_sha256: None,
                llama_bench_sha256: None,
                tuning_date: "2026-07-18T00:00:00Z".to_string(),
            },
            "profile-test".to_string(),
        );
        profile.context_size = 512;
        profile.batch_size = 1024;
        let issues = sanity_check(&profile);
        assert!(issues.iter().any(|i| i.contains("batch size")));
    }

    #[test]
    fn sanity_check_rejects_cpu_backend_with_nonzero_gpu_layers() {
        let model = test_model();
        let machine = test_machine_profile();
        let plan = test_plan(&model, &machine);
        let winner = test_winner();
        let stability = test_stability();
        let mut profile = build_profile(
            ProfileInputs {
                plan: &plan,
                winner: &winner,
                stability: &stability,
                confidence: RankingConfidence::High,
                machine_profile: &machine,
                model: &model,
                llama_cli_sha256: None,
                llama_bench_sha256: None,
                tuning_date: "2026-07-18T00:00:00Z".to_string(),
            },
            "profile-test".to_string(),
        );
        profile.backend = Backend::Cpu;
        profile.gpu_layers = 40;
        let issues = sanity_check(&profile);
        assert!(issues.iter().any(|i| i.contains("impossible combination")));
    }

    #[test]
    fn sanity_check_rejects_a_malformed_model_hash() {
        let model = test_model();
        let machine = test_machine_profile();
        let plan = test_plan(&model, &machine);
        let winner = test_winner();
        let stability = test_stability();
        let mut profile = build_profile(
            ProfileInputs {
                plan: &plan,
                winner: &winner,
                stability: &stability,
                confidence: RankingConfidence::High,
                machine_profile: &machine,
                model: &model,
                llama_cli_sha256: None,
                llama_bench_sha256: None,
                tuning_date: "2026-07-18T00:00:00Z".to_string(),
            },
            "profile-test".to_string(),
        );
        profile.model_sha256 = "not-a-real-sha256".to_string();
        let issues = sanity_check(&profile);
        assert!(issues.iter().any(|i| i.contains("sha256")));
    }

    #[test]
    fn save_and_load_roundtrip_preserves_the_profile() {
        let dir = tmp_dir("roundtrip");
        let model = test_model();
        let machine = test_machine_profile();
        let plan = test_plan(&model, &machine);
        let winner = test_winner();
        let stability = test_stability();
        let profile = build_profile(
            ProfileInputs {
                plan: &plan,
                winner: &winner,
                stability: &stability,
                confidence: RankingConfidence::High,
                machine_profile: &machine,
                model: &model,
                llama_cli_sha256: Some("cli-hash".to_string()),
                llama_bench_sha256: Some("bench-hash".to_string()),
                tuning_date: "2026-07-18T00:00:00Z".to_string(),
            },
            generate_profile_id(),
        );

        save_profile_to(&dir, &profile).unwrap();
        let loaded = load_profile_from(&dir, &profile.profile_id).unwrap();

        assert_eq!(loaded.profile_id, profile.profile_id);
        assert_eq!(loaded.model_sha256, profile.model_sha256);
        assert_eq!(loaded.threads, profile.threads);
        assert_eq!(loaded.stability, profile.stability);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn list_profile_ids_returns_sorted_ids() {
        let dir = tmp_dir("list");
        let model = test_model();
        let machine = test_machine_profile();
        let plan = test_plan(&model, &machine);
        let stability = test_stability();

        for id in ["profile-c", "profile-a", "profile-b"] {
            let mut winner = test_winner();
            winner.candidate_id = id.to_string();
            let profile = build_profile(
                ProfileInputs {
                    plan: &plan,
                    winner: &winner,
                    stability: &stability,
                    confidence: RankingConfidence::High,
                    machine_profile: &machine,
                    model: &model,
                    llama_cli_sha256: None,
                    llama_bench_sha256: None,
                    tuning_date: "2026-07-18T00:00:00Z".to_string(),
                },
                id.to_string(),
            );
            save_profile_to(&dir, &profile).unwrap();
        }

        let ids = list_profile_ids_in(&dir).unwrap();
        assert_eq!(ids, vec!["profile-a", "profile-b", "profile-c"]);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn listing_a_nonexistent_directory_returns_an_empty_list_not_an_error() {
        let dir = std::env::temp_dir().join("brute-profile-test-never-created-dir");
        let ids = list_profile_ids_in(&dir).unwrap();
        assert!(ids.is_empty());
    }

    #[test]
    fn generated_profile_ids_are_distinct() {
        assert_ne!(generate_profile_id(), generate_profile_id());
    }
}
