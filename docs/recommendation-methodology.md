# Recommendation & Ranking Methodology

## In plain language

BRUTE never recommends "the biggest model that technically fits." It
scores every catalog build against ten named factors, weighted according
to what you say you care about (fastest / balanced / highest quality /
lowest memory / longest context / coding / Arabic & general chat /
privacy-first), and the fit classification always gates the score so a
risky or oversized build can never outrank a safe one just because a
priority happens to favor size or speed.

## The ten components (`recommend::ComponentScores`)

| Component | What it measures |
|---|---|
| `headroom` | Fit engine's headroom ratio, normalized to 0–1 |
| `storage` | Whether disk space is known-sufficient |
| `backend` | Whether a supported backend is available |
| `calibration` | Proximity of the nearest real benchmark (Exact/Close/Distant/none) |
| `speed` | Nearest calibration's generation tok/s, normalized against a fixed 40 tok/s reference |
| `task_match` | Whether the build is tagged for the requested task |
| `context` | Whether the requested context is realistic for this build |
| `stability` | Nearest calibration's measured stability (Stable/Marginal/Unstable) |
| `quality_pref` | log-scaled parameter count (bigger = higher, capped at a 30B reference) |
| `openness_pref` | Rewards non-gated, known-license builds |

## Every weight is explicit (`ranking_formula_version: "stage1-v1"`)

Each priority is a fixed 10-value weight vector summing to 1.0, hardcoded
and documented in `recommend::weights_for`. Examples:

- **Fastest**: `speed=0.40` dominates; everything else is minor.
- **Highest quality**: `quality_pref=0.40` dominates.
- **Lowest memory**: `headroom=0.30` + `openness_pref=0.20` dominate.
- **Longest context**: `context=0.40` dominates.
- **Coding** / **Arabic & general chat**: `task_match=0.35` dominates,
  and the priority also implicitly sets the task filter if you didn't
  pass `--task` explicitly.
- **Privacy-first offline**: `openness_pref=0.35` dominates (non-gated,
  known-license builds score higher — a proxy for "no account/license
  click-through required to obtain it").
- **Balanced**: no single component exceeds 0.13 — the closest thing to
  an even blend.

See `src/recommend/mod.rs::weights_for` for the complete table.

## The fit-state gate always wins

```
score = (weighted sum of the 10 components) × fit_state_multiplier
```

| Fit state | Multiplier |
|---|---|
| Excellent | 1.0 |
| Good | 0.9 |
| Constrained | 0.7 |
| Experimental | 0.5 |
| Unknown | 0.25 |
| Not recommended | 0.05 |

This is what actually prevents "recommend the largest model because it
fits": a `HighestQuality`-weighted score for a `NotRecommended` 70B build
is multiplied by 0.05, so it can never outrank a `Good`-or-better smaller
build regardless of how much the quality-preference component favors it.

## Real proof that priority changes the outcome

```
$ brute recommend-model --priority lowest-memory --storage-path C:\Models
Recommended: Qwen2.5 0.5B Instruct (Q4_K_M) ... Score: 0.925

$ brute recommend-model --priority highest-quality --storage-path C:\Models
Recommended: Qwen2.5 Coder 1.5B Instruct (Q4_K_M) ... Score: 0.898
Safer fallback: Qwen2.5 0.5B Instruct (Q4_K_M)
```

Same machine, same catalog, same calibration store — different priority,
different top pick, and `highest-quality` still surfaces the smaller
model as an explicit "safer fallback" rather than hiding it. See
`recommend::tests::highest_quality_priority_prefers_a_larger_model_than_lowest_memory_priority`
and `recommend::tests::coding_priority_ranks_the_coding_model_above_pure_chat_models_of_similar_size`
for the automated versions of this proof.

## Recommended build, safer fallback, stronger optional

`recommend::recommend` picks:

- **Recommended**: whatever ranked #1 for the requested task/priority.
- **Safer fallback**: the highest-scoring *smaller*, Excellent/Good-fit
  build other than the recommendation (if one exists).
- **Stronger optional**: the highest-parameter-count build that's still
  at least Constrained fit and *larger* than the recommendation (if one
  exists).

Both are optional (`None` when no such build exists in the catalog) -
never fabricated to fill the slot.

## Explainability

Every recommendation carries two explanations (`recommend::explain`):

- **Simple** (`Explanation.simple`): one to two sentences, plain language,
  mentions the safer/stronger alternative when relevant. Real example:
  *"Fast and comfortable on your machine. A larger build (Qwen2.5 Coder
  1.5B Instruct (Q4_K_M)) may give better quality but will use
  significantly more memory."*
- **Technical** (`Explanation.technical`): detected resources, the exact
  memory-calculation formula with real numbers, which calibration record
  was used and how far away it is, backend/context assumptions, fit
  thresholds, and the ranking-formula version. See
  `docs/stage-1-verification.md` for a full real transcript from
  `brute explain-fit`.

Determinism: identical (catalog, profile, calibration store, task,
priority) always produces an identical ranked order — ties break on
`catalog_id` for a total order. See
`recommend::tests::ranking_is_deterministic`.
