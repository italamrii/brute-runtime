# Privacy model — Stage 2 through Stage 4

**Core product promise: your data never leaves your device.**

This document exists because Stage 2 introduces the first *persistent*
local state (saved tuning runs, runtime profiles) beyond the plain JSON
fixture files under `data/`. Everything below is either verified by a
test cited inline or is a direct, auditable property of the code.

## The one, explicit, user-triggered exception: model download

The core `brute` engine crate still has zero network dependencies -
that has not changed (see `docs/security-model.md` and
`docs/stage-1-verification.md` §10 for the dependency audit). The
desktop app's `commands/download.rs` module is the **only** place in
the entire codebase that makes a network request, and it exists
specifically to let a user download a catalog model they explicitly
chose, from its official source URL, with the destination path and
size shown before anything happens. It never runs automatically:

- No download starts without a user clicking a specific "download this
  model" button after seeing its size, license, and destination path.
- The default application behavior is offline - nothing calls this
  command on startup, on a timer, or as a side effect of any other
  action.
- The request goes to exactly the URL shown in the catalog entry's
  `official_source_url` field, or the URL the download command was
  explicitly given - never a URL BRUTE constructs from user data or
  telemetry.
- Progress, cancellation, and the real computed SHA-256 of what was
  downloaded are all shown to the user - nothing about the transfer is
  hidden.
- The downloaded file is only ever written to disk (via a `.partial`
  file, atomically renamed on success) - never executed.
- `ureq` (the HTTP client) is a dependency of the `brute-desktop` crate
  only, never of the core `brute` engine - every other command in the
  application (hardware inspection, library management, tuning, local
  generation) remains exactly as network-free as it always was.

See `docs/security-model.md` for the full IPC/command-boundary analysis
of this one command.

## What BRUTE never does (mandatory, all stages)

- No telemetry. No analytics. No background/automatic network requests.
- No country/locale detection.
- No user accounts.
- No cloud database.
- No remote logging.
- No automatic uploads (uploads do not exist in this application at all).
- No placeholder "data sharing" interfaces waiting to be turned on later.

Outside of the one explicit, user-triggered model download described
above, there is no code path anywhere in this repository that opens a
network socket. The core engine crate still has no HTTP/TLS/socket
dependency at all (`Cargo.toml` has none) - there is nothing to
disable there because nothing can connect from that crate in the first
place.

## Where Stage 2 state actually lives

