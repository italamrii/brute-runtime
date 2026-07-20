# Known limitations — Stage 0 through Stage 4

These are real, observed limitations, not a hedge-everything disclaimer.
Each one was either hit directly during verification on this machine or
is a deliberate, documented scope cut.

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

- **`-no-cnv` alone does not make llama-cli non-interactive**, and this
  build never prints a "load time" line at all. Found and fixed during the
  first live benchmark against a real model (Qwen2.5-0.5B-Instruct, which
  has a chat template):
  - `brute benchmark`'s single `llama-cli` timing run originally passed
    `-no-cnv` to force non-interactive one-shot completion. Against a
    model with a chat template, that flag only suppresses conversation-mode
    *chrome* - the process still enters an interactive stdin-read loop
    after generating, which with stdin redirected from NUL (as `brute`
    does) reads instant EOF forever and never exits. The benchmark timed
    out at 180s. Fixed by switching to `-st` (`--single-turn`), which
    "will not be interactive if first turn is predefined with `--prompt`"
    per llama-cli's own `--help` text - verified directly (1.7s wall time,
    clean exit 0, correct generated text) before changing the code. See
    `runtime::llama_cpp::build_cli_args`.
  - This build (`b10064`) does not print the classic
    `llama_perf_context_print: load time = ...` line under any flag
    combination tried (`--perf`, `--perf -v`) - only `-v`/`--log-verbose`
    produces per-request timing (`slot print_timing: ... prompt eval time
    = ...` / `eval time = ...`), and even then with no load-time line.
    `model_load_time_ms` is therefore genuinely `unavailable` when running
    against this binary, and the parser (`parse_cli_perf`) was rewritten
    to match on the metric phrase itself rather than a fixed line prefix,
    so it keeps working if a different llama-cli build's log format
    changes again. `docs/measurement-methodology.md` and the report's own
    `source` field for that field state this plainly rather than silently
    returning a stale/wrong number.

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
- ~~Live end-to-end benchmark against a real model is pending~~ **Done.**
  A real model (Qwen2.5-0.5B-Instruct, Q4_K_M, 630M params) was benchmarked
  live on this machine after the `-no-cnv`/`-st` fix above - see
  `docs/stage-0-verification.md` for the real numbers.
- **No workspace split.** Single binary crate; see
  `docs/architecture.md` for why and what the natural split points are.
- **`brute` targets Windows only.** No `cfg(unix)` paths exist; several
  modules (`hardware::windows`, `hardware::gpu`'s DXGI path, `security`
  binary verification against `.exe`) are Windows-specific by design per
  the Stage 0 brief.

## Stage 1: found and fixed during live verification

- **Fit engine originally over-penalized uncertainty** — every
  uncalibrated catalog-only estimate jumped straight to `Experimental`
  regardless of headroom, so even a tiny model with enormous free RAM was
  branded "substantial uncertainty." Fixed to downgrade one tier instead
  of jumping straight there. See `docs/model-fit-classification.md` and
  `docs/stage-1-verification.md` §2.
- **`recommend-model` originally presented a different-sized calibrated
  build's real tok/s numbers as flat "expected performance"** for the
  build actually being evaluated on a `Close` (not `Exact`) calibration
  match - exactly the "extrapolate as if scaling were linear" the brief
  explicitly forbids. Fixed with proximity-aware wording in
  `recommend::explain::describe_calibration_performance`. See
  `docs/calibration-methodology.md`.
- **`calibration_record_count` in the hardware profile originally counted
  every record in the store file, not just ones measured on this
  machine** - fixed to filter by `machine_id`.

## Stage 1: deliberate scope cuts

- **Partial GPU offload is not modeled precisely.** The estimator treats
  GPU offload as all-or-nothing (`EstimationConfig::full_gpu_offload`);
  `--n-gpu-layers`-style partial splits are a documented gap, not
  silently estimated as if precise.
- **The catalog is a small, hand-curated development fixture** (7 entries
  in `data/catalog/dev-catalog.json`), not a live or scraped index. Only
  one entry's numeric fields are independently verified against a real
  downloaded file; the rest are labeled order-of-magnitude approximations
  per-entry in `metadata_provenance`. Stage 1 does not fetch, scrape, or
  auto-update catalog data from any external source - by design (see
  `docs/security-model.md`).
