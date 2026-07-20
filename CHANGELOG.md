# Changelog

Format loosely follows [Keep a Changelog](https://keepachangelog.com/).
Dates are when the work was verified, not calendar-committed.

## [v0.1.0-beta.1] — 2026-07-20

First public beta. Windows-only, packaged and manually verified;
macOS/Linux are compile-verified only. See
[`docs/releases/v0.1.0-beta.1.md`](docs/releases/v0.1.0-beta.1.md) for
the full release notes.

### Added

- Core engine: real hardware inspection (CPU/RAM/GPU/OS/storage/power),
  GGUF metadata parsing, managed llama.cpp subprocess execution with
  SHA-256 binary verification, controlled benchmarking with real
  variance reporting.
- Hardware Capability Profile, a curated local model-build catalog,
  real-formula memory/disk estimation, six-state honest fit
  classification (Excellent → Not recommended, `Unknown` never
  collapsed into a negative result), task/priority-aware recommendation
  and ranking with a two-tier explanation.
- Backend capability verification that proves CPU/CUDA/Vulkan actually
  work (not just "detected"), bounded safe runtime auto-tuning
  (thread count / GPU offload / context+batch), a safety-first ranking
  comparator, and locally-saved, auto-invalidating runtime profiles.
- A trusted local model library: content-hash identity (not path),
  honest structural-vs-trust verification, duplicate detection, storage
  reporting, move/rename recovery, quarantine, and a library-wide health
  audit — nothing is ever silently copied, moved, or deleted.
- A Tauri + React + TypeScript Windows desktop application over the same
  engine: Overview, Hardware, Models, Discover Models (hardware-matched
  recommendations), Optimize, Run (local streaming generation), Profiles,
  Health, Settings. Full Arabic/English UI with RTL/LTR layout switching.
- Trusted runtime auto-resolution (bundled → advanced override →
  PATH-discovered → actionable "not found"), automatic GGUF discovery
  across a fixed, safe set of common folders, and an explicit,
  user-triggered model download flow with visible progress and
  cancellation.
- Cross-platform hardware-abstraction layer (compile-verified for
  Windows/macOS/Linux) as groundwork for future non-Windows support.
- Official BRUTE branding applied throughout the app and installer
  (native icons for all platforms, in-app brand integration, a
  regeneration script and documentation for replacing brand assets).
- Windows MSI + NSIS installer packaging, plus a polished end-user
  release folder/ZIP with bilingual READMEs and checksums.

### Fixed

- A Discover Models crash: a mismatch between the frontend's TypeScript
  type for the catalog's `License` field and the real Rust wire format
  caused every model card to crash the whole app to a blank screen on
  click. Fixed, and an Error Boundary added so a future render error of
  this class can't blank the app again.
- Windows console-window flashing: every background `llama-cli.exe`/
  `llama-bench.exe`/`nvidia-smi.exe`/`vulkaninfo.exe` launch was missing
  `CREATE_NO_WINDOW`, flashing a visible console window on every backend
  verification, tuning run, benchmark, local generation, and hardware
  scan. Fixed and verified with real process/window monitoring in an
  installed build.
- A Cargo build-script staleness bug where a regenerated app icon didn't
  reach the shipped executable until the build script was forced to
  re-run.

### Known limitations

See [`docs/known-limitations.md`](docs/known-limitations.md) for the
complete, itemized list — including the unsigned-beta status, no
download pause/resume, and exactly what "macOS/Linux compile-verified"
does and does not prove.
