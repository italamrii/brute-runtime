//! Runtime-profile and calibration association (spec section 10).
//!
//! Deliberately **not persisted** on `LibraryEntry` - both associations
//! are computed fresh, on demand, from the model's SHA-256 (runtime
//! profiles) or architecture/quantization/parameter-count (calibration).
//! A persisted list of profile/calibration IDs on the entry would be a
//! second source of truth that could silently drift out of sync with
//! the actual profile/calibration stores (e.g. a profile deleted
//! directly from `%LOCALAPPDATA%\BruteRuntime\profiles\` would leave a
//! dangling reference behind). Computing live costs almost nothing -
//! Stage 2's runtime profiles and calibration records are already small,
//! flat, locally-loaded stores - and it can never be stale. See
//! `docs/runtime-profile-association.md`.
//!
//! **Moves preserve profiles automatically, for free**: since the
//! association key is the model's content hash, not its path, a model
//! moved via `library locate` (which only rebinds `current_path` after
//! confirming the hash is unchanged) keeps exactly the same associated
//! profiles and calibration records before and after the move - nothing
//! about this module needs to know a move happened at all.

use super::LibraryEntry;
use crate::calibration::{CalibrationMatch, CalibrationStore};
use crate::runtime::Backend;
use crate::tuning::runtime_profile::RuntimeProfile;
use std::path::Path;

/// Every runtime profile saved locally whose `model_sha256` matches this
/// entry's own hash exactly. A model moved without a content change
/// keeps every one of these - profiles are never bound to a path.
pub fn find_runtime_profiles_for(sha256: &str, profiles_dir: &Path) -> Vec<RuntimeProfile> {
    let Ok(ids) = crate::tuning::runtime_profile::list_profile_ids_in(profiles_dir) else {
        return Vec::new();
    };

    ids.iter()
        .filter_map(|id| crate::tuning::runtime_profile::load_profile_from(profiles_dir, id).ok())
        .filter(|profile| profile.model_sha256 == sha256)
        .collect()
}

