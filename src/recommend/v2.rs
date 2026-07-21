//! Recommendation Engine v2 (Stage B.5): consumes the local
//! `preferences::Preferences` profile directly, rather than a single
//! `Priority`/`TaskCategory` pair, and scores every catalog build across
//! seven named, independently-inspectable dimensions instead of one
//! opaque total. Built as a preferences-aware layer *around* the
//! already-correct, already-tested v1 primitives
//! (`evaluate_build`/`fit::evaluate`/`estimator::estimate`/
//! `calibration::find_nearest`) - v2 never re-derives memory estimation,
//! fit classification, or calibration matching from scratch, so none of
//! that already-verified logic is at risk of a subtly different bug
//! creeping back in. v1 (`recommend::rank`/`recommend::recommend`,
//! still used by the Optimize page) is untouched.
//!
//! See `docs/recommendation-methodology-v2.md` for the full scoring
//! writeup and worked examples.

use super::{Priority, evaluate_build};
use crate::catalog::{Catalog, CommercialUse, License, ModelBuild, TaskCategory};
use crate::fit::FitState;
use crate::preferences::{GpuPreference, Preferences, SpeedQualityPriority, UseCase};
use crate::profile::HardwareCapabilityProfile;
use crate::runtime::Backend;
use serde::{Deserialize, Serialize};

pub const RECOMMEND_V2_FORMULA_VERSION: &str = "recommend-v2-stage-b5";

