# Safe tuning search space

`tuning::candidates::generate_plan` turns (model, machine profile,
verified GPU backend) into a bounded, deterministic `TuningPlan` —
never a brute-force cartesian product of every dimension.

## Why three independent groups, not a cartesian product

Threads × GPU layers × context × batch, fully crossed, would be dozens to
hundreds of combinations — unbounded in the general case and impossible
to display meaningfully in `brute tune --dry-run` before any benchmark
runs. Instead, each dimension generates its own small candidate group,
each anchored on the *same* fixed defaults for the other dimensions
(`TuningDefaults`: all logical threads, 0 GPU layers, 2048 context, 512
batch). A worked example on this machine (16 logical / 10 physical
cores, Qwen2.5-0.5B, 24 layers):

- **Threads group** (3 candidates): physical (10), logical (16), 75% of
  logical (12) — at `TuningDefaults`' context/batch/GPU-layers.
- **GPU-layers group** (5 candidates, only if a GPU backend was
  *verified* — see `docs/backend-verification.md`): 0/25/50/75/100% of
  24 layers = 0/6/12/18/24 — at `TuningDefaults`' threads/context/batch.
- **Context×batch group** (up to 12 candidates): context ∈
  {1024, 2048, 4096, up to 8192 if the model's own max context allows} ×
  batch ∈ {128, 256, 512} — at `TuningDefaults`' threads/GPU-layers.

This is a deliberate architectural tradeoff: a true adaptive/greedy
search (refine around whatever measured best so far) would need results
that don't exist yet at plan-generation time, which is incompatible with
"`--dry-run` shows the full plan before anything runs." Independent
groups anchored on shared defaults keep the whole plan pure and
computable in one pass, at the cost of not exploring interactions between
dimensions (e.g. "is 12 threads still best once GPU offload is on?" is
not directly tested — see `docs/known-limitations.md`).

## Pruning rules (applied before any candidate is queued)

| Rule | Where | Real formula reused |
|---|---|---|
| Thread count > logical CPU count | `generate_thread_candidates` | — |
| Predicted VRAM > 85% of dedicated VRAM (`VRAM_SAFETY_MARGIN`) | `generate_gpu_layer_candidates` | Stage 1's `estimator::formulas::kv_cache_bytes_exact` (offloaded fraction) |
| GPU layer count unknown (`hyperparameters.block_count` missing) | `generate_gpu_layer_candidates` | GPU tuning skipped entirely, not guessed |
| Batch size > context size | `generate_context_batch_candidates` | — |
| Context exceeds the model's own declared max context length | `generate_context_batch_candidates` | — |
| Predicted RAM > 85% of available RAM (`RAM_SAFETY_MARGIN`) | `generate_context_batch_candidates` | `estimator::formulas::kv_cache_bytes_exact`/`_range_estimate`, `DEFAULT_OS_SAFETY_RESERVE_BYTES` |
| Duplicate equivalent config (same backend/threads/gpu_layers/context/batch) | `dedup_candidates` | `HashSet` on the full tuple |
| Total candidate count > `MAX_CANDIDATES` (32) | `generate_plan` | Hard truncation, `TuningPlan.truncated = true` |

Every pruned candidate is recorded in `TuningPlan.pruned` with a
human-readable reason — never silently dropped. A candidate whose safety
inputs are unmeasurable (e.g. no adapter reports `dedicated_vram_bytes`)
is **not** pruned on that basis — pruning only fires when both the
predicted value and the safety ceiling are actually known; see
`tuning::candidates::tests::gpu_layer_candidates_are_still_generated_and_never_vram_pruned_when_vram_is_unmeasurable`.
This mirrors the project-wide rule: unknown is never silently treated as
either "safe" or "unsafe" by the pruner itself, but *is* treated as the
least-safe option later, by the ranker — see
`docs/tuning-ranking-methodology.md`.

## Prediction formulas (approximations, explicitly labeled)

- **VRAM** (`predict_vram_bytes`): per-layer weight bytes = file size ÷
  (total layers + 2) — the "+2" accounts for embedding/output tensors not
  belonging to any single transformer layer — times offloaded layer
  count, plus the offloaded fraction of the exact KV-cache formula, plus
  a fixed 400 MB compute-buffer overhead constant.
- **RAM** (`predict_ram_bytes`): full model file size (weights always
  assumed CPU-resident for this candidate group — GPU offload is tuned
  separately) plus the exact (or range-estimated, when hyperparameters
  are incomplete) KV-cache formula plus a 300 MB runtime overhead
  constant plus Stage 1's OS safety reserve.

Both are documented approximations reused from Stage 1's estimator, not
independent formulas — see `docs/model-memory-estimation.md` for the
underlying KV-cache math.

## Determinism

`generate_plan` is a pure function of its three inputs — same model +
profile + verified backend always produces the same candidate list in
the same order. Verified directly:
`tuning::candidates::tests::plan_is_deterministic`.

## Formula version

`TuningPlan.formula_version` is `tuning::TUNING_FORMULA_VERSION`
(`"stage2-v1"`) — bumped whenever a pruning rule, prediction formula, or
default changes, so a saved runtime profile
(`docs/runtime-profile-schema.md`) records exactly which generation
logic produced its winning candidate.
