# Architecture — Stage 0, Stage 1, and Stage 2

BRUTE Runtime is a single Rust binary crate (`brute`), not a workspace.
The module boundaries below are the seams a future multi-crate split
would fall along, but a workspace isn't justified yet at this scope.

```
src/
  main.rs           CLI entry point, command dispatch, exit codes
  cli/mod.rs         clap argument definitions
  errors.rs           BruteError and per-domain error enums (thiserror)
  provenance.rs        Stage 1's Provenance/Valued<T> - 5-way (adds "Catalog" to
                       Stage 0's 4-way Confidence), kept separate to avoid
                       destabilizing Stage 0's serialized shape
  hardware/
    mod.rs             Confidence, HardwareField<T>, HardwareReport aggregate
    cpu.rs              vendor/brand/cores (sysinfo) + ISA flags (raw-cpuid)
    memory.rs           GlobalMemoryStatusEx (Win32)
    gpu.rs              DXGI adapter enumeration + nvidia-smi + Vulkan probe
    power.rs             GetSystemPowerStatus (Win32); Stage 1 addition
    windows.rs          OS version (registry), architecture, storage free space
  models/
    mod.rs             ModelReport aggregate, inspect_model()
    gguf.rs             streaming GGUF header/KV/tensor-info parser; Stage 1
                         added architecture-hyperparameter extraction
                         (context_length, embedding_length, block_count,
                         attention head/kv-head counts) for the exact KV-cache
                         formula
    validation.rs       path safety (delegates to security::paths)
  runtime/
    mod.rs             Backend enum, RuntimeConfig
    process.rs          argv-based Command wrapper: timeout, kill, capture
    llama_cpp.rs         llama-bench/llama-cli argument building, output parsing,
                         binary hash verification
  benchmark/
    mod.rs             BenchmarkReport aggregate
    runner.rs            orchestrates one llama-cli + one llama-bench run
    metrics.rs            metric types (throughput, memory, timing)
    scoring.rs             stability classification from sample variance
  report/
    mod.rs             CapabilityReport aggregate, recommendation derivation
    json.rs              serde_json (de)serialization + Stage 1 report-path
                         username redaction
    terminal.rs           human-readable rendering with confidence tags
  security/
    mod.rs             re-exports
    paths.rs             canonicalize + regular-file validation + Stage 1
                         report-path redaction
    hashing.rs            SHA-256 (streamed) + binary pin verification
  profile/
    mod.rs             Stage 1: normalizes HardwareReport into the stable
                       HardwareCapabilityProfile schema; machine_id, confidence
                       roll-up
  catalog/
    mod.rs, schema.rs   Stage 1: ModelBuild schema, bounded/validated JSON
                       loading (untrusted input)
  estimator/
    mod.rs, formulas.rs  Stage 1: memory/disk range estimation, documented
                       formulas, versioned "stage1-v1"
  fit/
    mod.rs             Stage 1: six-state fit classifier
  calibration/
    mod.rs             Stage 1: real-benchmark record store, nearest-match
                       lookup with documented confidence degradation
  recommend/
    mod.rs, explain.rs   Stage 1: weighted ranking engine + two-tier
                       (simple/technical) explanation
  identity.rs          Stage 2: random, resettable, non-fingerprinting
                       local instance ID (BCryptGenRandom) - separate from
                       Stage 1's coarse hardware-derived machine_id
  backends/
    mod.rs             Stage 2: BackendStatus/BackendVerification - proves
                       a backend actually launches/loads/benchmarks, never
                       trusts detection alone; the anti-silent-fallback check
  tuning/
    mod.rs             Stage 2: TUNING_FORMULA_VERSION
    candidates.rs        bounded/deterministic candidate generation + pruning
    runner.rs             safety-guarded execution: timeouts, retries, RAM/
                         disk checks, cooldowns, cooperative cancellation
    stability.rs           5-state cross-repetition stability classifier
    ranking.rs              lexicographic-tier configuration ranking
    runtime_profile.rs      saved local profile schema, invalidation, sanity
    apply.rs                apply-and-verify workflow for a saved profile
```

