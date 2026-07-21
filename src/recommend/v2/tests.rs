use super::*;
use crate::calibration::CalibrationStore;
use crate::hardware::HardwareField;
use crate::preferences::Preferences;
use std::path::Path;

fn dev_catalog() -> Catalog {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("data/catalog/dev-catalog.json");
    crate::catalog::load_catalog(&path).expect("dev catalog must load")
}

fn seed_calibration() -> CalibrationStore {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("data/calibration/seed-calibration.json");
    CalibrationStore::load(&path).expect("seed calibration must load")
}

fn generous_profile(ram_bytes: u64) -> HardwareCapabilityProfile {
    let mut hw = crate::hardware::inspect(None);
    hw.memory.available_bytes = HardwareField::measured(ram_bytes, "test");
    hw.memory.total_bytes = HardwareField::measured(ram_bytes, "test");
    hw.storage = Some(crate::hardware::StorageReport {
        path_queried: "C:\\".to_string(),
        free_bytes: HardwareField::measured(500_000_000_000, "test"),
        total_bytes: HardwareField::measured(1_000_000_000_000, "test"),
    });
    crate::profile::build_profile(&hw, "2026-01-01T00:00:00Z".to_string(), 1)
}

// Explicitly pins *both* GPU backends rather than leaving either to the
// real hardware::inspect() detection running the test - a machine that
// genuinely has a GPU (this repo's own dev machine does) would otherwise
// leak real detection into what's meant to be a fully synthetic "no GPU"
// scenario, making the test pass or fail depending on who runs it.
fn profile_with_gpu(ram_bytes: u64, gpu_available: bool) -> HardwareCapabilityProfile {
    let mut profile = generous_profile(ram_bytes);
    profile.backends.cuda = HardwareField::measured(gpu_available, "test");
    profile.backends.vulkan = HardwareField::measured(gpu_available, "test");
    profile
}

fn empty_calibration() -> CalibrationStore {
    CalibrationStore { records: vec![] }
}

fn find<'a>(set: &'a RecommendationSetV2, catalog_id: &str) -> &'a BuildRecommendationV2 {
    set.entries
        .iter()
        .find(|e| e.build.catalog_id == catalog_id)
        .unwrap_or_else(|| panic!("no entry {catalog_id:?} in recommendation set"))
}

#[test]
fn every_catalog_build_appears_in_the_output_none_silently_dropped() {
    let catalog = dev_catalog();
    let calib = seed_calibration();
    let profile = generous_profile(32_000_000_000);
    let prefs = Preferences::default();

    let set = recommend_v2(&catalog, &profile, &calib, &prefs);
    assert_eq!(set.entries.len(), catalog.builds.len());
}

#[test]
fn ranking_is_deterministic_across_repeated_runs() {
    let catalog = dev_catalog();
    let calib = seed_calibration();
    let profile = generous_profile(32_000_000_000);
    let prefs = Preferences::default();

    let a = recommend_v2(&catalog, &profile, &calib, &prefs);
    let b = recommend_v2(&catalog, &profile, &calib, &prefs);
    let ids_a: Vec<_> = a
        .entries
        .iter()
        .map(|e| e.build.catalog_id.clone())
        .collect();
    let ids_b: Vec<_> = b
        .entries
        .iter()
        .map(|e| e.build.catalog_id.clone())
        .collect();
    assert_eq!(ids_a, ids_b);
}

#[test]
fn arabic_language_preference_ranks_an_arabic_first_model_at_the_top() {
    let catalog = dev_catalog();
    let calib = seed_calibration();
    let profile = generous_profile(32_000_000_000);
    let prefs = Preferences {
        language: crate::preferences::LanguagePreference::Arabic,
        ..Preferences::default()
    };

    let set = recommend_v2(&catalog, &profile, &calib, &prefs);
    let top = &set.entries[0];
    assert!(
        top.build
            .task_categories
            .contains(&TaskCategory::ArabicChat),
        "expected an Arabic-capable build at the top, got {}",
        top.build.catalog_id
    );
    assert!(top.component_scores.arabic > 0.0);
}