Everything Stage 2 persists lives under `%LOCALAPPDATA%\BruteRuntime\`
(`identity::default_local_state_dir()`), entirely outside this git
repository:

| File/dir | Contents | Written by |
|---|---|---|
| `instance-id` | The random local instance ID (below) | `identity::load_or_create_local_instance_id` |
| `profiles\<id>.json` | Saved runtime profiles (see `docs/runtime-profile-schema.md`) | `tuning::runtime_profile::save_profile_to` |
| `tune-status.json` | Best-effort progress for `brute tune status` | `main.rs`'s `write_tune_status` |
| `tune-cancel-flag` | Presence = a cancellation was requested | `brute tune cancel` |

None of these are ever read, written, or transmitted by any code path
other than the `brute` CLI itself running locally. Nothing under this
directory is committed to git, uploaded, or synced anywhere by BRUTE.

## `machine_id` vs `local_instance_id` — two different things, deliberately

Stage 1 introduced `profile::HardwareCapabilityProfile.machine_id`: a
hash of coarse, already-non-sensitive aggregate specs (CPU brand string,
core counts, RAM rounded to the nearest GiB, OS build number — see
`profile::compute_machine_id`). It is **not** a hardware serial number or
MAC address, but it **is** fully deterministic from hardware alone, which
means it *is*, in the technical sense, a stable identifier: run `brute`
twice on the same machine and you get the same `machine_id` both times,
including across a reinstall.

That's fine for `machine_id`'s original purpose — matching a saved
calibration record or runtime profile back to "the same machine it was
tuned on" — but it is exactly the property a *shareable* identifier must
not have ("must not create a stable cross-install fingerprint"). Stage 2
resolves this tension by keeping both, with strictly separated roles:

- **`machine_id`** (Stage 1): kept for internal/local matching only —
  `tuning::runtime_profile::check_still_valid` uses it to detect "this
  profile was tuned on a different machine." Terminal-only display;
  `profile::redact_machine_id_for_export` and
  `tuning::runtime_profile::sanitize_for_export` always strip it from
  anything written to a file or printed with `--json`, replacing it with
  the literal string `"omitted-from-export"`
  (`profile::REDACTED_MACHINE_ID_PLACEHOLDER`).
- **`local_instance_id`** (Stage 2, `identity` module): a genuinely
  random 128-bit value generated via `BCryptGenRandom` (the Windows CNG
  system RNG — a direct OS call, not a bundled PRNG crate), persisted to
  `instance-id` and reused until reset. It identifies nothing about the
  hardware and carries no meaning across machines — two runs on two
  different machines are exactly as likely to differ as two runs on the
  same one. This is what appears in `brute profile create`'s shareable
  JSON/file output instead of `machine_id`.

Verified: `identity::tests::generates_a_well_formed_random_id`,
`two_generated_ids_are_different`,
`profile::tests::redact_machine_id_for_export_replaces_the_real_id_and_nothing_else`,
`tuning::runtime_profile::tests::sanitize_for_export_redacts_machine_id_and_nothing_else`.

Reset it any time with `brute privacy reset-id` — this deletes the file
and a fresh, unrelated ID is generated on next use
(`identity::tests::reset_causes_a_brand_new_id_to_be_generated`).
`brute privacy show-id` prints the current one.

## What a saved runtime profile does and does not contain

See `docs/runtime-profile-schema.md` for the full field list. In summary:
a profile identifies the **model** by its SHA-256 content hash (never its
file path) and the **llama.cpp binaries** by their own SHA-256 (never a
version string that might embed a username-bearing build path). No field
in `RuntimeProfile` is a filesystem path. `brute profiles export` always
calls `sanitize_for_export` before writing, redacting `machine_id`.

## Backend verification and tuning never phone home

`brute backends verify` and `brute tune run` only ever launch local
subprocesses (`llama-cli.exe`/`llama-bench.exe`) via the same argv-based
`runtime::process::run` primitive Stage 0 established — see
`docs/security-model.md` for the command-injection analysis, which
applies unchanged. No result from either command is sent anywhere; it is
printed to the terminal and/or written to a local file the user names
explicitly.

## Stage 3: the local model library

`%LOCALAPPDATA%\BruteRuntime\library\index.json` joins `profiles\` and
`instance-id` under the same local-only root. Full detail (what's
stored, what's redacted on export, the "local private state may
legitimately contain real paths" split) lives in its own dedicated
document: `docs/library-privacy.md`. In short: `LibraryEntry` has no
machine-identity field at all (a model's identity is content hash plus
GGUF metadata, which says nothing about the machine), so there is
nothing to redact there beyond the real filesystem paths themselves -
`brute library export` replaces both `current_path` and
`original_import_path` with the placeholder `<LOCAL_MODEL_PATH>` before
writing anything.

## Stage 4: the desktop app changes none of the above

The Tauri desktop shell is a UI over the exact same engine and the exact
same `%LOCALAPPDATA%\BruteRuntime\` local state - it does not introduce
a second storage location, a settings-sync mechanism, or any new
persistent state beyond two small, inert, local-only additions:

- `localStorage` (inside the webview, never sent anywhere) for two pure
  UI preferences: the selected language (`brute.language`) and the
  configured llama.cpp binary directory (`brute.llamaBinPath`) - both
  device-local browser storage, not part of the engine's state, not
  exported, and cleared by "Reset local state" in Settings.
- In-memory only (cleared on app restart): which model/profile is
  currently selected, for the status bar and Run workspace
  (`lib/AppStatusContext.tsx`) - never written to disk at all.

No Tauri *plugin* with network capability (an auto-updater, a generic
HTTP-client plugin, etc.) is a dependency of
`desktop/src-tauri/Cargo.toml`. **UPDATED (Stage B.7):** `ureq` (a
plain Rust HTTP client crate, not a Tauri plugin) was added as a
`brute-desktop`-only dependency, used from exactly one place -
`desktop/src-tauri/src/commands/download.rs`'s `download_model`
command - and only ever reachable via an explicit user click on a
"Download" button that itself only renders when a catalog entry's
`exact_artifact_url` has been independently verified (see
`docs/safe-download-flow.md`). The root `brute` engine crate still has
zero network dependencies of any kind - confirmed by inspecting its
`Cargo.toml` directly, not just by convention. The desktop app
otherwise retains the same "nothing to disable because nothing can
connect" property Stage 0-3 established for the CLI - see
`docs/security-model.md` "Stage 4: Tauri desktop security boundary"
for the IPC-layer analysis.

## Stage B.10 audit (2026-07-21)

A dedicated, repository-wide sweep (not just the incremental per-
commit checks each Phase B stage already ran) - grepped
`desktop/src`, `desktop/src-tauri/src`, and the root `src` for network
APIs (`fetch`, `XMLHttpRequest`, `WebSocket`, `axios`, telemetry/
analytics SDK names), `localhost`/loopback references, `target=_blank`/
`window.open`, and both crates' `Cargo.toml` dependency lists; also
checked `tauri.conf.json`'s CSP and `capabilities/default.json`'s
permission set, and re-confirmed `api.ts` is the only file that calls
Tauri's `invoke()` (`grep -rl invoke desktop/src` returns exactly
`lib/api.ts` and the test-mock setup file, nothing else).

Findings: the only network-capable path is `ureq` inside
`download_model`, already documented above and in
`docs/safe-download-flow.md`; every `localhost`/loopback match is
either `urlSafety.ts`'s rejection list, its tests, or the Tauri
navigation guard's own allow-list check (which *rejects* everything
except the app's own origin - see `desktop/src-tauri/src/lib.rs`); no
`target=_blank`/`window.open` anywhere (also enforced structurally by
`structuralGuards.test.ts`); the CSP (`connect-src 'self' ipc:
http://ipc.localhost`) permits no external origin; `capabilities/
default.json` grants only `core:default`, `opener:default`, and the
two dialog open/save permissions - no shell, no filesystem plugin, no
updater. One cosmetic fix applied: a test fixture in
`TechnicalValue.test.tsx` had the developer's real local Windows
username hardcoded into an example path string; replaced with a
generic placeholder (no functional change, nothing was ever an actual
secret - it was always a synthetic example path, not a real
credential).

## Residual honesty notes

- `tune-status.json` and `tune-cancel-flag` are written best-effort
  (`write_tune_status` silently ignores I/O errors) so a permissions
  problem writing a *convenience* progress file can never abort an
  actual tuning run. This means `brute tune status` can occasionally be
  stale or absent even while a run is in progress — it is documented as
  informational, not authoritative.
- `machine_id` collision across two different machines with identical
  coarse specs remains an accepted, documented Stage 1 tradeoff (see
  `docs/security-model.md`) — Stage 2 does not change this, and does not
  rely on `machine_id` being globally unique, only "matches this local
  machine or it doesn't."
