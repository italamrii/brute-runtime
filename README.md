# BRUTE Runtime — Stage 0 + Stage 1 + Stage 2 + Stage 3

A Windows-first local-AI optimization system, still CLI-only — no
desktop UI yet. **Stage 0** proved the hard technical parts work with
real measurements (hardware inspection, GGUF parsing, llama.cpp
benchmarking). **Stage 1** builds a truthful hardware-intelligence and
model-fit engine on top: given a small local catalog of model builds, it
estimates whether each one will fit your machine, ranks them by task and
priority, and explains why — all before you download anything. **Stage
2** turns BRUTE from "predicts what fits" into a real local runtime
optimizer: it verifies backends actually work (not just that a driver
was detected), safely benchmarks a bounded set of candidate runtime
configurations for a model you already have, and saves the proven-best
one as a reusable local profile. **Stage 3** turns BRUTE into a trusted
local model library: it safely discovers, imports, verifies, and tracks
the GGUF models you already have on disk — content-hash identity, no
copying/moving/executing by default, honest trust states instead of
inferred "official" status, and health/duplicate/storage reporting that
never deletes anything on its own. See
[`docs/stage-1-hardware-intelligence.md`](docs/stage-1-hardware-intelligence.md),
[`docs/stage-2-runtime-auto-tuning.md`](docs/stage-2-runtime-auto-tuning.md),
and
[`docs/stage-3-trusted-local-library.md`](docs/stage-3-trusted-local-library.md)
for the full overviews.

## What Stage 0 does

- Inspects real hardware: OS version, CPU (vendor/brand/cores/instruction
  sets), RAM, GPU(s) (name/VRAM via DXGI, NVIDIA driver via `nvidia-smi`),
  CUDA/Vulkan availability, storage free space.
- Parses GGUF model metadata (architecture, parameter count, quantization,
  file size, SHA-256) by streaming only the header/metadata/tensor-info
  tables — never loading tensor data into memory.
- Detects truncated or corrupted GGUF files.
- Launches llama.cpp's official prebuilt `llama-bench`/`llama-cli`
  binaries as managed subprocesses: argv-based (no shell), timeout +
  kill, stdout/stderr capture, SHA-256 binary verification against a
  locally pinned hash.
- Runs a controlled, repeated-sample CPU benchmark (`llama-bench -o json`)
  and reports throughput with real variance (stddev, coefficient of
  variation), not a single noisy number.
- Derives a runtime recommendation strictly from what was actually
  measured in that run.
- Produces both a human-readable terminal report and a machine-readable
  JSON report, with every field tagged `measured` / `detected` /
  `inferred` / `unavailable`.

## What Stage 1 adds

- **Hardware Capability Profile** (`brute profile create`): a normalized,
  UI-stable snapshot of your machine (CPU, RAM, GPUs, backends, power
  state) with a non-identifying `machine_id` and a confidence roll-up.
- **Model Build Catalog** (`brute catalog list`/`show`): a small, local,
  explicitly-labeled curated list of exact model builds — BRUTE never
  downloads, hosts, or scrapes models; catalog entries are metadata plus
  an official source link.
- **Model Requirement Estimator**: real-formula memory/disk range
  estimates (not "file size = RAM needed"), always with assumptions and
  missing-inputs listed.
- **Fit Classification** (`brute fit`): six honest states — Excellent /
  Good / Constrained / Experimental / Not recommended / Unknown — with
  `Unknown` never collapsed into `Not recommended`.
- **Recommendation & ranking** (`brute recommend-model`,
  `brute explain-fit`): task/priority-aware ranking with explicit,
  documented weights, a safer fallback and a stronger optional pick, and
  a two-tier (simple + technical) explanation for every recommendation.
- **Calibration** (`brute calibrations list`, `--save-calibration` on
  `benchmark`): grounds estimates in real measurements and honestly
  degrades confidence the further a build is from anything actually
  benchmarked.

See [`docs/stage-1-hardware-intelligence.md`](docs/stage-1-hardware-intelligence.md)
for commands and real output, and the methodology docs linked at the
bottom of this file for exactly how every number is computed.

## What Stage 2 adds

- **Backend capability verification** (`brute backends verify`): proves
  CPU/CUDA/Vulkan actually launch a real process, load a real model, and
  complete a real tiny benchmark — never claims a GPU backend works
  merely because a driver was detected, and never silently falls back to
  CPU while reporting GPU success.
