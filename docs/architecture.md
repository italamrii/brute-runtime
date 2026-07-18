# Architecture — Stage 0 through Stage 4 (Windows desktop MVP)

BRUTE Runtime's engine lives in a `brute` **library** crate at the repo
root (`src/lib.rs`), reused unchanged by two front ends: the `brute` CLI
binary (`src/main.rs`) and the Tauri desktop app (`desktop/src-tauri/`).
Neither front end duplicates engine logic - both are thin, validated
wrappers. See "Stage 4: desktop application" below for the split and the
desktop-specific module tree; everything under `src/` below is Stage
0-3, unchanged in substance by the split (only crate membership moved).

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
  library/
    mod.rs               Stage 3: LibraryStore/LibraryEntry schema, atomic
                       JSON persistence, quarantine/forget/remove-managed
    scan.rs                bounded/cancellable directory discovery
    import.rs               validate/hash/parse pipeline + catalog matching
    verify.rs                GgufVerification pipeline, file-change
                          detection, locate/relocation recovery
    duplicates.rs             SHA-256-based duplicate grouping
    associations.rs            live lookup of Stage 2 profiles / Stage 1
                            calibration records by content hash
    storage.rs                read-only storage usage analysis
    audit.rs                  library-wide health report aggregation
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

## Stage 3 data flow

```
brute library scan <dir>
  library::scan::scan()   -> ScanResult (never imports)

brute library import <path>
  models::inspect_model()   (Stage 0: validate, hash, parse - unchanged)
  library::import::match_against_catalog()
  library::verify::verify_artifact()
  -> create or update LibraryEntry in LibraryStore -> save_to()

brute library verify <id> | --all
  library::verify::verify_entry()   (recomputes hash, updates trust/file_status)

brute library locate <id> <new-path>
  security::validate_regular_file() -> sha256_file() -> compare to entry.sha256
  match: rebind current_path, verify_entry()   |   mismatch: reject, entry untouched

brute library show <id>
  library::associations::find_runtime_profiles_for()     (Stage 2 profiles, by hash)
  library::associations::find_calibration_matches_for()   (Stage 1 calibration, by hash)

brute library audit
  library::duplicates::find_duplicate_groups()
  library::associations::*   (stale-profile/stale-calibration detection)
  -> AuditReport
```

## Why the local library is a bounded JSON file, not SQLite

Every real lookup Stage 3 needs is "by ID," "by hash," or "scan every
entry" - none of which need a query planner or joins, and entry counts
are expected in the hundreds to low thousands (measured directly to
10,000 synthetic entries - see `docs/stage-3-verification.md`).
Introducing `rusqlite`/`sqlx` would add a new dependency and a migration
framework for a problem `Vec<LibraryEntry>` plus a `BTreeMap` grouping
already solves within milliseconds at the tested scale. Full
justification: `docs/local-library-schema.md`.

## Why runtime-profile/calibration associations are computed live, never persisted

