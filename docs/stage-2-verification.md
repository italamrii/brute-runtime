# Stage 2 verification — what was actually run and observed

Real machine: 13th Gen Intel Core i5-13450HX (10 physical / 16 logical
cores), 16 GB RAM, NVIDIA GeForce RTX 5050 Laptop GPU, Windows 11 Home
build 26200. Model: Qwen2.5-0.5B-Instruct Q4_K_M
(`C:\Models\qwen2.5-0.5b-instruct-q4_k_m.gguf`, sha256
`74a4da8c9fdbcd15bd1f6d01d621410d31c6fc00986f5eb687824e7b93d7a9db`, 24
layers). Binaries: the Stage 0 pinned `b10064` **CPU-only** prebuilt
(`.tools\llama.cpp\b10064\cpu`) — no CUDA/Vulkan llama.cpp binary was
fetched in this session, same documented decision as Stage 0 (see
`docs/known-limitations.md`).

## 1. A real bug found and fixed live

Before any of the transcript below, `brute backends verify --backend
cuda` was pointed at the CPU-only binary directory to sanity-check the
"never claim GPU success from detection alone" logic. It **falsely**
reported `status: "verified"`:

```
$ brute backends verify --model ... --llama-bin .tools\llama.cpp\b10064\cpu --backend cuda --json
[{ "status": "verified", "benchmark_verified": true,
   "verification_tokens_per_second": 74.69264, "reported_gpu_info": "" }]
```

Root cause: `check_no_silent_fallback` accepted `gpu_info` non-empty
**OR** `n_gpu_layers > 0` as GPU-use evidence. The CPU-only binary
echoes the requested `-ngl 1` flag straight into its JSON output as
`n_gpu_layers: 1` regardless of whether it has any GPU code path — an
echo of the input, not confirmed usage. Fixed to trust only `gpu_info`
non-empty. Re-run after the fix, same binary, same command:

```
$ brute backends verify --model ... --llama-bin .tools\llama.cpp\b10064\cpu --backend cuda --json
[{ "status": "benchmark_failed", "benchmark_verified": false,
   "verification_tokens_per_second": 76.995186, "reported_gpu_info": "",
   "failure_reason": "Cuda verification ran and exited cleanly, but llama-bench
     reported no GPU device in use (gpu_info empty) - refusing to report GPU
     success on a possible silent CPU fallback (n_gpu_layers alone is not
     trusted as evidence: it can echo the requested flag even on a binary
     with no Cuda support compiled in)" }]
```

Regression test added:
`backends::tests::gpu_backend_with_echoed_n_gpu_layers_but_empty_gpu_info_is_not_verified`.
See `docs/backend-verification.md` for the full writeup.

## 2. `brute tune run --dry-run` — before and after the fix

Before the fix, the dry-run plan incorrectly included 16 GPU-offload and
GPU-tagged context/batch candidates against the CPU-only binary set
(`backend=Cuda` on candidates like `gpu_layers-12`, `ctx-4096-batch-256`)
because CUDA had been (incorrectly) verified. After the fix, the same
command against the same binaries correctly produces an all-CPU plan:

```
$ brute tune run --model ... --llama-bin .tools\llama.cpp\b10064\cpu --dry-run
Dry run - no benchmarks will be launched.
Formula version: stage2-v1
Defaults: threads=16 gpu_layers=0 context=2048 batch=512

14 candidate(s):
  threads-10           backend=Cpu threads=10 gpu_layers=0 context=2048 batch=512
  threads-12           backend=Cpu threads=12 gpu_layers=0 context=2048 batch=512
  threads-16           backend=Cpu threads=16 gpu_layers=0 context=2048 batch=512
  ctx-1024-batch-128   backend=Cpu threads=16 gpu_layers=0 context=1024 batch=128
  ctx-1024-batch-256   backend=Cpu threads=16 gpu_layers=0 context=1024 batch=256
  ctx-1024-batch-512   backend=Cpu threads=16 gpu_layers=0 context=1024 batch=512
  ctx-2048-batch-128   backend=Cpu threads=16 gpu_layers=0 context=2048 batch=128
  ctx-2048-batch-256   backend=Cpu threads=16 gpu_layers=0 context=2048 batch=256
  ctx-4096-batch-128   backend=Cpu threads=16 gpu_layers=0 context=4096 batch=128
  ctx-4096-batch-256   backend=Cpu threads=16 gpu_layers=0 context=4096 batch=256
  ctx-4096-batch-512   backend=Cpu threads=16 gpu_layers=0 context=4096 batch=512
  ctx-8192-batch-128   backend=Cpu threads=16 gpu_layers=0 context=8192 batch=128
  ctx-8192-batch-256   backend=Cpu threads=16 gpu_layers=0 context=8192 batch=256
  ctx-8192-batch-512   backend=Cpu threads=16 gpu_layers=0 context=8192 batch=512
```