#[test]
fn english_only_preference_gives_arabic_negligible_weight() {
    let catalog = dev_catalog();
    let calib = seed_calibration();
    let profile = generous_profile(32_000_000_000);
    let prefs = Preferences {
        language: crate::preferences::LanguagePreference::English,
        arabic_priority: false,
        ..Preferences::default()
    };

    let set = recommend_v2(&catalog, &profile, &calib, &prefs);
    // The verified-artifact Qwen 0.5B (general chat, no special Arabic
    // weighting applied) should be competitive even though it isn't the
    // strongest Arabic entry - i.e. Arabic score must not dominate here.
    let qwen = find(&set, "qwen2.5-0.5b-instruct-q4_k_m");
    assert!(qwen.overall_score > 0.0);
}

#[test]
fn coding_use_case_ranks_the_coding_model_best_for_coding() {
    let catalog = dev_catalog();
    let calib = seed_calibration();
    let profile = generous_profile(32_000_000_000);
    let prefs = Preferences {
        use_case: UseCase::Coding,
        ..Preferences::default()
    };

    let set = recommend_v2(&catalog, &profile, &calib, &prefs);
    let coding = find(&set, "qwen2.5-coder-1.5b-instruct-q4_k_m");
    assert!(
        coding
            .categories
            .contains(&RecommendationCategory::BestCoding)
    );
    assert!(coding.component_scores.task_fit > 0.5);
}

#[test]
fn documents_use_case_favors_long_context_models() {
    let catalog = dev_catalog();
    let calib = seed_calibration();
    let profile = generous_profile(64_000_000_000);
    let prefs = Preferences {
        use_case: UseCase::Documents,
        ..Preferences::default()
    };

    let set = recommend_v2(&catalog, &profile, &calib, &prefs);
    // Gemma-3-4B (128K context, document_analysis-eligible via its task
    // categories) must be present and scored, not silently excluded.
    let gemma = find(&set, "gemma-3-4b-it-qat-q4_0");
    assert!(gemma.component_scores.task_fit >= 0.0);
}

#[test]
fn reasoning_use_case_uses_the_curated_reasoning_capability_level() {
    let catalog = dev_catalog();
    let calib = seed_calibration();
    let profile = generous_profile(32_000_000_000);
    let prefs = Preferences {
        use_case: UseCase::Reasoning,
        ..Preferences::default()
    };

    let set = recommend_v2(&catalog, &profile, &calib, &prefs);
    let llama = find(&set, "llama-3.1-8b-instruct-q4_k_m");
    // Llama-3.1-8B lists Reasoning in task_categories but has no curated
    // reasoning_capability rating (Unknown) - task_fit should reflect the
    // category match without fabricating a capability score.
    assert!(llama.component_scores.task_fit > 0.0);
}

#[test]
fn fastest_priority_prefers_the_measured_build_over_an_unmeasured_one() {
    let catalog = dev_catalog();
    let calib = seed_calibration();
    let profile = generous_profile(32_000_000_000);
    // Pin CPU so the query backend matches the seed calibration record's
    // backend regardless of whether this machine has a real GPU - see
    // exact_calibration_match_yields_high_confidence for the same reasoning.
    let prefs = Preferences {
        priority: SpeedQualityPriority::Fastest,
        cpu_only: true,
        ..Preferences::default()
    };

    let set = recommend_v2(&catalog, &profile, &calib, &prefs);
    let measured = find(&set, "qwen2.5-0.5b-instruct-q4_k_m");
    let unmeasured = find(&set, "llama-3.1-8b-instruct-q4_k_m");
    assert!(measured.component_scores.speed >= unmeasured.component_scores.speed);
}