- **Calibration coverage is currently one architecture (Qwen2/`qwen2`) on
  one backend (CPU).** Every other architecture in the dev catalog
  (Llama) has no calibration data yet, so its fit/recommendation results
  are honestly downgraded for uncertainty rather than guessed. Growing
  the calibration store (`--save-calibration` on `brute benchmark`) is
  the intended way to close this gap over time - it does not happen
  automatically.
- **`quality_pref` and `openness_pref` reference scales are fixed
  constants** (30B params as the "large" reference; a simple
  gated/known-license heuristic), not learned or tuned against real user
  feedback - see `docs/recommendation-methodology.md`.
- **Bits-per-weight table underestimates small models by roughly a
  quarter** (measured: ~28% for Qwen2.5-0.5B Q4_K_M) because embedding/
  output tensors are a larger fraction of a small model's total size.
  Real `file_size_bytes` is always preferred when available; the table is
  only a fallback/cross-check. See `docs/model-memory-estimation.md`.
- **Catalog/calibration performance was only measured at small scale**
  (7 catalog entries, 1 calibration record) - see
  `docs/stage-1-verification.md` §9. Behavior at "hundreds or thousands"
  of entries is a linear extrapolation, not an independent measurement.

## Stage 2: found and fixed during live verification

- **A real silent-fallback false positive.** `check_no_silent_fallback`
  originally accepted `n_gpu_layers > 0` (from llama-bench's own JSON
  output) as evidence a GPU was used, alongside a non-empty `gpu_info`.
  Live testing against this repo's pinned **CPU-only** b10064 binary
  showed the binary echoes the requested `-ngl` flag straight back into
  `n_gpu_layers` regardless of whether it has any GPU support compiled
  in - `brute backends verify --backend cuda` against that binary
  falsely reported `status: "verified"`. Fixed to trust only `gpu_info`
  non-empty. See `docs/backend-verification.md` for the full before/
  after transcript and the regression test.
- **`brute tune status` originally conflated repetitions with
  candidates**, counting `run_repetition` closure calls (3 per
  candidate) rather than distinct candidates, misreporting "42/14
  candidates" on a real run. Fixed to derive progress from the
  candidate's position in the plan.

## Stage 2: deliberate scope cuts

- **CUDA/Vulkan execution not exercised live.** Same as Stage 0: no
  GPU-enabled llama.cpp binary was fetched in this environment.
  Detection and the refuse-to-fabricate-GPU-success behavior *were*
  verified live and thoroughly (see the false-positive bug above and
  `docs/stage-2-verification.md`) - only genuine end-to-end GPU
  *execution* (a real offloaded benchmark completing) was not.
- **Mid-process cancellation is not wired into individual tuning
  benchmark launches.** `brute tune cancel` is checked between
  repetitions/candidates (`tuning::runner::execute_plan`'s
  `is_cancelled` closure), not via `TickAction::Cancel` inside a single
  running `llama-bench` process, even though the underlying primitive
  supports it (see `docs/cancellation-and-process-safety.md`). Given the
  small fixed prompt/generation token counts tuning uses, the practical
  gap between "cancel requested" and "next checkpoint" is normally a few
  seconds on CPU.
- **The context×batch candidate group's "batch exceeds context" pruning
  branch is not exercised by the current fixed option sets** (context
  starts at 1024; batch tops out at 512, so batch can never exceed
  context with today's constants). The check remains as a defensive
  guard against a future change to those option lists, not dead code
  removed for lack of current coverage.
- **Backend driver-version changes are not separately tracked** for
  runtime-profile invalidation (spec section 9's "when detectable"
  qualifier). A driver change that actually matters changes what `brute
  backends verify` reports on the next `brute tune run`/`profiles
  verify`, which always re-verifies rather than trusting a cached flag -
  see `docs/runtime-profile-schema.md`.
- **The three tuning dimensions (threads, GPU offload, context×batch)
  are searched independently, not jointly.** "Is 12 threads still the
  best choice once GPU offload is on?" is not directly tested - each
  group is anchored on fixed defaults for the other dimensions, a
  deliberate tradeoff for deterministic, displayable dry-run planning.
  See `docs/tuning-search-space.md`.
- **"CUDA binary launches but model load fails" has no dedicated
  automated test** - it requires a real GPU-enabled binary to exercise
  honestly. Left as a live-validation scenario.
- **Backend verification and tuning benchmarks use small, fixed
  prompt/generation token counts** (16/8 for backend verification,
  64/32 for tuning), not the model's full practical context - by
  design, to keep a 32-candidate plan benchmarkable in bounded time; see
  `docs/tuning-search-space.md` and `docs/cancellation-and-process-safety.md`.

## Stage 3: found and fixed during development

- **A real cross-backend false-positive in calibration matching.**
  `associations::find_calibration_matches_for` originally iterated over
  CPU/CUDA/Vulkan and trusted `calibration::find_nearest`'s result for
  each - but `find_nearest` doesn't filter by backend (a mismatch only
  lowers its proximity score), so it returned the same single CPU
  record as a "match" for all three backends. A live test with one CPU
  record caught this directly (expected 1 match, got 3). Fixed by
  additionally requiring the returned record's own `backend` field to
  equal the one being asked about. See
  `docs/runtime-profile-association.md`.

