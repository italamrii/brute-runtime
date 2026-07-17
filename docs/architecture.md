# Architecture — Stage 0

BRUTE Runtime Stage 0 is a single Rust binary crate (`brute`), not a
workspace. The module boundaries below are the seams a future multi-crate
split would fall along, but a workspace isn't justified yet at this scope.

```
src/
  main.rs           CLI entry point, command dispatch, exit codes
  cli/mod.rs         clap argument definitions
  errors.rs           BruteError and per-domain error enums (thiserror)
  hardware/
    mod.rs             Confidence, HardwareField<T>, HardwareReport aggregate
    cpu.rs              vendor/brand/cores (sysinfo) + ISA flags (raw-cpuid)
    memory.rs           GlobalMemoryStatusEx (Win32)
    gpu.rs              DXGI adapter enumeration + nvidia-smi + Vulkan probe
    windows.rs          OS version (registry), architecture, storage free space
  models/
    mod.rs             ModelReport aggregate, inspect_model()
    gguf.rs             streaming GGUF header/KV/tensor-info parser
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
    json.rs              serde_json (de)serialization
    terminal.rs           human-readable rendering with confidence tags
  security/
    mod.rs             re-exports
    paths.rs             canonicalize + regular-file validation
    hashing.rs            SHA-256 (streamed) + binary pin verification
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

## Extension points for later stages

- `runtime::Backend` is an enum today; a `Backend` trait with `Cpu`/`Cuda`/
  `Vulkan` implementations is the natural next step if a second inference
  engine (not llama.cpp) is ever added.
- `hardware::gpu` isolates all vendor-specific detection (DXGI, nvidia-smi,
  Vulkan loader probing) behind the single `inspect_gpu()` entry point.
- `report::CapabilityReport` is the single serialization boundary; a future
  desktop UI would consume this JSON shape rather than calling internal
  modules directly.