- **Safe, bounded runtime auto-tuning** (`brute tune run`): generates a
  small, deterministic, pruned candidate list (thread count, GPU offload,
  context/batch size), safely benchmarks each one under resource guards
  (RAM/disk checks, timeouts, cooldowns, cooperative cancellation), and
  classifies each candidate's stability across repeated independent runs
  — never a brute-force cartesian search.
- **Configuration ranking**: picks a winner for your chosen priority
  (balanced by default) using a safety-first tiered comparator — a
  marginally faster but unstable configuration never outranks a proven
  stable one under the default priority.
- **Local runtime profiles** (`brute profiles list`/`show`/`verify`/
  `export`): saves the winning configuration locally, automatically
  invalidated when the model, binaries, or machine change, and re-verified
  (not just re-applied) before ever being treated as usable.

See [`docs/stage-2-runtime-auto-tuning.md`](docs/stage-2-runtime-auto-tuning.md)
for commands and real output, and the methodology docs linked at the
bottom of this file.

## What Stage 3 adds

- **Trusted local model library** (`brute library scan`/`import`/`list`/
  `show`): tracks the GGUF models you already have by content hash, not
  path — renamed files are recognized as the same model, same-name
  different-content files stay separate, and nothing is copied, moved,
  or executed by default.
- **Honest structural + trust verification** (`brute library verify`):
  never collapses "is this a valid GGUF," "does the hash match," and
  "is this an official/trusted source" into one boolean — a structurally
  valid file is not automatically trusted.
- **Duplicate detection and storage analysis** (`brute library
  duplicates`/`storage`): groups identical artifacts by hash and reports
  potential reclaimable space — never presented as safe to delete until
  you confirm it yourself.
- **Move/rename recovery** (`brute library locate`): only rebinds a
  tracked model's path once the candidate file's hash is confirmed
  identical to the original; Stage 2 runtime profiles and Stage 1
  calibration records stay valid across the move automatically, since
  they're associated by content hash too.
- **Library-wide health audit** (`brute library audit`): missing/
  modified/corrupt entries, duplicates, stale profiles/calibrations, and
  privacy concerns in your own alias/notes text — all in one report,
  with suggested (never automatic) next steps.
- **Safe lifecycle management**: `forget` removes library metadata only
  and never touches your file; quarantine/unquarantine always requires a
  real passing re-verification, never a bare flag flip.

See [`docs/stage-3-trusted-local-library.md`](docs/stage-3-trusted-local-library.md)
for commands and real output.

## What Stage 0/1/2/3 intentionally do NOT do

- No desktop UI. CLI only.
- No universal "AI capability score" — see `docs/measurement-methodology.md`.
- Does not build llama.cpp from source, does not vendor it, does not
  download or redistribute model files.
- Does not execute anything from inside a GGUF file or an adjacent file.
- Does not scrape, crawl, or auto-update the model catalog from the
  internet — it's a local, hand-curated fixture file.
- Fully local and offline-first: no telemetry, no analytics, no country/
  locale detection, no network code anywhere in the `brute` binary itself
  (confirmed by dependency and source audit — see
  `docs/stage-1-verification.md` §10).
- CUDA/Vulkan are *detected*; only the CPU backend was exercised live in
  this environment (no CUDA toolkit was installed here) — see
  `docs/known-limitations.md`.
- Does not scan a whole disk or scan automatically/in the background —
  every library scan/import names an explicit directory or file.
- Does not copy, move, rename, or delete any model file you manage
  yourself — `library forget` removes tracking metadata only.
- Does not infer "official" model status from a filename, publisher
  metadata, or the folder a file was found in — see
  `docs/trust-and-provenance.md`.

## Requirements