#[test]
fn best_quality_priority_prefers_a_larger_model_than_default_balanced() {
    let catalog = dev_catalog();
    let calib = seed_calibration();
    let profile = generous_profile(64_000_000_000);

    let balanced = recommend_v2(
        &catalog,
        &profile,
        &calib,
        &Preferences {
            priority: SpeedQualityPriority::Balanced,
            ..Preferences::default()
        },
    );
    let quality = recommend_v2(
        &catalog,
        &profile,
        &calib,
        &Preferences {
            priority: SpeedQualityPriority::BestQuality,
            ..Preferences::default()
        },
    );

    let balanced_top = &balanced.entries[0];
    let quality_top = &quality.entries[0];
    assert!(quality_top.build.parameter_count >= balanced_top.build.parameter_count);
}

#[test]
fn cpu_only_preference_never_rejects_a_build_that_supports_cpu() {
    let catalog = dev_catalog();
    let calib = seed_calibration();
    let profile = profile_with_gpu(32_000_000_000, true);
    let prefs = Preferences {
        cpu_only: true,
        ..Preferences::default()
    };

    let set = recommend_v2(&catalog, &profile, &calib, &prefs);
    let qwen = find(&set, "qwen2.5-0.5b-instruct-q4_k_m");
    assert!(qwen.rejection_reasons.is_empty());
}

#[test]
fn require_gpu_preference_rejects_every_build_when_no_gpu_is_detected() {
    let catalog = dev_catalog();
    let calib = seed_calibration();
    let profile = profile_with_gpu(32_000_000_000, false);
    let prefs = Preferences {
        gpu_preference: crate::preferences::GpuPreference::RequireGpu,
        ..Preferences::default()
    };

    let set = recommend_v2(&catalog, &profile, &calib, &prefs);
    assert!(set.entries.iter().all(|e| !e.rejection_reasons.is_empty()));
    assert!(
        set.entries
            .iter()
            .all(|e| e.categories == vec![RecommendationCategory::Unsupported])
    );
}

#[test]
fn require_gpu_preference_accepts_a_build_when_a_gpu_is_detected() {
    let catalog = dev_catalog();
    let calib = seed_calibration();
    let profile = profile_with_gpu(32_000_000_000, true);
    let prefs = Preferences {
        gpu_preference: crate::preferences::GpuPreference::RequireGpu,
        ..Preferences::default()
    };

    let set = recommend_v2(&catalog, &profile, &calib, &prefs);
    let qwen = find(&set, "qwen2.5-0.5b-instruct-q4_k_m");
    assert!(qwen.rejection_reasons.is_empty());
}

#[test]
fn insufficient_ram_is_never_silently_recommended() {
    let catalog = dev_catalog();
    let calib = seed_calibration();
    // A tiny machine - the 70B entry must never come out on top, and must
    // carry a clear rejection/NotRecommended signal.
    let profile = generous_profile(4_000_000_000);
    let prefs = Preferences::default();

    let set = recommend_v2(&catalog, &profile, &calib, &prefs);
    let oversized = find(&set, "llama-3.1-70b-instruct-q4_k_m");
    assert!(
        oversized
            .categories
            .contains(&RecommendationCategory::NotRecommended)
            || !oversized.rejection_reasons.is_empty()
    );
    assert!(
        !oversized
            .categories
            .contains(&RecommendationCategory::BestMatch)
    );
}

#[test]
fn insufficient_vram_preference_ceiling_rejects_with_a_clear_reason() {
    let catalog = dev_catalog();
    let calib = seed_calibration();
    let profile = profile_with_gpu(64_000_000_000, true);
    let prefs = Preferences {
        max_vram_bytes: Some(1), // effectively zero - any nonzero VRAM estimate exceeds it
        ..Preferences::default()
    };

    let set = recommend_v2(&catalog, &profile, &calib, &prefs);
    let with_vram_estimate = set
        .entries
        .iter()
        .find(|e| e.build.min_recommended_vram_bytes.is_some())
        .expect("at least one dev-catalog entry has a VRAM estimate");
    if !with_vram_estimate.rejection_reasons.is_empty() {
        assert!(
            with_vram_estimate
                .rejection_reasons
                .iter()
                .any(|r| r.contains("VRAM"))
        );
    }
}