## Data flow

```
brute inspect
  hardware::inspect()  ->  CapabilityReport { hardware, .. }  ->  report::terminal | report::json

brute model inspect
  models::inspect_model()  (security::validate_regular_file -> sha256_file -> gguf::inspect)
    -> CapabilityReport { hardware, model }

brute benchmark / recommend
  models::inspect_model()
  runtime_config_from_args()
  benchmark::run()
    runtime::llama_cpp::run_cli_once()   -> one-shot load-time/TTFT sample
    runtime::llama_cpp::run_bench()      -> repeated-sample throughput (llama-bench -o json)
    (both go through runtime::process::run - argv Command, timeout, capture)
  benchmark::scoring::classify_stability()
  report::build_recommendation()   (recommend only)
    -> CapabilityReport { hardware, model, benchmark, recommendation }

brute report --output x.json
  same pipeline as above (model/benchmark sections optional), written via report::json
```

## Why a streaming GGUF parser instead of a crate

GGUF metadata parsing is a small, well-specified binary format (magic,
version, two length-prefixed tables). Pulling in a full tensor/ML crate
(e.g. `candle`) to read a header would add a large dependency surface for
functionality we can implement directly, with exact control over the
one property that matters here: **never allocate memory proportional to a
hostile or oversized length field, and never read the tensor data blob**.
`src/models/gguf.rs` documents the wire format and the sanity caps applied
to every count/length before it is acted on.

## Why shell out to llama.cpp instead of binding to it

llama.cpp is a fast-moving C++ project built with CMake and (optionally)
CUDA/Vulkan SDKs. Binding to it via FFI would mean building it from source
in this repo's CI/dev environment (this Stage 0 environment has no `cmake`
and no CUDA toolkit installed) and re-exposing an unstable internal API.
Launching the official prebuilt `llama-bench`/`llama-cli` binaries as
subprocesses, with a pinned+verified download, is the documented, safe,
stable-surface way to integrate: see
[`docs/security-model.md`](security-model.md) for the trust boundary and
[`scripts/fetch-llama-cpp.ps1`](../scripts/fetch-llama-cpp.ps1) for how the
binaries are obtained.

## Stage 1 data flow

```
brute profile create
  hardware::inspect(storage_path)  ->  profile::build_profile()  ->  HardwareCapabilityProfile

brute catalog list/show
  catalog::load_catalog()  (bounded, validated, untrusted-input parsing)  ->  Catalog

brute fit / recommend-model / explain-fit
  hardware::inspect() -> profile::build_profile()
  catalog::load_catalog()
  calibration::CalibrationStore::load()
  for each candidate build:
    estimator::estimate(build, None, profile, config)   -> MemoryEstimate (range + assumptions)
    calibration::find_nearest(...)                       -> CalibrationMatch (Exact/Close/Distant) or None
    fit::evaluate(build, estimate, profile, has_calibration_support) -> FitResult (6-state)
  recommend::rank()      -> weighted score x fit-state multiplier, sorted
  recommend::recommend() -> primary + safer fallback + stronger optional
  recommend::explain::build_explanation() -> simple + technical two-tier explanation
```

## Stage 2 data flow

```
brute backends verify
  hardware::inspect() -> profile::build_profile()
  backends::verify_backend()  (per backend)  -> BackendVerification

brute tune run
  models::inspect_model()
  hardware::inspect() -> profile::build_profile()
  determine_verified_gpu_backend()   (backends::verify_backend, opportunistic)
  tuning::candidates::generate_plan()   -> TuningPlan (bounded, deterministic)
  [--dry-run stops here]
  tuning::runner::execute_plan()
    per candidate, per repetition: RAM/disk check -> runtime::llama_cpp::run_bench()
    -> tuning::stability::classify()          -> TuningRunSummary
  tuning::ranking::rank()             -> winner / runner-up / safer fallback
  [--save-profile] tuning::runtime_profile::build_profile() -> saved locally

brute profiles verify <id>
  tuning::runtime_profile::load_profile_from()
  tuning::runtime_profile::sanity_check()        (impossible-values gate)
  tuning::runtime_profile::check_still_valid()   (environment-compatibility gate)
  tuning::apply::apply_and_verify()              -> real short verification run
```

