# Recommendation Engine v2 (Stage B.5)

`src/recommend/v2.rs`. Consumes the local `preferences::Preferences`
profile directly (see `docs/user-preferences.md`), rather than v1's
single `Priority`/`TaskCategory` pair. v1 (`recommend::rank`/
`recommend::recommend`, `RANKING_FORMULA_VERSION = "stage1-v1"`, still
used by the Optimize page) is unchanged and untouched by this stage.

## Why a v2 module instead of rewriting v1

v2 is a preferences-aware layer built *around* v1's already-correct,
already-tested primitives - `recommend::evaluate_build`,
`fit::evaluate`, `estimator::estimate`, `calibration::find_nearest` -
rather than re-deriving memory estimation, fit classification, or
calibration matching from scratch. Every build's device-fit score and
FitState come directly from a real `evaluate_build` call; v2 only adds
new scoring dimensions and preference-driven filtering on top.

## Pipeline

For every build in the catalog:

1. **Pre-estimate hard filters** (`pre_estimate_rejection`) - cheap
   checks that don't need a memory estimate: excluded family,
   commercial-use requirement vs. unknown/non-allowed license, permitted-
   licenses allow-list, max-download-size ceiling, GPU-required-but-
   undetected, CPU-only-but-unsupported, no shared backend at all with
   the device. A build that fails any of these skips `evaluate_build`
   entirely (score `0.0`, `confidence: unknown`, categorized
   `unsupported`) - it never disappears from the output, it just never
   competes for a "Best X" tag.
2. **Backend selection** (`forced_backend`) - `cpu_only`/`gpu_preference`
   override v1's own CUDA-then-Vulkan-then-CPU auto-selection
   (`preferred_backend`) when the user has an explicit preference.
3. **`evaluate_build`** - the real, unmodified v1 call. Its `estimate`,
   `fit`, and `calibration_match` back every v2 score below.
