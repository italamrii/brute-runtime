# Stability classification

Two distinct, deliberately separate stability classifiers exist in this
codebase — do not confuse them:

- **`benchmark::scoring::classify_stability`** (Stage 0/1): classifies
  one benchmark report from the variance *within* `llama-bench`'s own
  internal `-r` repetitions (a single process launch, N samples inside
  it). 3-state: `Stable`/`Marginal`/`Unstable`.
- **`tuning::stability::classify`** (Stage 2, this document): classifies
  one *candidate configuration* from variance **across independent
  process launches** — full separate benchmark attempts, each one a
  distinct llama-bench invocation via `tuning::runner`. 5-state:
  `Stable`/`Marginal`/`Unstable`/`Failed`/`Unknown`.

The distinction matters: a single llama-bench invocation can report tight
internal variance while the *machine* is unstable across repeated
launches (background load, thermal throttling between runs, a transient
crash) — Stage 2's classifier is the one that catches that.

## The formula (`STABILITY_FORMULA_VERSION = "stage2-stability-v1"`)

Given N repetition outcomes (each: succeeded, timed out, cancelled,
generation tok/s, prompt tok/s):

1. **Zero repetitions attempted** → `Unknown` ("no repetitions were
   run").
2. **Zero successful repetitions** → `Failed`, with a breakdown of *why*
   each one failed (`failure_breakdown`: counts of timed-out vs
   cancelled vs crashed/non-zero-exit — spec section 7 explicitly asks
   for timeouts and crashes to be distinguishable, not folded into one
   generic "failed").
3. **Fewer than `MIN_SUCCESSFUL_REPETITIONS_FOR_VARIANCE` (2) successes**
   → `Unknown`, explicitly **not** `Stable` — a single successful run is
   never treated as proof of stability, regardless of how clean it
   looked.
4. **≥ 2 successes, but some repetitions failed** → `Unstable`,
   unconditionally, regardless of how tight the variance is among the
   runs that *did* succeed. A partial failure is never classified as
   stable.
5. **All requested repetitions succeeded** → coefficient of variation
   (`stddev / mean`, computed independently for generation and prompt
   throughput) determines the tier:
   - CV ≤ `CV_STABLE_MAX` (0.05) → `Stable`
   - CV ≤ `CV_MARGINAL_MAX` (0.15) → `Marginal`
   - CV > 0.15 → `Unstable`
   - The worse of the two (generation, prompt) CVs decides the tier.

Every `StabilityAssessment` carries `repetitions_requested`,
`repetitions_succeeded`, both CVs (`Option<f64>`, `None` when
unmeasurable), `formula_version`, and a human-readable `reason` string —
never just a bare enum value.

## Verified behavior

`tuning::stability::tests`: `no_repetitions_is_unknown`,
`single_success_is_unknown_not_stable`, `all_failed_is_failed`,
`low_variance_repeated_successes_is_stable`,
`moderate_variance_is_marginal`, `high_variance_is_unstable`,
`a_partial_failure_among_repetitions_is_unstable_even_with_tight_variance_on_successes`,
`cancelled_repetitions_count_as_not_succeeded`,
`zero_average_throughput_does_not_divide_by_zero`,
`formula_version_is_recorded_on_every_assessment`.

## Real-world result

On this machine, tuning Qwen2.5-0.5B-Instruct across 14 CPU candidates
(3 repetitions each) classified the winning `ctx-1024-batch-128`
candidate `Stable` (all 3 repetitions succeeded, tight throughput
variance around ~75.5 tok/s generation), while one nearby candidate
(`ctx-4096-batch-512`) was classified `Marginal` — see
`docs/stage-2-verification.md` for the full transcript.

## Design note: why the threshold is on *repetitions*, not one long run

Repeating short, independent process launches (rather than one longer
run) is what actually exercises "does this configuration hold up across
launches" — a single long-running process staying internally consistent
says nothing about launch-to-launch variance from background load,
thermal state drift between runs, or a driver hiccup on the Nth launch.
`tuning::runner`'s default is 3 repetitions per candidate
(`DEFAULT_REPETITIONS_PER_CANDIDATE`), each a fresh, independent
`llama-bench` process.