/// The nearest calibration record for this entry, per backend - reuses
/// Stage 1's exact `calibration::find_nearest` lookup (same architecture-
/// gated, bounded nearest-neighbor match used by `brute fit`/
/// `brute recommend-model`), never a separate/duplicated heuristic.
/// Entries with unknown architecture/quantization/parameter_count
/// (e.g. a corrupt or unsupported-version import) simply produce no
/// matches - never a guessed one.
///
/// `find_nearest` itself doesn't filter by backend - a backend mismatch
/// only lowers a candidate's *proximity score*, so asking it "nearest
/// for CUDA" against a CPU-only store still returns the CPU record
/// (with a mismatch note), not `None`. Iterating over every backend and
/// trusting that result naively would report the same single CPU record
/// three times, mislabeled as a CUDA and Vulkan match too. This function
/// additionally requires the returned record's own `backend` field to
/// equal the one being asked about, so a backend only appears here when
/// a record actually measured on it exists.
pub fn find_calibration_matches_for(
    store: &CalibrationStore,
    entry: &LibraryEntry,
) -> Vec<(Backend, CalibrationMatch)> {
    let (Some(architecture), Some(quantization), Some(parameter_count)) = (
        entry.architecture.as_deref(),
        entry.quantization.as_deref(),
        entry.parameter_count,
    ) else {
        return Vec::new();
    };

    [Backend::Cpu, Backend::Cuda, Backend::Vulkan]
        .into_iter()
        .filter_map(|backend| {
            crate::calibration::find_nearest(
                store,
                architecture,
                quantization,
                parameter_count,
                backend,
            )
            .filter(|m| m.record.backend == backend)
            .map(|m| (backend, m))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::benchmark::metrics::StabilityStatus;
    use crate::calibration::CalibrationRecord;
    use crate::library::{CatalogMatchResult, FileStatus, LIBRARY_SCHEMA_VERSION, TrustStatus};
    use crate::tuning::candidates::{TuningDefaults, TuningPlan};
    use crate::tuning::ranking::{CandidateMeasurements, RankedCandidate, RankingConfidence};
    use crate::tuning::runtime_profile::ProfileInputs;
    use crate::tuning::stability::{StabilityAssessment, TuningStabilityStatus};
    use std::path::PathBuf;

    fn tmp_dir(name: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "brute-library-associations-test-{name}-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn library_entry(sha256: &str) -> LibraryEntry {
        LibraryEntry {
            library_id: "lib-1".to_string(),
            schema_version: LIBRARY_SCHEMA_VERSION.to_string(),
            sha256: sha256.to_string(),
            file_size_bytes: 491_400_032,
            gguf_version: 3,
            tensor_count: 100,
            kv_count: 10,
            architecture: Some("qwen2".to_string()),
            quantization: Some("Q4_K_M".to_string()),
            parameter_count: Some(630_167_424),
            current_path: PathBuf::from(r"C:\Models\test.gguf"),
            original_import_path: None,
            imported_at_rfc3339: "2026-07-18T00:00:00Z".to_string(),
            last_verified_at_rfc3339: None,
            file_modified_at_rfc3339: None,
            file_status: FileStatus::Unchanged,
            trust: TrustStatus::LocalUnverifiedSource,
            last_verification: None,
            catalog_match: CatalogMatchResult::none(),
            alias: None,
            notes: None,
            quarantine: None,
            managed_copy: false,
        }
    }

    fn calibration_record(sha256_arch: &str) -> CalibrationRecord {
        let _ = sha256_arch;
        CalibrationRecord {
            recorded_at_rfc3339: "2026-07-18T00:00:00Z".to_string(),
            machine_id: "machine-test".to_string(),
            machine_profile_schema_version: "stage1-v1".to_string(),
            catalog_id: None,
            architecture: "qwen2".to_string(),
            quantization: "Q4_K_M".to_string(),
            parameter_count: 630_167_424,
            backend: Backend::Cpu,
            threads: 16,
            gpu_layers: 0,
            context_size: 2048,
            batch_size: 512,
            prompt_processing_tokens_per_second: 425.15,
            generation_tokens_per_second: 44.09,
            peak_ram_bytes: Some(542_609_408),
            peak_vram_bytes: None,
            stability: StabilityStatus::Stable,
        }
    }

    fn save_a_runtime_profile(dir: &Path, model_sha256: &str) {
        let plan = TuningPlan {
            formula_version: "test".to_string(),
            model_sha256: model_sha256.to_string(),
            machine_profile_schema_version: "stage1-v1".to_string(),
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
        let hw = crate::hardware::inspect(None);
        let machine_profile =
            crate::profile::build_profile(&hw, "2026-01-01T00:00:00Z".to_string(), 0);

        let profile = crate::tuning::runtime_profile::build_profile(
            ProfileInputs {
                plan: &plan,
                winner: &winner,
                stability: &stability,
                confidence: RankingConfidence::High,
                machine_profile: &machine_profile,
                model: &crate::models::ModelReport {
                    path: PathBuf::from(r"C:\Models\test.gguf"),
                    file_size_bytes: 491_400_032,
                    sha256: model_sha256.to_string(),
                    gguf_version: 3,
                    tensor_count: 100,
                    kv_count: 10,
                    architecture: Some("qwen2".to_string()),
                    name: None,
                    quantization: Some("Q4_K_M".to_string()),
                    dominant_tensor_type: None,
                    parameter_count: Some(630_167_424),
                    alignment: 32,
                    size_consistency_checked: true,
                    kv_preview: Default::default(),
                    hyperparameters: Default::default(),
                },
                llama_cli_sha256: None,
                llama_bench_sha256: None,
                tuning_date: "2026-07-18T00:00:00Z".to_string(),
            },
            crate::tuning::runtime_profile::generate_profile_id(),
        );
        crate::tuning::runtime_profile::save_profile_to(dir, &profile).unwrap();
    }

    #[test]
    fn finds_runtime_profiles_matching_the_exact_hash() {
        let dir = tmp_dir("profiles-match");
        save_a_runtime_profile(&dir, "target-hash");
        save_a_runtime_profile(&dir, "different-hash");

        let matches = find_runtime_profiles_for("target-hash", &dir);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].model_sha256, "target-hash");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn no_runtime_profiles_matches_when_hash_is_unknown() {
        let dir = tmp_dir("profiles-none");
        save_a_runtime_profile(&dir, "some-other-hash");

        let matches = find_runtime_profiles_for("nonexistent-hash", &dir);
        assert!(matches.is_empty());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_moved_model_keeps_the_same_associated_profiles_since_association_is_by_hash() {
        let dir = tmp_dir("profiles-move-preserved");
        save_a_runtime_profile(&dir, "stable-hash");

        let before_move = find_runtime_profiles_for("stable-hash", &dir);
        // Simulate `library locate` rebinding current_path - the hash
        // used for lookup is unaffected by any path change.
        let after_move = find_runtime_profiles_for("stable-hash", &dir);

        assert_eq!(before_move.len(), after_move.len());
        assert_eq!(before_move.len(), 1);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn finds_calibration_matches_for_a_known_architecture() {
        let mut store = CalibrationStore::default();
        store.add(calibration_record("qwen2"));

        let entry = library_entry("test-hash");
        let matches = find_calibration_matches_for(&store, &entry);

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].0, Backend::Cpu);
    }

    #[test]
    fn no_calibration_matches_for_unknown_metadata() {
        let mut store = CalibrationStore::default();
        store.add(calibration_record("qwen2"));

        let mut entry = library_entry("test-hash");
        entry.architecture = None; // e.g. a corrupt/unsupported import

        let matches = find_calibration_matches_for(&store, &entry);
        assert!(matches.is_empty());
    }

    #[test]
    fn no_calibration_matches_against_an_empty_store() {
        let store = CalibrationStore::default();
        let entry = library_entry("test-hash");
        let matches = find_calibration_matches_for(&store, &entry);
        assert!(matches.is_empty());
    }
}
