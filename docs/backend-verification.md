# Backend capability verification

Stage 1's `profile.backends.{cuda,vulkan}` answers "does a driver/adapter
that claims to support this backend exist?" — detection only. Stage 2's
`backends::verify_backend` answers a stronger, different question: "does
this backend actually launch a real llama.cpp process, load a real model,
and produce a real benchmark result?" The two must never be conflated —
`BackendStatus` is its own vocabulary, deliberately not reusing Stage 0's
`hardware::Confidence` or Stage 1's `provenance::Provenance`.

## The verification pipeline

`backends::verify_backend(backend, binary_dir, model, profile,
allow_unverified_binary, timeout)` runs through these gates in order,
stopping and returning honestly at the first one that fails:

1. **Detected?** (`detected()`) — reads `profile.backends.*`. CPU is
   always `Some(true)`. `None` (detection itself inconclusive) → status
   `Unknown`. `Some(false)` → status `Unavailable`. Neither ever proceeds
   further.
2. **Binary directory given?** No `binary_dir` → status `DetectedOnly` —
   this is the honest stopping point when a caller wants pure detection
   without a llama.cpp binary on hand.
3. **Binary present and hash-verified?** (`llama_cpp::verify_llama_binary`
   for both `llama-cli.exe` and `llama-bench.exe`) — missing file or hash
   mismatch → status `BinaryMissing`.
4. **Process actually launches?** (`llama_cpp::run_cli_version`, a
   `--version` smoke test) — failure → status `LaunchFailed`.
5. **Model loads and a tiny benchmark completes?**
   (`llama_cpp::run_bench` with small fixed values: 1 GPU layer, 16
   prompt tokens, 8 generation tokens, 1 repetition — proving the path
   works, not measuring real throughput) — process error → status
   `ModelLoadFailed`.
6. **No silent fallback?** (`check_no_silent_fallback`, below) — only
   this final check can produce status `Verified`.

`BackendVerification`'s JSON shape matches the spec exactly:

```json
{
  "backend": "cuda",
  "detected": true,
  "binary_available": true,
  "launch_verified": true,
  "model_load_verified": true,
  "benchmark_verified": true,
  "status": "verified",
  "failure_reason": null,
  "verification_tokens_per_second": 76.99,
  "reported_gpu_info": "NVIDIA GeForce RTX 5050 Laptop GPU"
}
```

## The anti-fabrication check — and a real bug it caught

`check_no_silent_fallback` is the check that actually earns
`Verified`/`BenchmarkFailed` for a GPU backend. It looks at
`llama-bench`'s own JSON output (`BenchRow.gpu_info`), not the exit code
alone — a process can launch cleanly, "load" the model on CPU, and exit
0 even when it silently ignored `-ngl` because the binary has no GPU
support compiled in.

**This is not a hypothetical concern.** During Stage 2 live testing on
this machine, `brute backends verify --backend cuda` was pointed at the
repo's *CPU-only* pinned b10064 binary (`.tools\llama.cpp\b10064\cpu`) —
and the first implementation reported `status: "verified"` with real
throughput numbers. The bug: the check originally accepted
`gpu_info` non-empty **OR** `n_gpu_layers > 0` as evidence. The CPU-only
build echoes the requested `-ngl 1` flag straight back into its JSON
output as `n_gpu_layers: 1` regardless of whether it has any GPU code
path at all — it's an echo of the input, not confirmed usage. Meanwhile
`gpu_info` was correctly empty.

Fixed by trusting **only** `gpu_info` non-empty. `gpu_info` is populated
by llama.cpp exclusively when a device backend actually initialized —
there is no equivalent echo-back risk for a string field the tool has to
construct from real device enumeration. Verified before/after with the
real binary:

```
# Before the fix
"benchmark_verified": true, "status": "verified", "reported_gpu_info": ""

# After the fix
"benchmark_verified": false, "status": "benchmark_failed",
"failure_reason": "Cuda verification ran and exited cleanly, but llama-bench
  reported no GPU device in use (gpu_info empty) - refusing to report GPU
  success on a possible silent CPU fallback (n_gpu_layers alone is not
  trusted as evidence: it can echo the requested flag even on a binary
  with no Cuda support compiled in)"
```

Regression test:
`backends::tests::gpu_backend_with_echoed_n_gpu_layers_but_empty_gpu_info_is_not_verified`,
which reproduces exactly this JSON shape (`gpu_info: Some("")`,
`n_gpu_layers: Some(1)`) and asserts it is never `Verified`.

## How the rest of Stage 2 uses this

`tuning::candidates::generate_plan` only generates GPU-offload candidates
when handed `Some(backend)` for a backend that actually passed this
pipeline (`BackendStatus::Verified`) — never from `profile.backends.*`
detection alone. `main.rs`'s `determine_verified_gpu_backend` performs
this check automatically before every `brute tune run` unless
`--backend cpu` is explicitly given.

## CLI

```
brute backends verify --model <path> --llama-bin <dir> [--backend cpu|cuda|vulkan] [--allow-unverified-binary] [--timeout-secs 60] [--json]
```

Without `--backend`, verifies CPU, CUDA, and Vulkan all three. Exit code
is always 0 — this is an informational/diagnostic command; a genuinely
unavailable backend is not a `brute` failure, it's an honest finding.
