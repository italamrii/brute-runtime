# Model Requirement Estimator

## In plain language

Before you download a model, BRUTE tries to answer: "how much RAM/VRAM/
disk will this actually use on my machine?" It never uses the crude rule
"file size = RAM needed" — it separately estimates the model weights, the
conversation memory (KV cache), and runtime overhead, and always gives a
**range**, not a single falsely-precise number.

## The five components (see `src/estimator/`)

1. **Model weights** — `model_weights_bytes`. Always taken from the
   catalog's own `file_size_bytes` when present (`Catalog` provenance —
   this is the single most reliable number available pre-download). A
   fallback formula (`parameter_count × bits-per-weight ÷ 8`) exists for
   cross-checking and for the rare case a file size is missing.
2. **KV cache** — the "conversation memory" that grows with context
   length. Two modes:
   - **Exact** (`EstimateQuality::PreciseFormula`): when the model has
     actually been parsed (e.g. via `brute model inspect`) and its real
     transformer hyperparameters are known, the textbook formula applies:
     `bytes = 2 × layers × kv_heads × head_dim × context_length × bytes_per_element`.
   - **Coarse** (`EstimateQuality::CoarseApproximation`): for a
     catalog-only build with no file downloaded yet, KV cache is estimated
     as **2%–8% of model weight size per 4096 tokens of context** — a
     deliberately wide range, not a specific number.
3. **Runtime overhead** — compute buffers, tokenizer, process baseline.
   Fixed range (50 MB–300 MB), informed by the one real calibration point
   available: the real Qwen2.5-0.5B run's peak process memory
   (542,609,408 bytes) minus its weights (491,400,032 bytes) minus its
   *exact*-formula KV cache at that context (25,165,824 bytes at 2048
   tokens) leaves ~26 MB of actual overhead for a tiny CPU model — at the
   low end of the documented range, as expected.
4. **OS safety reserve** — a flat 1.5 GiB (1,610,612,736 bytes) policy
   reservation for the OS and other running applications. A policy
   choice, not a measurement.
5. **VRAM** (GPU backends only) — Stage 1 models offload as all-or-nothing
   (either full GPU offload or CPU); VRAM = weights + KV cache + a
   300–800 MB compute-buffer allowance. Partial offload is a documented
   limitation (`docs/known-limitations.md`).

## Worked example (real numbers, from `brute explain-fit`)

```
Memory calculation: formula stage1-v1: weights=491400032 bytes (catalog)
  + KV cache 78624005-314496020 bytes (CoarseApproximation)
  + runtime overhead 50000000-300000000 bytes
  + OS reserve 1610612736 bytes
  = estimated total 620024037-1105896052 bytes;
  available RAM 8735199232 bytes -> headroom 6018690444 bytes
```

This is the coarse (catalog-only) path for the 0.5B model at 32768
context. `headroom_bytes = available_ram − os_reserve − total_high`, and
`headroom_ratio = headroom_bytes / available_ram` (here ≈0.69) is what
feeds the fit classifier.

## Bits-per-weight table and its known limitation

`estimator::formulas::bits_per_weight` maps quantization labels (Q4_K_M,
Q8_0, F16, …) to published average bits-per-weight. **This table
systematically underestimates small models**: embedding/output tensors
are conventionally kept at higher precision regardless of the overall
quant label, and they're a much larger fraction of total size in a
small-hidden-dim model. Measured example: the naive table estimates
~385 MB for Qwen2.5-0.5B Q4_K_M; the real file is 491,400,032 bytes — a
~28% underestimate (see `estimator::formulas::tests::estimate_weights_bytes_is_a_same_order_of_magnitude_lower_bound_for_small_models`).
This is exactly why real `file_size_bytes` is always preferred over the
formula — the formula is only a fallback/cross-check.

## Every estimate reports

- `formula_version` (`"stage1-v1"`)
- `quality` (`PreciseFormula` or `CoarseApproximation`)
- Low/high range for weights, KV cache, overhead, total RAM, and VRAM
- `assumptions: Vec<String>` — every formula/policy choice actually used
- `missing_inputs: Vec<String>` — anything that couldn't be determined
- `context_realistic` — whether the requested context is within what the
  catalog entry documents support for
- `paging_likely` — derived from `headroom_bytes < 0`, never guessed
