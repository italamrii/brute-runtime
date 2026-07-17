# Stage 0 verification log

This is a record of what was actually run and observed while building
Stage 0, not a claim of what should theoretically work. Timestamps are
from the 2026-07-18 build session on the target machine (13th Gen Intel
Core i5-13450HX, 16 GB RAM, NVIDIA GeForce RTX 5050 Laptop GPU, Windows 11
Home build 26200).

## 1. Build

```
cargo build --release
```
Result: succeeds, zero warnings.

## 2. Automated tests

```
cargo test
```
Result: **49 passed, 0 failed.** Covers: GGUF parsing (valid/truncated/bad
magic/bad version/hostile counts), path validation (including a real
Arabic + space filename `نموذج اختبار model.gguf`), SHA-256 (known test
vectors + binary pin verify/mismatch/no-pin), process argument
construction (including a command-injection-shaped path surviving as one
literal argv element, never split/interpreted), process timeout/kill,
report JSON round-trip, and recommendation-derivation logic.

One real bug was caught and fixed during this pass: the GGUF fixture
tests originally shared a single temp directory keyed only by process ID;
since `cargo test` runs tests in parallel by default, one test's
`remove_dir_all` cleanup could delete another concurrently-running test's
fixture file. Fixed by giving every fixture its own directory
(pid + nanosecond timestamp).

## 3. Formatting and linting

```
cargo fmt --check      # clean
cargo clippy --all-targets -- -D warnings   # clean, 0 warnings
```

## 4. Fetch and verify the pinned llama.cpp binary

```
.\scripts\fetch-llama-cpp.ps1 -Backend cpu
```
Real output:
```
Downloading llama-b10064-bin-win-cpu-x64.zip (b10064, cpu)...
Verifying SHA-256 against pinned manifest value...
SHA-256 verified: c9b770b584a007a1aeea1b729e0e4724fb79a2cb136ece46be92704aaee5099e
Extracting to ...\.tools\llama.cpp\b10064\cpu...
Pinned ...\llama-cli.exe -> 9f89e2ca70026abed6ec2561ab33e5dcb080581b9af7a061b404a13f88155a90
Pinned ...\llama-bench.exe -> a869f27e08ac6c2f326eee1bd1a33c37dcfe5d75d41424fbf1a2573b09a88784
```
The downloaded zip's SHA-256 matched the value pinned in
`scripts/llama-cpp-manifest.json`, which was itself read independently
from GitHub's release-asset digest API before any code was written.

## 5. `brute doctor` (binary verification + process launch smoke test)

First invocation timed out at 15s (see
`docs/known-limitations.md` - first-run Defender/SmartScreen scan of a
freshly extracted, never-executed binary). Second invocation, same binary:
```
llama-cli --version exit code: Some(0)
timed out: false
stdout:

stderr:
version: 10064 (86d86ed43)
built with Clang 20.1.8 for Windows x86_64

llama.cpp binary at .tools\llama.cpp\b10064\cpu is verified and launchable.
```
Confirms: hash verification against the local pin, argv-based process
launch (no shell), exit code capture, and stderr capture all work against
the real binary.

## 6. `brute inspect` (real hardware)

Ran for real on this machine. Every field came back `measured` or
`detected` with a real value except driver version for non-NVIDIA
adapters (correctly `unknown`, never guessed). Full output is in this
repo's README. Notable real finding: the registry `ProductName` value
reports "Windows 10 Home" on this Windows 11 machine - a genuine OS/
registry quirk, not a `brute` bug (see `known-limitations.md`).

## 7. `brute model inspect` (synthetic fixture)

No real `.gguf` file was available on this machine during implementation.
A minimal, clearly-synthetic GGUF fixture (one fake 4-element F32 tensor,
176 bytes total, built with a throwaway PowerShell script mirroring the
Rust unit-test fixture) was used to verify the CLI command itself - path
validation, SHA-256 hashing, GGUF parsing, and report rendering all
wired correctly:
```
== Model ==
  File size: 176 bytes
  SHA-256: a5b09f2953f4a7111e2c0b61deb8bf4e858c4d3ea317da0de4c44a19506c8b67
  GGUF version: 3
  Tensor count: 1
  Architecture: llama
  Quantization: F32
  Parameter count: 4
  Tensor-data size consistency check: passed
```
A truncated copy of the same fixture was then rejected cleanly:
```
error: file is truncated: expected to read 4 bytes for scalar u32, got 0
```
(exit code 1).

## 8. `brute benchmark` against the synthetic fixture

Run against the real, verified `llama-bench`/`llama-cli` binaries. Since
the synthetic fixture has no real hyperparameters, llama.cpp itself
correctly refused to load it:
```
error: process exited with non-zero status Some(1): ... error loading model hyperparameters: key not found in model: llama.context_length
...
llama_server exited with code 1
```
This is the expected, correct outcome for a non-model file - it confirms
binary verification, argument construction (including a path containing
spaces), process launch, stderr capture, and non-zero-exit propagation
all work end to end against the real llama.cpp binaries.

## 9. `brute model inspect` against a real model

