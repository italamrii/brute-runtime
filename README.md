# BRUTE Runtime — Stage 0

A Windows-first local-AI optimization system. This repository is **Stage
0**: a technical feasibility spike, not the product. It proves the hard
parts work with real measurements before any UI is built.

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

## What Stage 0 intentionally does NOT do

- No desktop UI. CLI only.
- No universal "AI capability score" — see `docs/measurement-methodology.md`.
- Does not build llama.cpp from source, does not vendor it, does not
  download or redistribute model files.
- Does not execute anything from inside a GGUF file or an adjacent file.
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
- [`docs/security-model.md`](docs/security-model.md) — threat model, binary verification, supply-chain trust
- [`docs/measurement-methodology.md`](docs/measurement-methodology.md) — exactly how every number is obtained
- [`docs/known-limitations.md`](docs/known-limitations.md) — real gaps, not hedging
- [`docs/stage-0-verification.md`](docs/stage-0-verification.md) — what was actually run and observed, with real output
