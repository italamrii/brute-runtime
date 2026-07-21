//! Recommendation & Ranking Engine: scores every catalog build against a
//! machine profile for a given task/priority, and derives a primary
//! recommendation plus a safer fallback and a stronger optional build.
//! Every weight is an explicit, documented constant - see
//! `docs/recommendation-methodology.md` for the full table and a worked
//! example. Task matching uses curated catalog tags only, never invented
//! benchmark claims.

pub mod explain;
pub mod v2;

use crate::calibration::{self, CalibrationMatch, CalibrationProximity, CalibrationStore};
use crate::catalog::{Catalog, ModelBuild, TaskCategory};
use crate::estimator::{self, EstimationConfig, MemoryEstimate};
use crate::fit::{self, FitResult, FitState};
use crate::profile::HardwareCapabilityProfile;
use crate::runtime::Backend;
use serde::{Deserialize, Serialize};

pub const RANKING_FORMULA_VERSION: &str = "stage1-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    Fastest,
    Balanced,
    HighestQuality,
    LowestMemory,
    LongestContext,
    Coding,
    ArabicGeneralChat,
    PrivacyOffline,
}

/// One named, documented weight per scoring component. Every row sums to
/// 1.0 - see `docs/recommendation-methodology.md` for the full table and
/// the reasoning behind each priority's shape.
#[derive(Debug, Clone, Copy)]
struct Weights {
    headroom: f64,
    storage: f64,
    backend: f64,
    calibration: f64,
    speed: f64,
    task_match: f64,
    context: f64,
    stability: f64,
    quality_pref: f64,
    openness_pref: f64,
}

fn weights_for(priority: Priority) -> Weights {
    match priority {
        Priority::Balanced => Weights {
            headroom: 0.12,
            storage: 0.05,
            backend: 0.08,
            calibration: 0.12,
            speed: 0.13,
            task_match: 0.12,
            context: 0.12,
            stability: 0.10,
            quality_pref: 0.08,
            openness_pref: 0.08,
        },
        Priority::Fastest => Weights {
            headroom: 0.08,
            storage: 0.04,
            backend: 0.08,
            calibration: 0.10,
            speed: 0.40,
            task_match: 0.06,
            context: 0.06,
            stability: 0.08,
            quality_pref: 0.04,
            openness_pref: 0.06,
        },
        Priority::HighestQuality => Weights {
            headroom: 0.08,
            storage: 0.04,
            backend: 0.06,
            calibration: 0.08,
            speed: 0.05,
            task_match: 0.08,
            context: 0.10,
            stability: 0.06,
            quality_pref: 0.40,
            openness_pref: 0.05,
        },
        Priority::LowestMemory => Weights {
            headroom: 0.30,
            storage: 0.08,
            backend: 0.06,
            calibration: 0.08,
            speed: 0.05,
            task_match: 0.06,
            context: 0.06,
            stability: 0.06,
            quality_pref: 0.05,
            openness_pref: 0.20,
        },
        Priority::LongestContext => Weights {
            headroom: 0.08,
            storage: 0.04,
            backend: 0.06,
            calibration: 0.05,
            speed: 0.04,
            task_match: 0.06,
            context: 0.40,
            stability: 0.06,
            quality_pref: 0.12,
            openness_pref: 0.09,
        },
        Priority::Coding => Weights {
            headroom: 0.08,
            storage: 0.04,
            backend: 0.06,
            calibration: 0.08,
            speed: 0.08,
            task_match: 0.35,
            context: 0.06,
            stability: 0.06,
            quality_pref: 0.10,
            openness_pref: 0.09,
        },
        Priority::ArabicGeneralChat => Weights {
            headroom: 0.08,
            storage: 0.04,
            backend: 0.06,
            calibration: 0.08,
            speed: 0.08,
            task_match: 0.35,
            context: 0.06,
            stability: 0.06,
            quality_pref: 0.10,
            openness_pref: 0.09,
        },
        Priority::PrivacyOffline => Weights {
            headroom: 0.12,
            storage: 0.05,
            backend: 0.08,
            calibration: 0.08,
            speed: 0.05,
            task_match: 0.06,
            context: 0.08,
            stability: 0.06,
            quality_pref: 0.07,
            openness_pref: 0.35,
        },
    }
}

