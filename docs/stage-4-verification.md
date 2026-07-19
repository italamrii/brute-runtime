# Stage 4 verification — what was actually run and observed

This is Stage 4's equivalent of `docs/stage-0-verification.md` through
`docs/stage-3-verification.md`: an honest record of what was actually
executed on real hardware for the desktop application, not a checklist
marked complete by assumption.

**Machine**: 13th Gen Intel Core i5-13450HX (10 physical / 16 logical
cores), 16 GB RAM, NVIDIA GeForce RTX 5050 Laptop GPU, Windows 11 Home
build 26200. **Model**: `C:\Models\qwen2.5-0.5b-instruct-q4_k_m.gguf`,
SHA-256 `74a4da8c9fdbcd15bd1f6d01d621410d31c6fc00986f5eb687824e7b93d7a9db`
(already imported from Stage 3 as library ID
`model-instance-12ddb13fc87b9f539bd5d87baab1c840`). **Saved profile**:
`profile-instance-f052be34d631ff2889844a7551eea020` (from real Stage 2
tuning).

## What was verified

1. **Lib/bin split preserves Stage 0-3 behavior.** `cargo test` at the
   repo root: 301/301 passed, `cargo fmt --check` clean, `cargo clippy
   --all-targets -- -D warnings` clean — both immediately after the split
   and again after every subsequent engine change in this stage (the
   streaming primitives, the three glue functions promoted to `pub`).

2. **The desktop backend compiles and lints clean.** `cargo build` /
   `cargo test` / `cargo fmt --check` / `cargo clippy --all-targets -- -D
   warnings` all pass inside `desktop/src-tauri/` — 17/17 Rust tests,
   zero clippy warnings.

3. **The frontend typechecks, lints, and builds.** `npm run build` (`tsc
   && vite build`) succeeds with zero TypeScript errors. `npm run lint`
   (eslint) reports 0 errors (2 harmless `react-refresh` warnings on
   files that intentionally co-locate a context provider and its hook -
   a standard React pattern, not a defect). `npm test` (vitest): 20/20
   frontend unit tests pass, covering confidence-badge labeling, i18n
   RTL/LTR switching and missing-key fallback, and the Models page's
   forget-vs-delete wording and quarantine badge.

