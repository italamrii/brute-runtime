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
all work end to end against the real llama.cpp binaries. **A live
benchmark producing real tokens/sec numbers is pending a real `.gguf`
model file**, which was not available in this environment - see the next
section.

## 9. Pending: live benchmark against a real model

Once a real GGUF model path is available:
```
cargo run --release -- benchmark --model <path> --llama-bin .tools\llama.cpp\b10064\cpu --backend cpu
cargo run --release -- recommend --model <path> --llama-bin .tools\llama.cpp\b10064\cpu --backend cpu
cargo run --release -- report --output brute-report.json --model <path> --llama-bin .tools\llama.cpp\b10064\cpu
```
This will exercise the parts of the acceptance criteria that specifically
require a real, loadable model: real timings, real tokens/sec, real peak
memory.