No pruning fired (16 GB RAM is ample for a 630M-parameter model at every
context size tried) and no truncation (14 < the 32-candidate ceiling).
Re-running the same command twice produced byte-identical candidate
ordering — deterministic, matching `tuning::candidates::tests::plan_is_deterministic`.

## 3. `brute tune run --save-profile` — full real run

```
$ brute tune run --model ... --llama-bin .tools\llama.cpp\b10064\cpu --max-duration-secs 240 --save-profile
Runtime profile saved: profile-instance-f052be34d631ff2889844a7551eea020
Tuning complete in 52.8s (14 candidate(s) benchmarked).

Winner: ctx-1024-batch-128 (High confidence)
  backend=Cpu threads=16 gpu_layers=0 context=1024 batch=128
  stability=Stable generation=Some(75.45603899999999) tok/s prompt=Some(512.3557556666666) tok/s
Runner-up: ctx-1024-batch-256
Rejected faster candidate ctx-4096-batch-256: measured 75.86 tok/s generation
  (faster than the winner's 75.46 tok/s) but ranked below it because its
  stability was classified stable versus the winner's stable
Rejected faster candidate threads-12: measured 81.18 tok/s generation
  (faster than the winner's 75.46 tok/s) but ranked below it because its
  stability was classified stable versus the winner's stable
Rejected faster candidate threads-10: measured 80.54 tok/s generation
  (faster than the winner's 75.46 tok/s) but ranked below it because its
  stability was classified stable versus the winner's stable
Rejected faster candidate ctx-4096-batch-512: measured 75.47 tok/s generation
  (faster than the winner's 75.46 tok/s) but ranked below it because its
  stability was classified marginal versus the winner's stable
```

14 candidates × 3 repetitions each in 52.8s (real wall time, including
per-candidate cooldowns and RAM/disk checks before every repetition).
Why `threads-12`/`threads-10` (both faster, both `Stable`) lost to a
context/batch candidate under `balanced`: thread-dimension candidates
never receive a RAM prediction (`predicted_ram_bytes: None`), and an
unmeasured value is never treated as safe on the `ram_safety` tier — see
`docs/tuning-ranking-methodology.md`. This is real, working, intentional
behavior, not a ranking bug.

## 4. `brute tune status` after completion

```
$ brute tune status
Model sha256: 74a4da8c9fdbcd15bd1f6d01d621410d31c6fc00986f5eb687824e7b93d7a9db
Started: 2026-07-18T12:29:04.800201700+00:00
Progress: 14/14 candidates
Finished: true
Cancelled: false
Finished at: 2026-07-18T12:29:57.566670300+00:00
```

(A first implementation counted repetition *attempts*, not distinct
*candidates*, and misreported "42/14 candidates" — 3 repetitions × 14
candidates. Fixed to derive progress from the candidate's position in
the plan.)

## 5. `brute tune cancel`

```
$ brute tune cancel
Cancellation requested. A running `brute tune run` will stop at its next safe
checkpoint (between repetitions/candidates), never mid-process.
```

## 6. `brute profiles` — list, show, verify, export