4. **A real `cargo tauri dev` launch succeeded against real hardware.**
   The full dependency tree (417 crates including `tauri`, `windows-rs`,
   `wry`/`webview2-com`) compiled from a clean state in ~1m18s, and
   `brute-desktop.exe` launched and stayed running (confirmed via
   `tasklist`, PID present with a stable memory footprint) with no
   panic in the log. This is the real webview window, backed by the
   real Rust command surface - not the Vite dev server alone (which was
   separately confirmed to serve the correct HTML/JS on
   `localhost:1420`, but that alone doesn't exercise Tauri IPC).

5. **The desktop command layer was exercised against this machine's real
   local state**, not just against synthetic fixtures:
   - `commands::library::tests::library_list_shows_the_real_imported_model_with_its_real_hash_if_present`
     calls the exact `library_list` command function used by the
     Models page and asserts the real Qwen2.5-0.5B model appears with
     its real SHA-256 and a `current_path` that is a real file on disk.
   - `commands::profiles::tests::profiles_show_returns_the_real_saved_stage2_profile_if_present`
     calls the exact `profiles_show` command function used by the
     Profiles page and asserts the real Stage 2 profile loads with a
     genuine 64-character SHA-256.
   - Both tests are written to skip (not fail) on a machine without
     this local state, matching the pattern already established by
     `stage3_performance_test.rs` in the core crate.
   - Independently cross-checked via `cargo run --bin brute -- library
     list --json`, confirming the CLI and the desktop command layer
     read the exact same `%LOCALAPPDATA%\BruteRuntime\` state and agree
     on every field.

6. **Backend/model-path robustness was verified with deliberately bad
   input**, not just the happy path: `backends_verify` against a
   nonexistent model and a nonexistent binary directory returns an
   honest non-`Verified` status for all three backends rather than
   panicking; `tune_dry_run` against a nonexistent model path returns a
   clean `Err`; `profiles_show`/`profiles_delete` against an unknown
   profile ID return a clean `Err`, never a panic.

7. **UTF-8 chunk-boundary streaming was verified with a real split
   multi-byte Arabic character**, not just ASCII: `drain_utf8_prefix`'s
   test suite includes splitting the Arabic word "بيانات" mid-character
   across two simulated stdout chunks and confirming the two partial
   emissions reassemble to the exact original string.

8. **Production build succeeded end-to-end, twice, with a real bug
   caught and fixed in between.** `npm run tauri build` produced both an
   MSI (WiX) and an NSIS installer. The first pass revealed
   `bundle.resources` was not yet configured - a packaged install would
   have shipped with no catalog/calibration seed data - fixed and
   re-verified by an administrative MSI extraction confirming
   `data\catalog\dev-catalog.json` and `data\calibration\seed-calibration.json`
   are genuinely present alongside `brute-desktop.exe` in the installed
   payload. A real disk-space exhaustion (`os error 112`, ~128 MB free)
   was hit and resolved by clearing regenerable `target/debug/` build
   caches. See `docs/windows-packaging.md` for exact artifact sizes and
   SHA-256 checksums.

## What was not independently verified in this session, and why

- **Interactive UI screenshots and manual click-through testing** (spec
  section 24's screenshot requirement) were not captured. This agent
  session has no screen-capture or GUI-automation tool available, and a
  Tauri desktop window cannot be driven headlessly the way a browser-
  based Vite dev server can. The app was launched successfully (item 4
  above) and left running for manual inspection during this session, but
  a full manual walkthrough (onboarding → hardware → import → fit →
  tune → run a prompt → audit → export) needs to be performed by a human
  with the running app, or in a follow-up session with screen-capture
  tooling.
- **A real GPU-backend (CUDA/Vulkan) tuning run and a real streamed local
  prompt generation** were not executed end-to-end through the UI in
  this session (both require the pinned llama.cpp binary directory to be
  configured in Settings first, which is itself a manual first-run
  step). The underlying engine calls (`run_cli_streaming`,
  `run_candidate_repetition`, `determine_verified_gpu_backend`) are the
  same, already-tested Stage 0-2 primitives the CLI already exercises
  live - see `docs/stage-2-verification.md` for the CLI-side real tuning
  run and `docs/backend-verification.md` for the real CUDA/Vulkan
  detection-vs-verification distinction on this exact machine.
- **Arabic RTL rendering was verified by unit test (direction attribute
  and translated string content) but not by visual inspection** for the
  same screen-capture-tooling reason as above.

## Visual redesign closure pass (post–Stage 4 MVP)

A subsequent session preserved Claude’s unfinished `tokens.css` /
`global.css` redesign and finished wiring every page to that premium
near-black / charcoal / deep-red systems console language.

### Verified in the redesign session

1. **Frontend suite green:** `npm test` 20/20, `npm run lint` 0 errors
   (same 2 `react-refresh` warnings), `npm run build` (`tsc && vite
   build`) clean.
2. **Core engine unchanged and green:** root `cargo test` 301/301,
   `cargo fmt --check` clean, `cargo clippy --all-targets -- -D warnings`
   clean.
3. **Desktop crate green:** `desktop/src-tauri` `cargo test --tests`
   17/17, `cargo fmt --check` clean, `cargo clippy --all-targets -- -D
   warnings` clean.
4. **Privacy/network scan of frontend and desktop Rust sources:** no
   `fetch` / `axios` / WebSocket / analytics / crash-reporting SDKs.
   Official catalog URLs remain inert metadata strings only.
5. **Forget wording remains honest:** Models inspector button is
   “Remove from library”; helper text states the model file remains on
   disk (frontend test covers this).

### Packaging / interactive workflow

Production packaging (`npm run tauri build`) and a full interactive
click-through (including Arabic visual inspection and real Qwen local
run) may be re-confirmed in the same session after the build finishes —
checksums and installer paths will be recorded in
`docs/windows-packaging.md` when artifacts are on disk.

## Responsiveness pass: async command execution (post-redesign)

A real, previously reported symptom - the installed Windows build
becoming "Not Responding" - was root-caused and fixed in this session.

**Root cause**: Tauri v2 dispatches `#[tauri::command]` handlers on the
same thread that receives WebView2 IPC messages. A plain (non-`async`)
command that does real work - subprocess launches, file hashing, GGUF
parsing - blocks that thread for the full duration, freezing the window.
This is Tauri's own documented behavior, not a bug in Tauri itself.

**Fix**: every command that does disk I/O, hashing, GGUF parsing, or
launches a subprocess was converted to `async fn`, with the real work
moved onto a blocking worker thread via
`tauri::async_runtime::spawn_blocking`. Converted: all of
`commands/library.rs` (14 commands - import, scan, import_directory,
verify, refresh, audit, locate, alias, note, forget, quarantine,
unquarantine, quarantined, export, list, show, associations, duplicates,
storage), all of `commands/catalog.rs` (catalog_list/show,
calibrations_list, fit_evaluate, recommend_model, explain_fit - these
call `hardware::inspect`, which itself shells out to `nvidia-smi`),
`commands/hardware.rs::hardware_profile`, `commands/backends.rs::backends_verify`,
`commands/profiles.rs` (list/show/verify/export/delete), and
`commands/tuning.rs::tune_dry_run` (it calls
`determine_verified_gpu_backend`, which launches real short verification
subprocesses even in a "dry run"). `tune_run` and `local_run_generate`
were already async from the original Stage 4 build. Every command keeps
a thin `async fn` wrapper around a private, plain `_impl` function
specifically so the existing synchronous unit tests need no async test
runtime - no test coverage was lost or weakened by this refactor.

**Why this is safe for the frontend**: `invoke()` on the TypeScript side
always returns a `Promise` regardless of whether the underlying Rust
command is `fn` or `async fn` - this change is invisible to
`lib/api.ts` and every page that calls it. No frontend code changed as
a result of this fix.

**Verified**: 17/17 desktop Rust tests still pass, `cargo fmt --check`
and `cargo clippy --all-targets -- -D warnings` clean on the desktop
crate, root engine untouched and still 301/301 green, and a fresh
`cargo tauri dev` launch succeeded and stayed running (confirmed via
`tasklist`) with the new async code compiled in.

## Real run-state model (post-redesign)

The Run workspace's status display previously derived a coarse
idle/active/stopped/failed/complete label purely from `running`/`outcome`
booleans - no visibility into what was actually happening during a run.
This session added a `RunPhase` enum
(`commands/run.rs`) with the specific phases required for a serious
local-inference control workspace: `Preparing` → `ValidatingRuntime` →
`LoadingModel` → `Generating` → `Stopping` → `Completed`/`Failed`/`Cancelled`.
Every transition is driven by a genuine engine signal, never a timer:
`ValidatingRuntime` runs a real `verify_llama_binary` hash check (with a
real, distinct failure path if it fails, before anything is spawned);
`Generating` fires the moment the first real stdout byte arrives from
the child process; `Stopping` fires the moment cancellation is actually
observed inside the process-polling tick closure. Phases are pushed to
the frontend via a new `"local-run-phase"` window event, mirrored in
`lib/types.ts`'s `RunPhase` union, and drive `Run.tsx`'s status badge
directly (falling back to the old outcome-derived label only before the
first phase event of a run has arrived). The Run page was also given
explicit "Runtime binary" and "Memory estimate" rows, closing two gaps
against the required Run-page field list (selected model, selected
profile, runtime binary, backend, threads, GPU layers, context, batch,
memory estimate, measured throughput, run state, output stream, Stop,
failure recovery - all now present).

## Fresh privacy/network audit (post-redesign)

Re-ran the network/telemetry grep sweep across `desktop/src`,
`desktop/src-tauri/src`, and `src` after all of the above changes:
`fetch`, `axios`, `XMLHttpRequest`, `WebSocket`, `telemetry`,
`analytics`, `tracking`, `crash report`, `remote log`, `upload`,
`cloud`, hidden update checks. Every match was either UI copy stating
the no-upload/no-telemetry/no-cloud guarantee, a CSS class name
(`telemetry-strip`/`telemetry-item` - the Run page's live-metrics
tiles, an unrelated naming coincidence, not actual telemetry), or a doc
comment describing the same privacy guarantee. Confirmed zero network
crates in either `Cargo.toml` (root or desktop), zero analytics/CDN
packages in `desktop/package.json`, and no external URLs in
`index.html`/`vite.config.ts` beyond a single doc-comment link.

## Honest summary

The desktop application builds, links, tests, and launches cleanly
end-to-end on real hardware, and its command layer has been proven
correct against this machine's real imported model and real saved
tuning profile - not synthetic data. The visual redesign closes the
sparse Stage 4 card layout into a dense systems-console UI while
preserving all real engine metrics. A real UI-freeze root cause
(synchronous Tauri commands blocking the IPC/WebView2 thread) was found
and fixed by converting every I/O- or subprocess-touching command to
run on a blocking worker thread, and the Run workspace now surfaces a
genuine multi-phase execution state instead of a coarse
idle/active/done label. What remains for full production sign-off is a
human (or future tooled session) actually clicking through every
workflow and capturing screenshots, which this session's toolset cannot
perform itself.

## Cross-platform architecture, trusted runtime resolution, model discovery/catalog/download (final pass)

A subsequent pass in the same overall effort, after explicit user
direction to proceed with compile-only cross-platform work plus a set
of named release blockers, added:

1. **Cross-platform hardware detection**, isolated behind
   `src/platform/{windows,macos,linux}.rs`. Verified via real
   cross-target `cargo check`/`cargo clippy --all-targets -- -D
   warnings` for `x86_64-unknown-linux-gnu`, `x86_64-apple-darwin`, and
   `aarch64-apple-darwin` - all five checks passed with zero errors and
   zero warnings. **Not run on real macOS/Linux hardware** - see
   `docs/cross-platform.md` for exactly what compile-verified does and
   does not prove. A real bug this caught: `runtime::process`'s
   per-process peak-memory sampling used unconditional Win32 calls with
   no `cfg(windows)` guard - fixed with a real Linux implementation
   (`/proc/<pid>/status` `VmHWM`) and an honest `None` on macOS.
2. **Trusted runtime auto-resolution**
   (`commands::runtime::resolve_runtime`): bundled (strictly hash-
   verified) → advanced-settings override (lenient) → PATH-discovered
   (lenient) → actionable not-found state. The CPU-only pinned llama.cpp
   runtime is now bundled into the packaged app
   (`tauri.conf.json`'s `bundle.resources`) and was confirmed present -
   including its `.sha256` pin files - via a real MSI extraction (see
   `docs/windows-packaging.md`). The manual runtime-path text field
   moved to a new Settings → Advanced section; ordinary users no longer
   need to configure a path.
3. **Automatic model discovery** (`commands::discovery`): scans a
   fixed, bounded set of common local model locations (Downloads,
   Documents, LM Studio's and Ollama's documented model caches, and
   `C:\Models` on Windows) using the same bounded `library::scan::scan`
   the manual scan flow already used - never the whole disk. Wired into
   the first-run Onboarding screen, which now shows real discovered
   candidates (or the specified empty-state message) instead of
   discarding scan results.
4. **A "Discover Models" catalog page** reusing the existing
   `recommend`/`fit` engine (no new Rust logic needed) with
   Recommended/Compatible/Heavy/Not Recommended status per catalog
   entry, filters, and an explicit "Open official source" action.
5. **An explicit, user-triggered model download command**
   (`commands::download::download_model`) - the first network-capable
   code in the entire codebase, confined to the `brute-desktop` crate
   only (the core `brute` engine remains dependency-free of any
   HTTP/TLS crate). Streams to a `.partial` file, reports live progress,
   supports cancellation, computes a real SHA-256 of what was
   downloaded, and only atomically renames to the final destination on
   success. Pause/resume is a documented, deferred gap (see
   `docs/known-limitations.md`).

**Verified in this pass**: root engine 305/305 tests (up from 301 -
new `platform`/`hardware` test coverage), desktop crate 25/25 tests (up
from 17 - new `discovery`/`runtime`/`download` command tests), both
`cargo fmt --check` and `cargo clippy --all-targets -- -D warnings`
clean on both crates and on all three additional cross-compilation
targets, frontend `npm run build`/`npm run lint`/`npm test` all clean
(20/20 tests), a live `cargo tauri dev` session that picked up every
change via its file watcher and stayed running throughout, and a full
production `npm run tauri build` that produced a working MSI (23.5 MB)
and NSIS installer (13.2 MB) with the bundled runtime independently
confirmed present via MSI extraction - see `docs/windows-packaging.md`
for exact checksums. A real bug was caught and fixed mid-session by
this verification discipline: two test-fixture Windows paths written
via a shell heredoc lost their backslash escaping, which cross-checking
against a fresh `cargo build`/`cargo test` run caught immediately
(fixed with raw string literals).

Interactive screenshots and native macOS/Linux execution remain the
same documented gap as the rest of this file - this session's toolset
cannot capture them.
