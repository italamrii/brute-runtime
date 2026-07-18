//! Model Requirement Estimator: predicts disk/RAM/VRAM needs for a
//! catalog build on a specific machine, before the user downloads
//! anything. Every number is a range with an explicit formula version,
//! documented assumptions, and a list of what inputs were missing - never
//! a single falsely-precise value. See `docs/model-memory-estimation.md`.

pub mod formulas;

use crate::catalog::ModelBuild;
use crate::models::gguf::GgufHyperparameters;
use crate::profile::HardwareCapabilityProfile;
use crate::provenance::Valued;
use crate::runtime::Backend;
use serde::Serialize;

/// Whether the KV-cache figure used the exact per-architecture formula
/// (real hyperparameters were available, e.g. from an already-downloaded
/// file) or a coarse catalog-only range heuristic. The fit classifier and
/// recommendation engine both read this to decide how much to trust the
/// estimate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EstimateQuality {
    PreciseFormula,
    CoarseApproximation,
}

#[derive(Debug, Clone, Serialize)]
pub struct EstimationConfig {
    pub context_length: u32,
    pub backend: Backend,
    /// Stage 1 models GPU offload as all-or-nothing (either the full model
    /// fits on the GPU or we estimate CPU-only) - partial-offload memory
    /// splitting is a documented limitation, not modeled precisely yet.
    pub full_gpu_offload: bool,
    pub kv_cache_bytes_per_element: u64,
    pub os_safety_reserve_bytes: u64,
}

