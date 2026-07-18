# Privacy model — Stage 2

**Core product promise: your data never leaves your device.**

This document exists because Stage 2 introduces the first *persistent*
local state (saved tuning runs, runtime profiles) beyond the plain JSON
fixture files under `data/`. Everything below is either verified by a
test cited inline or is a direct, auditable property of the code (no
network client is linked into the binary at all — see
`docs/security-model.md` and `docs/stage-1-verification.md` §10 for the
zero-network-crate dependency audit, which Stage 2 does not change).

## What BRUTE never does (mandatory, all stages)

- No telemetry. No analytics. No network requests of any kind.
- No country/locale detection.
- No user accounts.
- No cloud database.
- No remote logging.
- No automatic uploads.
- No placeholder "data sharing" interfaces waiting to be turned on later.

There is no code path anywhere in this repository that opens a network
socket. This isn't a policy toggle that could be flipped — no HTTP/TLS/
socket crate is a dependency of the `brute` binary at all (`Cargo.toml`
has none), so there is nothing to disable.

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