/// How much real evidence backs this entry's scores - distinct from any
/// single component score, since a build can score well while resting
/// entirely on curated/estimated data. Never conflate `High` with
/// "matches your preferences perfectly"; it only means the numbers
/// behind the score are trustworthy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfidenceLevel {
    /// A real calibration record closely matches this exact build
    /// (architecture, quantization, backend, parameter count all close).
    High,
    /// A calibration record exists for the architecture but is a looser
    /// match, or memory estimation used real per-file hyperparameters.
    Medium,
    /// Nothing but curated catalog metadata and a coarse formula backs
    /// this entry - a legitimate result, just not a measured one.
    Low,
    /// A required input (available RAM, backend detection) could not be
    /// determined at all - never silently treated as Low.
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecommendationCategory {
    BestMatch,
    BestArabic,
    Fastest,
    Balanced,
    BestQuality,
    BestCoding,
    BestDocument,
    HeavyButPossible,
    NotRecommended,
    Unsupported,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct ComponentScoresV2 {
    pub device_fit: f64,
    pub arabic: f64,
    pub task_fit: f64,
    pub speed: f64,
    pub quality: f64,
    pub trust: f64,
    pub license_fit: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct BuildRecommendationV2 {
    pub build: ModelBuild,
    pub overall_score: f64,
    pub component_scores: ComponentScoresV2,
    pub confidence: ConfidenceLevel,
    /// Empty when the build was not hard-rejected. Non-empty means this
    /// entry was excluded from "Best X" category tagging entirely - it
    /// still appears in the output (never silently dropped), just always
    /// tagged `Unsupported` and never a top pick.
    pub rejection_reasons: Vec<String>,
    pub explanation: String,
    pub categories: Vec<RecommendationCategory>,
    /// Internal fit-state carryover from v1's `fit::evaluate`, exposed so
    /// callers/tests can inspect exactly which tier drove `HeavyButPossible`/
    /// `NotRecommended`/`Unknown` without re-deriving it from the score.
    pub fit_state: Option<FitState>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecommendationSetV2 {
    pub formula_version: String,
    pub entries: Vec<BuildRecommendationV2>,
}

fn map_use_case(use_case: UseCase) -> TaskCategory {
    match use_case {
        UseCase::GeneralAssistant => TaskCategory::GeneralChat,
        UseCase::Coding => TaskCategory::Coding,
        UseCase::Documents => TaskCategory::DocumentAnalysis,
        UseCase::Writing => TaskCategory::Writing,
        UseCase::Summarization => TaskCategory::Summarization,
        UseCase::Reasoning => TaskCategory::Reasoning,
    }
}

fn map_priority(priority: SpeedQualityPriority) -> Priority {
    match priority {
        SpeedQualityPriority::Fastest => Priority::Fastest,
        SpeedQualityPriority::Balanced => Priority::Balanced,
        SpeedQualityPriority::BestQuality => Priority::HighestQuality,
    }
}

fn family_key(build: &ModelBuild) -> &str {
    build.family_id.as_deref().unwrap_or(&build.family)
}

fn backend_available_on_device(backend: Backend, profile: &HardwareCapabilityProfile) -> bool {
    match backend {
        Backend::Cpu => profile.backends.cpu,
        Backend::Cuda => profile.backends.cuda.value == Some(true),
        Backend::Vulkan => profile.backends.vulkan.value == Some(true),
    }
}

/// Checks that don't need a memory estimate first - cheap, and skip the
/// (comparatively expensive) `evaluate_build` call entirely when a build
/// is already disqualified on preference grounds alone.
fn pre_estimate_rejection(
    build: &ModelBuild,
    profile: &HardwareCapabilityProfile,
    prefs: &Preferences,
) -> Option<String> {
    let family = family_key(build);
    if prefs.excluded_families.iter().any(|f| f == family) {
        return Some(format!(
            "the {family:?} family is on your excluded-families list"
        ));
    }

    if prefs.commercial_use_required {
        if build.license == License::Unknown {
            return Some(
                "commercial use is required, but this build's license status is unknown - never assumed permissive".to_string(),
            );
        }
        if build.commercial_use != CommercialUse::Allowed {
            return Some(
                "commercial use is required, but this build's license does not explicitly establish commercial use as allowed".to_string(),
            );
        }
    }

    if !prefs.permitted_licenses.is_empty() {
        let permitted = match &build.license {
            License::Known { identifier } => prefs
                .permitted_licenses
                .iter()
                .any(|p| p.eq_ignore_ascii_case(identifier)),
            License::Unknown => false,
        };
        if !permitted {
            return Some("this build's license is not on your permitted-licenses list".to_string());
        }
    }

    if let Some(max) = prefs.max_download_size_bytes
        && build.file_size_bytes > max
    {
        return Some(format!(
            "download size ({} bytes) exceeds your {} byte maximum-download-size preference",
            build.file_size_bytes, max
        ));
    }

    if prefs.gpu_preference == GpuPreference::RequireGpu {
        let has_gpu_backend = (build.supported_backends.contains(&Backend::Cuda)
            && backend_available_on_device(Backend::Cuda, profile))
            || (build.supported_backends.contains(&Backend::Vulkan)
                && backend_available_on_device(Backend::Vulkan, profile));
        if !has_gpu_backend {
            return Some(
                "your preferences require a GPU, but no GPU backend this build supports was detected on this device".to_string(),
            );
        }
    }

    if prefs.cpu_only && !build.supported_backends.contains(&Backend::Cpu) {
        return Some(
            "your preferences require CPU-only, but this build does not list CPU among its supported backends".to_string(),
        );
    }

    let any_shared_backend = build
        .supported_backends
        .iter()
        .any(|b| backend_available_on_device(*b, profile));
    if !any_shared_backend {
        return Some("no backend this build supports was detected on this device".to_string());
    }

    None
}

fn forced_backend(
    build: &ModelBuild,
    profile: &HardwareCapabilityProfile,
    prefs: &Preferences,
) -> Backend {
    if prefs.cpu_only {
        return Backend::Cpu;
    }
    if prefs.gpu_preference != GpuPreference::NoPreference {
        if build.supported_backends.contains(&Backend::Cuda)
            && backend_available_on_device(Backend::Cuda, profile)
        {
            return Backend::Cuda;
        }
        if build.supported_backends.contains(&Backend::Vulkan)
            && backend_available_on_device(Backend::Vulkan, profile)
        {
            return Backend::Vulkan;
        }
    }
    super::preferred_backend(build, profile)
}

fn capability_score(level: crate::catalog::CapabilityLevel) -> f64 {
    use crate::catalog::CapabilityLevel::*;
    match level {
        Unknown => 0.0,
        Basic => 0.25,
        Good => 0.5,
        Strong => 0.75,
        Excellent => 1.0,
    }
}

fn trust_score(build: &ModelBuild) -> f64 {
    use crate::catalog::VerificationStatus::*;
    let verification_component: f64 = match build.artifact_verification {
        Unknown => 0.0,
        CuratedMetadata => 0.2,
        SourceVerified => 0.4,
        ArtifactUrlVerified => 0.6,
        ChecksumVerified => 0.75,
        Downloaded => 0.8,
        IntegrityVerified => 0.85,
        RuntimeCompatible => 0.9,
        Benchmarked => 0.95,
        DeviceVerified => 1.0,
        Unsupported => 0.0,
    };
    let license_component = match (&build.license, build.gated_access) {
        (License::Known { .. }, Some(false)) => 1.0,
        (License::Known { .. }, Some(true)) => 0.7,
        (License::Known { .. }, None) => 0.85,
        (License::Unknown, _) => 0.2,
    };
    // Weighted toward artifact verification - a build with a beautifully
    // documented license but zero artifact evidence is still a landing
    // page, not something BRUTE has actually confirmed exists.
    (verification_component * 0.65 + license_component * 0.35).clamp(0.0, 1.0)
}

fn license_fit_score(build: &ModelBuild, prefs: &Preferences) -> f64 {
    if prefs.commercial_use_required
        && (build.license == License::Unknown || build.commercial_use != CommercialUse::Allowed)
    {
        return 0.0;
    }
    if !prefs.permitted_licenses.is_empty() {
        let permitted = match &build.license {
            License::Known { identifier } => prefs
                .permitted_licenses
                .iter()
                .any(|p| p.eq_ignore_ascii_case(identifier)),
            License::Unknown => false,
        };
        return if permitted { 1.0 } else { 0.0 };
    }
    match &build.license {
        License::Known { .. } if build.commercial_use == CommercialUse::Allowed => 1.0,
        License::Known { .. } => 0.7,
        License::Unknown => 0.4,
    }
}

fn task_fit_score(build: &ModelBuild, use_case: UseCase, v1_task_match: f64) -> f64 {
    let capability_signal = match use_case {
        UseCase::Coding => Some(capability_score(build.coding_capability)),
        UseCase::Reasoning => Some(capability_score(build.reasoning_capability)),
        _ => None,
    };
    match capability_signal {
        // Blend the curated capability level with v1's already-correct
        // task_categories membership check - neither alone is enough:
        // membership without a capability rating is a coarse yes/no,
        // and a capability rating on a build that doesn't even claim the
        // task category is not credible either.
        Some(capability) if v1_task_match >= 0.9 => {
            (v1_task_match * 0.5 + capability * 0.5).clamp(0.0, 1.0)
        }
        _ => v1_task_match,
    }
}

fn arabic_score(build: &ModelBuild) -> f64 {
    let category_signal = if build.task_categories.contains(&TaskCategory::ArabicChat) {
        0.5
    } else {
        0.0
    };
    let capability_signal = capability_score(build.arabic_capability) * 0.5;
    (category_signal + capability_signal).clamp(0.0, 1.0)
}

fn quality_score(build: &ModelBuild, v1_quality_pref: f64) -> f64 {
    let capability = capability_score(build.general_quality);
    if capability > 0.0 {
        (v1_quality_pref * 0.4 + capability * 0.6).clamp(0.0, 1.0)
    } else {
        v1_quality_pref
    }
}

fn confidence_for(
    fit_state: Option<FitState>,
    calibration_proximity: Option<crate::calibration::CalibrationProximity>,
) -> ConfidenceLevel {
    use crate::calibration::CalibrationProximity::*;
    if fit_state == Some(FitState::Unknown) {
        return ConfidenceLevel::Unknown;
    }
    match calibration_proximity {
        Some(Exact) => ConfidenceLevel::High,
        Some(Close) => ConfidenceLevel::Medium,
        Some(Distant) | None => ConfidenceLevel::Low,
    }
}

fn build_explanation(
    build: &ModelBuild,
    scores: &ComponentScoresV2,
    fit_state: Option<FitState>,
    confidence: ConfidenceLevel,
) -> String {
    let mut parts = Vec::new();
    match fit_state {
        Some(FitState::Excellent) => parts.push("Fits your device comfortably.".to_string()),
        Some(FitState::Good) => parts.push("Fits your device well.".to_string()),
        Some(FitState::Constrained) => {
            parts.push("Fits, but with tight memory headroom.".to_string())
        }
        Some(FitState::Experimental) => parts
            .push("May fit, but this estimate is uncertain - treat as experimental.".to_string()),
        Some(FitState::NotRecommended) => parts
            .push("Likely exceeds your device's available memory - not recommended.".to_string()),
        Some(FitState::Unknown) => parts.push("Device fit could not be determined.".to_string()),
        None => {}
    }
    if scores.arabic >= 0.5 {
        parts.push("Strong Arabic support.".to_string());
    }
    if scores.trust >= 0.6 {
        parts.push("Artifact source/URL independently confirmed.".to_string());
    } else if scores.trust < 0.3 {
        parts.push("Not independently verified - curated metadata only.".to_string());
    }
    match confidence {
        ConfidenceLevel::High => {
            parts.push("Speed/quality figures are measured on comparable hardware.".to_string())
        }
        ConfidenceLevel::Low => {
            parts.push("Speed/quality figures are estimated, not measured.".to_string())
        }
        ConfidenceLevel::Unknown => {
            parts.push("Not enough device information to evaluate confidently.".to_string())
        }
        ConfidenceLevel::Medium => {}
    }
    if parts.is_empty() {
        format!("{} - no further detail available.", build.display_name)
    } else {
        parts.join(" ")
    }
}

/// Scores every build in `catalog` against `profile` and `prefs`. Never
/// drops a build silently - a hard-rejected build still appears with a
/// non-empty `rejection_reasons` and is always categorized `Unsupported`.
/// Deterministic: ties break on `catalog_id` ascending.
pub fn recommend_v2(
    catalog: &Catalog,
    profile: &HardwareCapabilityProfile,
    calibration_store: &crate::calibration::CalibrationStore,
    prefs: &Preferences,
) -> RecommendationSetV2 {
    let use_case = prefs.use_case;
    let task = map_use_case(use_case);
    let priority = map_priority(prefs.priority);

    let mut entries: Vec<BuildRecommendationV2> = catalog
        .builds
        .iter()
        .map(|build| {
            if let Some(reason) = pre_estimate_rejection(build, profile, prefs) {
                return BuildRecommendationV2 {
                    build: build.clone(),
                    overall_score: 0.0,
                    component_scores: ComponentScoresV2 {
                        device_fit: 0.0,
                        arabic: arabic_score(build),
                        task_fit: 0.0,
                        speed: 0.0,
                        quality: 0.0,
                        trust: trust_score(build),
                        license_fit: license_fit_score(build, prefs),
                    },
                    confidence: ConfidenceLevel::Unknown,
                    rejection_reasons: vec![reason],
                    explanation: format!("Not shown as a recommendation: {}.", build.display_name),
                    categories: vec![RecommendationCategory::Unsupported],
                    fit_state: None,
                };
            }

            let backend = forced_backend(build, profile, prefs);
            let evaluation = evaluate_build(
                build,
                profile,
                calibration_store,
                Some(task),
                priority,
                None,
                Some(backend),
            );

            let mut rejection_reasons = Vec::new();
            if let Some(max) = prefs.max_ram_bytes
                && evaluation.estimate.estimated_total_ram_bytes_high > max
            {
                rejection_reasons.push(format!(
                    "estimated RAM use ({} bytes) exceeds your {} byte maximum-RAM preference",
                    evaluation.estimate.estimated_total_ram_bytes_high, max
                ));
            }
            if let Some(max) = prefs.max_vram_bytes
                && let Some(vram_high) = evaluation.estimate.estimated_vram_bytes_high
                && vram_high > max
            {
                rejection_reasons.push(format!(
                    "estimated VRAM use ({vram_high} bytes) exceeds your {max} byte maximum-VRAM preference"
                ));
            }
            if evaluation.fit.state == FitState::NotRecommended {
                rejection_reasons.push(
                    "the high end of the estimated memory requirement exceeds available RAM after the OS safety reserve"
                        .to_string(),
                );
            }

            let device_fit = super::fit_state_multiplier(evaluation.fit.state);
            let arabic = arabic_score(build);
            let task_fit = task_fit_score(build, use_case, evaluation.component_scores.task_match);
            let speed = evaluation.component_scores.calibration.max(evaluation.component_scores.speed * 0.6);
            let quality = quality_score(build, evaluation.component_scores.quality_pref);
            let trust = trust_score(build);
            let license_fit = license_fit_score(build, prefs);

            let confidence = confidence_for(
                Some(evaluation.fit.state),
                evaluation.calibration_match.as_ref().map(|m| m.proximity),
            );

            // Weight table: preferences-driven, not a fixed Priority
            // lookup - language/arabic_priority shift weight onto the
            // arabic dimension directly, on top of whatever the v1
            // priority weighting already contributed to device_fit/speed/
            // quality/task_fit.
            let arabic_weight = if prefs.language == crate::preferences::LanguagePreference::Arabic {
                0.30
            } else if prefs.arabic_priority || prefs.language == crate::preferences::LanguagePreference::Both {
                0.12
            } else {
                0.02
            };
            let remaining = 1.0 - arabic_weight;
            let device_fit_w = remaining * 0.28;
            let task_fit_w = remaining * 0.22;
            let speed_w = remaining * 0.14;
            let quality_w = remaining * 0.16;
            let trust_w = remaining * 0.12;
            let license_fit_w = remaining * 0.08;

            let overall_score = if !rejection_reasons.is_empty() {
                0.0
            } else {
                (device_fit * device_fit_w
                    + arabic * arabic_weight
                    + task_fit * task_fit_w
                    + speed * speed_w
                    + quality * quality_w
                    + trust * trust_w
                    + license_fit * license_fit_w)
                    .clamp(0.0, 1.0)
            };

            let component_scores = ComponentScoresV2 {
                device_fit,
                arabic,
                task_fit,
                speed,
                quality,
                trust,
                license_fit,
            };
            let explanation = build_explanation(build, &component_scores, Some(evaluation.fit.state), confidence);

            let categories = if !rejection_reasons.is_empty() {
                vec![RecommendationCategory::Unsupported]
            } else {
                match evaluation.fit.state {
                    FitState::NotRecommended => vec![RecommendationCategory::NotRecommended],
                    FitState::Unknown => vec![RecommendationCategory::Unknown],
                    FitState::Experimental | FitState::Constrained => vec![RecommendationCategory::HeavyButPossible],
                    FitState::Excellent | FitState::Good => Vec::new(),
                }
            };

            BuildRecommendationV2 {
                build: build.clone(),
                overall_score,
                component_scores,
                confidence,
                rejection_reasons,
                explanation,
                categories,
                fit_state: Some(evaluation.fit.state),
            }
        })
        .collect();

    entries.sort_by(|a, b| {
        b.overall_score
            .partial_cmp(&a.overall_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.build.catalog_id.cmp(&b.build.catalog_id))
    });

    assign_best_category_tags(&mut entries, use_case);

    RecommendationSetV2 {
        formula_version: RECOMMEND_V2_FORMULA_VERSION.to_string(),
        entries,
    }
}

fn eligible_indices(entries: &[BuildRecommendationV2]) -> Vec<usize> {
    entries
        .iter()
        .enumerate()
        .filter(|(_, e)| e.rejection_reasons.is_empty())
        .map(|(i, _)| i)
        .collect()
}

fn tag_best<F: Fn(&BuildRecommendationV2) -> f64>(
    entries: &mut [BuildRecommendationV2],
    eligible: &[usize],
    score_fn: F,
    min_score: f64,
    category: RecommendationCategory,
) {
    let best = eligible
        .iter()
        .copied()
        .filter(|&i| score_fn(&entries[i]) > min_score)
        .max_by(|&a, &b| {
            score_fn(&entries[a])
                .partial_cmp(&score_fn(&entries[b]))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    entries[a]
                        .build
                        .catalog_id
                        .cmp(&entries[b].build.catalog_id)
                })
        });
    if let Some(i) = best {
        entries[i].categories.push(category);
    }
}

fn assign_best_category_tags(entries: &mut [BuildRecommendationV2], use_case: UseCase) {
    let eligible = eligible_indices(entries);
    if eligible.is_empty() {
        return;
    }

    // BestMatch: entries are already sorted by overall_score desc with a
    // deterministic tie-break, so the first eligible index is exactly it.
    entries[eligible[0]]
        .categories
        .push(RecommendationCategory::BestMatch);

    tag_best(
        entries,
        &eligible,
        |e| e.component_scores.arabic,
        0.0,
        RecommendationCategory::BestArabic,
    );
    tag_best(
        entries,
        &eligible,
        |e| e.component_scores.speed,
        0.0,
        RecommendationCategory::Fastest,
    );
    tag_best(
        entries,
        &eligible,
        |e| e.component_scores.quality,
        0.0,
        RecommendationCategory::BestQuality,
    );
    // Balanced: the entry with the highest *minimum* of its four primary
    // component scores - "no glaring weakness" rather than "wins on one
    // axis," a distinct notion from BestMatch (which follows the user's
    // actual weighting).
    tag_best(
        entries,
        &eligible,
        |e| {
            e.component_scores
                .device_fit
                .min(e.component_scores.task_fit)
                .min(e.component_scores.quality)
                .min(e.component_scores.speed)
        },
        0.0,
        RecommendationCategory::Balanced,
    );

    if use_case != UseCase::Coding {
        let coding_eligible: Vec<usize> = eligible
            .iter()
            .copied()
            .filter(|&i| {
                entries[i]
                    .build
                    .task_categories
                    .contains(&TaskCategory::Coding)
            })
            .collect();
        tag_best(
            entries,
            &coding_eligible,
            |e| capability_score(e.build.coding_capability).max(0.5),
            0.0,
            RecommendationCategory::BestCoding,
        );
    } else {
        tag_best(
            entries,
            &eligible,
            |e| {
                capability_score(e.build.coding_capability).max(
                    if e.build.task_categories.contains(&TaskCategory::Coding) {
                        0.5
                    } else {
                        0.0
                    },
                )
            },
            0.0,
            RecommendationCategory::BestCoding,
        );
    }

    let document_eligible: Vec<usize> = eligible
        .iter()
        .copied()
        .filter(|&i| {
            entries[i]
                .build
                .task_categories
                .contains(&TaskCategory::DocumentAnalysis)
        })
        .collect();
    tag_best(
        entries,
        &document_eligible,
        |e| e.build.context_length.unwrap_or(0) as f64,
        0.0,
        RecommendationCategory::BestDocument,
    );
}

#[cfg(test)]
mod tests;