A persisted `associated_runtime_profile_ids` field on `LibraryEntry`
would be a second source of truth that could silently drift from the
actual profile store (e.g. a profile deleted directly from
`%LOCALAPPDATA%\BruteRuntime\profiles\` would leave a dangling
reference). `library::associations` looks both up fresh, by content
hash, on every call - the cost is negligible at the measured scale, and
the result can never be stale. See
`docs/runtime-profile-association.md`.

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
- `library::LibraryEntry.managed_copy` and `remove_managed`'s root-
  boundary check already exist, ready for a future managed-copy import
  mode (`--copy-into-library`) without needing new safety plumbing - see
  `docs/model-import-and-verification.md`.
- Stage 0-2 commands (`brute benchmark`, `brute tune run`, etc.) still
  take a raw file path - a future stage could add a `--library-id`
  alternative that resolves through `LibraryStore` (and, at that point,
  would be the natural place to make quarantine actually block a
  launch, not just `brute library verify`).
- `catalog::ModelBuild` has no field for an independently curated
  expected SHA-256; adding one (verified against a real downloaded
  file, as one dev-catalog entry already is) would let
  `import::match_against_catalog` reach the `Exact`/`ExpectedHashMatched`
  tiers that exist in the vocabulary today but can never fire - see
  `docs/trust-and-provenance.md`.

## Stage 4: desktop application

```
desktop/
  src-tauri/                Rust backend (Tauri v2)
    Cargo.toml               depends on `brute = { path = "../.." }`
    src/
      lib.rs                  tauri::Builder setup, full command registration
      main.rs                 desktop_lib::run() entry point
      state.rs                 AppState: in-memory tune/run cancellation flags only
      paths.rs                 resolves bundled seed data (dev-catalog.json,
                               seed-calibration.json) in dev vs. packaged builds
      commands/
        hardware.rs             hardware_profile
        catalog.rs               catalog_list/show, calibrations_list,
                                 fit_evaluate, recommend_model, explain_fit
        backends.rs               backends_verify
        library.rs                 full trusted-library surface (list, show,
                                   associations, import, scan, import_directory,
                                   verify, refresh, audit, duplicates, storage,
                                   locate, alias, note, forget, quarantine,
                                   unquarantine, quarantined, export)
        tuning.rs                   tune_dry_run, tune_run (async, progress
                                   events, cancellable), tune_cancel
        profiles.rs                   profiles_list/show/verify/export/delete
        run.rs                         local_run_generate (streaming),
                                       local_run_cancel
    capabilities/default.json    explicit permission allowlist (see
                                 docs/security-model.md)
    tauri.conf.json               strict CSP, asset protocol disabled, window
  src/                         React + TypeScript frontend
    lib/api.ts                   the only module that calls Tauri `invoke` -
                                 every command has one typed wrapper here
    lib/types.ts                  hand-maintained TypeScript mirrors of the
                                 Rust serde types returned by commands/
    lib/AppStatusContext.tsx       in-memory "what's currently selected"
                                 (active model/profile/backend) for the status
                                 bar and Run workspace - never persisted
    i18n/                          English/Arabic strings + LTR/RTL context
    pages/                          one file per nav destination (Overview,
                                 Hardware, Models, Optimize, Run, Profiles,
                                 Health, Settings) plus Onboarding
    styles/                         tokens.css (design tokens) + global.css
```

### Why a lib/bin split instead of duplicating engine code in the desktop crate

The Stage 4 mandate is explicit: preserve the Rust core as the single
source of truth, never re-implement hardware/model/tuning/library logic
in TypeScript (or in a second Rust copy). Splitting `src/main.rs`'s
`mod` declarations into a `src/lib.rs` the CLI binary now depends on was
a purely organizational change (zero logic changes - verified by an
unchanged 299/299 test count immediately after the split) that lets
`desktop/src-tauri` depend on the exact same `brute` crate the CLI uses.

### A few pieces of CLI-only glue were promoted into the library, not duplicated

Three small pieces of orchestration glue that used to live as private
functions in `src/main.rs` are now `pub` functions in the core library,
specifically so the desktop backend can call the identical code instead
of re-implementing it:

- `tuning::runner::run_candidate_repetition` - maps one `llama-bench`
  attempt onto a `RepetitionSample`. Used by both `brute tune run` and
  the desktop `tune_run` command.
- `backends::determine_verified_gpu_backend` - decides which GPU backend
  (if any) tuning may generate offload candidates for, based on a real
  verification run, not detection alone. Same caller list.
- `profile::build_profile_for_machine` - builds a `HardwareCapabilityProfile`
  with a real `calibration_record_count` scoped to the current machine.
  Used by the CLI's `build_profile_for_cli` and the desktop `hardware_profile`/
  `catalog.rs` commands.

### Why the desktop backend has almost no state of its own

`state::AppState` holds exactly two `Mutex<Option<Arc<AtomicBool>>>`
fields - a cancellation flag for whichever tuning run or local-generation
session is currently active, mirroring the core engine's own
`TickAction`/`is_cancelled` closure vocabulary rather than inventing a
new one. Every durable fact (library index, runtime profiles,
calibration records) is read fresh from the Rust core's own local state
(`%LOCALAPPDATA%\BruteRuntime\`) on every command call - the same
"compute live, never cache a second source of truth" principle Stage 3's
`library::associations` already established (see "Why runtime-profile/
calibration associations are computed live" above).

### Why a genuinely new streaming primitive was added, not a fake one

The Run workspace (spec section 13) needs real, incremental token
delivery so Arabic and English text appear as they're generated, not
replayed after the process exits. Rather than buffer the full output and
chunk it artificially after the fact (which would be indistinguishable
from a real stream in the UI but would violate "never animate fake
measurements"), `runtime::process::run_streaming` and
`runtime::llama_cpp::run_cli_streaming` were added as genuinely new,
tested engine primitives (parallel to, not replacing, `run`/
`run_cli_once`) that deliver 4096-byte stdout chunks to a caller-supplied
closure as they're read. The desktop `local_run_generate` command then
buffers across chunk boundaries (`commands/run.rs::drain_utf8_prefix`,
unit-tested against a real split multi-byte Arabic character) before
emitting text to the frontend, since llama-cli's raw byte reads have no
regard for UTF-8 character boundaries.

### Why the frontend has almost no client-side business logic

`lib/api.ts` is the only module allowed to call `invoke` - every page
imports typed functions from it rather than calling Tauri directly, so
the full command surface is auditable in one file. Pages hold only
presentation state (what's selected, what's being typed, live output
buffers); every number, badge, and classification shown to the user
comes directly from a command's JSON response, never recomputed or
estimated in TypeScript. See `docs/security-model.md` for the Tauri IPC
boundary this depends on.
