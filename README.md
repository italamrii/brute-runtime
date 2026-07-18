# BRUTE Runtime — Stage 0 + Stage 1

A Windows-first local-AI optimization system, still CLI-only — no
desktop UI yet. **Stage 0** proved the hard technical parts work with
real measurements (hardware inspection, GGUF parsing, llama.cpp
benchmarking). **Stage 1** builds a truthful hardware-intelligence and
model-fit engine on top: given a small local catalog of model builds, it
estimates whether each one will fit your machine, ranks them by task and
priority, and explains why — all before you download anything. See
[`docs/stage-1-hardware-intelligence.md`](docs/stage-1-hardware-intelligence.md)
for the Stage 1 overview.

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

## What Stage 0/1 intentionally do NOT do

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
