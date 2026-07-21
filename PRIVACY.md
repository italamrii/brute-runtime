# Privacy

**Core promise: بياناتك ما تطلع من جهازك — your data never leaves your device.**

This is a summary for GitHub visitors. The full technical detail —
verified against the source, not just asserted — lives in
[`docs/privacy-model.md`](docs/privacy-model.md) (engine/runtime state)
and [`docs/library-privacy.md`](docs/library-privacy.md) (the local
model library and what a sanitized export redacts).

## What BRUTE never does

- No user accounts, no sign-in.
- No telemetry, no analytics, no crash reporting, no remote logging.
- No automatic uploads. Uploads do not exist in this application at all.
- No background or automatic network requests of any kind.
- No country/locale detection.
- No cloud database or settings sync.

## The one, explicit exception: model download

The **only** network-capable code in the entire codebase is the
explicit, user-triggered model download in the desktop app
(`desktop/src-tauri/src/commands/download.rs`). It:

- Never starts without you clicking a specific "download this model"
  button, after seeing its size, license, and destination path.
- Only ever requests the exact URL you were shown — never a URL BRUTE
  constructs from your data.
- Shows live progress and is cancellable.
- Writes to disk only (via an atomically-renamed `.partial` file) —
  the downloaded file is never executed.
- Verifies the file's SHA-256 against the catalog's own curated
  checksum when one is available, deleting the file rather than
  keeping it on a mismatch — never silently trusting unverified
  content. Adding a downloaded file to your local library is always a
  separate, explicit action — nothing auto-imports.

The core `brute` engine crate (the CLI, and everything the desktop app's
backend wraps) has **zero** network dependencies at the `Cargo.toml`
level — there is nothing to disable there because nothing can connect
from that crate at all. This is enforced structurally, not just by
convention: see [`docs/security-model.md`](docs/security-model.md) and
the network-path audit in [`SECURITY.md`](SECURITY.md).

Opening a model's official source page (e.g. on Hugging Face) always
opens your system's default browser as a separate process — it never
loads inside the BRUTE window.

## Where local state lives

Everything BRUTE persists lives under `%LOCALAPPDATA%\BruteRuntime\` on
your own device — never inside this repository, never synced anywhere:

| Path | Contents |
|---|---|
| `instance-id` | A random local identifier (not derived from hardware) used only to associate your own saved profiles with each other |
| `profiles\` | Saved runtime tuning profiles |
| `library\index.json` | Your local trusted model library index |
| `preferences.json` | Your local model-recommendation preferences (language, task, priority, and advanced constraints) |
| `tune-status.json`, `tune-cancel-flag` | Best-effort tuning progress/cancellation state |

Uninstalling BRUTE does not delete this directory or any of your GGUF
model files — see [`docs/windows-packaging.md`](docs/windows-packaging.md).

## What we don't claim

We do not claim BRUTE is "100% private" or "completely secure" as a
blanket statement — no software honestly can make that claim. What we
claim is specific and verifiable: no code path in this repository sends
your data anywhere unless you explicitly trigger it, and that claim is
backed by an auditable source tree.

## Questions or a suspected privacy issue?

See [`SECURITY.md`](SECURITY.md) for how to report a concern privately.
