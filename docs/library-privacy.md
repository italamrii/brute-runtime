# Library privacy

Extends `docs/privacy-model.md` for Stage 3. Same mandatory list applies
unchanged: no telemetry, no analytics, no country detection, no user
accounts, no cloud database, no remote logging, no model uploads, no
remote scanning, no automatic downloads, no background network access,
no placeholder sharing architecture, no remote catalog sync, no silent
opening of external links.

## Where library state lives

`%LOCALAPPDATA%\BruteRuntime\library\index.json` -
alongside Stage 2's `profiles\` and `instance-id`, under the same
`identity::default_local_state_dir()` root, entirely outside this git
repository. Never uploaded, synced, or transmitted by anything in this
codebase.

## Local private state contains real paths - by necessity, never by accident

`LibraryEntry.current_path` and `original_import_path` hold real,
unredacted local filesystem paths in the private index file, because
BRUTE genuinely needs them to locate and eventually launch a model. This
is the same "local private state may legitimately contain what public
exports must never contain" split established for Stage 2's runtime
profiles (which store no paths at all) and Stage 1's `machine_id`
(kept internal, redacted on export) - see `docs/privacy-model.md`.

## What `brute library export` redacts

```
brute library export --output <file>
```

Every entry is passed through `sanitize_entry_for_export` before
serialization:

- `current_path` → the literal placeholder `<LOCAL_MODEL_PATH>`
  (`library::REDACTED_PATH_PLACEHOLDER`).
- `original_import_path` → the same placeholder, if it was set.

Nothing else changes - the content hash, GGUF metadata, trust/file
status, catalog match, and any alias/notes text the user explicitly
attached are preserved, since none of those are personally identifying
on their own. Verified:
`library::tests::sanitize_for_export_replaces_paths_and_nothing_else`.

## `machine_id` and `local_instance_id` don't even appear here

Unlike Stage 2's `RuntimeProfile` (which stores the coarse `machine_id`
internally and must actively redact it on export), `LibraryEntry` has
**no machine-identity field at all** - a model's identity is purely its
content hash and GGUF metadata, which says nothing about the machine it
happens to be sitting on. There is nothing to redact because nothing
identifying was ever recorded. Confirmed on the real exported library:
grepping the real machine's exported `library-export.json` for
`machine_id`/`local_instance_id` matches nothing (0 occurrences) - see
`docs/stage-3-verification.md`.

## Privacy concerns in *user-supplied* metadata

Since `alias`/`notes` are free text the user chooses to write, they
*could* accidentally contain a path or username (e.g. an alias pasted
from a file explorer breadcrumb). `brute library audit` proactively
flags this as a `privacy_concerns` finding - a heuristic check
(`looks_like_a_filesystem_path`) for text resembling
`C:\Users\...`/`\AppData\...` or a generic `drive:\path` shape - purely
informational, never auto-redacted (the user's own free text is theirs
to edit), and never exported without the user seeing the audit warning
first. Verified:
`library::audit::tests::a_path_like_alias_is_flagged_as_a_privacy_concern`,
`an_ordinary_alias_is_never_flagged`.

## Catalog source URLs remain inert

Exactly as established in Stage 1: `official_source_url` is stored and
displayed as plain text. Nothing in `library` (or anywhere else in this
codebase) ever opens it automatically.

## No network required, confirmed

Every `brute library` command operates entirely on local files - no
dependency in `Cargo.toml` provides network I/O (confirmed by the same
dependency audit maintained since Stage 1 - see
`docs/stage-1-verification.md` §10), so there is no code path that
*could* make a network request, not merely a policy against it.
