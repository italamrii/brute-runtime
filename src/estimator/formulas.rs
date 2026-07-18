//! Pure memory-estimation formulas - no I/O, no hardware/catalog types,
//! easy to unit-test at exact boundary values. See
//! `docs/model-memory-estimation.md` for the derivation and worked
//! examples of every formula here.

pub const FORMULA_VERSION: &str = "stage1-v1";

/// llama.cpp's default KV-cache element type is F16 (2 bytes/element).
pub const DEFAULT_KV_CACHE_BYTES_PER_ELEMENT: u64 = 2;

/// Reserved for the OS and other running applications - a policy choice,
/// not a measurement. 1.5 GiB is a conservative default for a modern
/// Windows desktop session.
pub const DEFAULT_OS_SAFETY_RESERVE_BYTES: u64 = 1_610_612_736;

/// Approximate average bits-per-weight for common GGUF quantizations,
/// published by the llama.cpp/GGML project. Real per-tensor K-quant mixes
/// vary a few percent around these averages depending on model shape.
///
/// These are known to systematically *underestimate* small models (see
/// `estimate_weights_bytes_from_params`) - they describe the quantized
/// weight-matrix tensors, but embedding/output tensors are conventionally
/// kept at higher precision (often F16/F32) regardless of the overall
/// quant label, and those fixed-size tensors are a much larger fraction
/// of total file size for a small hidden-dim model than a large one.
/// Real measured example: Qwen2.5-0.5B-Instruct Q4_K_M's real file is
/// 491,400,032 bytes; this table's naive estimate is ~385MB - a ~28%
/// underestimate. Real `file_size_bytes` from the catalog (or an
/// already-parsed file) is always preferred over this table; it exists
/// only as a fallback/cross-check. See `docs/model-memory-estimation.md`.
pub fn bits_per_weight(quantization: &str) -> Option<f64> {
    let q = quantization.trim().to_uppercase();
    let bpw = match q.as_str() {
        "F32" => 32.0,
        "F16" | "BF16" => 16.0,
        "Q8_0" => 8.5,
        "Q8_1" => 9.0,
        "Q6_K" => 6.5625,
        "Q5_K_M" | "Q5_K" | "Q5_K_S" => 5.6875,
        "Q5_0" => 5.5,
        "Q5_1" => 6.0,
        "Q4_K_M" | "Q4_K" => 4.89,
        "Q4_K_S" => 4.58,
        "Q4_0" => 4.5,
        "Q4_1" => 5.0,
        "Q3_K_M" | "Q3_K" => 3.9,
        "Q3_K_S" | "Q3_K_L" => 3.5,
        "Q2_K" => 3.35,
        _ => return None,
    };
    Some(bpw)
}

/// `parameter_count * (bits_per_weight(quantization) / 8)` - a cross-check
/// against a catalog's own declared `file_size_bytes`, and a fallback when
/// no file size is known at all. Returns `None` for an unrecognized
/// quantization label rather than guessing a default. See the caveat on
/// `bits_per_weight` about small models - real `file_size_bytes` should
/// always be preferred when available.
pub fn estimate_weights_bytes_from_params(parameter_count: u64, quantization: &str) -> Option<u64> {
    let bpw = bits_per_weight(quantization)?;
    Some((parameter_count as f64 * (bpw / 8.0)).round() as u64)
}

/// Exact KV-cache size for one forward pass' worth of context, using the
/// standard transformer formula:
///
/// `bytes = 2 (K and V) * n_layers * n_kv_heads * head_dim * context_length * bytes_per_element`
///
/// Requires real architecture hyperparameters (from an actually-parsed
/// GGUF file, not catalog metadata) - see `kv_cache_bytes_range_estimate`
/// for the catalog-only fallback.
pub fn kv_cache_bytes_exact(
    n_layers: u64,
    n_kv_heads: u64,
    head_dim: u64,
    context_length: u64,
    bytes_per_element: u64,
) -> u64 {
    2u64.saturating_mul(n_layers)
        .saturating_mul(n_kv_heads)
        .saturating_mul(head_dim)
        .saturating_mul(context_length)
        .saturating_mul(bytes_per_element)
}

