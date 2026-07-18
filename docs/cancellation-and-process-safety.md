# Cancellation and process safety

Spec section 6 requires: total tuning timeout, per-run timeout, max
candidate count, max retry count, cancellation support, guaranteed
child-process cleanup, crash/OOM detection, disk-space and
available-memory checks before every run, a cooldown between heavy runs,
and safe recovery after interruption — and, absolutely, **never an
orphaned llama.cpp process left behind.**

## The `TickAction` primitive

`runtime::process::run` (Stage 0) already guaranteed a killed-and-reaped
child on timeout. Stage 2 extended its `on_tick` closure to return a
`TickAction`:

```rust
pub enum TickAction { Continue, Cancel }
```

Returning `Cancel` on any poll iteration (~every 50ms) makes `run` kill
and reap the child exactly like a timeout does — but the outcome is
recorded as `ProcessRun.cancelled = true`, kept **distinct** from
`timed_out`, so nothing downstream ever misreports a deliberate
cancellation as a performance timeout. Verified:
`runtime::process::tests::cancel_action_kills_the_process_and_marks_cancelled_not_timed_out`
(kills a 30-second `ping` in under 5 seconds via `Cancel`, asserts
`cancelled && !timed_out && !succeeded()`).

Every layer above `process::run` (`llama_cpp::run_bench`/`run_cli_once`/
`run_cli_version`, `benchmark::runner::run`) threads this same signature
through — cancellation support isn't bolted onto tuning specifically,
it's a property of the shared process primitive every command uses.

## Where cancellation is actually checked during `brute tune run`

`tuning::runner::execute_plan` polls a caller-supplied `is_cancelled`
closure **between repetitions and between candidates** — never
mid-process. The CLI wires this to a plain file's existence:
`%LOCALAPPDATA%\BruteRuntime\tune-cancel-flag`
(`identity::default_local_state_dir`). `brute tune cancel` from another
terminal just creates that file; the running `brute tune run` process
notices it at its next checkpoint and stops, marking every remaining
candidate `skipped` with `SkipReason::Cancelled` rather than silently
truncating the summary.

This is a deliberate, documented granularity choice, not an oversight:
each tuning repetition already uses small, fast prompt/generation token
counts (`TUNE_PROMPT_TOKENS = 64`, `TUNE_GEN_TOKENS = 32`), so the gap
between "cancel requested" and "next checkpoint" is normally a few
seconds at most on CPU. True mid-process (`TickAction::Cancel`-driven)
cancellation of an individual tuning benchmark launch is not wired in
this CLI layer — see `docs/known-limitations.md`.

Verified: `tuning::runner::tests::cancellation_stops_remaining_candidates_and_marks_the_summary_cancelled`,
`a_cancelled_sample_mid_run_stops_the_whole_plan`. Live-verified: `brute
tune cancel` writes the flag file and prints the checkpoint-boundary
caveat verbatim.

## Guards enforced by `tuning::runner::execute_plan`

| Guard | Field/const | Behavior |
|---|---|---|
| Total time budget | `RunnerConfig.total_timeout` (default 30 min) | Remaining candidates marked `skipped` with `SkipReason::TotalTimeoutExpired` once elapsed |
| Per-run timeout | `RuntimeConfig.timeout_secs` (per-candidate, via `runtime::process::run`) | Already Stage 0's kill-on-timeout guarantee |
| Max candidate count | `tuning::candidates::MAX_CANDIDATES` (32) | Enforced at plan-generation time (see `docs/tuning-search-space.md`), not by the runner |
| Max retries | `RunnerConfig.max_retries_per_repetition` (default 1) | One retry on a single spurious repetition failure only |
| Available RAM check | `RunnerConfig.min_available_ram_bytes` (default 256 MB) | Checked fresh via a caller-supplied closure **before every single repetition attempt**, not once per candidate |
| Free disk check | `RunnerConfig.min_free_disk_bytes` (default 500 MB) | Same - fresh before every attempt |
| Cooldown around heavy runs | `RunnerConfig.cooldown_between_heavy_runs` (default 750ms) | Applied when a candidate offloads any GPU layers or requests context ≥ 4096 - before that candidate and before whatever follows it |
| Crash detection | `RepetitionSample.crashed` | Set when a repetition failed for a reason that was neither a timeout nor a cancellation |

The RAM/disk closures are called fresh on every attempt (not cached),
since a long tuning run's environment can genuinely change mid-run.

Verified: `tuning::runner::tests::insufficient_ram_skips_without_ever_calling_run_repetition`,
`insufficient_disk_skips_without_ever_calling_run_repetition`,
`total_timeout_expiry_skips_remaining_candidates`,
`a_single_spurious_failure_is_retried_and_can_still_recover`,
`heavy_candidates_incur_a_cooldown_sleep`.

## What Stage 2 explicitly does NOT touch

Per spec section 6: BIOS, drivers, power limits, voltage, clocks, fan
curves, registry performance settings, and Windows security settings are
never modified. `brute` optimizes runtime *parameters passed to
llama.cpp* only (threads, GPU layers, context, batch) — nothing about
the machine itself.

## No orphaned processes

Because every launch goes through the same `runtime::process::run`
primitive, and that primitive always either waits for natural exit or
explicitly `kill()`s and then `wait()`s the child before returning, there
is no code path in this crate that spawns a `llama-cli`/`llama-bench`
process and returns without having waited on it — a cancelled or
timed-out tuning run cannot leave a runaway process behind by
construction, not merely by convention.
