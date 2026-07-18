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

## Honest summary

The desktop application builds, links, tests, and launches cleanly
end-to-end on real hardware, and its command layer has been proven
correct against this machine's real imported model and real saved
tuning profile - not synthetic data. What remains for full production
sign-off is a human (or future tooled session) actually clicking through
every workflow and capturing screenshots, which this session's toolset
cannot perform itself.
