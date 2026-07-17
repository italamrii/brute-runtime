# Measurement methodology

## Confidence levels

Every hardware and benchmark value is wrapped in a `HardwareField<T>` with a
`confidence` and a `source` string. The four levels:

| Level | Meaning | Examples in this codebase |
|---|---|---|
| `measured` | Read directly from an OS API or CPU instruction, no interpretation | `GlobalMemoryStatusEx`, `GetDiskFreeSpaceExW`, `cpuid`, DXGI adapter enumeration, `GetProcessMemoryInfo` |
| `detected` | From an external tool or indirect signal we trust but don't control | `nvidia-smi` output, `vulkan-1.dll` presence, `PROCESSOR_ARCHITECTURE` env var |
| `inferred` | Derived by heuristic from other facts, not observed directly | "CUDA available" from driver presence, without running a CUDA kernel; "time to first token" approximated from prompt-eval time |
| `unavailable` | We tried and could not determine the value | Any API call that failed, any field never defaulted to a plausible-looking value |

A value is **never** silently promoted to a higher confidence than its
actual source justifies, and an `unavailable` field is never populated with
a guess.

## Hardware fields

| Field | Source | Confidence |
|---|---|---|
| OS product name / display version / build | Registry `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion` | measured |
| Process architecture | `std::env::consts::ARCH` (compile-time) | measured |
| Native architecture | `PROCESSOR_ARCHITEW6432` / `PROCESSOR_ARCHITECTURE` env vars | detected |
| CPU vendor/brand | `sysinfo::Cpu` | measured |
| Physical/logical cores | `sysinfo::System::physical_core_count` / `std::thread::available_parallelism` | measured |
| Instruction sets (AVX2, AVX512*, FMA, ...) | `cpuid` via `raw-cpuid` | measured |
| Total/available RAM | `GlobalMemoryStatusEx` | measured |
| GPU name / dedicated VRAM | DXGI `IDXGIAdapter1::GetDesc1` | measured |
| GPU driver version (NVIDIA only) | `nvidia-smi --query-gpu` | detected |
| CUDA available | NVIDIA adapter present (DXGI) + working `nvidia-smi` | inferred (driver presence, not kernel execution) |
| Vulkan available | `vulkaninfo --summary` exit code if present, else `vulkan-1.dll` presence | measured / detected (tiered - see `hardware/gpu.rs`) |
| Storage free/total | `GetDiskFreeSpaceExW` | measured |

**Why native architecture is env-var based, not `GetNativeSystemInfo`:**
`GetNativeSystemInfo` requires reading a field out of a C union
(`SYSTEM_INFO.Anonymous.Anonymous.wProcessorArchitecture`) whose exact
binding shape in the `windows` crate version pinned here was not worth the
risk of a subtle unsafe-field-path bug for Stage 0. `PROCESSOR_ARCHITEW6432`
(set by the OS loader specifically to expose the true native architecture
to a 32-bit process running under WOW64) and `PROCESSOR_ARCHITECTURE` are
the same technique Windows batch scripts have used for this for years, and
are graded `detected` (not `measured`) to reflect that they're an
environment-variable signal rather than a direct struct read.

## GGUF parsing

`models/gguf.rs` reads only:
1. The 24-byte header (magic, version, tensor_count, kv_count).
2. The metadata key/value table - every value is parsed (to keep the byte
   stream position correct) but only an allow-listed set of keys
   (`general.architecture`, `general.name`, `general.file_type`, etc.) are
   retained in memory.
3. The tensor-info table (name, dimensions, ggml type, data offset) - used
   to compute parameter count (product of dimensions, summed across
   tensors) and to cross-check the implied tensor-data size against the
   actual file size.

The tensor data blob itself is **never read**.

**Parameter count** is computed directly from the tensor shape table (a sum
of per-tensor element counts), not read from a metadata field (most GGUF
files don't carry one) - this is a real computation over measured header
data, not an estimate.

**Truncation/corruption detection**: every length-prefixed field (a KV
string, an array) is checked against both a fixed sanity cap and the
actual remaining file size before being acted on. After the tensor-info
table is parsed, the parser computes the minimum file size the declared
tensor offsets and sizes imply and compares it against the real file size
via `std::fs::metadata` - if the file is shorter than what the header
promises, `GgufError::Truncated` is returned. This size check covers the
common tensor formats (F32, F16, BF16, Q4_0/Q4_1/Q5_0/Q5_1/Q8_0/Q8_1, and
the K-quants Q2_K through Q8_K) - see `tensor_byte_size()` for the exact
block-size table. For an unrecognized `ggml_type` the check is skipped
rather than guessed at (`size_consistency_checked` in the JSON report tells
you which happened).

## Benchmark measurements

Two llama.cpp tools are used, each for what it's actually designed to
measure:

- **`llama-bench -o json`**: prompt-processing and generation throughput
  (tokens/sec), with `-r` repeated samples per test producing `avg_ts`,
  `stddev_ts`, and the full `samples_ts` array. This is the primary,
  highest-confidence throughput number - it comes from llama.cpp's own
  purpose-built benchmarking harness, not a wall-clock measurement around a
  single run. Graded `measured`.
- **`llama-cli`** (single run): its own `llama_perf_context_print` stderr
  log gives model load time directly (`measured`... graded `detected` here
  because it's parsed out of the tool's text log rather than an API return
  value). "Time to first token" is **approximated** as the prompt-eval
  time from that same log line and graded `inferred` - it is not an
  independently captured per-token timestamp, and the report says so.

**Memory**: peak process working set comes from `GetProcessMemoryInfo`
(`PeakWorkingSetSize`), queried on the child process handle right after it
exits - the OS's own peak accounting, not a sampled maximum. System RAM
before/during/after is `GlobalMemoryStatusEx`, sampled at the start, on
every poll tick of the process-wait loop (~50ms interval), and after exit;
"min during" is the minimum available-RAM sample observed, i.e. peak
system-wide usage during the run.

**Repeated-run variance**: `benchmark/scoring.rs` classifies stability from
the coefficient of variation (stddev/mean) of `llama-bench`'s own repeated
samples: ≤5% = Stable, ≤15% = Marginal, above = Unstable. A run that
timed out or exited non-zero is always Unstable regardless of variance.

## What Stage 0 does not claim

No "AI capability score." No prediction for a configuration that was never
actually run. `report::build_recommendation` only ever restates a
completed benchmark's own measured range (min/max of the repeated samples)
- see the `basis` field it always includes.