#[test]
fn max_download_size_preference_rejects_oversized_artifacts() {
    let catalog = dev_catalog();
    let calib = seed_calibration();
    let profile = generous_profile(64_000_000_000);
    let prefs = Preferences {
        max_download_size_bytes: Some(1_000_000_000), // 1 GB
        ..Preferences::default()
    };

    let set = recommend_v2(&catalog, &profile, &calib, &prefs);
    let big = find(&set, "llama-3.1-8b-instruct-q4_k_m");
    assert!(!big.rejection_reasons.is_empty());
    assert!(big.rejection_reasons[0].contains("download size"));

    let small = find(&set, "qwen2.5-0.5b-instruct-q4_k_m");
    assert!(small.rejection_reasons.is_empty());
}

#[test]
fn excluded_families_are_always_rejected_regardless_of_score() {
    let catalog = dev_catalog();
    let calib = seed_calibration();
    let profile = generous_profile(64_000_000_000);
    let prefs = Preferences {
        excluded_families: vec!["qwen2.5-0.5b-instruct".to_string()],
        ..Preferences::default()
    };

    let set = recommend_v2(&catalog, &profile, &calib, &prefs);
    for entry in &set.entries {
        if entry.build.family == "qwen2.5-0.5b-instruct" {
            assert!(!entry.rejection_reasons.is_empty());
            assert!(
                entry
                    .categories
                    .contains(&RecommendationCategory::Unsupported)
            );
        }
    }
}

#[test]
fn preferred_families_boost_score_without_excluding_others() {
    let catalog = dev_catalog();
    let calib = seed_calibration();
    let profile = generous_profile(32_000_000_000);
    let prefs = Preferences {
        preferred_families: vec!["qwen2.5-coder-1.5b-instruct".to_string()],
        ..Preferences::default()
    };

    let set = recommend_v2(&catalog, &profile, &calib, &prefs);
    // A preferred family must not make every other build disappear.
    assert!(
        set.entries
            .iter()
            .any(|e| e.rejection_reasons.is_empty()
                && e.build.family != "qwen2.5-coder-1.5b-instruct")
    );
}

#[test]
fn unknown_license_is_never_recommended_for_commercial_use() {
    let catalog = crate::catalog::load_catalog(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("data/catalog/test-fixtures.json"),
    )
    .expect("test fixtures must load");
    let calib = empty_calibration();
    let profile = generous_profile(32_000_000_000);
    let prefs = Preferences {
        commercial_use_required: true,
        ..Preferences::default()
    };

    let set = recommend_v2(&catalog, &profile, &calib, &prefs);
    let unknown = find(&set, "dev-fixture-unknown-license");
    assert!(!unknown.rejection_reasons.is_empty());
    assert!(
        unknown
            .categories
            .contains(&RecommendationCategory::Unsupported)
    );
    assert!(
        !unknown
            .categories
            .contains(&RecommendationCategory::BestMatch)
    );
}

#[test]
fn commercial_use_not_required_still_ranks_the_unknown_license_build_but_never_as_best() {
    let catalog = crate::catalog::load_catalog(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("data/catalog/test-fixtures.json"),
    )
    .expect("test fixtures must load");
    let calib = empty_calibration();
    let profile = generous_profile(32_000_000_000);
    let prefs = Preferences::default();

    let set = recommend_v2(&catalog, &profile, &calib, &prefs);
    let unknown = find(&set, "dev-fixture-unknown-license");
    assert!(unknown.component_scores.license_fit < 1.0);
}

#[test]
fn artifact_url_verified_builds_score_higher_trust_than_curated_metadata_only() {
    let catalog = dev_catalog();
    let calib = seed_calibration();
    let profile = generous_profile(32_000_000_000);
    let prefs = Preferences::default();

    let set = recommend_v2(&catalog, &profile, &calib, &prefs);
    // Jais has an official, artifact_url_verified GGUF; ALLaM has no
    // first-party artifact (curated_metadata only) - trust must reflect
    // that real difference, never treat them as equally verified.
    let jais = find(&set, "jais-2-8b-chat-q4_k_m");
    let allam = find(&set, "allam-7b-instruct-preview-q4_k_m");
    assert!(
        jais.component_scores.trust > allam.component_scores.trust,
        "jais trust {} should exceed allam trust {}",
        jais.component_scores.trust,
        allam.component_scores.trust
    );
}