## Stage 3: deliberate scope cuts

- **Managed-copy import mode (`--copy-into-library`) is not
  implemented.** The spec explicitly permits skipping it "if it adds
  unnecessary complexity" - every Stage 3 import operates on the file's
  existing location. `LibraryEntry.managed_copy` is `false` on every
  entry this codebase creates, so `remove-managed` always honestly
  reports there's nothing to remove; its path-traversal guard is real
  and tested regardless. See `docs/model-import-and-verification.md`.
- **`ExpectedHashMatched` trust can never be reached today** - the
  catalog schema (`catalog::ModelBuild`) has no field for an
  independently curated expected SHA-256, only descriptive metadata.
  The strongest automatic catalog match is `CatalogMetadataMatched`
  (file size + GGUF metadata agree), one honest tier below what a real
  hash match would justify. See `docs/trust-and-provenance.md`.
- **The real Qwen model's catalog match is `Weak`, not `Strong`**,
  because its GGUF-parsed quantization label (`MOSTLY_Q4_K_M`, from
  llama.cpp's legacy `general.file_type` enum) differs textually from
  the curated catalog's label (`Q4_K_M`) - a real naming-convention
  mismatch observed live, not a hypothetical. Normalizing quantization
  strings before comparison would close this gap; not attempted this
  stage to avoid destabilizing Stage 1's existing catalog-matching
  conventions.
- **Runtime profile and calibration associations are computed live, not
  persisted** on `LibraryEntry`, by deliberate design (avoids a second
  source of truth that could drift) - see
  `docs/runtime-profile-association.md`. This means listing associations
  for many entries costs a fresh lookup each time rather than an O(1)
  field read; acceptable at the measured scale (see
  `docs/stage-3-verification.md`), worth revisiting if entry counts grow
  far beyond what was tested.
- **Quarantine blocks `brute library verify`, but no `brute benchmark`/
  `brute tune` command yet consults a library entry's quarantine status**
  - Stage 3's deliverable is the library system itself
  (`is_quarantined()` exists as the check a future integration would
  call), not modifying Stage 0/2's benchmark/tune commands to look
  models up by library ID at all. Every Stage 0-2 command still takes a
  raw file path, unaware the library exists.
- **"Inaccessible file" (permission-denied, as opposed to missing) has
  no dedicated live test** - reliably provisioning a permission-denied
  fixture in an automated Windows test environment is impractical; the
  code path is exercised by construction (`quick_file_status`'s
  `Inaccessible` branch), not by a dedicated fixture.
- **Long-Windows-path handling relies on `std::fs`'s own transparent
  long-path support** rather than an explicit `\\?\` prefix construction
  in `library` code - verified working (a 12-level-deep synthetic path
  well past 260 characters was discovered correctly), but not stress-
  tested against the absolute historical `MAX_PATH` edge cases some
  older Windows APIs still enforce.

## Stage 4 (desktop application)

- **The desktop build is unsigned.** No real code-signing certificate
  was available for this MVP. The app displays "Development build —
  publisher signature not yet configured." rather than claiming a
  verified publisher, and Windows SmartScreen will show its standard
  unrecognized-publisher warning on first run - BRUTE does not attempt to
  suppress or bypass it. See `docs/windows-packaging.md`.
- **CUDA/Vulkan live verification depends on which llama.cpp binaries the
  user points BRUTE at.** The desktop app ships no llama.cpp binary
  itself (matching the CLI's own model - see `docs/security-model.md`
  §2-3); on a machine where only the pinned CPU-only binary from Stage 0
  is available, the Hardware page and backend verification honestly
  report CUDA/Vulkan as detected-but-unverified rather than fabricating a
  verified status. This is the same honest degradation Stage 2 already
  established for the CLI, carried through unchanged.
- **No automated screenshot capture in this session.** Real-machine
  end-to-end validation (spec section 24) was performed by (a) a
  successful `cargo tauri dev` launch against real hardware and (b) two
  live Rust tests (`library_list_shows_the_real_imported_model_with_its_real_hash_if_present`,
  `profiles_show_returns_the_real_saved_stage2_profile_if_present`) that
  exercise the desktop command layer directly against this machine's real
  imported Qwen2.5-0.5B model and real saved Stage 2 runtime profile.
  Interactive UI screenshots require a human (or a screen-capture tool
  not available in this session) to actually click through the running
  app - see `docs/stage-4-verification.md` for exactly what was and
  was not verified this way.
- **Session history is intentionally absent from the Run workspace.**
  Per the Stage 4 scope decision, prompts and outputs are never persisted
  by default and no local session-history feature was added - each Run
  session exists only in the webview's memory until the page is left.
- **The frontend's TypeScript types in `lib/types.ts` are hand-maintained
  mirrors of the Rust `serde` types**, not generated from a schema. A
  future engine type change requires updating both sides by hand; nothing
  currently detects a mismatch except a runtime shape error during manual
  testing (there is no `specta`/schema-generation step in this MVP).
- **Only one auto-tune or one local-generation session can be active at a
  time**, matching the CLI's own single-run model (`AppState` holds one
  cancellation flag per session type) - not a limitation introduced by
  the desktop layer, an intentional continuation of Stage 2's design.

## Cross-platform architecture and model discovery/download (post-MVP pass)

- **macOS and Linux are compile-verified only, not run-verified.** The
  core engine now builds cleanly for `x86_64-unknown-linux-gnu`,
  `x86_64-apple-darwin`, and `aarch64-apple-darwin` (real cross-target
  `cargo check`/`cargo clippy`, zero warnings), but no macOS or Linux
  machine has actually executed this code. See `docs/cross-platform.md`
  for the full, itemized "what this does and does not prove."
- **Model download does not support pause/resume.** `download_model`
  supports start, live progress, cancel, and post-download checksum
  computation, but not HTTP range-request resumption - a cancelled
  download must restart from zero. See `docs/security-model.md` item 21
  and `commands/download.rs`'s own doc comment.
- **Per-process peak memory sampling has no macOS implementation.**
  Windows (`GetProcessMemoryInfo`) and Linux (`/proc/<pid>/status`
  `VmHWM`) both report a real OS-tracked peak; macOS would need
  `libproc`/`task_info` FFI, judged out of scope for this pass -
  `query_peak_working_set` honestly returns `None` there rather than a
  mislabeled current-RSS reading.
- **The Rust test suite's own process-launch tests are Windows-only.**
  `runtime::process`'s tests launch `cmd.exe` directly; they will fail
  to run (not fail to compile) on a Linux/macOS CI runner. Cross-target
  `cargo check`/`clippy` were used to verify the *shipped* code compiles
  everywhere; making the test harness itself OS-neutral is deferred to
  the CI pipeline work.
- **ARM64 CPU instruction-set detection reports nothing, honestly.**
  `raw_cpuid` only reads x86/x86_64 CPUID leaves; on Apple Silicon or
  ARM Linux, `hardware::cpu::detect_instruction_sets` returns an empty
  list rather than guessing - llama.cpp's ARM/NEON kernel dispatch is a
  separate, not-yet-modeled concern.
- **The "Discover Models" catalog page reuses the existing curated
  `dev-catalog.json` fixture** - it does not add a second, larger
  catalog or any remote catalog sync (which remains explicitly out of
  scope - see the privacy model's no-remote-catalog-sync guarantee).
  Fit/recommendation data shown there comes from the same
  `recommend`/`fit` engine calls the Optimize page already used.
- **The common-folder model auto-discovery list is a fixed, small set**
  (Downloads, Documents, LM Studio's and Ollama's documented model
  cache paths, and `C:\Models` on Windows) - not a configurable list of
  arbitrary user-added folders yet; "Add folder" (a manual, one-off
  directory scan) is available as a complement, not a persisted list of
  additional auto-scan locations.

## Navigation-safety hardening and branding pass

- **The Discover Models catalog has no in-app "Download" action yet, by
  design.** `ModelBuild` (`src/catalog/schema.rs`) only carries
  `official_source_url` - the model's official page/repository, meant
  for a human to review license and pick the right file, not a
  verified direct link to one specific `.gguf` artifact. The already-
  implemented `download_model`/`cancel_download` Tauri commands
  (`desktop/src-tauri/src/commands/download.rs`) correctly stream-
  download-and-verify *any* http(s) URL a caller gives them, but wiring
  a "Download" button to `official_source_url` today would silently
  download the wrong content (an HTML page, not model weights) and
  mislabel it as a model - exactly the kind of false-success this
  project refuses to ship. Closing this gap requires sourcing and
  pinning a real, per-artifact direct file URL (and ideally a
  publisher-supplied hash) into the catalog schema, which is a data/
  schema change out of scope for this pass. The Discover Models modal
  therefore currently offers exactly one external action - "Open
  official source," which opens the real page in the system browser -
  and both the Tauri command layer and the Rust unit tests for the
  download flow remain in place and correct for when real per-artifact
  URLs are added.
- **A real Cargo build-script staleness bug was caught during installed-
  build verification of this pass.** `tauri_build::build()` (called from
  `desktop/src-tauri/build.rs`) only emits `cargo:rerun-if-changed` for
  `tauri.conf.json` itself, not for the *content* of the icon files that
  config references. After regenerating `icons/icon.ico` in place (same
  path, new bytes) and running a full `npm run tauri build`, the shipped
  `brute-desktop.exe` still carried the *previous* build's Windows icon
  resource - verified directly with `SHGetFileInfo` (the real API
  Explorer/taskbar/Start Menu use to read an exe's icon), not just a
  visual glance. Cargo saw no reason to re-run the build script (nothing
  it was told to watch had changed) even though `cargo build` reported a
  full recompile of the crate. Fixed two ways: (1) `build.rs` now
  explicitly emits `cargo:rerun-if-changed` for every file under
  `icons/`, so a future icon replacement reliably triggers a rebuild; (2)
  verified fixed by touching `build.rs` to force one rebuild and
  re-checking with `SHGetFileInfo` before shipping this pass's
  installers. Anyone hitting a similarly "the exe didn't pick up my new
  icon" symptom on an older checkout should `cargo clean -p
  brute-desktop --release` (or touch `build.rs`) once, then rebuild.
- **The critical "recommended model click navigates the whole WebView to
  a localhost error page" failure could not be reproduced from static
  analysis of the current tree** - every code path that opens an
  external URL (`Discover.tsx`'s one `openUrl` call site) already used
  `@tauri-apps/plugin-opener`, which launches the OS's real default
  browser as a *separate process* and was never capable of replacing
  the app's own WebView content, and no `localhost`/`window.location`/
  anchor-tag navigation exists anywhere else in the frontend source,
  the production `dist/` bundle, or the catalog data. Rather than leave
  this as an unresolved report, the fix applied is a structural,
  cannot-be-bypassed backstop regardless of root cause: a Rust-level
  `on_navigation` WebView guard (`desktop/src-tauri/src/lib.rs`)
  rejects any navigation attempt whose scheme/host isn't the app's own
  page origin, a frontend URL-safety check
  (`desktop/src/lib/urlSafety.ts`) rejects localhost/loopback/private/
  malformed URLs before `openUrl` is ever called, and model details now
  open in a real internal dialog (`desktop/src/components/Modal.tsx`)
  with Back/Close/Escape - so even an unidentified future regression in
  this class cannot trap the user. If the original report was from an
  older build of this codebase (predating the current `Discover.tsx`/
  `openUrl` implementation), this note should be read as "fixed at the
  architecture level," not "root cause confirmed."
- **A real, 100%-reproducible crash was found and fixed while
  investigating a "Discover Models opens cascading windows" report.**
  Dynamic testing in the installed NSIS build (UI Automation clicks +
  screenshot capture, since this environment has no interactive GUI
  access) found no actual native-window duplication - `Get-Process`/
  `EnumWindows` confirmed exactly one process and one window throughout;
  a one-off cascaded-title-bars screenshot turned out to be a transient
  DWM compositing artifact from the diagnostic script's own
  `SetForegroundWindow`/`ShowWindow` calls, gone on the very next
  capture. The real bug: clicking *any* model card blanked the entire
  window to solid black (confirmed via screenshot; the accessibility
  tree collapsed to ~18 generic elements, i.e. an unmounted React tree).
  Root cause: `desktop/src/lib/types.ts`'s `License` type
  (`{known:{identifier}} | "unknown"`) did not match the real Rust
  `#[serde(tag = "status", rename_all = "snake_case")]` wire format
  (`{"status":"known","identifier":...}` / `{"status":"unknown"}`,
  confirmed directly against `data/catalog/dev-catalog.json`) -
  `Discover.tsx`'s license ternary always took its "not unknown" branch
  (comparing an object to a string is always false) and read
  `.known.identifier` off an object that never has a `.known` property,
  throwing on *every* catalog entry with no Error Boundary anywhere to
  catch it, unmounting the whole app. Fixed by correcting the type, both
  call sites (the crash and a silently-broken "verified source only"
  filter that used the same wrong comparison), adding
  `desktop/src/components/ErrorBoundary.tsx` as a structural safeguard
  (a future render error of this class now shows a recoverable "Back to
  Overview" panel instead of blanking the app), and adding
  `desktop/src/structuralGuards.test.ts`, which scans every frontend
  source file for `WebviewWindow`/`window.open`/`target="_blank"`/etc.
  so a real multi-window regression would be caught even though this
  particular report wasn't actually one. Re-verified in a freshly
  rebuilt installed NSIS build: all 7 catalog cards (both license
  branches), rapid double-click, and a real OS-level Escape keypress all
  confirmed single-window, no crash, stable ~43 MB memory, no orphan
  processes.

## Console-window flashing (found and fixed)

A real, confirmed bug: `brute-desktop.exe` is correctly built as a
Windows GUI-subsystem app (`windows_subsystem = "windows"` in
`desktop/src-tauri/src/main.rs`), but every child process it launches -
`llama-cli.exe`/`llama-bench.exe` via `runtime::process::run`/
`run_streaming` (backend verification, tuning, benchmarking, local
generation - every single invocation), plus `nvidia-smi.exe`/
`vulkaninfo.exe` via `hardware::gpu` (hardware detection, run on every
app launch) - are console-subsystem binaries. Spawning a console-
subsystem child from a GUI-subsystem parent without `CREATE_NO_WINDOW`
makes Windows allocate and flash a new visible console window for that
child. Fixed by adding `runtime::process::configure_no_window` (applies
`CREATE_NO_WINDOW` via `std::os::windows::process::CommandExt::
creation_flags` on Windows, a no-op elsewhere) at every one of these
call sites, plus a structural guard test in both crates
(`no_window_flash_guard_test.rs`) that scans all source files for
`Command::new(` and fails if the same file doesn't also call
`configure_no_window`, so a future process launch can't silently
reintroduce the flash. `std::process::Command` has no getter for its own
Windows creation flags, so this can only be proven by dynamic testing in
an installed build, not a pure unit test - proven for real in a freshly
rebuilt installed NSIS build via parent-process-scoped monitoring
(`Get-CimInstance Win32_Process` walking the parent chain back to
`brute-desktop.exe`'s PID, plus `EnumWindows`/`IsWindowVisible` on every
detected descendant) while driving the app through Hardware-page GPU
detection and an actual local model generation (real prompt "hello"
against the real imported Qwen2.5-0.5B-Instruct model, streamed
end-to-end into the in-app output panel: "Prompt: 360.6 t/s |
Generation: 56.2 t/s"). Both `nvidia-smi.exe`/`vulkaninfo.exe` (Hardware
page) and the real `llama-cli.exe` run each spawned their normal/expected
`conhost.exe` host process (`CREATE_NO_WINDOW` suppresses the *window*,
not conhost creation itself, which is correct Windows behavior) but every
one of them had zero visible windows at any point in the monitoring
window - confirming no console ever flashed.