/// Coarse, architecture-agnostic KV-cache range used when only catalog
/// metadata is available (no real hyperparameters to run the exact
/// formula on) - i.e. the pre-download "will this fit" case the
/// recommendation engine mostly runs. Deliberately wide and explicitly
/// approximate rather than a fake-precise single number: KV cache is
/// modeled as 2%-8% of model weight size per full 4096-token context
/// window, a commonly-cited order-of-magnitude rule of thumb for
/// GQA-equipped modern architectures at the low end and non-GQA
/// architectures at the high end.
pub fn kv_cache_bytes_range_estimate(weights_bytes: u64, context_length: u64) -> (u64, u64) {
    let context_units = (context_length as f64 / 4096.0).max(0.25);
    let low = (weights_bytes as f64 * 0.02 * context_units).round() as u64;
    let high = (weights_bytes as f64 * 0.08 * context_units).round() as u64;
    (low, high)
}

/// Documented runtime overhead range (compute buffers, tokenizer, process
/// baseline) not otherwise counted in weights or KV cache. The low end is
/// informed by the one real calibration point available at Stage 1 launch
/// (Qwen2.5-0.5B on CPU: observed peak process memory minus weights minus
/// exact-formula KV cache was ~27 MB) - see
/// `docs/calibration-methodology.md`. The high end is a conservative
/// allowance for larger models and GPU compute buffers.
pub fn runtime_overhead_bytes_range() -> (u64, u64) {
    (50_000_000, 300_000_000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bits_per_weight_matches_published_values_for_common_quants() {
        assert_eq!(bits_per_weight("F16"), Some(16.0));
        assert_eq!(bits_per_weight("q4_k_m"), Some(4.89)); // case-insensitive
        assert_eq!(bits_per_weight("Q8_0"), Some(8.5));
        assert_eq!(bits_per_weight("not-a-real-quant"), None);
    }

    #[test]
    fn estimate_weights_bytes_is_a_same_order_of_magnitude_lower_bound_for_small_models() {
        // Real values from the Qwen2.5-0.5B-Instruct Q4_K_M GGUF this repo
        // actually inspected: 630167424 params, 491400032 real bytes. The
        // naive bits-per-weight table underestimates by ~28% for this
        // model (see the doc comment on `bits_per_weight`) because its
        // embedding/output tensors dominate a larger fraction of the file
        // than they would in a bigger model. This test locks in that this
        // is a *bounded, understood* underestimate (same order of
        // magnitude, not wildly off) rather than asserting false
        // precision - real `file_size_bytes` is always preferred when
        // available, this table is only ever a fallback/cross-check.
        let estimated = estimate_weights_bytes_from_params(630167424, "Q4_K_M").unwrap();
        let real = 491400032u64;
        let ratio = estimated as f64 / real as f64;
        assert!(
            (0.6..1.15).contains(&ratio),
            "ratio was {ratio}, estimate {estimated}"
        );
    }

    #[test]
    fn kv_cache_exact_matches_hand_computed_value_for_qwen_0_5b() {
        // Real Qwen2.5-0.5B hyperparameters: 24 layers, 2 kv heads,
        // head_dim = 896/14 = 64, F16 KV cache (2 bytes/elem).
        let bytes = kv_cache_bytes_exact(24, 2, 64, 2048, 2);
        assert_eq!(bytes, 2 * 24 * 2 * 64 * 2048 * 2);
        assert_eq!(bytes, 25_165_824);
    }

    #[test]
    fn kv_cache_range_grows_with_context_length() {
        let (low_2k, high_2k) = kv_cache_bytes_range_estimate(1_000_000_000, 2048);
        let (low_8k, high_8k) = kv_cache_bytes_range_estimate(1_000_000_000, 8192);
        assert!(low_8k > low_2k);
        assert!(high_8k > high_2k);
        assert!(low_2k <= high_2k);
    }

    #[test]
    fn kv_cache_range_never_produces_low_greater_than_high() {
        for ctx in [512u64, 2048, 4096, 32768, 131072] {
            let (low, high) = kv_cache_bytes_range_estimate(500_000_000, ctx);
            assert!(low <= high, "ctx={ctx} produced low={low} > high={high}");
        }
    }
}