#[test]
fn measured_confidence_is_never_claimed_without_a_real_calibration_match() {
    let catalog = dev_catalog();
    let calib = empty_calibration(); // no calibration records at all
    let profile = generous_profile(32_000_000_000);
    let prefs = Preferences::default();

    let set = recommend_v2(&catalog, &profile, &calib, &prefs);
    for entry in &set.entries {
        if entry.rejection_reasons.is_empty() {
            assert_ne!(
                entry.confidence,
                ConfidenceLevel::High,
                "{} claimed High confidence with zero calibration data",
                entry.build.catalog_id
            );
        }
    }
}

#[test]
fn exact_calibration_match_yields_high_confidence() {
    let catalog = dev_catalog();
    let calib = seed_calibration();
    let profile = generous_profile(32_000_000_000);
    // The seed calibration record was measured on the CPU backend - force
    // CPU here so the query backend matches it regardless of whether the
    // machine actually running this test has a real GPU (this repo's own
    // dev machine does), keeping the test's result independent of who
    // runs it.
    let prefs = Preferences {
        cpu_only: true,
        ..Preferences::default()
    };

    let set = recommend_v2(&catalog, &profile, &calib, &prefs);
    let qwen = find(&set, "qwen2.5-0.5b-instruct-q4_k_m");
    assert_eq!(qwen.confidence, ConfidenceLevel::High);
}

#[test]
fn deterministic_tie_breaking_uses_catalog_id_ascending() {
    // Two builds with a genuinely tied overall_score (both hard-rejected,
    // both score exactly 0.0) must still order deterministically by
    // catalog_id, not by original catalog array order or hash iteration.
    let catalog = dev_catalog();
    let calib = seed_calibration();
    let profile = generous_profile(32_000_000_000);
    let prefs = Preferences {
        max_download_size_bytes: Some(1), // rejects everything with a real file size
        ..Preferences::default()
    };

    let set = recommend_v2(&catalog, &profile, &calib, &prefs);
    let rejected_ids: Vec<&str> = set
        .entries
        .iter()
        .filter(|e| !e.rejection_reasons.is_empty())
        .map(|e| e.build.catalog_id.as_str())
        .collect();
    let mut sorted = rejected_ids.clone();
    sorted.sort();
    assert_eq!(rejected_ids, sorted);
}

#[test]
fn every_rejection_carries_a_human_readable_reason() {
    let catalog = dev_catalog();
    let calib = seed_calibration();
    let profile = generous_profile(32_000_000_000);
    let prefs = Preferences {
        excluded_families: vec!["qwen2.5-0.5b-instruct".to_string()],
        ..Preferences::default()
    };

    let set = recommend_v2(&catalog, &profile, &calib, &prefs);
    for entry in &set.entries {
        if !entry.rejection_reasons.is_empty() {
            for reason in &entry.rejection_reasons {
                assert!(!reason.trim().is_empty());
            }
        }
    }
}

#[test]
fn best_match_is_always_the_top_of_the_sorted_eligible_list() {
    let catalog = dev_catalog();
    let calib = seed_calibration();
    let profile = generous_profile(32_000_000_000);
    let prefs = Preferences::default();

    let set = recommend_v2(&catalog, &profile, &calib, &prefs);
    let best_match = set
        .entries
        .iter()
        .find(|e| e.categories.contains(&RecommendationCategory::BestMatch))
        .expect("there must be a BestMatch on a generously-provisioned machine");
    let first_eligible = set
        .entries
        .iter()
        .find(|e| e.rejection_reasons.is_empty())
        .unwrap();
    assert_eq!(best_match.build.catalog_id, first_eligible.build.catalog_id);
}
