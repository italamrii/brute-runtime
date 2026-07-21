//! Fit Classification Engine: turns a `MemoryEstimate` + catalog build +
//! machine profile into one of six user-facing states. Every rule is an
//! explicit, ordered boundary check - see `docs/model-fit-classification.md`
//! for the full rationale and worked examples. `Unknown` is checked before
//! `NotRecommended` and is never collapsed into it: a build we simply
//! cannot evaluate is not the same claim as a build we evaluated and
//! rejected.

use crate::catalog::ModelBuild;
use crate::estimator::{EstimateQuality, MemoryEstimate};
use crate::profile::HardwareCapabilityProfile;
use crate::runtime::Backend;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FitState {
    Excellent,
    Good,
    Constrained,
    Experimental,
    NotRecommended,
    Unknown,
}

impl FitState {
    pub fn as_str(&self) -> &'static str {
        match self {
            FitState::Excellent => "excellent",
            FitState::Good => "good",
            FitState::Constrained => "constrained",
            FitState::Experimental => "experimental",
            FitState::NotRecommended => "not_recommended",
            FitState::Unknown => "unknown",
        }
    }
}

/// Ratio thresholds for the headroom-driven tiers, as a fraction of total
/// available RAM. Versioned alongside the estimator's formula version
/// since fit classification is downstream of it.
pub const FIT_RULES_VERSION: &str = "stage1-v1";
const EXCELLENT_HEADROOM_RATIO: f64 = 0.5;
const GOOD_HEADROOM_RATIO: f64 = 0.2;

#[derive(Debug, Clone, Serialize)]
pub struct FitResult {
    pub state: FitState,
    pub reasons: Vec<String>,
    pub headroom_ratio: Option<f64>,
    pub rules_version: String,
}

/// `has_calibration_support` should be true when a calibration record
/// exists for this build (or a close match - same architecture/quant
/// family within a reasonable parameter-count distance); it is the input
/// that lets `Experimental` reflect genuine uncertainty rather than just
/// tight headroom. Computed by the caller (see `calibration` module) so
/// this module stays decoupled from the calibration store.
pub fn evaluate(
    build: &ModelBuild,
    estimate: &MemoryEstimate,
    profile: &HardwareCapabilityProfile,
    has_calibration_support: bool,
) -> FitResult {
    let mut reasons = Vec::new();

    let backend_supported = backend_availability(build, profile);
    let Some(backend_supported) = backend_supported else {
        reasons.push(
            "could not determine whether this machine has a backend this build supports"
                .to_string(),
        );
        return unknown(reasons);
    };

    let Some(available_ram_bytes) = estimate.available_ram_bytes.value else {
        reasons.push("available RAM on this machine is unknown".to_string());
        return unknown(reasons);
    };

    if !backend_supported {
        reasons.push(format!(
            "no backend this build supports ({:?}) is available on this machine",
            build.supported_backends
        ));
        return not_recommended(reasons);
    }

    let disk_sufficient = disk_sufficient(build, profile);
    if disk_sufficient == Some(false) {
        reasons.push("insufficient free disk space for this build's file size".to_string());
        return not_recommended(reasons);
    }
    if disk_sufficient.is_none() {
        reasons.push("free disk space could not be determined - assumed sufficient".to_string());
    }

    let Some(headroom) = estimate.headroom_bytes else {
        reasons.push("could not compute a memory headroom for this build".to_string());
        return unknown(reasons);
    };

    if headroom < 0 {
        reasons.push(
            "the high end of the estimated memory requirement exceeds available RAM after the OS safety reserve"
                .to_string(),
        );
        return not_recommended(reasons);
    }

    let headroom_ratio = headroom as f64 / available_ram_bytes as f64;

    if !estimate.context_realistic {
        reasons.push(format!(
            "requested context {} exceeds what this build is documented to support",
            estimate.context_requested
        ));
        return FitResult {
            state: FitState::Constrained,
            reasons,
            headroom_ratio: Some(headroom_ratio),
            rules_version: FIT_RULES_VERSION.to_string(),
        };
    }

    enum HeadroomTier {
        Excellent,
        Good,
        Constrained,
    }

    let tier = if headroom_ratio >= EXCELLENT_HEADROOM_RATIO {
        HeadroomTier::Excellent
    } else if headroom_ratio >= GOOD_HEADROOM_RATIO {
        reasons.push(
            "adequate headroom; some compromise on context/batch size may help stability"
                .to_string(),
        );
        HeadroomTier::Good
    } else {
        reasons.push(
            "headroom is tight - reduced context, batch size, or GPU offload is likely required"
                .to_string(),
        );
        HeadroomTier::Constrained
    };

    // An uncalibrated coarse estimate downgrades the verdict by one tier
    // rather than jumping straight to Experimental: a model that would
    // still fit even under a much more pessimistic estimate (huge
    // headroom) shouldn't be branded "substantial uncertainty" just
    // because no calibration exists yet - the margin itself already
    // absorbs that uncertainty. Only a tight-margin build genuinely earns
    // Experimental, since there the *combination* of tight margin and an
    // unverified estimate is a real, compounding risk.
    let uncertain =
        estimate.quality == EstimateQuality::CoarseApproximation && !has_calibration_support;
    if uncertain {
        reasons.push(
            "no real per-file hyperparameters or nearby calibration data back this estimate - reduced confidence, downgraded one tier"
                .to_string(),
        );
    }

    let state = match (tier, uncertain) {
        (HeadroomTier::Excellent, false) => FitState::Excellent,
        (HeadroomTier::Excellent, true) => FitState::Good,
        (HeadroomTier::Good, false) => FitState::Good,
        (HeadroomTier::Good, true) => FitState::Experimental,
        (HeadroomTier::Constrained, false) => FitState::Constrained,
        (HeadroomTier::Constrained, true) => FitState::Experimental,
    };

    FitResult {
        state,
        reasons,
        headroom_ratio: Some(headroom_ratio),
        rules_version: FIT_RULES_VERSION.to_string(),
    }
}

