# Configuration ranking methodology

`tuning::ranking::rank(plan, summary, priority)` turns a completed
tuning run into an ordered list plus a winner, runner-up, and safer
fallback. Formula version `TUNING_RANKING_FORMULA_VERSION =
"stage2-ranking-v1"` — distinct from Stage 1's *model* ranking formula
(`recommend::RANKING_FORMULA_VERSION = "stage1-v1"`), which ranks
catalog builds, not runtime configurations. Don't confuse the two.

## Why lexicographic tiers, not a weighted sum

Stage 1's model ranking (`docs/recommendation-methodology.md`) uses a
weighted linear score — appropriate there because every component is
already normalized to `[0,1]` and gated by a fit-state multiplier.
Stage 2 ranks *measured configurations* where one dimension (raw
throughput) can vary by very large, unbounded factors between
candidates. A weighted sum cannot guarantee "a marginally faster
unstable configuration must NOT outrank a stable safe one" for an
arbitrary combination of magnitudes — a sufficiently large speed gap
could always out-weigh a small stability penalty. A **lexicographic
comparator** guarantees this by construction: a candidate only wins on a
later tier once every earlier tier is exactly tied.

Every priority's tier list starts with the same first tier —
`completed` (did any repetition of this candidate actually succeed?) —
so an incomplete candidate can never win under any priority, regardless
of how the rest of the tiers are ordered. Verified:
`tuning::ranking::tests::incomplete_candidates_never_win_regardless_of_priority`.

## Balanced (the default) — safety-first, exactly per spec section 8

```
[completed, stability, ram_safety, generation, prompt, context, fewer_threads]
```

This is the spec's literal priority order: 1) successful completion,
2) stability, 3) safe memory headroom, 4) generation performance,
5) prompt performance, 6) reasonable context, 7) lower resource
pressure. Stability sits ahead of *every* throughput tier, so a faster-
but-`Unstable` candidate can never outrank a slower `Stable` one under
`Balanced`. Verified:
`tuning::ranking::tests::balanced_priority_never_lets_a_faster_unstable_candidate_outrank_a_stable_one`.

## The other six priorities — named criterion first, stability as tiebreak

`FastestGeneration`, `FastestPromptProcessing`, `LowestMemory`,
`LongestContext`, and `LaptopFriendly` all put their named optimization
target *ahead* of the stability tier (with `MaximumStability` being the
one exception, where stability is deliberately still dominant — the
whole point of choosing that priority). This is an intentional
divergence from `Balanced`: a user who explicitly asks for "fastest
generation" gets the genuinely fastest completed candidate, even if only
`Marginal` — but is never left in the dark about the tradeoff, because
`safer_fallback` (below) is computed independently of the winner and
always surfaces the best proven-`Stable` alternative when one differs
from the pick. Verified:
`tuning::ranking::tests::fastest_generation_priority_picks_raw_speed_even_when_only_marginal`
(asserts the winner is the faster `Marginal` candidate **and** the
safer fallback correctly points to the slower `Stable` one instead).

| Priority | Tier order (after `completed`) |
|---|---|
| `balanced` | stability, ram_safety, generation, prompt, context, fewer_threads |
| `fastest_generation` | generation, stability, prompt, ram_safety |
| `fastest_prompt_processing` | prompt, stability, generation, ram_safety |
| `lowest_memory` | ram_safety, stability, generation |
| `longest_context` | context, stability, generation |
| `maximum_stability` | stability, generation, prompt |
| `laptop_friendly` | ram_safety, fewer_threads, stability, generation |

Unmeasured values (`None`) map to `f64::MIN` in every tier — an
unmeasured candidate never wins a tier over a measured one, never
treated as an optimistic default.

## Output fields

- **`winner`** / **`runner_up`**: top two ranked *completed* candidates.
- **`safer_fallback`**: the winner itself if it's already `Stable`,
  otherwise the highest-ranked candidate that genuinely *is* `Stable`
  (or `None` if none completed and was stable).
- **`rejected_faster_candidates`**: every candidate with strictly higher
  measured generation throughput than the winner that still ranked
  below it, each with an explicit reason naming both stability
  classifications — this is what makes a "faster but riskier" rejection
  visible rather than silent.
- **`confidence`**: `High` (winner `Stable`), `Medium` (winner
  `Marginal`), `Low` (winner `Unstable`/`Failed`/`Unknown`, or no
  winner at all).
- **`unknown_values`**: human-readable notes on what the winner's own
  measurements don't cover (e.g. "predicted VRAM usage is unknown") —
  never silently omitted.
- **`model_sha256`** / **`machine_profile_schema_version`**: carried
  straight from the source `TuningPlan`, so a ranking result is always
  traceable to exactly which model/machine it applies to.

## Real result on this machine

Ranking 14 CPU candidates for Qwen2.5-0.5B-Instruct under `balanced`
picked `ctx-1024-batch-128` (75.46 tok/s generation, `Stable`) over
`threads-12` (81.18 tok/s, also `Stable` but with **unknown** predicted
RAM — thread-dimension candidates never get a RAM prediction, so they
lose the `ram_safety` tier to any context/batch candidate with a known,
safe RAM estimate) and `ctx-4096-batch-512` (75.47 tok/s but only
`Marginal`). See `docs/stage-2-verification.md` for the full transcript.
