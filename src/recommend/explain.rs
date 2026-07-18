//! Two-tier explainability: a short plain-language summary for ordinary
//! users, and a technical breakdown of exactly how the recommendation was
//! computed. Simple descriptions stay short by design - no marketing copy,
//! no long model-card prose.

use super::BuildEvaluation;
use crate::calibration::{CalibrationMatch, CalibrationProximity};
use crate::fit::FitState;
use crate::profile::HardwareCapabilityProfile;
use serde::Serialize;

/// Phrases a calibration match's throughput numbers according to how close
/// it actually is - never presents a different-sized/quantized build's
/// real measurement as if it were a direct prediction for the build in
/// hand. This is the one place that wording is decided, so `main.rs`'s
/// terminal output and any future caller stay consistent and honest.
pub fn describe_calibration_performance(m: &CalibrationMatch) -> String {
    match m.proximity {
        CalibrationProximity::Exact => format!(
            "Expected performance (measured on this exact build): {:.1} tok/s prompt, {:.1} tok/s generation",
            m.record.prompt_processing_tokens_per_second, m.record.generation_tokens_per_second
        ),
        CalibrationProximity::Close => format!(
            "Reference performance from a similar but not identical calibrated build ({}): {:.1} tok/s prompt, {:.1} tok/s generation - a rough guide only, not a measurement of this build",
            m.distance_notes.join("; "),
            m.record.prompt_processing_tokens_per_second,
            m.record.generation_tokens_per_second
        ),
        CalibrationProximity::Distant => format!(
            "Only a distant calibration reference exists ({}): {:.1} tok/s prompt, {:.1} tok/s generation on a substantially different build - not a meaningful prediction for this one",
            m.distance_notes.join("; "),
            m.record.prompt_processing_tokens_per_second,
            m.record.generation_tokens_per_second
        ),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TechnicalExplanation {
    pub detected_resources: String,
    pub memory_calculation: String,
    pub calibration_used: Option<String>,
    pub backend_assumptions: String,
    pub context_and_batch_assumptions: String,
    pub fit_thresholds: String,
    pub confidence_calculation: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Explanation {
    pub simple: String,
    pub technical: TechnicalExplanation,
}

pub fn build_explanation(
    recommended: &BuildEvaluation,
    safer_fallback: &Option<BuildEvaluation>,
    stronger_optional: &Option<BuildEvaluation>,
    profile: &HardwareCapabilityProfile,
) -> Explanation {
    Explanation {
        simple: simple_explanation(recommended, safer_fallback, stronger_optional),
        technical: technical_explanation(recommended, profile),
    }
}

fn simple_explanation(
    recommended: &BuildEvaluation,
    safer_fallback: &Option<BuildEvaluation>,
    stronger_optional: &Option<BuildEvaluation>,
) -> String {
    let mut s = match recommended.fit.state {
        FitState::Excellent => "Fast and comfortable on your machine.".to_string(),
        FitState::Good => "Should run well on your machine, possibly with minor tuning.".to_string(),
        FitState::Constrained => "Will run, but likely only with a reduced context size or slower settings.".to_string(),
        FitState::Experimental => "May work, but this hasn't been backed by a real benchmark on hardware like yours - treat memory and speed numbers as rough guesses.".to_string(),
        FitState::NotRecommended => "Not recommended - this build is unlikely to fit your machine's memory or backend.".to_string(),
        FitState::Unknown => "Not enough information about your machine or this build to tell whether it fits.".to_string(),
    };

    if matches!(recommended.fit.state, FitState::Excellent | FitState::Good) {
        if let Some(stronger) = stronger_optional {
            s.push_str(&format!(
                " A larger build ({}) may give better quality but will use significantly more memory.",
                stronger.build.display_name
            ));
        }
    } else if let Some(safer) = safer_fallback {
        s.push_str(&format!(
            " {} is a safer, smaller alternative that should fit more comfortably.",
            safer.build.display_name
        ));
    }

    s
}

fn technical_explanation(
    recommended: &BuildEvaluation,
    profile: &HardwareCapabilityProfile,
) -> TechnicalExplanation {
    let detected_resources = format!(
        "CPU: {} ({} physical / {} logical cores); RAM: {} total, {} currently available; GPU(s): {}",
        profile.cpu.brand.value.as_deref().unwrap_or("unknown"),
        profile
            .cpu
            .physical_cores
            .value
            .map(|v| v.to_string())
            .unwrap_or_else(|| "unknown".to_string()),
        profile
            .cpu
            .logical_cores
            .value
            .map(|v| v.to_string())
            .unwrap_or_else(|| "unknown".to_string()),
        profile
            .memory
            .total_bytes
            .value
            .map(|v| format!("{v} bytes"))
            .unwrap_or_else(|| "unknown".to_string()),
        profile
            .memory
            .available_bytes
            .value
            .map(|v| format!("{v} bytes"))
            .unwrap_or_else(|| "unknown".to_string()),
        if profile.gpus.is_empty() {
            "none detected".to_string()
        } else {
            profile
                .gpus
                .iter()
                .map(|g| g.name.clone())
                .collect::<Vec<_>>()
                .join(", ")
        }
    );

    let est = &recommended.estimate;
    let memory_calculation = format!(
        "formula {}: weights={} bytes (catalog) + KV cache {}-{} bytes ({:?}) + runtime overhead {}-{} bytes + OS reserve {} bytes = estimated total {}-{} bytes; available RAM {} bytes -> headroom {} bytes",
        est.formula_version,
        est.model_weights_bytes.value.unwrap_or(0),
        est.kv_cache_bytes_low.value.unwrap_or(0),
        est.kv_cache_bytes_high.value.unwrap_or(0),
        est.quality,
        est.runtime_overhead_bytes_low,
        est.runtime_overhead_bytes_high,
        est.os_safety_reserve_bytes,
        est.estimated_total_ram_bytes_low,
        est.estimated_total_ram_bytes_high,
        est.available_ram_bytes
            .value
            .map(|v| v.to_string())
            .unwrap_or_else(|| "unknown".to_string()),
        est.headroom_bytes
            .map(|v| v.to_string())
            .unwrap_or_else(|| "unknown".to_string()),
    );

    let calibration_used = recommended.calibration_match.as_ref().map(|m| {
        format!(
            "nearest calibration record ({:?} match): {} prompt tok/s, {} gen tok/s, {:?} stability, recorded {}{}",
            m.proximity,
            m.record.prompt_processing_tokens_per_second,
            m.record.generation_tokens_per_second,
            m.record.stability,
            m.record.recorded_at_rfc3339,
            if m.distance_notes.is_empty() {
                String::new()
            } else {
                format!(" - differences: {}", m.distance_notes.join("; "))
            }
        )
    });

    let backend_assumptions = format!(
        "backend={:?}, full_gpu_offload assumption applied where applicable - partial offload is not modeled precisely in Stage 1",
        recommended.build.supported_backends
    );

    let context_and_batch_assumptions = format!(
        "requested context={}, context_realistic={}",
        est.context_requested, est.context_realistic
    );

    let fit_thresholds = format!(
        "fit rules {}: Excellent >=50% headroom, Good >=20% headroom, Constrained <20% (all fit-state dependent gates applied before headroom ratio); this build's headroom_ratio={:?}",
        recommended.fit.rules_version, recommended.fit.headroom_ratio
    );

    let confidence_calculation = format!(
        "ranking formula {}: score = weighted sum of 10 named components x fit-state multiplier ({:?} -> see docs/recommendation-methodology.md for the full weight table)",
        super::RANKING_FORMULA_VERSION,
        recommended.fit.state
    );

    TechnicalExplanation {
        detected_resources,
        memory_calculation,
        calibration_used,
        backend_assumptions,
        context_and_batch_assumptions,
        fit_thresholds,
        confidence_calculation,
    }
}