fn unknown(reasons: Vec<String>) -> FitResult {
    FitResult {
        state: FitState::Unknown,
        reasons,
        headroom_ratio: None,
        rules_version: FIT_RULES_VERSION.to_string(),
    }
}

fn not_recommended(reasons: Vec<String>) -> FitResult {
    FitResult {
        state: FitState::NotRecommended,
        reasons,
        headroom_ratio: None,
        rules_version: FIT_RULES_VERSION.to_string(),
    }
}

/// `None` means we couldn't determine backend availability at all (should
/// not normally happen since CPU is always known-available); `Some(false)`
/// means none of the build's supported backends are usable here.
fn backend_availability(build: &ModelBuild, profile: &HardwareCapabilityProfile) -> Option<bool> {
    let mut any_known = false;
    for backend in &build.supported_backends {
        let available = match backend {
            Backend::Cpu => Some(true),
            Backend::Cuda => profile.backends.cuda.value,
            Backend::Vulkan => profile.backends.vulkan.value,
        };
        if let Some(available) = available {
            any_known = true;
            if available {
                return Some(true);
            }
        }
    }
    if any_known { Some(false) } else { None }
}

fn disk_sufficient(build: &ModelBuild, profile: &HardwareCapabilityProfile) -> Option<bool> {
    let free = profile.storage.as_ref()?.free_bytes.value?;
    // 10% margin above the raw download size for extraction/temp files.
    let required = build
        .file_size_bytes
        .saturating_add(build.file_size_bytes / 10);
    Some(free >= required)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{CommercialUse, License, TaskCategory};
    use crate::estimator::EstimateQuality;
    use crate::hardware::HardwareField;
    use crate::provenance::Valued;

    fn build(
        supported_backends: Vec<Backend>,
        file_size_bytes: u64,
        context_sizes: Vec<u32>,
    ) -> ModelBuild {
        ModelBuild {
            catalog_id: "test".to_string(),
            family: "test".to_string(),
            display_name: "Test".to_string(),
            publisher: "Test".to_string(),
            official_source_url: "https://example.com".to_string(),
            official_repository_id: "test/test".to_string(),
            filename: "test.gguf".to_string(),
            architecture: "llama".to_string(),
            parameter_count: 1_000_000_000,
            quantization: "Q4_K_M".to_string(),
            file_size_bytes,
            estimated_disk_bytes: Some(file_size_bytes),
            estimated_runtime_memory_bytes: None,
            min_recommended_ram_bytes: 1_000_000_000,
            min_recommended_vram_bytes: None,
            supported_backends,
            context_sizes,
            task_categories: vec![TaskCategory::GeneralChat],
            short_description: "test".to_string(),
            strength: "test".to_string(),
            limitation: "test".to_string(),
            license: License::Known {
                identifier: "apache-2.0".to_string(),
            },
            commercial_use: CommercialUse::Allowed,
            gated_access: Some(false),
            metadata_provenance: "test".to_string(),
            last_reviewed: "2026-07-18".to_string(),
            family_id: None,
            model_id: None,
            artifact_id: None,
            exact_model_name: None,
            version: None,
            context_length: None,
            file_format: None,
            runtime_provider: None,
            minimum_runtime_version: None,
            license_url: None,
            source_verification: Default::default(),
            artifact_verification: Default::default(),
            exact_artifact_url: None,
            checksum_algorithm: None,
            checksum_value: None,
            checksum_source: None,
            curator_notes: None,
            arabic_capability: Default::default(),
            coding_capability: Default::default(),
            reasoning_capability: Default::default(),
            general_quality: Default::default(),
            speed_category: Default::default(),
            evidence_source: Default::default(),
            benchmark_confidence: None,
        }
    }

    fn estimate(
        quality: EstimateQuality,
        headroom_bytes: Option<i64>,
        available_ram_bytes: Option<u64>,
        context_realistic: bool,
    ) -> MemoryEstimate {
        MemoryEstimate {
            formula_version: "stage1-v1".to_string(),
            quality,
            model_weights_bytes: Valued::catalog(1_000_000_000u64, "test"),
            kv_cache_bytes_low: Valued::inferred(1_000_000u64, "test"),
            kv_cache_bytes_high: Valued::inferred(2_000_000u64, "test"),
            runtime_overhead_bytes_low: 50_000_000,
            runtime_overhead_bytes_high: 300_000_000,
            os_safety_reserve_bytes: 1_610_612_736,
            estimated_total_ram_bytes_low: 1_100_000_000,
            estimated_total_ram_bytes_high: 1_300_000_000,
            estimated_vram_bytes_low: None,
            estimated_vram_bytes_high: None,
            available_ram_bytes: match available_ram_bytes {
                Some(v) => HardwareField::measured(v, "test").into(),
                None => HardwareField::<u64>::unavailable("test").into(),
            },
            headroom_bytes,
            paging_likely: headroom_bytes.map(|h| h < 0).unwrap_or(false),
            context_requested: 2048,
            context_realistic,
            assumptions: vec![],
            missing_inputs: vec![],
        }
    }

    fn profile_with_backends(
        cuda: Option<bool>,
        vulkan: Option<bool>,
        free_disk: Option<u64>,
    ) -> HardwareCapabilityProfile {
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
        profile.storage = free_disk.map(|f| crate::hardware::StorageReport {
            path_queried: "C:\\".to_string(),
            free_bytes: HardwareField::measured(f, "test"),
            total_bytes: HardwareField::measured(f * 2, "test"),
        });
        profile
    }

    #[test]
    fn excellent_when_headroom_is_comfortable_and_calibrated() {
        let b = build(vec![Backend::Cpu], 1_000_000_000, vec![2048, 4096]);
        let est = estimate(
            EstimateQuality::PreciseFormula,
            Some(6_000_000_000),
            Some(8_000_000_000),
            true,
        );
        let profile = profile_with_backends(None, None, Some(100_000_000_000));
        let result = evaluate(&b, &est, &profile, true);
        assert_eq!(result.state, FitState::Excellent);
    }

    #[test]
    fn good_when_headroom_is_adequate_but_not_comfortable() {
        let b = build(vec![Backend::Cpu], 1_000_000_000, vec![2048, 4096]);
        // ratio ~0.3
        let est = estimate(
            EstimateQuality::PreciseFormula,
            Some(2_400_000_000),
            Some(8_000_000_000),
            true,
        );
        let profile = profile_with_backends(None, None, Some(100_000_000_000));
        let result = evaluate(&b, &est, &profile, true);
        assert_eq!(result.state, FitState::Good);
    }

    #[test]
    fn constrained_when_headroom_is_tight() {
        let b = build(vec![Backend::Cpu], 1_000_000_000, vec![2048, 4096]);
        // ratio ~0.05
        let est = estimate(
            EstimateQuality::PreciseFormula,
            Some(400_000_000),
            Some(8_000_000_000),
            true,
        );
        let profile = profile_with_backends(None, None, Some(100_000_000_000));
        let result = evaluate(&b, &est, &profile, true);
        assert_eq!(result.state, FitState::Constrained);
    }

    #[test]
    fn not_recommended_when_headroom_is_negative() {
        let b = build(vec![Backend::Cpu], 1_000_000_000, vec![2048, 4096]);
        let est = estimate(
            EstimateQuality::PreciseFormula,
            Some(-500_000_000),
            Some(2_000_000_000),
            true,
        );
        let profile = profile_with_backends(None, None, Some(100_000_000_000));
        let result = evaluate(&b, &est, &profile, true);
        assert_eq!(result.state, FitState::NotRecommended);
    }

    #[test]
    fn not_recommended_when_no_supported_backend_is_available() {
        let b = build(vec![Backend::Cuda], 1_000_000_000, vec![2048]);
        let est = estimate(
            EstimateQuality::PreciseFormula,
            Some(6_000_000_000),
            Some(8_000_000_000),
            true,
        );
        let profile = profile_with_backends(Some(false), Some(false), Some(100_000_000_000));
        let result = evaluate(&b, &est, &profile, true);
        assert_eq!(result.state, FitState::NotRecommended);
    }

    #[test]
    fn not_recommended_when_disk_is_insufficient() {
        let b = build(vec![Backend::Cpu], 10_000_000_000, vec![2048]);
        let est = estimate(
            EstimateQuality::PreciseFormula,
            Some(6_000_000_000),
            Some(8_000_000_000),
            true,
        );
        let profile = profile_with_backends(None, None, Some(1_000_000_000)); // 1GB free, model is 10GB
        let result = evaluate(&b, &est, &profile, true);
        assert_eq!(result.state, FitState::NotRecommended);
    }

    #[test]
    fn unknown_when_available_ram_is_unknown() {
        let b = build(vec![Backend::Cpu], 1_000_000_000, vec![2048]);
        let est = estimate(EstimateQuality::PreciseFormula, None, None, true);
        let profile = profile_with_backends(None, None, Some(100_000_000_000));
        let result = evaluate(&b, &est, &profile, true);
        assert_eq!(result.state, FitState::Unknown);
    }

    #[test]
    fn unknown_is_never_collapsed_into_not_recommended() {
        let b = build(vec![Backend::Cpu], 1_000_000_000, vec![2048]);
        let est = estimate(EstimateQuality::PreciseFormula, None, None, true);
        let profile = profile_with_backends(None, None, Some(100_000_000_000));
        let result = evaluate(&b, &est, &profile, true);
        assert_ne!(result.state, FitState::NotRecommended);
        assert_eq!(result.state, FitState::Unknown);
    }

    #[test]
    fn uncalibrated_coarse_estimate_downgrades_excellent_to_good_not_experimental() {
        // Huge headroom (ratio 0.75) absorbs the estimate's own
        // uncertainty - being uncalibrated should cost one tier
        // (Excellent -> Good), not force a jump all the way to
        // Experimental. Experimental is reserved for when uncertainty
        // *and* tight margin compound (see the test below).
        let b = build(vec![Backend::Cpu], 1_000_000_000, vec![2048, 4096]);
        let est = estimate(
            EstimateQuality::CoarseApproximation,
            Some(6_000_000_000),
            Some(8_000_000_000),
            true,
        );
        let profile = profile_with_backends(None, None, Some(100_000_000_000));
        let result = evaluate(&b, &est, &profile, false);
        assert_eq!(result.state, FitState::Good);
    }

    #[test]
    fn uncalibrated_coarse_estimate_with_tight_headroom_is_experimental() {
        let b = build(vec![Backend::Cpu], 1_000_000_000, vec![2048, 4096]);
        // ratio ~0.05 - Constrained tier, then downgraded for uncertainty.
        let est = estimate(
            EstimateQuality::CoarseApproximation,
            Some(400_000_000),
            Some(8_000_000_000),
            true,
        );
        let profile = profile_with_backends(None, None, Some(100_000_000_000));
        let result = evaluate(&b, &est, &profile, false);
        assert_eq!(result.state, FitState::Experimental);
    }

    #[test]
    fn coarse_estimate_is_not_experimental_when_calibration_backs_it() {
        let b = build(vec![Backend::Cpu], 1_000_000_000, vec![2048, 4096]);
        let est = estimate(
            EstimateQuality::CoarseApproximation,
            Some(6_000_000_000),
            Some(8_000_000_000),
            true,
        );
        let profile = profile_with_backends(None, None, Some(100_000_000_000));
        let result = evaluate(&b, &est, &profile, true);
        assert_eq!(result.state, FitState::Excellent);
    }

    #[test]
    fn deterministic_same_inputs_produce_same_result() {
        let b = build(vec![Backend::Cpu], 1_000_000_000, vec![2048, 4096]);
        let est = estimate(
            EstimateQuality::PreciseFormula,
            Some(6_000_000_000),
            Some(8_000_000_000),
            true,
        );
        let profile = profile_with_backends(None, None, Some(100_000_000_000));
        let r1 = evaluate(&b, &est, &profile, true);
        let r2 = evaluate(&b, &est, &profile, true);
        assert_eq!(r1.state, r2.state);
        assert_eq!(r1.headroom_ratio, r2.headroom_ratio);
    }
}