/// The fit state gates the final score multiplicatively - priority-driven
/// component weighting can only ever differentiate *within* a fit tier,
/// never override a NotRecommended/Unknown verdict into a top pick. This
/// is what keeps the engine from "recommending the largest model merely
/// because it technically fits": a poor-headroom Excellent-adjacent build
/// still outranks a NotRecommended giant regardless of priority.
fn fit_state_multiplier(state: FitState) -> f64 {
    match state {
        FitState::Excellent => 1.0,
        FitState::Good => 0.9,
        FitState::Constrained => 0.7,
        FitState::Experimental => 0.5,
        FitState::Unknown => 0.25,
        FitState::NotRecommended => 0.05,
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ComponentScores {
    pub headroom: f64,
    pub storage: f64,
    pub backend: f64,
    pub calibration: f64,
    pub speed: f64,
    pub task_match: f64,
    pub context: f64,
    pub stability: f64,
    pub quality_pref: f64,
    pub openness_pref: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct BuildEvaluation {
    pub build: ModelBuild,
    pub estimate: MemoryEstimate,
    pub fit: FitResult,
    pub calibration_match: Option<CalibrationMatch>,
    pub component_scores: ComponentScores,
    pub score: f64,
}

/// Evaluates one build against the machine profile: estimates memory,
/// finds the nearest calibration record, classifies fit, and scores it
/// for the given priority/task. Pure with respect to the catalog - never
/// mutates it.
pub fn evaluate_build(
    build: &ModelBuild,
    profile: &HardwareCapabilityProfile,
    calibration_store: &CalibrationStore,
    task: Option<TaskCategory>,
    priority: Priority,
    context_length: Option<u32>,
    backend: Option<Backend>,
) -> BuildEvaluation {
    let backend = backend.unwrap_or_else(|| preferred_backend(build, profile));
    let context_length =
        context_length.unwrap_or_else(|| build.context_sizes.iter().copied().max().unwrap_or(2048));
    let config = EstimationConfig::for_backend(backend, context_length);
    let estimate = estimator::estimate(build, None, profile, &config);

    let calibration_match = calibration::find_nearest(
        calibration_store,
        &build.architecture,
        &build.quantization,
        build.parameter_count,
        backend,
    );
    let has_calibration_support = calibration_match
        .as_ref()
        .is_some_and(|m| m.proximity != CalibrationProximity::Distant);

    let fit = fit::evaluate(build, &estimate, profile, has_calibration_support);

    let effective_task = task.or_else(|| implicit_task_for_priority(priority));
    let component_scores = score_components(
        build,
        &estimate,
        &fit,
        calibration_match.as_ref(),
        effective_task,
    );
    let weights = weights_for(priority);
    let base_score = component_scores.headroom * weights.headroom
        + component_scores.storage * weights.storage
        + component_scores.backend * weights.backend
        + component_scores.calibration * weights.calibration
        + component_scores.speed * weights.speed
        + component_scores.task_match * weights.task_match
        + component_scores.context * weights.context
        + component_scores.stability * weights.stability
        + component_scores.quality_pref * weights.quality_pref
        + component_scores.openness_pref * weights.openness_pref;

    let score = base_score * fit_state_multiplier(fit.state);

    BuildEvaluation {
        build: build.clone(),
        estimate,
        fit,
        calibration_match,
        component_scores,
        score,
    }
}

fn preferred_backend(build: &ModelBuild, profile: &HardwareCapabilityProfile) -> Backend {
    if build.supported_backends.contains(&Backend::Cuda)
        && profile.backends.cuda.value == Some(true)
    {
        Backend::Cuda
    } else if build.supported_backends.contains(&Backend::Vulkan)
        && profile.backends.vulkan.value == Some(true)
    {
        Backend::Vulkan
    } else {
        Backend::Cpu
    }
}

fn implicit_task_for_priority(priority: Priority) -> Option<TaskCategory> {
    match priority {
        Priority::Coding => Some(TaskCategory::Coding),
        Priority::ArabicGeneralChat => Some(TaskCategory::ArabicChat),
        _ => None,
    }
}

fn score_components(
    build: &ModelBuild,
    estimate: &MemoryEstimate,
    fit: &FitResult,
    calibration_match: Option<&CalibrationMatch>,
    task: Option<TaskCategory>,
) -> ComponentScores {
    let headroom = fit
        .headroom_ratio
        .map(|r| (r / 0.75).clamp(0.0, 1.0))
        .unwrap_or(0.0);

    let storage = if fit.state == FitState::NotRecommended
        && fit.reasons.iter().any(|r| r.contains("disk"))
    {
        0.0
    } else {
        1.0
    };

    let backend = if fit.state == FitState::NotRecommended
        && fit.reasons.iter().any(|r| r.contains("backend"))
    {
        0.0
    } else {
        1.0
    };

    let calibration = match calibration_match.map(|m| m.proximity) {
        Some(CalibrationProximity::Exact) => 1.0,
        Some(CalibrationProximity::Close) => 0.6,
        Some(CalibrationProximity::Distant) => 0.3,
        None => 0.0,
    };

    // Speed is normalized against a fixed, documented reference rate
    // rather than the candidate set (keeps scores stable and comparable
    // run to run instead of shifting whenever the catalog changes).
    // 40 tok/s generation is a reasonable "comfortable" reference point
    // for interactive chat on commodity hardware.
    const SPEED_REFERENCE_TOKENS_PER_SECOND: f64 = 40.0;
    let speed = match calibration_match {
        Some(m) => (m.record.generation_tokens_per_second / SPEED_REFERENCE_TOKENS_PER_SECOND)
            .clamp(0.0, 1.0),
        None => 0.5,
    };

    let task_match = match task {
        Some(t) if build.task_categories.contains(&t) => 1.0,
        Some(_) => 0.2,
        None => 1.0,
    };

    let context = if estimate.context_realistic { 1.0 } else { 0.2 };

    let stability = match calibration_match.map(|m| m.record.stability) {
        Some(crate::benchmark::metrics::StabilityStatus::Stable) => 1.0,
        Some(crate::benchmark::metrics::StabilityStatus::Marginal) => 0.6,
        Some(crate::benchmark::metrics::StabilityStatus::Unstable) => 0.2,
        None => 0.5,
    };

    // Reference scale for "how large is large" - 30B params treated as
    // the practical upper end of what a well-provisioned consumer/
    // workstation machine can run; deliberately not tied to any single
    // catalog entry so it stays stable as the catalog grows.
    const QUALITY_REFERENCE_PARAMS: f64 = 30_000_000_000.0;
    let quality_pref =
        ((build.parameter_count as f64).ln() / QUALITY_REFERENCE_PARAMS.ln()).clamp(0.0, 1.0);

    let openness_pref = match (build.gated_access, &build.license) {
        (Some(false), crate::catalog::License::Known { .. }) => 1.0,
        (Some(true), _) => 0.3,
        (None, _) | (_, crate::catalog::License::Unknown) => 0.5,
    };

    ComponentScores {
        headroom,
        storage,
        backend,
        calibration,
        speed,
        task_match,
        context,
        stability,
        quality_pref,
        openness_pref,
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RankedCatalog {
    pub ranking_formula_version: String,
    pub priority: Priority,
    pub task: Option<TaskCategory>,
    pub evaluations: Vec<BuildEvaluation>,
}

/// Ranks every build in the catalog, highest score first. Deterministic:
/// identical inputs (catalog, profile, calibration store, task, priority)
/// always produce the same order (ties broken by `catalog_id` for a
/// total, stable order).
pub fn rank(
    catalog: &Catalog,
    profile: &HardwareCapabilityProfile,
    calibration_store: &CalibrationStore,
    task: Option<TaskCategory>,
    priority: Priority,
) -> RankedCatalog {
    let mut evaluations: Vec<BuildEvaluation> = catalog
        .builds
        .iter()
        .filter(|b| task.is_none_or(|t| b.task_categories.contains(&t)))
        .map(|b| evaluate_build(b, profile, calibration_store, task, priority, None, None))
        .collect();

    evaluations.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.build.catalog_id.cmp(&b.build.catalog_id))
    });

    RankedCatalog {
        ranking_formula_version: RANKING_FORMULA_VERSION.to_string(),
        priority,
        task,
        evaluations,
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Recommendation {
    pub recommended: BuildEvaluation,
    pub safer_fallback: Option<BuildEvaluation>,
    pub stronger_optional: Option<BuildEvaluation>,
    pub explanation: explain::Explanation,
}

/// Picks a primary recommendation from a ranked catalog, plus a safer
/// fallback (smaller/more-certain, if one exists) and a stronger optional
/// build (bigger, still at least Constrained fit, if one exists) - never
/// the largest model merely because it technically fits: the *primary*
/// pick is always whatever ranked #1 under the requested priority, with
/// the fit-state gate already applied.
pub fn recommend(
    ranked: &RankedCatalog,
    profile: &HardwareCapabilityProfile,
) -> Option<Recommendation> {
    let recommended = ranked.evaluations.first()?.clone();

    let safer_fallback = ranked
        .evaluations
        .iter()
        .filter(|e| e.build.catalog_id != recommended.build.catalog_id)
        .filter(|e| matches!(e.fit.state, FitState::Excellent | FitState::Good))
        .filter(|e| e.build.file_size_bytes < recommended.build.file_size_bytes)
        .max_by(|a, b| {
            a.score
                .partial_cmp(&b.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .cloned();

    let stronger_optional = ranked
        .evaluations
        .iter()
        .filter(|e| e.build.catalog_id != recommended.build.catalog_id)
        .filter(|e| {
            matches!(
                e.fit.state,
                FitState::Excellent | FitState::Good | FitState::Constrained
            )
        })
        .filter(|e| e.build.parameter_count > recommended.build.parameter_count)
        .max_by_key(|e| e.build.parameter_count)
        .cloned();

    let explanation =
        explain::build_explanation(&recommended, &safer_fallback, &stronger_optional, profile);

    Some(Recommendation {
        recommended,
        safer_fallback,
        stronger_optional,
        explanation,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware::HardwareField;
    use std::path::Path;

    fn dev_catalog() -> Catalog {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("data/catalog/dev-catalog.json");
        crate::catalog::load_catalog(&path).expect("dev catalog must load")
    }

    fn seed_calibration() -> CalibrationStore {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("data/calibration/seed-calibration.json");
        CalibrationStore::load(&path).expect("seed calibration must load")
    }

    /// A generously-provisioned, CPU-only synthetic profile so most of the
    /// dev catalog gets Excellent/Good fit except the deliberately
    /// oversized 70B negative-test entry.
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

    #[test]
    fn ranking_is_deterministic() {
        let catalog = dev_catalog();
        let calib = seed_calibration();
        let profile = generous_profile(32_000_000_000);

        let r1 = rank(&catalog, &profile, &calib, None, Priority::Balanced);
        let r2 = rank(&catalog, &profile, &calib, None, Priority::Balanced);

        let ids1: Vec<_> = r1
            .evaluations
            .iter()
            .map(|e| e.build.catalog_id.clone())
            .collect();
        let ids2: Vec<_> = r2
            .evaluations
            .iter()
            .map(|e| e.build.catalog_id.clone())
            .collect();
        assert_eq!(ids1, ids2);
    }

    #[test]
    fn lowest_memory_priority_never_picks_the_oversized_70b_model() {
        let catalog = dev_catalog();
        let calib = seed_calibration();
        let profile = generous_profile(64_000_000_000); // even on a big machine

        let ranked = rank(&catalog, &profile, &calib, None, Priority::LowestMemory);
        let top = &ranked.evaluations[0];
        assert_ne!(top.build.catalog_id, "llama-3.1-70b-instruct-q4_k_m");
    }

    #[test]
    fn oversized_model_is_not_recommended_on_a_modest_machine() {
        let catalog = dev_catalog();
        let calib = seed_calibration();
        let profile = generous_profile(16_000_000_000);

        let evaluation = evaluate_build(
            catalog.get("llama-3.1-70b-instruct-q4_k_m").unwrap(),
            &profile,
            &calib,
            None,
            Priority::Balanced,
            None,
            None,
        );
        assert_eq!(evaluation.fit.state, FitState::NotRecommended);
    }

    #[test]
    fn coding_priority_ranks_the_coding_model_above_pure_chat_models_of_similar_size() {
        let catalog = dev_catalog();
        let calib = seed_calibration();
        let profile = generous_profile(32_000_000_000);

        let ranked = rank(&catalog, &profile, &calib, None, Priority::Coding);
        let top = &ranked.evaluations[0];
        assert_eq!(top.build.catalog_id, "qwen2.5-coder-1.5b-instruct-q4_k_m");
    }

    #[test]
    fn highest_quality_priority_prefers_a_larger_model_than_lowest_memory_priority() {
        let catalog = dev_catalog();
        let calib = seed_calibration();
        let profile = generous_profile(32_000_000_000);

        let low_mem = rank(&catalog, &profile, &calib, None, Priority::LowestMemory);
        let quality = rank(&catalog, &profile, &calib, None, Priority::HighestQuality);

        let low_mem_top = &low_mem.evaluations[0];
        let quality_top = &quality.evaluations[0];
        assert!(
            quality_top.build.parameter_count >= low_mem_top.build.parameter_count,
            "quality pick ({}) should not be smaller than lowest-memory pick ({})",
            quality_top.build.parameter_count,
            low_mem_top.build.parameter_count
        );
    }

    #[test]
    fn recommendation_includes_a_calibrated_explanation_for_the_seeded_model() {
        let catalog = dev_catalog();
        let calib = seed_calibration();
        let profile = generous_profile(16_000_000_000);

        let ranked = rank(&catalog, &profile, &calib, None, Priority::Balanced);
        let rec = recommend(&ranked, &profile).expect("should produce a recommendation");
        assert!(!rec.explanation.simple.is_empty());
        assert!(
            rec.explanation.simple.len() < 400,
            "simple explanation should stay short"
        );
    }

    #[test]
    fn task_filter_excludes_builds_without_the_requested_task_category() {
        let catalog = dev_catalog();
        let calib = seed_calibration();
        let profile = generous_profile(32_000_000_000);

        let ranked = rank(
            &catalog,
            &profile,
            &calib,
            Some(TaskCategory::Coding),
            Priority::Balanced,
        );
        assert!(
            ranked
                .evaluations
                .iter()
                .all(|e| e.build.task_categories.contains(&TaskCategory::Coding))
        );
    }

    #[test]
    fn unknown_license_fixture_still_ranks_but_never_claims_commercial_use_allowed() {
        // Synthetic Unknown-license data lives in test-fixtures.json, never
        // in the file production code actually loads (dev-catalog.json) -
        // see catalog::tests::production_catalog_never_contains_the_test_fixture_entry.
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("data/catalog/test-fixtures.json");
        let catalog = crate::catalog::load_catalog(&path).expect("test fixtures must load");
        let build = catalog.get("dev-fixture-unknown-license").unwrap();
        assert_eq!(build.commercial_use, crate::catalog::CommercialUse::Unknown);
        assert_ne!(build.commercial_use, crate::catalog::CommercialUse::Allowed);
    }
}
