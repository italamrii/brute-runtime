# CLI reference

This is the detailed command-line reference for the `brute` engine
binary — the same engine the desktop app is built on (see
[`architecture.md`](architecture.md)). If you just want to run the
Windows desktop app, you don't need any of this — see the root
[`README.md`](../README.md) instead.

## Requirements (CLI / building from source)

- Windows 10/11, x86_64.
- Rust (this repo was built and tested with `rustc 1.98.0-nightly`; any
  recent stable toolchain should also work — `rustup default stable` if
  you don't already have one).
- PowerShell 5.1+ (built in on Windows) — only needed to run
  `scripts/fetch-llama-cpp.ps1`.
- **Not required** for the CLI as built: CMake, a CUDA toolkit, or the
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
`security-model.md` for the full trust chain.

> First run of a freshly downloaded `.exe` on Windows can be slow (Windows
> Defender/SmartScreen scanning it for the first time) — pass a generous
> `--timeout-secs` on your first benchmark run against a newly fetched
> binary. See `known-limitations.md`.

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

## Hardware intelligence & model fit commands

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

## Runtime auto-tuning & backend verification commands

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
committed to this repo, never uploaded anywhere. See `privacy-model.md`.

## Trusted local model library commands

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
never uploaded anywhere. See `library-privacy.md`.

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

See `measurement-methodology.md` for the full field-by-field breakdown
and `known-limitations.md` for real gaps found while building and
verifying this on real hardware.

## What the CLI intentionally does NOT do

- No universal "AI capability score" — see `measurement-methodology.md`.
- Does not build llama.cpp from source, does not vendor it, does not
  download or redistribute model files.
- Does not execute anything from inside a GGUF file or an adjacent file.
- Does not scrape, crawl, or auto-update the model catalog from the
  internet — it's a local, hand-curated fixture file.
- Fully local and offline-first: no telemetry, no analytics, no country/
  locale detection, no network code anywhere in the `brute` binary itself
  (confirmed by dependency and source audit — see
  `stage-1-verification.md` §10).
- CUDA/Vulkan are *detected*; only the CPU backend was exercised live in
  this environment (no CUDA toolkit was installed here) — see
  `known-limitations.md`.
- Does not scan a whole disk or scan automatically/in the background —
  every library scan/import names an explicit directory or file.
- Does not copy, move, rename, or delete any model file you manage
  yourself — `library forget` removes tracking metadata only.
- Does not infer "official" model status from a filename, publisher
  metadata, or the folder a file was found in — see
  `trust-and-provenance.md`.