- Windows 10/11, x86_64.
- Rust (this repo was built and tested with `rustc 1.98.0-nightly`; any
  recent stable toolchain should also work — `rustup default stable` if
  you don't already have one).
- PowerShell 5.1+ (built in on Windows) — only needed to run
  `scripts/fetch-llama-cpp.ps1`.
- **Not required** for Stage 0 as built: CMake, a CUDA toolkit, or the
  Vulkan SDK. llama.cpp is used as a prebuilt official binary, not built
  from source. (Visual Studio Build Tools with the MSVC C++ toolset is
  used to compile this Rust project's Win32 API bindings — if you have
  Rust installed via the standard Windows installer this is almost
  certainly already satisfied.)

## Build

```powershell
cargo build --release
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

## Getting a llama.cpp binary

`brute` never downloads anything itself. Run the fetch script once:

```powershell
.\scripts\fetch-llama-cpp.ps1              # CPU backend (default)
.\scripts\fetch-llama-cpp.ps1 -Backend vulkan
```

This downloads the exact release pinned in
`scripts/llama-cpp-manifest.json`, verifies its SHA-256 against the value
recorded there (itself read from GitHub's own release-asset digest, not
computed locally), extracts it to a gitignored `.tools\llama.cpp\<tag>\<backend>\`,
and writes a `<binary>.sha256` pin file next to each extracted `.exe` —
that pin is what `brute` checks before every subsequent launch. See
`docs/security-model.md` for the full trust chain.

> First run of a freshly downloaded `.exe` on Windows can be slow (Windows
> Defender/SmartScreen scanning it for the first time) — pass a generous
> `--timeout-secs` on your first benchmark run against a newly fetched
> binary. See `docs/known-limitations.md`.

## Importing a GGUF model

Just point commands at a `.gguf` file already on disk — `brute` never
downloads or redistributes model files. Get one from the model's official
source (e.g. Hugging Face) yourself, then:

```powershell
cargo run --release -- model inspect --path "C:\Models\your-model.gguf"
```

## Commands

```powershell
# Hardware capability report (human-readable)
cargo run --release -- inspect

# Same, machine-readable
cargo run --release -- inspect --json

# Validate + parse a GGUF file's metadata
cargo run --release -- model inspect --path "C:\Models\your-model.gguf"

# Verify a fetched llama.cpp binary is present, pinned, and launchable
cargo run --release -- doctor --llama-bin ".tools\llama.cpp\b10064\cpu"

# Run a controlled benchmark
cargo run --release -- benchmark `
  --model "C:\Models\your-model.gguf" `
  --llama-bin ".tools\llama.cpp\b10064\cpu" `
  --backend cpu

# Benchmark + derive a recommended runtime profile
cargo run --release -- recommend `
  --model "C:\Models\your-model.gguf" `
  --llama-bin ".tools\llama.cpp\b10064\cpu" `
  --backend cpu

# Full JSON report (hardware + model + benchmark + recommendation)
cargo run --release -- report `
  --output .\brute-report.json `
  --model "C:\Models\your-model.gguf" `
  --llama-bin ".tools\llama.cpp\b10064\cpu"
```

Run any command with `--help` for the full flag list (thread count,
context/batch size, prompt/gen token counts, repetitions, timeout,
`--allow-unverified-binary`).

## Stage 1 commands (hardware intelligence & model fit)

```powershell
# Normalized hardware profile
cargo run --release -- profile create --storage-path "C:\Models"

# The curated local model-build catalog
cargo run --release -- catalog list
cargo run --release -- catalog show qwen2.5-0.5b-instruct-q4_k_m

# Fit classification for one build or every build
cargo run --release -- fit --model qwen2.5-0.5b-instruct-q4_k_m --storage-path "C:\Models"
cargo run --release -- fit --all --storage-path "C:\Models"

# Ranked recommendation for a task/priority
cargo run --release -- recommend-model --priority balanced --storage-path "C:\Models"
cargo run --release -- recommend-model --task coding --priority coding --storage-path "C:\Models"

# Simple + technical explanation for one build
cargo run --release -- explain-fit --model qwen2.5-0.5b-instruct-q4_k_m --storage-path "C:\Models"

# What real benchmark data backs these estimates
cargo run --release -- calibrations list
```

`--catalog` defaults to `data/catalog/dev-catalog.json`, `--calibration`
to `data/calibration/seed-calibration.json` — both plain, reviewable JSON
files. Priorities: `fastest`, `balanced`, `highest-quality`,
`lowest-memory`, `longest-context`, `coding`, `arabic-general-chat`,
`privacy-offline`. Task categories: `general-chat`, `coding`,
`arabic-chat`, `reasoning`.

To grow the calibration store with a real measurement from your own
machine, add `--save-calibration <path>` to a `brute benchmark` run.

## Stage 2 commands (runtime auto-tuning & backend verification)

```powershell
# Verify CPU/CUDA/Vulkan actually work end to end (not just "detected")
cargo run --release -- backends verify `
  --model "C:\Models\your-model.gguf" --llama-bin ".tools\llama.cpp\b10064\cpu"

# Show the bounded candidate plan without launching anything
cargo run --release -- tune run `
  --model "C:\Models\your-model.gguf" --llama-bin ".tools\llama.cpp\b10064\cpu" --dry-run

# Actually run it, save the winning configuration as a local profile
cargo run --release -- tune run `
  --model "C:\Models\your-model.gguf" --llama-bin ".tools\llama.cpp\b10064\cpu" `
  --priority balanced --save-profile

# Check progress of a running (or the most recent) tuning run, from another terminal
cargo run --release -- tune status

# Request cancellation of a running tuning run
cargo run --release -- tune cancel

# Saved local runtime profiles
cargo run --release -- profiles list
cargo run --release -- profiles show <profile-id>
cargo run --release -- profiles verify <profile-id> --model "C:\Models\your-model.gguf" --llama-bin ".tools\llama.cpp\b10064\cpu"
cargo run --release -- profiles export <profile-id> --output profile.json
```

Priorities: `balanced` (default, safety-first — never lets a faster
unstable configuration outrank a proven stable one), `fastest-generation`,
`fastest-prompt-processing`, `lowest-memory`, `longest-context`,
`maximum-stability`, `laptop-friendly`. Saved profiles and tuning
progress/cancel state live under `%LOCALAPPDATA%\BruteRuntime\` — never
committed to this repo, never uploaded anywhere. See
`docs/privacy-model.md`.

## Stage 3 commands (trusted local model library)

```powershell
# Discover GGUF files in a directory - never imports anything
cargo run --release -- library scan "C:\Models"
cargo run --release -- library scan "C:\Models" --recursive

# Explicitly import one model - reads/hashes/parses only, never modifies it
cargo run --release -- library import "C:\Models\your-model.gguf" --alias "My model"

# Scan + import every GGUF candidate in a directory
cargo run --release -- library import-directory "C:\Models" --recursive

# List / inspect tracked models
cargo run --release -- library list
cargo run --release -- library show <library-id>

# Full re-verification (recomputes the hash) - one entry or all
cargo run --release -- library verify <library-id>
cargo run --release -- library verify --all

# Cheap size/mtime-only refresh (no hashing)
cargo run --release -- library refresh --all

# Library-wide health report, duplicate groups, storage usage
cargo run --release -- library audit
cargo run --release -- library duplicates
cargo run --release -- library storage

# Recover a moved/renamed model (only rebinds on an exact hash match)
cargo run --release -- library locate <library-id> "C:\NewLocation\model.gguf"

# Local metadata only - never touches the file
cargo run --release -- library alias <library-id> "Display name"
cargo run --release -- library note <library-id> "Free-text note"

# Remove tracking metadata - the file is NOT deleted
cargo run --release -- library forget <library-id>

# Hold a suspicious entry back from use; lifting it re-verifies first
cargo run --release -- library quarantine <library-id> --reason "hash mismatch"
cargo run --release -- library unquarantine <library-id>

# Sanitized export - no local paths, no machine identifiers
cargo run --release -- library export --output library.json
```

Library state (`index.json`) lives under
`%LOCALAPPDATA%\BruteRuntime\library\` — never committed to this repo,
never uploaded anywhere. See `docs/library-privacy.md`.

## Interpreting confidence and unavailable values

Every hardware and benchmark field looks like this in JSON:
```json
{ "value": 16873545728, "confidence": "measured", "source": "Win32 GlobalMemoryStatusEx.ullTotalPhys" }
```
and in the terminal report:
```
Total RAM (bytes): 16873545728  [measured, source: Win32 GlobalMemoryStatusEx.ullTotalPhys]
```

- **measured**: a direct OS API/CPU instruction result. Trust it as-is.
- **detected**: from an external tool or indirect signal (e.g.
  `nvidia-smi`, a DLL's presence). Reliable but one step removed from a
  direct API call.
- **inferred**: a heuristic derived from other facts, not directly
  observed (e.g. "CUDA available" from driver presence without running a
  CUDA kernel). Treat as a reasonable guess, not a guarantee.
- **unavailable**: `brute` tried and could not determine the value. The
  `source` string explains why. Never populated with a plausible-looking
  default — if you see `unavailable`, that means exactly that, not zero.

See `docs/measurement-methodology.md` for the full field-by-field
breakdown and `docs/known-limitations.md` for real gaps found while
building and verifying this on real hardware.

## Documentation

- [`docs/architecture.md`](docs/architecture.md) — module layout, data flow, design rationale
- [`docs/security-model.md`](docs/security-model.md) — threat model, binary verification, supply-chain trust, privacy
- [`docs/measurement-methodology.md`](docs/measurement-methodology.md) — exactly how every Stage 0 number is obtained
- [`docs/known-limitations.md`](docs/known-limitations.md) — real gaps, not hedging
- [`docs/stage-0-verification.md`](docs/stage-0-verification.md) — Stage 0: what was actually run and observed
- [`docs/stage-1-hardware-intelligence.md`](docs/stage-1-hardware-intelligence.md) — Stage 1 overview and commands
- [`docs/model-catalog-schema.md`](docs/model-catalog-schema.md) — catalog fields, validation, the dev fixture data
- [`docs/model-memory-estimation.md`](docs/model-memory-estimation.md) — the memory/disk estimation formulas
- [`docs/model-fit-classification.md`](docs/model-fit-classification.md) — the six fit states and their exact rules
- [`docs/recommendation-methodology.md`](docs/recommendation-methodology.md) — ranking weights and explainability
- [`docs/calibration-methodology.md`](docs/calibration-methodology.md) — how real benchmarks ground the estimates
- [`docs/stage-1-verification.md`](docs/stage-1-verification.md) — Stage 1: what was actually run and observed, including two real bugs found and fixed live
- [`docs/stage-2-runtime-auto-tuning.md`](docs/stage-2-runtime-auto-tuning.md) — Stage 2 overview and commands
- [`docs/backend-verification.md`](docs/backend-verification.md) — the verification pipeline and a real silent-fallback bug it caught
- [`docs/tuning-search-space.md`](docs/tuning-search-space.md) — bounded candidate generation and pruning rules
- [`docs/stability-classification.md`](docs/stability-classification.md) — the 5-state cross-repetition stability formula
- [`docs/tuning-ranking-methodology.md`](docs/tuning-ranking-methodology.md) — the tiered ranking comparator and why not a weighted sum
- [`docs/cancellation-and-process-safety.md`](docs/cancellation-and-process-safety.md) — timeouts, retries, resource guards, cooperative cancellation
- [`docs/runtime-profile-schema.md`](docs/runtime-profile-schema.md) — saved profile fields, invalidation, sanity checking
- [`docs/privacy-model.md`](docs/privacy-model.md) — the mandatory no-telemetry/no-cloud list, `machine_id` vs `local_instance_id`
- [`docs/stage-2-verification.md`](docs/stage-2-verification.md) — Stage 2: what was actually run and observed, including a real bug found and fixed live
- [`docs/stage-3-trusted-local-library.md`](docs/stage-3-trusted-local-library.md) — Stage 3 overview and commands
- [`docs/local-library-schema.md`](docs/local-library-schema.md) — the storage-format decision, schema, atomicity, migration
- [`docs/model-identity.md`](docs/model-identity.md) — why content hash, not path, is identity
- [`docs/model-import-and-verification.md`](docs/model-import-and-verification.md) — the import pipeline and the `GgufVerification` structure
- [`docs/trust-and-provenance.md`](docs/trust-and-provenance.md) — the trust states and why some can never fire with today's catalog data
- [`docs/duplicate-detection.md`](docs/duplicate-detection.md) — SHA-256-based grouping, never filename similarity
- [`docs/model-file-change-detection.md`](docs/model-file-change-detection.md) — cheap vs full verification, and the `locate` recovery workflow
- [`docs/runtime-profile-association.md`](docs/runtime-profile-association.md) — why associations are computed live, never persisted, and a real bug that taught why
- [`docs/storage-management.md`](docs/storage-management.md) — usage totals, duplicates as a potential (never safe) reclaim estimate
- [`docs/quarantine-and-recovery.md`](docs/quarantine-and-recovery.md) — forget vs remove-managed vs (never) delete; quarantine that can't be bypassed
- [`docs/library-security.md`](docs/library-security.md) — path traversal, scan loop prevention, untrusted metadata
- [`docs/library-privacy.md`](docs/library-privacy.md) — what's redacted on export and why nothing more needs to be
- [`docs/stage-3-verification.md`](docs/stage-3-verification.md) — Stage 3: what was actually run and observed, including a real bug found and fixed live