The user provided a real model: Qwen2.5-0.5B-Instruct, Q4_K_M quantization,
`C:\Models\qwen2.5-0.5b-instruct-q4_k_m.gguf` (491,400,032 bytes).
```
== Model ==
  File size: 491400032 bytes
  SHA-256: 74a4da8c9fdbcd15bd1f6d01d621410d31c6fc00986f5eb687824e7b93d7a9db
  GGUF version: 3
  Tensor count: 291
  KV count: 26
  Architecture: qwen2
  Name: qwen2.5-0.5b-instruct
  Quantization: MOSTLY_Q4_K_M
  Parameter count: 630167424
  Tensor-data size consistency check: passed
```
Parameter count (630,167,424) is computed directly from the tensor shape
table, not read from metadata - it matches the model's known ~0.5B-class
size once embedding/lm-head tensors are counted.

## 10. `brute benchmark` against the real model - first attempt failed, real bug found and fixed

The first live run against a real chat-template model
(`--timeout-secs 180`) timed out. Direct manual reproduction outside
`brute` confirmed `llama-cli.exe` itself hangs indefinitely with the
original `-no-cnv` flag when stdin is redirected from NUL, because
`-no-cnv` doesn't fully disable llama-cli's post-generation interactive
read loop for chat-template models - it just suppresses some chat-mode
output. Switching to `-st` (`--single-turn`, per llama-cli's own
`--help`: *"will not be interactive if first turn is predefined with
`--prompt`"*) was verified manually first (1.7s wall time, exit 0, correct
generated text), then applied to `runtime::llama_cpp::build_cli_args`,
along with adding `-v` so the (differently-formatted, prefix-less)
`slot print_timing:` lines this build emits are captured, and rewriting
`parse_cli_perf` to match on the metric phrase rather than a fixed line
prefix. Full detail in `docs/known-limitations.md`. Test suite updated
(50/50 passing) with a regression test locking in both the new flag and
the new line-format parsing, using the exact line captured from this
binary. `cargo fmt --check` and `cargo clippy --all-targets -- -D
warnings` clean after the fix.

## 11. `brute benchmark` against the real model - real numbers

```
cargo run --release -- benchmark --model "C:\Models\qwen2.5-0.5b-instruct-q4_k_m.gguf" --llama-bin ".tools\llama.cpp\b10064\cpu" --backend cpu --timeout-secs 180
```
```
== Benchmark ==
  Backend: cpu
  Threads: 16
  Context size: 2048
  Batch size: 512
  Repetitions: 3
  Stability: Stable
  CLI run: exit=Some(0) timed_out=false
  Bench run: exit=Some(0) timed_out=false
  Model load time (ms): (unavailable)  [llama-cli did not print a load time line]
  Time to first token (ms, approx.): 134.11  [inferred, source: approximated as llama-cli's prompt-eval time; not an independently captured per-token timestamp]
  Prompt processing: 425.15 tok/s (stddev 3.74, CV 0.009)
  Generation: 44.09 tok/s (stddev 0.96, CV 0.022)
  Peak process memory (bytes): 542609408  [measured, source: Win32 GetProcessMemoryInfo.PeakWorkingSetSize (llama-bench process)]
  System RAM available before (bytes): 8791633920  [measured, source: Win32 GlobalMemoryStatusEx (before run)]
  System RAM available min-during (bytes): 8237555712  [measured, source: Win32 GlobalMemoryStatusEx (polled during run)]
  System RAM available after (bytes): 8794189824  [measured, source: Win32 GlobalMemoryStatusEx (after run)]
```
`model_load_time_ms` is honestly `unavailable` (not zero, not guessed) -
see `docs/known-limitations.md` for exactly why this llama-cli build
doesn't expose it. A second run (via `recommend`) came back with
generation stability `Marginal` instead of `Stable` (CV 0.055 vs 0.022) -
real run-to-run variance on this machine, not a bug; this is exactly what
the repeated-sample stability classification is for.

## 12. `brute recommend`

```
== Recommended profile (from this run only) ==
  backend: cpu
  thread count: 16
  gpu layers: 0
  context size tested: 2048
  batch size tested: 512
  expected prompt processing speed: 401.24-407.66 tok/s
  expected generation speed: 42.29-47.12 tok/s
  observed peak memory: 541876224 bytes
  stability confidence: Marginal
  basis: 3 repetition(s) via llama-bench on the exact model/config recorded above; not extrapolated to other models or settings
```

## 13. `brute report --output brute-report.json`

Produced 1,540 lines of valid, pretty-printed JSON (validated with `serde_json::from_str` round-trip in `report::json` tests, and spot-checked with `grep` after generation: `"stability": "marginal"`, `"avg_tokens_per_second": 389.850945` / `44.282249`, `"backend": "cpu"` present and correctly nested throughout). Not committed to the repo (`brute-report.json` is gitignored, as generated output should be).

All Stage 0 acceptance criteria are now satisfied against a real model on
real hardware, with the one real bug found along the way (the `-no-cnv`
interactive-hang) fixed, tested, and documented rather than papered over.
