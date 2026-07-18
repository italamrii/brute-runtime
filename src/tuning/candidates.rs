//! Safe Tuning Search Space: bounded, deterministic candidate generation.
//!
//! Deliberately **not** a full cartesian product. Each of the three tuning
//! dimensions (threads, GPU offload, context×batch) generates a small,
//! independent candidate group anchored on the *same* sensible defaults
//! for the other dimensions - this keeps the whole plan computable and
//! displayable up front (`brute tune --dry-run`) without depending on
//! results that don't exist yet, and keeps the total candidate count
//! small enough to actually benchmark in bounded time. See
//! `docs/tuning-search-space.md` for the full rationale and worked
//! example.

use crate::models::ModelReport;
use crate::profile::HardwareCapabilityProfile;
use crate::runtime::Backend;
use serde::Serialize;

/// Hard ceiling on the number of candidates a plan may contain, applied
/// after generation+pruning+dedup. A documented safety guard, not
/// expected to be hit by the dimensions implemented today.
pub const MAX_CANDIDATES: usize = 32;

/// GPU memory safety margin: a candidate predicted to use more than this
/// fraction of dedicated VRAM is pruned before execution.
const VRAM_SAFETY_MARGIN: f64 = 0.85;

/// RAM safety margin (mirrors Stage 1's `estimator` OS reserve philosophy,
/// applied here per-candidate for context/batch pruning).
const RAM_SAFETY_MARGIN: f64 = 0.85;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateDimension {
    Threads,
    GpuLayers,
    ContextBatch,
}

