# Changelog

Format loosely follows [Keep a Changelog](https://keepachangelog.com/).
Dates are when the work was verified, not calendar-committed.

## [Unreleased] — Phase B: Multi-Family Model Intelligence

Not yet tagged as a release. Builds on v0.1.0-beta.1's foundation with
a preferences-aware recommendation engine, a real curated multi-family
catalog, and a fully wired safe download flow. See the dedicated docs
linked below for each stage's full detail.

### Added

- **macOS (Apple Silicon) desktop build.** The Tauri app now builds an
  `.app` + `.dmg` on arm64 macOS: `bundle.targets` is `"all"` (native
  targets per host OS — `.app`/`.dmg` on macOS, MSI/NSIS on Windows), a
  new `scripts/fetch-llama-cpp.sh` fetches and pin-verifies the bundled
  macOS llama.cpp runtime (the counterpart of the existing `.ps1`), and
  the manifest gained `macos-arm64`/`macos-x64` entries anchored to
  upstream release digests. The full engine + desktop test suites pass on
  real Apple Silicon hardware, and the bundled runtime is pin-verified
  and launchable (`brute doctor`). The macOS build is unsigned/not
  notarized (Gatekeeper will warn on first launch, the parallel of the
  Windows SmartScreen notice) and has not had full manual GUI end-to-end
  sign-off, so it is not held to the Windows "release-ready" bar yet.
  See [`BUILDING.md`](BUILDING.md) and
  [`docs/cross-platform.md`](docs/cross-platform.md).
- A local, offline-only user preference profile (language, task,
  speed/quality priority, plus ~13 advanced constraints like max
  RAM/VRAM/download size, preferred/excluded families, permitted
  licenses) - never synced, no account. See
  [`docs/user-preferences.md`](docs/user-preferences.md).
- Recommendation Engine v2: seven-component scoring (device fit,
  Arabic, task fit, speed, quality, trust, license fit) that reads the
  preference profile directly, with deterministic tie-breaking, an
  explicit confidence level (`High` only for a real on-device
  calibration match), and a rejection reason for every excluded build.
  v1 (still used by Optimize) is untouched. See
  [`docs/recommendation-methodology-v2.md`](docs/recommendation-methodology-v2.md).
- An expanded, additive catalog schema (verification-status ladder,
  curated capability levels, evidence-source tracking) and four new,
  individually web-verified catalog entries spanning Arabic (Jais-2,
  ALLaM), vision (Gemma-3), and a long-context general model (Phi-4-
  mini) - plus a hard split between the production catalog and test-
  only fixtures so a synthetic entry can never ship. See
  [`docs/model-catalog-schema.md`](docs/model-catalog-schema.md).
- Discover Models redesign: 18 real filter dimensions, 7 score-backed
  sort options, curated capability/speed/verification badges, an
  "Installed" cross-reference against the local library, and a
  4-model comparison view with a plain-language "leads in" summary
  underneath the raw side-by-side table. See
  [`docs/discover-models.md`](docs/discover-models.md) and
  [`docs/model-comparison.md`](docs/model-comparison.md).
- A safe verified download flow: a real "Download" button (shown only
  once a catalog entry's exact artifact URL is independently verified),
  a disk-space preflight check, SHA-256 checksum verification before
  the file is kept, and an explicit "Add to library" step - nothing
  auto-imports. See
  [`docs/safe-download-flow.md`](docs/safe-download-flow.md).
- An 8-mode chat model chooser (Auto/Fastest/Balanced/Best quality/
  Arabic/Coding/Documents/Vision) that picks the best already-installed
  model for the chosen mode via Recommendation Engine v2, restricted to
  models with a real catalog match. See
  [`docs/chat-model-selector.md`](docs/chat-model-selector.md).

### Fixed

- The Discover Models catalog page's test fixtures no longer ship
  inside the same file the installed app loads in production
  (`data/catalog/dev-catalog.json`) - a synthetic unknown-license entry
  was moved to `data/catalog/test-fixtures.json`, which no production
  code path ever reads.

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