4. **Post-estimate rejections** - `max_ram_bytes`/`max_vram_bytes`
   preference ceilings (independent of, and possibly stricter than, the
   device's own actual headroom) and `fit.state == NotRecommended`
   (memory that doesn't safely fit the device at all) both add a
   rejection reason and zero the score - "never recommend a model that
   exceeds safe memory limits without a clear warning" is enforced here,
   not left to the UI.
5. **Seven component scores**, each `0.0..=1.0`:

| Score | Source |
|---|---|
| `device_fit` | v1's `fit_state_multiplier(fit.state)` - Excellent 1.0 → NotRecommended 0.05 |
| `arabic` | Half from `task_categories.contains(ArabicChat)`, half from the curated `arabic_capability` `CapabilityLevel` |
| `task_fit` | v1's task-category match, blended with `coding_capability`/`reasoning_capability` when the use case is Coding/Reasoning |
| `speed` | The larger of v1's calibration-match score and its speed component - real measured throughput when a calibration record exists, never a fabricated number when it doesn't |
| `quality` | v1's parameter-count-based `quality_pref`, blended with the curated `general_quality` `CapabilityLevel` when set |
| `trust` | 65% from `artifact_verification`'s `VerificationStatus` (Unknown 0.0 → DeviceVerified 1.0), 35% from license-known-ness and gated-access |
| `license_fit` | `0.0` if `commercial_use_required` and the license doesn't establish it; `0.0`/`1.0` against the `permitted_licenses` allow-list when non-empty; otherwise a soft preference for a known, commercially-open license |

6. **Overall score** - a weighted sum of the seven components. The
   Arabic weight is preference-driven, not fixed: `0.30` when
   `language == arabic`, `0.12` when `language == both` or
   `arabic_priority` is set, `0.02` otherwise (never literally zero - a
   build with strong Arabic support can still edge out a tie even for an
   English-focused user). The remaining weight splits `28% device_fit /
   22% task_fit / 14% speed / 16% quality / 12% trust / 8% license_fit`.
   Any rejection reason forces `overall_score = 0.0` regardless of how
   well the components would otherwise have scored.
7. **Confidence** - `High` only for an `Exact` calibration match, `Medium`
   for `Close`, `Low` for `Distant`/no match, `Unknown` when
   `fit.state == Unknown` (e.g. available RAM couldn't be determined at
   all). "Never claim measured performance when only estimated" is
   structural here: `High` is unreachable without a real calibration
   record backing it.
8. **Plain-language explanation** - one to four short sentences built
   from the fit state, Arabic score, trust score, and confidence - never
   a bare number.

## Categories

Computed once over the sorted, non-rejected entries (`assign_best_category_tags`):

- **`best_match`** - the top of the deterministically-sorted list (see
  below) - always exactly one, if any build is eligible at all.
- **`best_arabic`** - highest `arabic` score among eligible builds with
  `arabic > 0.0`. Never tagged if no build has any real Arabic signal.
- **`fastest`** - highest `speed` score among eligible builds.
- **`best_quality`** - highest `quality` score.
- **`balanced`** - highest *minimum* of (`device_fit`, `task_fit`,
  `quality`, `speed`) - "no glaring weakness," a genuinely different
  notion from `best_match` (which follows the user's actual weighting).
- **`best_coding`** - among builds whose `task_categories` include
  Coding, the highest coding-capability-aware score.
- **`best_document`** - among builds whose `task_categories` include
  DocumentAnalysis, the longest `context_length`.
- **`heavy_but_possible`** - fit state `Experimental` or `Constrained`:
  might work, real uncertainty.
- **`not_recommended`** - fit state `NotRecommended`: memory doesn't
  safely fit, shown with the reason, never silently hidden.
- **`unknown`** - fit state `Unknown`: could not be evaluated at all
  (e.g. available RAM undetermined).
- **`unsupported`** - hard-rejected on preference/backend/license
  grounds - always exactly this category, regardless of what the scores
  would otherwise have been.

A single build can carry multiple tags (e.g. `best_match` and
`best_arabic` on the same entry) except the terminal
unsupported/not_recommended/unknown/heavy_but_possible states, which are
mutually exclusive by construction (one `fit_state` per build).

## Determinism

Every tie - the primary sort and every `tag_best` category pick - breaks
on `catalog_id` (ascending for the primary sort). Two calls with
identical inputs always produce identical output, and identical output
ordering across a process restart (no hash-map iteration, no
wall-clock-dependent tiebreak).

## Verified real bugs found and fixed while testing this stage

- **Tie-break direction was inverted**: `tag_best`'s comparator compared
  `(b, a)` instead of `(a, b)`, so category ties picked the
  *lexicographically smaller* `catalog_id` instead of the larger one -
  caught by `coding_use_case_ranks_the_coding_model_best_for_coding`
  initially tagging the wrong build.
- **A real data gap, not a scoring bug**: `qwen2.5-coder-1.5b-instruct`
  predates Stage B.1 and had no curated `coding_capability`, so it lost a
  genuine tie to a newer entry that did have one. Fixed by curating
  `coding_capability: good` onto it (an honest, defensible call - it's a
  real, purpose-built Qwen Coder variant per the publisher's own
  positioning), not by weakening the scoring formula.
- **Test-environment leakage**: several early test cases used
  `hardware::inspect(None)`-backed synthetic profiles without pinning the
  CUDA/Vulkan detection fields, so tests asserting exact calibration
  confidence or GPU-rejection behavior passed or failed depending on
  whether the machine actually running them has a real GPU (this repo's
  own dev machine does). Fixed by explicitly pinning both GPU backend
  fields (or forcing `cpu_only`) in every test that depends on backend
  selection, rather than relying on real hardware state.

## Known limitations

- `speed`'s calibration-based component only ever reflects records
  already in the calibration store - a build with zero calibration data
  gets v1's neutral `0.5` fallback, never a fabricated number, but that
  also means `speed` cannot yet distinguish "genuinely slow" from
  "unmeasured" beyond the `confidence` field.
- `quality`/`arabic`/coding/reasoning capability scores are only as good
  as the curated `CapabilityLevel` fields in the catalog - most existing
  (pre-Stage-B.1) entries report `unknown` for these and fall back to
  parameter-count/task-category-only heuristics.
- The Arabic-weight formula (`0.30`/`0.12`/`0.02`) and the
  `28/22/14/16/12/8` remaining split are initial, documented constants,
  not tuned against real user feedback - same caveat v1's own weight
  table already carries.
- **UPDATED (Stage B.6):** the Discover Models page now calls
  `recommend_v2` directly (see `docs/discover-models.md`). The Chat
  page's model selector does not call it yet - that's Stage B.8 (Chat
  selector integration).