#[derive(Debug, Clone, Serialize)]
pub struct Candidate {
    pub id: String,
    pub dimension: CandidateDimension,
    pub backend: Backend,
    pub threads: u32,
    pub gpu_layers: u32,
    pub context_size: u32,
    pub batch_size: u32,
    pub predicted_vram_bytes: Option<u64>,
    pub predicted_ram_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PrunedCandidate {
    pub id: String,
    pub dimension: CandidateDimension,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TuningDefaults {
    pub threads: u32,
    pub gpu_layers: u32,
    pub context_size: u32,
    pub batch_size: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct TuningPlan {
    pub formula_version: String,
    pub model_sha256: String,
    pub machine_profile_schema_version: String,
    pub defaults: TuningDefaults,
    pub candidates: Vec<Candidate>,
    pub pruned: Vec<PrunedCandidate>,
    pub truncated: bool,
}

/// Generates the full bounded plan. `verified_gpu_backend` is the single
/// GPU backend (if any) that `backends::verify_backend` actually
/// confirmed - GPU-offload candidates are only generated when this is
/// `Some`, never merely from driver detection.
pub fn generate_plan(
    model: &ModelReport,
    profile: &HardwareCapabilityProfile,
    verified_gpu_backend: Option<Backend>,
) -> TuningPlan {
    let logical = profile.cpu.logical_cores.value.unwrap_or(1) as u32;
    let physical = profile
        .cpu
        .physical_cores
        .value
        .map(|v| v as u32)
        .unwrap_or(logical);
    let available_ram = profile.memory.available_bytes.value;

    let defaults = TuningDefaults {
        threads: logical.max(1),
        gpu_layers: 0,
        context_size: 2048,
        batch_size: 512,
    };

    let mut candidates = Vec::new();
    let mut pruned = Vec::new();

    generate_thread_candidates(physical, logical, &defaults, &mut candidates, &mut pruned);

    if let Some(backend) = verified_gpu_backend {
        generate_gpu_layer_candidates(
            backend,
            model,
            profile,
            &defaults,
            &mut candidates,
            &mut pruned,
        );
    }

    generate_context_batch_candidates(
        model,
        available_ram,
        verified_gpu_backend.unwrap_or(Backend::Cpu),
        &defaults,
        &mut candidates,
        &mut pruned,
    );

    dedup_candidates(&mut candidates);

    let truncated = candidates.len() > MAX_CANDIDATES;
    if truncated {
        candidates.truncate(MAX_CANDIDATES);
    }

    TuningPlan {
        formula_version: super::TUNING_FORMULA_VERSION.to_string(),
        model_sha256: model.sha256.clone(),
        machine_profile_schema_version: profile.schema_version.clone(),
        defaults,
        candidates,
        pruned,
        truncated,
    }
}

fn generate_thread_candidates(
    physical: u32,
    logical: u32,
    defaults: &TuningDefaults,
    candidates: &mut Vec<Candidate>,
    pruned: &mut Vec<PrunedCandidate>,
) {
    let seventy_five_pct = ((logical as f64) * 0.75).round().max(1.0) as u32;
    let mut raw = vec![physical, logical, seventy_five_pct];
    raw.sort_unstable();
    raw.dedup();

    for threads in raw {
        let id = format!("threads-{threads}");
        if threads == 0 || threads > logical {
            pruned.push(PrunedCandidate {
                id,
                dimension: CandidateDimension::Threads,
                reason: format!("thread count {threads} exceeds logical CPU count {logical}"),
            });
            continue;
        }
        candidates.push(Candidate {
            id,
            dimension: CandidateDimension::Threads,
            backend: Backend::Cpu,
            threads,
            gpu_layers: defaults.gpu_layers,
            context_size: defaults.context_size,
            batch_size: defaults.batch_size,
            predicted_vram_bytes: None,
            predicted_ram_bytes: None,
        });
    }
}

fn generate_gpu_layer_candidates(
    backend: Backend,
    model: &ModelReport,
    profile: &HardwareCapabilityProfile,
    defaults: &TuningDefaults,
    candidates: &mut Vec<Candidate>,
    pruned: &mut Vec<PrunedCandidate>,
) {
    let Some(total_layers) = model.hyperparameters.block_count else {
        pruned.push(PrunedCandidate {
            id: "gpu_layers-*".to_string(),
            dimension: CandidateDimension::GpuLayers,
            reason: "model's layer count is unknown (no real hyperparameters parsed) - GPU offload tuning skipped rather than guessed".to_string(),
        });
        return;
    };

    let dedicated_vram = profile
        .gpus
        .iter()
        .filter_map(|g| g.dedicated_vram_bytes)
        .max();

    for fraction in [0.0, 0.25, 0.5, 0.75, 1.0] {
        let gpu_layers = ((total_layers as f64) * fraction).round() as u32;
        let id = format!("gpu_layers-{gpu_layers}");

        let predicted_vram =
            predict_vram_bytes(model, total_layers, gpu_layers, defaults.context_size);

        if let (Some(predicted), Some(available)) = (predicted_vram, dedicated_vram) {
            let limit = (available as f64 * VRAM_SAFETY_MARGIN) as u64;
            if predicted > limit {
                pruned.push(PrunedCandidate {
                    id,
                    dimension: CandidateDimension::GpuLayers,
                    reason: format!(
                        "predicted VRAM {predicted} bytes exceeds the {:.0}% safety margin of {available} bytes dedicated VRAM ({limit} bytes)",
                        VRAM_SAFETY_MARGIN * 100.0
                    ),
                });
                continue;
            }
        }

        candidates.push(Candidate {
            id,
            dimension: CandidateDimension::GpuLayers,
            backend,
            threads: defaults.threads,
            gpu_layers,
            context_size: defaults.context_size,
            batch_size: defaults.batch_size,
            predicted_vram_bytes: predicted_vram,
            predicted_ram_bytes: None,
        });
    }
}

/// Approximates VRAM for offloading `gpu_layers` of `total_layers`:
/// per-layer weight bytes (model weights spread evenly across layers plus
/// two "layer-equivalents" for embedding/output tensors - a documented
/// approximation, not exact), the offloaded fraction of the exact KV-cache
/// formula when hyperparameters allow it, and a fixed compute-buffer
/// overhead. See `docs/tuning-search-space.md`.
fn predict_vram_bytes(
    model: &ModelReport,
    total_layers: u64,
    gpu_layers: u32,
    context_size: u32,
) -> Option<u64> {
    if gpu_layers == 0 {
        return Some(0);
    }
    let per_layer_bytes = model.file_size_bytes as f64 / (total_layers as f64 + 2.0);
    let weights_vram = per_layer_bytes * gpu_layers as f64;

    let kv_vram = match (
        model.hyperparameters.attention_head_count,
        model.hyperparameters.attention_head_count_kv,
        model.hyperparameters.embedding_length,
    ) {
        (Some(heads), kv_heads, Some(embedding_length)) if heads > 0 => {
            let head_dim = model
                .hyperparameters
                .attention_key_length
                .unwrap_or(embedding_length / heads);
            let n_kv_heads = kv_heads.unwrap_or(heads);
            let fraction_offloaded = gpu_layers as f64 / total_layers.max(1) as f64;
            let full_kv = crate::estimator::formulas::kv_cache_bytes_exact(
                total_layers,
                n_kv_heads,
                head_dim,
                context_size as u64,
                crate::estimator::formulas::DEFAULT_KV_CACHE_BYTES_PER_ELEMENT,
            );
            (full_kv as f64) * fraction_offloaded
        }
        _ => 0.0,
    };

    const GPU_COMPUTE_BUFFER_OVERHEAD_BYTES: f64 = 400_000_000.0;
    Some((weights_vram + kv_vram + GPU_COMPUTE_BUFFER_OVERHEAD_BYTES) as u64)
}

fn generate_context_batch_candidates(
    model: &ModelReport,
    available_ram: Option<u64>,
    backend: Backend,
    defaults: &TuningDefaults,
    candidates: &mut Vec<Candidate>,
    pruned: &mut Vec<PrunedCandidate>,
) {
    let mut context_options = vec![1024u32, 2048, 4096];
    if let Some(max_ctx) = model.hyperparameters.context_length {
        let larger = 8192u32.min(max_ctx as u32);
        if larger > 4096 {
            context_options.push(larger);
        }
    }

    let batch_options = [128u32, 256, 512];

    for &context_size in &context_options {
        for &batch_size in &batch_options {
            let id = format!("ctx-{context_size}-batch-{batch_size}");

            if batch_size > context_size {
                pruned.push(PrunedCandidate {
                    id,
                    dimension: CandidateDimension::ContextBatch,
                    reason: format!("batch size {batch_size} exceeds context size {context_size}"),
                });
                continue;
            }

            if let Some(max_ctx) = model.hyperparameters.context_length
                && context_size as u64 > max_ctx
            {
                pruned.push(PrunedCandidate {
                    id,
                    dimension: CandidateDimension::ContextBatch,
                    reason: format!("context {context_size} exceeds this model's maximum supported context {max_ctx}"),
                });
                continue;
            }

            let predicted_ram = predict_ram_bytes(model, context_size, backend);
            if let (Some(predicted), Some(available)) = (predicted_ram, available_ram) {
                let limit = (available as f64 * RAM_SAFETY_MARGIN) as u64;
                if predicted > limit {
                    let margin_pct = RAM_SAFETY_MARGIN * 100.0;
                    pruned.push(PrunedCandidate {
                        id,
                        dimension: CandidateDimension::ContextBatch,
                        reason: format!(
                            "predicted RAM {predicted} bytes exceeds the {margin_pct:.0}% safety margin of available RAM ({limit} bytes) - likely paging"
                        ),
                    });
                    continue;
                }
            }

            candidates.push(Candidate {
                id,
                dimension: CandidateDimension::ContextBatch,
                backend,
                threads: defaults.threads,
                gpu_layers: defaults.gpu_layers,
                context_size,
                batch_size,
                predicted_vram_bytes: None,
                predicted_ram_bytes: predicted_ram,
            });
        }
    }
}

fn predict_ram_bytes(model: &ModelReport, context_size: u32, backend: Backend) -> Option<u64> {
    // CPU-resident weights only matter when the backend actually keeps
    // them in system RAM; for a GPU backend this candidate group still
    // assumes CPU-side weights (offload is tuned separately), so the
    // conservative estimate always includes full weights here.
    let _ = backend;
    let weights = model.file_size_bytes;

    let kv = match (
        model.hyperparameters.block_count,
        model.hyperparameters.attention_head_count,
        model.hyperparameters.embedding_length,
    ) {
        (Some(layers), Some(heads), Some(embedding_length)) if heads > 0 => {
            let head_dim = model
                .hyperparameters
                .attention_key_length
                .unwrap_or(embedding_length / heads);
            let n_kv_heads = model
                .hyperparameters
                .attention_head_count_kv
                .unwrap_or(heads);
            crate::estimator::formulas::kv_cache_bytes_exact(
                layers,
                n_kv_heads,
                head_dim,
                context_size as u64,
                crate::estimator::formulas::DEFAULT_KV_CACHE_BYTES_PER_ELEMENT,
            )
        }
        _ => {
            let (low, _high) = crate::estimator::formulas::kv_cache_bytes_range_estimate(
                weights,
                context_size as u64,
            );
            low
        }
    };

    const RUNTIME_OVERHEAD_BYTES: u64 = 300_000_000;
    const OS_RESERVE_BYTES: u64 = crate::estimator::formulas::DEFAULT_OS_SAFETY_RESERVE_BYTES;
    Some(weights + kv + RUNTIME_OVERHEAD_BYTES + OS_RESERVE_BYTES)
}

fn dedup_candidates(candidates: &mut Vec<Candidate>) {
    let mut seen = std::collections::HashSet::new();
    candidates.retain(|c| {
        let key = (
            c.backend,
            c.threads,
            c.gpu_layers,
            c.context_size,
            c.batch_size,
        );
        seen.insert(key)
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware::HardwareField;
    use crate::models::gguf::GgufHyperparameters;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn test_model(file_size_bytes: u64, hyperparameters: GgufHyperparameters) -> ModelReport {
        ModelReport {
            path: PathBuf::from(r"C:\models\test.gguf"),
            file_size_bytes,
            sha256: "testsha256".to_string(),
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
            hyperparameters,
        }
    }

    fn qwen_hyperparameters() -> GgufHyperparameters {
        GgufHyperparameters {
            context_length: Some(32768),
            embedding_length: Some(896),
            block_count: Some(24),
            attention_head_count: Some(14),
            attention_head_count_kv: Some(2),
            attention_key_length: None,
            attention_value_length: None,
        }
    }

    fn test_profile(
        logical: usize,
        physical: usize,
        ram: u64,
        vram: Option<u64>,
    ) -> HardwareCapabilityProfile {
        let hw_base = crate::hardware::inspect(None);
        let mut hw = hw_base;
        hw.cpu.logical_cores = HardwareField::measured(logical, "test");
        hw.cpu.physical_cores = HardwareField::measured(physical, "test");
        hw.memory.available_bytes = HardwareField::measured(ram, "test");
        hw.memory.total_bytes = HardwareField::measured(ram, "test");
        if let Some(vram_bytes) = vram {
            hw.gpu.adapters = HardwareField::measured(
                vec![crate::hardware::GpuAdapter {
                    name: "Test GPU".to_string(),
                    vendor: crate::hardware::GpuVendor::Nvidia,
                    dedicated_vram_bytes: Some(vram_bytes),
                    shared_system_memory_bytes: None,
                    driver_version: None,
                }],
                "test",
            );
        }
        crate::profile::build_profile(&hw, "2026-01-01T00:00:00Z".to_string(), 0)
    }

    #[test]
    fn thread_candidates_never_exceed_logical_core_count() {
        let model = test_model(491_400_032, qwen_hyperparameters());
        let profile = test_profile(16, 10, 16_000_000_000, None);
        let plan = generate_plan(&model, &profile, None);

        let thread_candidates: Vec<_> = plan
            .candidates
            .iter()
            .filter(|c| c.dimension == CandidateDimension::Threads)
            .collect();
        assert!(!thread_candidates.is_empty());
        for c in thread_candidates {
            assert!(c.threads <= 16);
            assert!(c.threads >= 1);
        }
    }

    #[test]
    fn no_gpu_layer_candidates_without_a_verified_gpu_backend() {
        let model = test_model(491_400_032, qwen_hyperparameters());
        let profile = test_profile(16, 10, 16_000_000_000, Some(8_000_000_000));
        let plan = generate_plan(&model, &profile, None);

        assert!(
            !plan
                .candidates
                .iter()
                .any(|c| c.dimension == CandidateDimension::GpuLayers)
        );
    }

    #[test]
    fn gpu_layer_candidates_generated_when_backend_verified_and_layers_known() {
        let model = test_model(491_400_032, qwen_hyperparameters());
        let profile = test_profile(16, 10, 16_000_000_000, Some(8_000_000_000));
        let plan = generate_plan(&model, &profile, Some(Backend::Cuda));

        let gpu_candidates: Vec<_> = plan
            .candidates
            .iter()
            .filter(|c| c.dimension == CandidateDimension::GpuLayers)
            .collect();
        assert!(!gpu_candidates.is_empty());
        assert!(gpu_candidates.iter().any(|c| c.gpu_layers == 0));
        assert!(gpu_candidates.iter().any(|c| c.gpu_layers == 24)); // full offload, 24 layers
    }

    /// A verified GPU backend with genuinely unmeasurable VRAM (no
    /// adapter reported `dedicated_vram_bytes`) must still generate GPU
    /// candidates - never silently skip GPU tuning just because the
    /// safety-margin check has nothing to compare against - but must
    /// never prune any of them on a VRAM basis it cannot actually check.
    #[test]
    fn gpu_layer_candidates_are_still_generated_and_never_vram_pruned_when_vram_is_unmeasurable() {
        let model = test_model(491_400_032, qwen_hyperparameters());
        let profile = test_profile(16, 10, 16_000_000_000, None);
        let plan = generate_plan(&model, &profile, Some(Backend::Cuda));

        let gpu_candidates: Vec<_> = plan
            .candidates
            .iter()
            .filter(|c| c.dimension == CandidateDimension::GpuLayers)
            .collect();
        assert_eq!(gpu_candidates.len(), 5); // 0/25/50/75/100% of 24 layers
        assert!(gpu_candidates.iter().any(|c| c.gpu_layers == 24));
        assert!(
            plan.pruned
                .iter()
                .all(|p| p.dimension != CandidateDimension::GpuLayers)
        );
        // Still predicted (an estimate), just never checked against an
        // unknown ceiling.
        assert!(
            gpu_candidates
                .iter()
                .all(|c| c.predicted_vram_bytes.is_some())
        );
    }

    #[test]
    fn full_offload_pruned_when_vram_is_too_small() {
        let model = test_model(491_400_032, qwen_hyperparameters());
        // Tiny VRAM - even a small model's full offload should be pruned.
        let profile = test_profile(16, 10, 16_000_000_000, Some(50_000_000));
        let plan = generate_plan(&model, &profile, Some(Backend::Cuda));

        assert!(
            !plan
                .candidates
                .iter()
                .any(|c| c.dimension == CandidateDimension::GpuLayers && c.gpu_layers == 24)
        );
        assert!(plan.pruned.iter().any(|p| p.id == "gpu_layers-24"));
    }

    #[test]
    fn gpu_layer_tuning_skipped_honestly_when_layer_count_unknown() {
        let model = test_model(491_400_032, GgufHyperparameters::default());
        let profile = test_profile(16, 10, 16_000_000_000, Some(8_000_000_000));
        let plan = generate_plan(&model, &profile, Some(Backend::Cuda));

        assert!(
            !plan
                .candidates
                .iter()
                .any(|c| c.dimension == CandidateDimension::GpuLayers)
        );
        assert!(
            plan.pruned
                .iter()
                .any(|p| p.dimension == CandidateDimension::GpuLayers)
        );
    }

    #[test]
    fn batch_never_exceeds_context_in_generated_candidates() {
        let model = test_model(491_400_032, qwen_hyperparameters());
        let profile = test_profile(16, 10, 16_000_000_000, None);
        let plan = generate_plan(&model, &profile, None);

        for c in plan
            .candidates
            .iter()
            .filter(|c| c.dimension == CandidateDimension::ContextBatch)
        {
            assert!(c.batch_size <= c.context_size);
        }
    }

    #[test]
    fn oversized_context_pruned_on_a_low_ram_machine() {
        let model = test_model(491_400_032, qwen_hyperparameters());
        let profile = test_profile(16, 10, 700_000_000, None); // ~700MB available
        let plan = generate_plan(&model, &profile, None);

        assert!(
            !plan
                .candidates
                .iter()
                .any(|c| c.dimension == CandidateDimension::ContextBatch && c.context_size >= 4096)
        );
    }

    /// A candidate whose context size exceeds the model's own declared
    /// maximum (`hyperparameters.context_length`) must be pruned with a
    /// reason naming that limit - distinct from the RAM-driven pruning
    /// path above, and not reachable with the default fixtures'
    /// generous 32768-token context length.
    #[test]
    fn context_exceeding_the_models_own_max_context_length_is_pruned() {
        let mut hyperparameters = qwen_hyperparameters();
        hyperparameters.context_length = Some(1536); // below the 2048 default option
        let model = test_model(491_400_032, hyperparameters);
        let profile = test_profile(16, 10, 16_000_000_000, None); // ample RAM
        let plan = generate_plan(&model, &profile, None);

        assert!(
            !plan
                .candidates
                .iter()
                .any(|c| c.dimension == CandidateDimension::ContextBatch && c.context_size > 1536)
        );
        assert!(
            plan.pruned
                .iter()
                .any(|p| p.dimension == CandidateDimension::ContextBatch
                    && p.reason.contains("maximum supported context"))
        );
    }

    #[test]
    fn plan_is_deterministic() {
        let model = test_model(491_400_032, qwen_hyperparameters());
        let profile = test_profile(16, 10, 16_000_000_000, Some(8_000_000_000));
        let plan1 = generate_plan(&model, &profile, Some(Backend::Cuda));
        let plan2 = generate_plan(&model, &profile, Some(Backend::Cuda));

        let ids1: Vec<_> = plan1.candidates.iter().map(|c| c.id.clone()).collect();
        let ids2: Vec<_> = plan2.candidates.iter().map(|c| c.id.clone()).collect();
        assert_eq!(ids1, ids2);
    }

    #[test]
    fn candidate_count_never_exceeds_the_documented_maximum() {
        let model = test_model(491_400_032, qwen_hyperparameters());
        let profile = test_profile(16, 10, 16_000_000_000, Some(8_000_000_000));
        let plan = generate_plan(&model, &profile, Some(Backend::Cuda));
        assert!(plan.candidates.len() <= MAX_CANDIDATES);
    }

    #[test]
    fn no_duplicate_equivalent_candidates() {
        let model = test_model(491_400_032, qwen_hyperparameters());
        let profile = test_profile(4, 4, 16_000_000_000, None); // physical == logical, collapses candidates
        let plan = generate_plan(&model, &profile, None);

        let mut seen = std::collections::HashSet::new();
        for c in &plan.candidates {
            let key = (
                c.backend,
                c.threads,
                c.gpu_layers,
                c.context_size,
                c.batch_size,
            );
            assert!(seen.insert(key), "duplicate candidate: {:?}", c.id);
        }
    }
}