## Why Stage 2 has its own `BackendStatus` instead of reusing `Confidence`/`Provenance`

Stage 0's `Confidence` (Measured/Detected/Inferred/Unavailable) and
Stage 1's `Provenance` (adds Catalog) both describe *how sure we are
about a fact*. Stage 2's `BackendStatus` describes something
categorically different: *how far a verification pipeline got* (detected
→ binary present → launches → model loads → benchmark completes →
confirmed-not-a-fallback). Collapsing it into `Confidence` would force
`LaunchFailed`/`ModelLoadFailed`/`BenchmarkFailed` - genuinely distinct
failure points a user needs to distinguish to fix the right thing - into
one `Unavailable`. `backends::BackendVerification` mirrors the exact
8-field structured shape specified for Stage 2, independent of both
existing vocabularies. See `docs/backend-verification.md`.

## Why Stage 2 candidate generation is three independent groups, not staged/adaptive refinement

A greedy search that refines around the best-measured-so-far result
needs measurements that don't exist yet at plan-generation time -
incompatible with `--dry-run` showing the full plan before anything
runs. `tuning::candidates::generate_plan` instead produces three
independent candidate groups (threads, GPU layers, context×batch), each
anchored on the same fixed defaults for the other dimensions. This keeps
the whole plan pure, deterministic, and computable in one pass, at the
documented cost of not exploring cross-dimension interactions. See
`docs/tuning-search-space.md`.

## Why Stage 1 has a separate `Provenance`/`Valued<T>` instead of widening `Confidence`

Stage 0's `hardware::Confidence` is 4-way (Measured/Detected/Inferred/
Unavailable) and its serialized shape is exercised by every existing
Stage 0 test and consumer. Stage 1 needs a 5th category — "this is
curated catalog metadata about the *model*, not a fact about *this
machine*" — which doesn't fit any of the four existing meanings without
blurring the distinction the product principle requires. Rather than add
a variant to `Confidence` (which every existing `match` on it would need
to handle, and which would change Stage 0's serialized enum), Stage 1
defines its own `provenance::Provenance` + `Valued<T>`, with a `From<
HardwareField<T>>` conversion for reuse where a Stage 0 fact flows into a
Stage 1 structure (e.g. `profile::HardwareCapabilityProfile`). Stage 0's
types and JSON shape are completely unchanged - verified by
`report::json::tests::stage0_json_field_paths_remain_present_after_stage1_additions`.

## Extension points for later stages

- `runtime::Backend` is an enum today; a `Backend` trait with `Cpu`/`Cuda`/
  `Vulkan` implementations is the natural next step if a second inference
  engine (not llama.cpp) is ever added.
- `hardware::gpu` isolates all vendor-specific detection (DXGI, nvidia-smi,
  Vulkan loader probing) behind the single `inspect_gpu()` entry point.
- `report::CapabilityReport` is the single serialization boundary; a future
  desktop UI would consume this JSON shape rather than calling internal
  modules directly.
- `catalog::Catalog` is currently loaded from one local file; a future
  stage could add multiple catalog sources merged together without
  changing `ModelBuild`'s schema.
- `calibration::CalibrationStore` is a flat JSON file today; the
  `find_nearest` lookup is already decoupled from storage, so a future
  stage could swap in a different store (e.g. SQLite) without touching
  `fit`/`recommend`.
- `tuning::runtime_profile` is a flat per-file JSON store today (mirrors
  `calibration::CalibrationStore`'s tradeoffs); a future desktop UI could
  read `%LOCALAPPDATA%\BruteRuntime\profiles\` directly via the same
  `RuntimeProfile` JSON shape rather than a new IPC surface.
- `tuning::runner::execute_plan`'s `is_cancelled`/RAM/disk closures are
  already decoupled from any concrete I/O - a future stage adding a
  proper cross-process progress channel (rather than the current
  best-effort `tune-status.json` file) would only need to change the
  CLI-layer closures in `main.rs`, not the runner itself.
