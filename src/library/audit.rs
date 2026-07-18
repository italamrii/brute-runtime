//! Library health audit (spec section 14). Purely a read-only report -
//! nothing here repairs, deletes, or modifies anything; every finding
//! comes with a suggested `brute library ...` command for the user to
//! run themselves. See `docs/stage-3-trusted-local-library.md`.

use super::verify::quick_file_status;
use super::{
    FileStatus, LIBRARY_SCHEMA_VERSION, LibraryStore, TrustStatus, associations, duplicates,
};
use crate::calibration::CalibrationStore;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct StaleAssociationNote {
    pub library_id: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PrivacyConcern {
    pub library_id: String,
    pub field: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuditReport {
    pub healthy: Vec<String>,
    pub missing: Vec<String>,
    pub modified: Vec<String>,
    pub corrupt: Vec<String>,
    pub duplicate_groups: Vec<duplicates::DuplicateGroup>,
    pub stale_profiles: Vec<StaleAssociationNote>,
    pub stale_calibrations: Vec<StaleAssociationNote>,
    pub unknown_provenance: Vec<String>,
    pub unsupported: Vec<String>,
    pub privacy_concerns: Vec<PrivacyConcern>,
    pub schema_migration_needed: Vec<String>,
    /// Human-readable next steps - always a suggestion naming an actual
    /// `brute library` command, never an automatic fix.
    pub recommendations: Vec<String>,
}

/// Runs the full audit. `profiles_dir` locates saved Stage 2 runtime
/// profiles (see `docs/runtime-profile-association.md`); `calibration_store`
/// is optional since not every invocation has one loaded.
pub fn audit(
    store: &LibraryStore,
    profiles_dir: &Path,
    calibration_store: Option<&CalibrationStore>,
) -> AuditReport {
    let mut healthy = Vec::new();
    let mut missing = Vec::new();
    let mut modified = Vec::new();
    let mut corrupt = Vec::new();
    let mut unknown_provenance = Vec::new();
    let mut unsupported = Vec::new();
    let mut privacy_concerns = Vec::new();
    let mut schema_migration_needed = Vec::new();
    let mut stale_profiles = Vec::new();
    let mut stale_calibrations = Vec::new();

    for entry in &store.entries {
        let status = quick_file_status(entry);

        match status {
            FileStatus::Missing => missing.push(entry.library_id.clone()),
            FileStatus::Modified | FileStatus::Replaced | FileStatus::Inaccessible => {
                modified.push(entry.library_id.clone())
            }
            FileStatus::Unchanged | FileStatus::Moved => {}
        }

        match entry.trust {
            TrustStatus::Corrupt => corrupt.push(entry.library_id.clone()),
            TrustStatus::Unknown => unknown_provenance.push(entry.library_id.clone()),
            TrustStatus::Unsupported => unsupported.push(entry.library_id.clone()),
            _ => {}
        }

        if entry.schema_version != LIBRARY_SCHEMA_VERSION {
            schema_migration_needed.push(entry.library_id.clone());
        }

        let is_problem_free = status == FileStatus::Unchanged
            && !matches!(
                entry.trust,
                TrustStatus::Corrupt
                    | TrustStatus::Unknown
                    | TrustStatus::Unsupported
                    | TrustStatus::Missing
                    | TrustStatus::ModifiedSinceVerification
            )
            && entry.quarantine.is_none();
        if is_problem_free {
            healthy.push(entry.library_id.clone());
        }

        for (field, text) in [
            ("alias", entry.alias.as_deref()),
            ("notes", entry.notes.as_deref()),
        ] {
            if let Some(text) = text
                && looks_like_a_filesystem_path(text)
            {
                privacy_concerns.push(PrivacyConcern {
                    library_id: entry.library_id.clone(),
                    field: field.to_string(),
                    detail: "this text looks like it may contain a filesystem path or username - \
                             review before including it in a shareable export"
                        .to_string(),
                });
            }
        }

        // A changed/missing file means any saved profile or calibration
        // record keyed to this entry's *recorded* hash no longer
        // describes what's actually at this path right now.
        if matches!(
            status,
            FileStatus::Modified | FileStatus::Replaced | FileStatus::Missing
        ) {
            let profile_count =
                associations::find_runtime_profiles_for(&entry.sha256, profiles_dir).len();
            if profile_count > 0 {
                stale_profiles.push(StaleAssociationNote {
                    library_id: entry.library_id.clone(),
                    detail: format!(
                        "{profile_count} saved runtime profile(s) reference this entry's recorded \
                         hash, but the file has changed - re-run `brute library verify {}` before \
                         trusting them",
                        entry.library_id
                    ),
                });
            }

            if let Some(calibration_store) = calibration_store {
                let calibration_count =
                    associations::find_calibration_matches_for(calibration_store, entry).len();
                if calibration_count > 0 {
                    stale_calibrations.push(StaleAssociationNote {
                        library_id: entry.library_id.clone(),
                        detail: format!(
                            "{calibration_count} calibration record(s) matched this entry's recorded \
                             metadata, but the file has changed - re-benchmark before trusting them"
                        ),
                    });
                }
            }
        }
    }

    healthy.sort();
    missing.sort();
    modified.sort();
    corrupt.sort();
    unknown_provenance.sort();
    unsupported.sort();
    schema_migration_needed.sort();
    stale_profiles.sort_by(|a, b| a.library_id.cmp(&b.library_id));
    stale_calibrations.sort_by(|a, b| a.library_id.cmp(&b.library_id));
    privacy_concerns.sort_by(|a, b| a.library_id.cmp(&b.library_id));

    let duplicate_groups = duplicates::find_duplicate_groups(store);

    let recommendations = build_recommendations(
        &missing,
        &modified,
        &corrupt,
        &duplicate_groups,
        &schema_migration_needed,
    );

    AuditReport {
        healthy,
        missing,
        modified,
        corrupt,
        duplicate_groups,
        stale_profiles,
        stale_calibrations,
        unknown_provenance,
        unsupported,
        privacy_concerns,
        schema_migration_needed,
        recommendations,
    }
}

fn looks_like_a_filesystem_path(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains(r"c:\users\")
        || lower.contains(r"\appdata\")
        || (text.contains(':') && text.contains('\\'))
}

fn build_recommendations(
    missing: &[String],
    modified: &[String],
    corrupt: &[String],
    duplicate_groups: &[duplicates::DuplicateGroup],
    schema_migration_needed: &[String],
) -> Vec<String> {
    let mut recommendations = Vec::new();
    if !missing.is_empty() {
        recommendations.push(format!(
            "{} entr{} missing - run `brute library locate <id> <new-path>` if moved, or \
             `brute library forget <id>` if gone for good.",
            missing.len(),
            if missing.len() == 1 {
                "y is"
            } else {
                "ies are"
            }
        ));
    }
    if !modified.is_empty() {
        recommendations.push(format!(
            "{} entr{} changed on disk - run `brute library verify <id>` to confirm.",
            modified.len(),
            if modified.len() == 1 { "y" } else { "ies" }
        ));
    }
    if !corrupt.is_empty() {
        recommendations.push(format!(
            "{} entr{} corrupt - consider `brute library quarantine <id> --reason <reason>`.",
            corrupt.len(),
            if corrupt.len() == 1 {
                "y is"
            } else {
                "ies are"
            }
        ));
    }
    if !duplicate_groups.is_empty() {
        recommendations.push(format!(
            "{} duplicate group(s) found - run `brute library duplicates` for details before \
             forgetting any copy.",
            duplicate_groups.len()
        ));
    }
    if !schema_migration_needed.is_empty() {
        recommendations.push(format!(
            "{} entr{} on an older schema version - re-importing will bring them current.",
            schema_migration_needed.len(),
            if schema_migration_needed.len() == 1 {
                "y is"
            } else {
                "ies are"
            }
        ));
    }
    recommendations
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::{CatalogMatchResult, LibraryEntry};
    use std::path::PathBuf;

    fn tmp_dir(name: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "brute-library-audit-test-{name}-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn entry(id: &str, path: &Path, trust: TrustStatus) -> LibraryEntry {
        LibraryEntry {
            library_id: id.to_string(),
            schema_version: LIBRARY_SCHEMA_VERSION.to_string(),
            sha256: format!("hash-{id}"),
            file_size_bytes: 100,
            gguf_version: 3,
            tensor_count: 10,
            kv_count: 5,
            architecture: Some("llama".to_string()),
            quantization: Some("Q4_K_M".to_string()),
            parameter_count: Some(500_000_000),
            current_path: path.to_path_buf(),
            original_import_path: None,
            imported_at_rfc3339: "2026-01-01T00:00:00Z".to_string(),
            last_verified_at_rfc3339: None,
            file_modified_at_rfc3339: None,
            file_status: FileStatus::Unchanged,
            trust,
            last_verification: None,
            catalog_match: CatalogMatchResult::none(),
            alias: None,
            notes: None,
            quarantine: None,
            managed_copy: false,
        }
    }

    #[test]
    fn a_healthy_available_entry_is_reported_healthy() {
        let dir = tmp_dir("healthy");
        let path = dir.join("model.gguf");
        std::fs::write(&path, b"content").unwrap();
        let mut e = entry("lib-1", &path, TrustStatus::LocalUnverifiedSource);
        e.file_size_bytes = std::fs::metadata(&path).unwrap().len();

        let mut store = LibraryStore::default();
        store.entries.push(e);

        let report = audit(&store, &dir.join("profiles"), None);
        assert_eq!(report.healthy, vec!["lib-1".to_string()]);
        assert!(report.missing.is_empty());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_missing_file_is_reported_missing_and_never_healthy() {
        let dir = tmp_dir("missing");
        let path = dir.join("gone.gguf");
        let e = entry("lib-1", &path, TrustStatus::LocalUnverifiedSource);

        let mut store = LibraryStore::default();
        store.entries.push(e);

        let report = audit(&store, &dir.join("profiles"), None);
        assert_eq!(report.missing, vec!["lib-1".to_string()]);
        assert!(report.healthy.is_empty());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn corrupt_and_unsupported_and_unknown_trust_are_reported_separately() {
        let dir = tmp_dir("trust-states");
        let mut store = LibraryStore::default();
        store.entries.push(entry(
            "corrupt-one",
            &dir.join("a.gguf"),
            TrustStatus::Corrupt,
        ));
        store.entries.push(entry(
            "unsupported-one",
            &dir.join("b.gguf"),
            TrustStatus::Unsupported,
        ));
        store.entries.push(entry(
            "unknown-one",
            &dir.join("c.gguf"),
            TrustStatus::Unknown,
        ));

        let report = audit(&store, &dir.join("profiles"), None);
        assert_eq!(report.corrupt, vec!["corrupt-one".to_string()]);
        assert_eq!(report.unsupported, vec!["unsupported-one".to_string()]);
        assert_eq!(report.unknown_provenance, vec!["unknown-one".to_string()]);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_quarantined_entry_is_never_reported_healthy_even_if_available() {
        let dir = tmp_dir("quarantined");
        let path = dir.join("model.gguf");
        std::fs::write(&path, b"content").unwrap();
        let mut e = entry("lib-1", &path, TrustStatus::LocalUnverifiedSource);
        e.file_size_bytes = std::fs::metadata(&path).unwrap().len();
        e.quarantine = Some(super::super::QuarantineInfo {
            reason: "test".to_string(),
            quarantined_at_rfc3339: "2026-01-01T00:00:00Z".to_string(),
        });

        let mut store = LibraryStore::default();
        store.entries.push(e);

        let report = audit(&store, &dir.join("profiles"), None);
        assert!(report.healthy.is_empty());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn duplicate_groups_appear_in_the_report() {
        let dir = tmp_dir("dup-in-audit");
        let mut store = LibraryStore::default();
        let mut a = entry("a", &dir.join("a.gguf"), TrustStatus::LocalUnverifiedSource);
        a.sha256 = "shared".to_string();
        let mut b = entry("b", &dir.join("b.gguf"), TrustStatus::LocalUnverifiedSource);
        b.sha256 = "shared".to_string();
        store.entries.push(a);
        store.entries.push(b);

        let report = audit(&store, &dir.join("profiles"), None);
        assert_eq!(report.duplicate_groups.len(), 1);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_path_like_alias_is_flagged_as_a_privacy_concern() {
        let dir = tmp_dir("privacy-concern");
        let mut e = entry(
            "lib-1",
            &dir.join("model.gguf"),
            TrustStatus::LocalUnverifiedSource,
        );
        e.alias = Some(r"C:\Users\alice\Downloads\my-special-model".to_string());

        let mut store = LibraryStore::default();
        store.entries.push(e);

        let report = audit(&store, &dir.join("profiles"), None);
        assert_eq!(report.privacy_concerns.len(), 1);
        assert_eq!(report.privacy_concerns[0].field, "alias");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_ordinary_alias_is_never_flagged() {
        let dir = tmp_dir("no-privacy-concern");
        let mut e = entry(
            "lib-1",
            &dir.join("model.gguf"),
            TrustStatus::LocalUnverifiedSource,
        );
        e.alias = Some("My favorite chat model".to_string());

        let mut store = LibraryStore::default();
        store.entries.push(e);

        let report = audit(&store, &dir.join("profiles"), None);
        assert!(report.privacy_concerns.is_empty());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn recommendations_never_claim_an_automatic_fix() {
        let dir = tmp_dir("recommendations");
        let mut store = LibraryStore::default();
        store.entries.push(entry(
            "missing-one",
            &dir.join("gone.gguf"),
            TrustStatus::LocalUnverifiedSource,
        ));

        let report = audit(&store, &dir.join("profiles"), None);
        assert!(!report.recommendations.is_empty());
        assert!(report.recommendations[0].contains("brute library"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn empty_library_audits_cleanly() {
        let dir = tmp_dir("empty-audit");
        let store = LibraryStore::default();
        let report = audit(&store, &dir.join("profiles"), None);
        assert!(report.healthy.is_empty());
        assert!(report.missing.is_empty());
        assert!(report.recommendations.is_empty());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn audit_ordering_is_deterministic() {
        let dir = tmp_dir("deterministic");
        let mut store = LibraryStore::default();
        store.entries.push(entry(
            "zzz",
            &dir.join("gone1.gguf"),
            TrustStatus::LocalUnverifiedSource,
        ));
        store.entries.push(entry(
            "aaa",
            &dir.join("gone2.gguf"),
            TrustStatus::LocalUnverifiedSource,
        ));

        let first = audit(&store, &dir.join("profiles"), None);
        let second = audit(&store, &dir.join("profiles"), None);
        assert_eq!(first.missing, second.missing);
        assert_eq!(first.missing, vec!["aaa".to_string(), "zzz".to_string()]);

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Saves a minimal real `tuning::runtime_profile::RuntimeProfile` -
    /// duplicated locally per this codebase's per-module test-fixture
    /// convention (see `associations::tests::save_a_runtime_profile` for
    /// the same pattern).
    fn save_a_runtime_profile(dir: &Path, model_sha256: &str) {
        use crate::runtime::Backend;
        use crate::tuning::candidates::{TuningDefaults, TuningPlan};
        use crate::tuning::ranking::{CandidateMeasurements, RankedCandidate, RankingConfidence};
        use crate::tuning::runtime_profile::ProfileInputs;
        use crate::tuning::stability::{StabilityAssessment, TuningStabilityStatus};

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

    /// End-to-end: a real saved runtime profile keyed to a model's
    /// original hash must be flagged `stale_profiles` once that library
    /// entry's file has changed - spec acceptance criterion 12
    /// ("changed models invalidate relevant profiles").
    #[test]
    fn a_changed_entry_with_an_associated_runtime_profile_is_flagged_stale() {
        let dir = tmp_dir("stale-profile");
        let profiles_dir = dir.join("profiles");
        std::fs::create_dir_all(&profiles_dir).unwrap();

        let path = dir.join("model.gguf");
        std::fs::write(&path, b"original content").unwrap();
        let original_size = std::fs::metadata(&path).unwrap().len();
        save_a_runtime_profile(&profiles_dir, "original-hash");

        let mut e = entry("lib-1", &path, TrustStatus::LocalUnverifiedSource);
        e.sha256 = "original-hash".to_string();
        e.file_size_bytes = original_size;

        // Simulate the file having changed since the entry was recorded
        // (different size than what's on record) without needing a full
        // hash recompute - `quick_file_status` already catches this.
        std::fs::write(&path, b"a completely different and longer content string").unwrap();

        let mut store = LibraryStore::default();
        store.entries.push(e);

        let report = audit(&store, &profiles_dir, None);
        assert_eq!(report.modified, vec!["lib-1".to_string()]);
        assert_eq!(report.stale_profiles.len(), 1);
        assert_eq!(report.stale_profiles[0].library_id, "lib-1");
        assert!(report.stale_profiles[0].detail.contains("runtime profile"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_unchanged_entry_with_an_associated_profile_is_never_flagged_stale() {
        let dir = tmp_dir("not-stale-profile");
        let profiles_dir = dir.join("profiles");
        std::fs::create_dir_all(&profiles_dir).unwrap();

        let path = dir.join("model.gguf");
        std::fs::write(&path, b"stable content").unwrap();
        let size = std::fs::metadata(&path).unwrap().len();
        save_a_runtime_profile(&profiles_dir, "stable-hash");

        let mut e = entry("lib-1", &path, TrustStatus::LocalUnverifiedSource);
        e.sha256 = "stable-hash".to_string();
        e.file_size_bytes = size;

        let mut store = LibraryStore::default();
        store.entries.push(e);

        let report = audit(&store, &profiles_dir, None);
        assert!(report.stale_profiles.is_empty());

        std::fs::remove_dir_all(&dir).ok();
    }
}
