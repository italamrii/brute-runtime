# Known limitations — Stage 0

These are real, observed limitations, not a hedge-everything disclaimer.
Each one was either hit directly during Stage 0 verification on this
machine or is a deliberate, documented scope cut.

## Observed during verification on this machine

- **Registry `ProductName` says "Windows 10 Home" on this Windows 11
  machine.** `brute inspect`'s OS product name field reads
  `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\ProductName`, which on
  this machine (Windows 11 Home, build 26200) still literally contains the
  string `Windows 10 Home` - a long-standing, widely reported Microsoft
  quirk where that particular registry value was never updated for many
  Windows 11 builds/images. `DisplayVersion` (25H2) and `CurrentBuildNumber`
  (26200) are correct and are what actually distinguish the OS version.
  This is reported faithfully (it's genuinely what the registry says, and
  the field's `source` says exactly which registry value it came from) -
  it is not a bug in `brute`, and it's a good illustration of why every
  field carries its exact source rather than a bare value.

- **First execution of a freshly downloaded `.exe` is slow.** The first
  ever run of `llama-cli.exe` immediately after `fetch-llama-cpp.ps1`
  extracted it took long enough to exceed a 15-second timeout (Windows
  Defender/SmartScreen scanning a new, unrecognized binary on first
  execution is the standard explanation for this and matches the observed
  one-time-only pattern - subsequent runs completed in well under a
  second). `brute doctor`'s internal timeout was raised to 60s to absorb
  this. A benchmark run's `--timeout-secs` may need to be set generously
  for the very first invocation against a newly fetched binary. The same
  pattern was also observed once with `cargo test` itself immediately
  after a fresh rebuild (`os error 4551`, "An Application Control policy
  has blocked this file") - it cleared on an immediate retry with no code
  changes, consistent with a one-time endpoint-security scan of the newly
  compiled test binary rather than a real test failure.

- **Canonicalized paths render with the `\\?\` extended-length prefix** in
  reports (e.g. `\\?\C:\Users\...`) because `std::fs::canonicalize` on
  Windows returns that form. Functionally correct (every Win32 API and
  llama.cpp itself accepts it), just visually unfamiliar in the terminal
  report.

## Deliberate Stage 0 scope cuts

- **No CUDA execution, only CUDA detection.** This environment has no CUDA
  toolkit installed; NVIDIA GPU presence and a working driver are detected
  via DXGI + `nvidia-smi` (graded `inferred` for actual CUDA usability,
  since no CUDA kernel was ever run). The CUDA llama.cpp release build
  (145-249 MB) was not auto-fetched in Stage 0; it can be fetched with the
  same pinned/verified mechanism as the CPU build by extending
  `scripts/llama-cpp-manifest.json`.
- **Vulkan execution binary not exercised end-to-end.** Vulkan
  *detection* works and was verified live (`vulkaninfo --summary` succeeds
  on this machine). The Vulkan llama.cpp binary is documented and pinned
  in the manifest but a live Vulkan benchmark was not run in this session.
- **"Time to first token" is approximated**, not independently measured
  per-token - see `docs/measurement-methodology.md`.
- **GPU driver version is only resolved for NVIDIA adapters** (via
  `nvidia-smi`); Intel/AMD driver version is reported as unknown rather
  than guessed.
- **No universal "AI capability score."** Explicitly out of scope per the
  Stage 0 brief - see `report::build_recommendation`.
- **Live end-to-end benchmark against a real model is pending.** No GGUF
  file was available on this machine during implementation. Everything
  through the GGUF parser, process launcher, and llama.cpp argument/output
  handling was verified against: (a) synthetic byte-level fixtures for the
  parser, and (b) a live run against the real, fetched llama.cpp binaries,
  which correctly loaded, correctly rejected a synthetic non-model GGUF
  file with a real llama.cpp error (`key not found in model:
  llama.context_length`), and correctly propagated that failure. A real
  model benchmark run (with real tokens/sec numbers) is the next step once
  a `.gguf` file is provided.
- **No workspace split.** Single binary crate; see
  `docs/architecture.md` for why and what the natural split points are.
- **`brute` targets Windows only.** No `cfg(unix)` paths exist; several
  modules (`hardware::windows`, `hardware::gpu`'s DXGI path, `security`
  binary verification against `.exe`) are Windows-specific by design per
  the Stage 0 brief.