impl EstimationConfig {
    pub fn for_backend(backend: Backend, context_length: u32) -> Self {
        Self {
            context_length,
            backend,
            full_gpu_offload: backend != Backend::Cpu,
            kv_cache_bytes_per_element: formulas::DEFAULT_KV_CACHE_BYTES_PER_ELEMENT,
            os_safety_reserve_bytes: formulas::DEFAULT_OS_SAFETY_RESERVE_BYTES,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryEstimate {
    pub formula_version: String,
    pub quality: EstimateQuality,
    pub model_weights_bytes: Valued<u64>,
    pub kv_cache_bytes_low: Valued<u64>,
    pub kv_cache_bytes_high: Valued<u64>,
    pub runtime_overhead_bytes_low: u64,
    pub runtime_overhead_bytes_high: u64,
    pub os_safety_reserve_bytes: u64,
    pub estimated_total_ram_bytes_low: u64,
    pub estimated_total_ram_bytes_high: u64,
    pub estimated_vram_bytes_low: Option<u64>,
    pub estimated_vram_bytes_high: Option<u64>,
    pub available_ram_bytes: Valued<u64>,
    /// Available RAM minus OS reserve minus the *high* (conservative)
    /// total-RAM estimate. Negative means the high estimate would not fit.
    pub headroom_bytes: Option<i64>,
    pub paging_likely: bool,
    pub context_requested: u32,
    pub context_realistic: bool,
    pub assumptions: Vec<String>,
    pub missing_inputs: Vec<String>,
}

/// `hyperparameters` is `Some` only when the exact file has actually been
/// parsed (e.g. via `brute model inspect`) - for pure catalog-based
/// pre-download estimation it will be `None` and the KV-cache figure falls
/// back to the coarse range heuristic.
pub fn estimate(
    build: &ModelBuild,
    hyperparameters: Option<&GgufHyperparameters>,
    profile: &HardwareCapabilityProfile,
    config: &EstimationConfig,
) -> MemoryEstimate {
    let mut assumptions = Vec::new();
    let mut missing_inputs = Vec::new();

    let model_weights_bytes = Valued::catalog(
        build.file_size_bytes,
        "catalog file_size_bytes (curated metadata, not independently re-verified unless noted in the build's own metadata_provenance)",
    );

    if let Some(formula_estimate) =
        formulas::estimate_weights_bytes_from_params(build.parameter_count, &build.quantization)
    {
        let ratio = formula_estimate as f64 / build.file_size_bytes as f64;
        if !(0.5..2.0).contains(&ratio) {
            assumptions.push(format!(
                "catalog file_size_bytes ({}) differs from a parameter_count x bits-per-weight cross-check ({formula_estimate}) by more than 2x - possible catalog data-entry error",
                build.file_size_bytes
            ));
        }
    } else {
        assumptions.push(format!(
            "quantization {:?} not in the bits-per-weight table; skipped the weights-size cross-check",
            build.quantization
        ));
    }

    let (quality, kv_low, kv_high) =
        estimate_kv_cache(build, hyperparameters, config, &mut assumptions);

    let (overhead_low, overhead_high) = formulas::runtime_overhead_bytes_range();
    assumptions.push(format!(
        "runtime overhead assumed {overhead_low}-{overhead_high} bytes (compute buffers, tokenizer, process baseline) - see docs/calibration-methodology.md"
    ));
    assumptions.push(format!(
        "OS safety reserve assumed {} bytes, reserved for the OS and other running applications",
        config.os_safety_reserve_bytes
    ));

    let weights = build.file_size_bytes;
    let total_low = weights
        .saturating_add(kv_low.value.unwrap_or(0))
        .saturating_add(overhead_low);
    let total_high = weights
        .saturating_add(kv_high.value.unwrap_or(0))
        .saturating_add(overhead_high);

    let available_ram_bytes: Valued<u64> = crate::hardware::HardwareField {
        value: profile.memory.available_bytes.value,
        confidence: profile.memory.available_bytes.confidence,
        source: profile.memory.available_bytes.source.clone(),
    }
    .into();

    let headroom_bytes = available_ram_bytes
        .value
        .map(|avail| avail as i64 - config.os_safety_reserve_bytes as i64 - total_high as i64);

    if available_ram_bytes.value.is_none() {
        missing_inputs.push("current available RAM is unknown on this machine".to_string());
    }

    let paging_likely = headroom_bytes.map(|h| h < 0).unwrap_or(false);
    if headroom_bytes.is_none() {
        missing_inputs
            .push("cannot determine paging risk without a known available-RAM value".to_string());
    }

    let context_realistic = build
        .context_sizes
        .iter()
        .any(|&c| c >= config.context_length);
    if !context_realistic {
        assumptions.push(format!(
            "requested context {} exceeds every context size this catalog entry lists support for ({:?})",
            config.context_length, build.context_sizes
        ));
    }

    let (vram_low, vram_high) = estimate_vram(
        config,
        weights,
        kv_low.value,
        kv_high.value,
        &mut assumptions,
    );
    if config.backend == Backend::Cpu {
        missing_inputs.push("VRAM estimate not applicable - CPU backend selected".to_string());
    }

    MemoryEstimate {
        formula_version: formulas::FORMULA_VERSION.to_string(),
        quality,
        model_weights_bytes,
        kv_cache_bytes_low: kv_low,
        kv_cache_bytes_high: kv_high,
        runtime_overhead_bytes_low: overhead_low,
        runtime_overhead_bytes_high: overhead_high,
        os_safety_reserve_bytes: config.os_safety_reserve_bytes,
        estimated_total_ram_bytes_low: total_low,
        estimated_total_ram_bytes_high: total_high,
        estimated_vram_bytes_low: vram_low,
        estimated_vram_bytes_high: vram_high,
        available_ram_bytes,
        headroom_bytes,
        paging_likely,
        context_requested: config.context_length,
        context_realistic,
        assumptions,
        missing_inputs,
    }
}

fn estimate_kv_cache(
    build: &ModelBuild,
    hyperparameters: Option<&GgufHyperparameters>,
    config: &EstimationConfig,
    assumptions: &mut Vec<String>,
) -> (EstimateQuality, Valued<u64>, Valued<u64>) {
    if let Some(hp) = hyperparameters {
        let exact = (|| {
            let n_layers = hp.block_count?;
            let embedding_length = hp.embedding_length?;
            let head_count = hp.attention_head_count?;
            let head_dim = match hp.attention_key_length {
                Some(v) => v,
                None => embedding_length.checked_div(head_count)?,
            };
            let n_kv_heads = hp.attention_head_count_kv.unwrap_or(head_count);
            Some((n_layers, n_kv_heads, head_dim))
        })();

        if let Some((n_layers, n_kv_heads, head_dim)) = exact {
            let bytes = formulas::kv_cache_bytes_exact(
                n_layers,
                n_kv_heads,
                head_dim,
                config.context_length as u64,
                config.kv_cache_bytes_per_element,
            );
            assumptions.push(format!(
                "KV cache computed exactly from this file's own hyperparameters: {n_layers} layers x {n_kv_heads} kv-heads x {head_dim} head_dim x {} context x {} bytes/element",
                config.context_length, config.kv_cache_bytes_per_element
            ));
            let note = "exact formula (2 x layers x kv_heads x head_dim x context x bytes_per_element) using real hyperparameters parsed from this GGUF file";
            let v = Valued::inferred(bytes, note);
            return (EstimateQuality::PreciseFormula, v.clone(), v);
        }
    }

    let (low, high) = formulas::kv_cache_bytes_range_estimate(
        build.file_size_bytes,
        config.context_length as u64,
    );
    assumptions.push(
        "KV cache estimated as a coarse 2%-8% of model weight size per 4096 context tokens - real per-file hyperparameters were not available (this build has not been downloaded/parsed)".to_string(),
    );
    (
        EstimateQuality::CoarseApproximation,
        Valued::inferred(low, "coarse catalog-only range estimate, lower bound"),
        Valued::inferred(high, "coarse catalog-only range estimate, upper bound"),
    )
}

fn estimate_vram(
    config: &EstimationConfig,
    weights_bytes: u64,
    kv_low: Option<u64>,
    kv_high: Option<u64>,
    assumptions: &mut Vec<String>,
) -> (Option<u64>, Option<u64>) {
    if config.backend == Backend::Cpu {
        return (None, None);
    }
    if !config.full_gpu_offload {
        assumptions.push(
            "partial GPU offload is not modeled precisely in Stage 1 - VRAM estimate omitted; see docs/known-limitations.md".to_string(),
        );
        return (None, None);
    }

    // Compute-buffer/context overhead on the GPU side, documented
    // assumption (CUDA/Vulkan context, cuBLAS/compute buffers).
    const GPU_OVERHEAD_LOW: u64 = 300_000_000;
    const GPU_OVERHEAD_HIGH: u64 = 800_000_000;
    assumptions.push(format!(
        "full GPU offload assumed: VRAM = weights + KV cache + {GPU_OVERHEAD_LOW}-{GPU_OVERHEAD_HIGH} bytes compute-buffer overhead"
    ));

    let low = weights_bytes
        .saturating_add(kv_low.unwrap_or(0))
        .saturating_add(GPU_OVERHEAD_LOW);
    let high = weights_bytes
        .saturating_add(kv_high.unwrap_or(0))
        .saturating_add(GPU_OVERHEAD_HIGH);
    (Some(low), Some(high))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{CommercialUse, License, ModelBuild, TaskCategory};
    use crate::provenance::Provenance;

    fn test_build() -> ModelBuild {
        ModelBuild {
            catalog_id: "test".to_string(),
            family: "test-family".to_string(),
            display_name: "Test".to_string(),
            publisher: "Test".to_string(),
            official_source_url: "https://example.com".to_string(),
            official_repository_id: "test/test".to_string(),
            filename: "test.gguf".to_string(),
            architecture: "qwen2".to_string(),
            parameter_count: 630_167_424,
            quantization: "Q4_K_M".to_string(),
            file_size_bytes: 491_400_032,
            estimated_disk_bytes: Some(491_400_032),
            estimated_runtime_memory_bytes: None,
            min_recommended_ram_bytes: 1_500_000_000,
            min_recommended_vram_bytes: Some(800_000_000),
            supported_backends: vec![Backend::Cpu, Backend::Cuda],
            context_sizes: vec![2048, 4096, 32768],
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
        }
    }

    fn test_profile(available_ram: Option<u64>) -> HardwareCapabilityProfile {
        let mut hw = crate::hardware::inspect(None);
        hw.memory.available_bytes = match available_ram {
            Some(v) => crate::hardware::HardwareField::measured(v, "test"),
            None => crate::hardware::HardwareField::unavailable("test"),
        };
        crate::profile::build_profile(&hw, "2026-01-01T00:00:00Z".to_string(), 0)
    }

    #[test]
    fn coarse_estimate_used_when_no_hyperparameters_available() {
        let build = test_build();
        let profile = test_profile(Some(8_000_000_000));
        let config = EstimationConfig::for_backend(Backend::Cpu, 2048);
        let est = estimate(&build, None, &profile, &config);
        assert_eq!(est.quality, EstimateQuality::CoarseApproximation);
        assert!(est.kv_cache_bytes_low.value.unwrap() <= est.kv_cache_bytes_high.value.unwrap());
    }

    #[test]
    fn precise_estimate_used_when_real_hyperparameters_available() {
        let build = test_build();
        let profile = test_profile(Some(8_000_000_000));
        let config = EstimationConfig::for_backend(Backend::Cpu, 2048);
        let hp = GgufHyperparameters {
            context_length: Some(32768),
            embedding_length: Some(896),
            block_count: Some(24),
            attention_head_count: Some(14),
            attention_head_count_kv: Some(2),
            attention_key_length: None,
            attention_value_length: None,
        };
        let est = estimate(&build, Some(&hp), &profile, &config);
        assert_eq!(est.quality, EstimateQuality::PreciseFormula);
        // Real hand-computed value: 2*24*2*64*2048*2 = 25,165,824.
        assert_eq!(est.kv_cache_bytes_low.value, Some(25_165_824));
        assert_eq!(est.kv_cache_bytes_high.value, Some(25_165_824));
    }

    #[test]
    fn headroom_is_negative_when_ram_is_too_small() {
        let build = test_build();
        let profile = test_profile(Some(200_000_000)); // 200MB - tiny
        let config = EstimationConfig::for_backend(Backend::Cpu, 2048);
        let est = estimate(&build, None, &profile, &config);
        assert!(est.headroom_bytes.unwrap() < 0);
        assert!(est.paging_likely);
    }

    #[test]
    fn headroom_is_positive_on_a_generously_provisioned_machine() {
        let build = test_build();
        let profile = test_profile(Some(64_000_000_000)); // 64GB
        let config = EstimationConfig::for_backend(Backend::Cpu, 2048);
        let est = estimate(&build, None, &profile, &config);
        assert!(est.headroom_bytes.unwrap() > 0);
        assert!(!est.paging_likely);
    }

    #[test]
    fn missing_available_ram_is_reported_not_defaulted() {
        let build = test_build();
        let profile = test_profile(None);
        let config = EstimationConfig::for_backend(Backend::Cpu, 2048);
        let est = estimate(&build, None, &profile, &config);
        assert_eq!(est.headroom_bytes, None);
        assert!(!est.missing_inputs.is_empty());
        assert_eq!(est.available_ram_bytes.provenance, Provenance::Unknown);
    }

    #[test]
    fn unrealistic_context_is_flagged() {
        let build = test_build(); // max context_sizes entry is 32768
        let profile = test_profile(Some(8_000_000_000));
        let config = EstimationConfig::for_backend(Backend::Cpu, 200_000);
        let est = estimate(&build, None, &profile, &config);
        assert!(!est.context_realistic);
    }

    #[test]
    fn cpu_backend_never_produces_a_vram_estimate() {
        let build = test_build();
        let profile = test_profile(Some(8_000_000_000));
        let config = EstimationConfig::for_backend(Backend::Cpu, 2048);
        let est = estimate(&build, None, &profile, &config);
        assert_eq!(est.estimated_vram_bytes_low, None);
        assert_eq!(est.estimated_vram_bytes_high, None);
    }

    #[test]
    fn cuda_full_offload_produces_a_vram_estimate_at_least_the_weight_size() {
        let build = test_build();
        let profile = test_profile(Some(8_000_000_000));
        let config = EstimationConfig::for_backend(Backend::Cuda, 2048);
        let est = estimate(&build, None, &profile, &config);
        assert!(est.estimated_vram_bytes_low.unwrap() >= build.file_size_bytes);
    }
}