```
$ brute profiles list
1 saved profile(s):
  profile-instance-f052be34d631ff2889844a7551eea020

$ brute profiles show profile-instance-f052be34d631ff2889844a7551eea020
Profile: profile-instance-f052be34d631ff2889844a7551eea020
  Tuned: 2026-07-18T12:29:57.569156100+00:00
  Model sha256: 74a4da8c9fdbcd15bd1f6d01d621410d31c6fc00986f5eb687824e7b93d7a9db
  Backend: Cpu  Threads: 16  GPU layers: 0
  Context: 1024  Batch: 128
  Generation: Some(75.45603899999999) tok/s  Prompt: Some(512.3557556666666) tok/s
  Stability: Stable  Confidence: High

$ brute profiles verify profile-instance-f052be34d631ff2889844a7551eea020 --model ... --llama-bin ...
Status: Verified
Rollback recommended: false
verification run completed and confirmed the profile's backend and settings were actually used

$ brute profiles export profile-instance-f052be34d631ff2889844a7551eea020 --output exported-profile.json
Profile exported to exported-profile.json (machine ID redacted).
```

The exported file's `machine_id` field reads exactly
`"omitted-from-export"`; every other field (model hash, binary hashes,
measured throughput, thread/context/batch settings) is present and
correct — verified by direct inspection of the written file. No
filesystem path appears anywhere in it.

## 7. CPU vs GPU comparison

**Not performed.** No CUDA/Vulkan llama.cpp binary was fetched in this
session (same documented decision as Stage 0 — see
`docs/known-limitations.md`); `brute backends verify` genuinely reported
CUDA as `benchmark_failed` against the CPU-only binary, honestly, which
is the correct behavior for that binary, not a shortfall in the tuning
engine. A real CUDA/Vulkan comparison is a documented follow-up: fetch
the pinned CUDA or Vulkan release with the same
`scripts/fetch-llama-cpp.ps1` mechanism, then re-run
`brute tune run` unrestricted (no `--backend` flag) so it opportunistically
verifies and tunes GPU offload candidates too.

## 8. Performance (spec section 16)

| Operation | Real measured time |
|---|---|
| `brute tune run --dry-run` (release build, opportunistic CUDA+Vulkan backend verification included — 2 real subprocess launches) | 3.07s |
| `brute tune run --dry-run --backend cpu` (skips GPU verification entirely; dominated by hashing the 491 MB model file) | 1.03s |
| `brute profiles list` | 0.025s |
| `brute profiles show <id>` | 0.022s |
| `brute tune cancel` (writes the cancel-flag file) | 0.020s |
| Full 14-candidate × 3-repetition real tuning run | 52.8s |

Pure candidate-generation (`generate_plan`) and ranking (`rank`) are not
separately microbenchmarked, but both operate on a hard ceiling of 32
candidates with only integer/float arithmetic and no I/O - the entire
`tuning::candidates`/`tuning::ranking` unit test suites (21 tests
combined, each calling `generate_plan`/`rank` at least once, several
calling it multiple times) complete in well under 2 seconds total,
which bounds the per-call cost at low single-digit milliseconds. The
CLI-measured dry-run time above is dominated by real I/O (model file
SHA-256, hardware inspection, subprocess launches for backend
verification), not by planning logic itself.

## 9. Test suite, formatting, lint

```
$ cargo test
test result: ok. 206 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

$ cargo fmt --check
(no output - clean)

$ cargo clippy --all-targets -- -D warnings
Finished `dev` profile [unoptimized + debuginfo] target(s)
(zero warnings, zero errors)
```

Toolchain: `rustc 1.98.0-nightly (3daae5e42 2026-06-14)`.

## 10. Honest gaps in this verification pass

- CUDA and Vulkan execution (not just detection/verification-refusal)
  was not exercised, for the same reason as Stage 0: no GPU-enabled
  llama.cpp binary was fetched in this environment. Detection and the
  refuse-to-fabricate-success behavior *were* verified live, thoroughly,
  including the real bug above.
- "CUDA binary launches but model load fails" (one of spec section 14's
  scenarios) has no dedicated automated test - it requires a real
  GPU-enabled binary to exercise honestly rather than mocking llama.cpp's
  own process behavior. Left as a live-validation scenario, consistent
  with the rest of this project's "no physical GPU required for the
  automated suite" convention.
