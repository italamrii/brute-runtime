# Stage 2 — Runtime Auto-Tuning & Backend Selection

## In plain language

Stage 1 answered "will this model fit my machine?" before you download
anything. Stage 2 answers a different question, after you've picked a
model: **given the exact model file and this exact machine, which
runtime settings (backend, thread count, GPU offload, context, batch
size) actually work best — proven by real, repeated benchmarks, not
predicted?**

It does this in four steps:

1. **Verify backends actually work**, not just that a driver was
   detected (`brute backends verify`) — see `docs/backend-verification.md`.
2. **Generate a small, bounded, safe candidate list** of configurations
   worth trying (`tuning::candidates`) — see `docs/tuning-search-space.md`.
3. **Safely benchmark each candidate** with repeated samples, resource
   guards, and cooperative cancellation (`tuning::runner`) — see
   `docs/cancellation-and-process-safety.md` and
   `docs/stability-classification.md`.
4. **Rank the results** for a priority you choose, and optionally save
   the winner as a reusable local profile (`tuning::ranking`,
   `tuning::runtime_profile`) — see `docs/tuning-ranking-methodology.md`
   and `docs/runtime-profile-schema.md`.

Every measurement is real (a real llama.cpp subprocess actually ran);
every prediction (VRAM/RAM headroom) is explicitly labeled as a
prediction; a config is never recommended for being fast if it wasn't
also proven stable — see `docs/tuning-ranking-methodology.md`. Nothing
leaves this machine — see `docs/privacy-model.md`.

## Architecture

See `docs/architecture.md` for the full module map. In one sentence:
`backends::verify_backend` decides which GPU backend (if any) is trusted
enough to tune for; `tuning::candidates::generate_plan` turns
(model, machine profile, verified backend) into a bounded `TuningPlan`;
`tuning::runner::execute_plan` safely benchmarks it under resource/time
guards, producing repeated-sample results per candidate;
`tuning::stability::classify` turns those repetitions into a 5-state
verdict; `tuning::ranking::rank` turns the whole run into a winner/
runner-up/safer-fallback for a chosen priority; `tuning::runtime_profile`
optionally persists the winner locally; `tuning::apply::apply_and_verify`
later confirms a saved profile still actually works before treating it
as applied.

## Commands

```
brute backends verify --model <path> --llama-bin <dir> [--backend cpu|cuda|vulkan] [--allow-unverified-binary] [--json]

brute tune run --model <path> --llama-bin <dir> [--priority balanced] [--backend cpu|cuda|vulkan]
                [--max-duration-secs N] [--dry-run] [--allow-unverified-binary] [--save-profile] [--json]
brute tune status [--json]
brute tune cancel

brute profiles list [--json]
brute profiles show <profile-id> [--json]
brute profiles verify <profile-id> --model <path> --llama-bin <dir> [--json]
brute profiles export <profile-id> --output <file>
```

The CLI groups `tune run`/`status`/`cancel` and `profiles
list`/`show`/`verify`/`export` under two subcommand namespaces rather
than the spec's flatter `brute tune ...`/`brute tune status` form — the
spec explicitly allows CLI naming to be "improved while preserving
clarity," and an explicit `run` keyword is what let clap express "flags
here" and "a bare status/cancel command" under one consistent
subcommand shape rather than a special-cased parser.

`--dry-run` shows the full planned/pruned candidate list without
launching a single benchmark — deterministic, fast (see
`docs/known-limitations.md` for measured plan-generation time).

## Real output on this machine

```
$ brute tune run --model C:\Models\qwen2.5-0.5b-instruct-q4_k_m.gguf `
    --llama-bin .tools\llama.cpp\b10064\cpu --max-duration-secs 240 --save-profile
Runtime profile saved: profile-instance-f052be34d631ff2889844a7551eea020
Tuning complete in 52.8s (14 candidate(s) benchmarked).

Winner: ctx-1024-batch-128 (High confidence)
  backend=Cpu threads=16 gpu_layers=0 context=1024 batch=128
  stability=Stable generation=Some(75.46) tok/s prompt=Some(512.36) tok/s
Runner-up: ctx-1024-batch-256
Rejected faster candidate threads-12: measured 81.18 tok/s generation (faster than
  the winner's 75.46 tok/s) but ranked below it because its stability was
  classified stable versus the winner's stable (formula stage2-ranking-v1)
```

See `docs/stage-2-verification.md` for the complete transcript across
every command, including the real bug found and fixed during live
verification.

## What Stage 2 explicitly does NOT do

- No BIOS/driver/power-limit/registry/Windows-security changes — see
  `docs/cancellation-and-process-safety.md`.
- No telemetry, no network requests, no cloud database — see
  `docs/privacy-model.md`.
- No brute-force cartesian search — see `docs/tuning-search-space.md`
  for why and what the bounded alternative is.
- Never claims a GPU backend works from driver detection alone, and
  never silently falls back from GPU to CPU while reporting GPU success
  — see `docs/backend-verification.md`.
