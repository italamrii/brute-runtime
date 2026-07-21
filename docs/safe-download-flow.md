# Safe verified download flow (Stage B.7)

`desktop/src-tauri/src/commands/download.rs` (backend) +
`desktop/src/pages/Discover.tsx` (the only frontend caller). Wires the
already-existing `download_model`/`cancel_download` Tauri commands
(built in an earlier pass, previously called from nowhere - see
`docs/known-limitations.md`'s history) up to a real "Download" button,
adding checksum verification and a disk-space preflight check.

## Where the button appears

Only in the Discover Models details dialog, and only when the
catalog's own `exact_artifact_url` is set - i.e. `artifact_verification`
is `ArtifactUrlVerified` or higher (`docs/model-catalog-schema.md`).
Every other entry still offers only "Open official source." The button
never appears based on `official_source_url` alone - that field is a
landing page, not a direct file link, and treating it as one was
explicitly ruled out in Stage B.6.

## Flow

1. **Choose a destination** - `save({ defaultPath: build.filename })`
   (`@tauri-apps/plugin-dialog`, the same dialog `Profiles.tsx` already
   uses for exports) opens the OS's native save dialog. Cancelling it
   returns the user to the details view with nothing started - no
   network call has happened yet.
2. **Disk-space preflight** (`check_download_space`) - queries real
   free space at the destination via `hardware::system::inspect_storage`
   (`GetDiskFreeSpaceExW` on Windows), compared against the catalog's
   own `file_size_bytes` plus a 256 MiB safety margin
   (`DOWNLOAD_SPACE_SAFETY_MARGIN_BYTES`). Shown before the user can
   start the download - "not enough space" is a warning, not a silent
   block, since the query can itself be wrong (a symlinked/mapped
   drive, a network share) and the write will still fail cleanly if it
   really doesn't fit.
3. **Download** (`download_model`) - unchanged network/streaming
   behavior from the original implementation: streams to
   `<destination>.partial`, emits `download-progress` events (bytes
   downloaded, total from `Content-Length` if the server sends one),
   supports cancellation via `cancel_download` (cooperative, checked
   between chunks - deletes the `.partial` file, never leaves a
   truncated file at the real destination).
4. **Checksum verification** - new this stage. When the catalog build
   has `checksum_algorithm == "sha256"` and a `checksum_value`, the
   frontend passes it as `expected_sha256`. The backend computes the
   real SHA-256 of the downloaded bytes and compares *before* the
   rename to the final destination:
   - Match → renamed, `checksum_verified: true`.
   - Mismatch → the `.partial` file is deleted, nothing is saved at the
     destination, `succeeded: false`, `checksum_verified: false`, and a
     clear error message - never a silent fallback to "trust it
     anyway."
   - No expected value available (most catalog entries today) → the
     download still succeeds and is kept, but `checksum_verified` is
     `null` and the UI says so explicitly ("No independent checksum was
     available to verify this file against") - never upgraded to a
     false "verified."
5. **Result** - success shows the final path, the computed SHA-256, the
   checksum-verified status, and an explicit "Add to library" button
   (calls the existing `library_import` command - the download flow
   never auto-imports). Cancellation and failure are shown as distinct,
   honest states, each with a "Retry" action that re-opens the
   destination picker rather than silently reusing stale state.

## What stayed the same

- `ureq` remains the only network dependency in the entire codebase,
  scoped to `brute-desktop` only - the core `brute` engine crate still
  has zero network dependencies (`docs/privacy-model.md`).
- Only `http://`/`https://` URLs are ever passed to `ureq`
  (`is_supported_url`) - unchanged from the original implementation.
- A cancelled or failed download never leaves a truncated or
  unverified file at the real destination path - only ever at the
  `.partial` path, which is always cleaned up.
- No automatic downloads: the button requires an explicit click, the
  destination requires an explicit picker confirmation, and the
  library import after a successful download requires its own explicit
  click.

## Tests

Rust (`desktop/src-tauri/src/commands/download.rs`): checksum
comparison (case-insensitive match/mismatch), disk-space sufficiency
math (unknown-free-space stays honestly unknown, the safety margin is
actually required, a clearly-too-small disk is flagged) - all as pure,
I/O-free unit tests. Frontend (`desktop/src/pages/Discover.test.tsx`,
"safe verified download flow" describe block): button visibility gated
on `exact_artifact_url`, destination-picker cancellation, the
disk-space check and its insufficient-space warning, the expected
checksum being passed through correctly (including the null case),
success with checksum-verified status and library import, an honest
checksum-mismatch failure, cancel-in-progress, and Retry from both a
failure and a cancellation.

## Known limitations

- Pause/resume (HTTP range-request resumption) is not implemented - a
  cancelled or failed download must restart from zero.
- Only SHA-256 is verified. A catalog entry with a different
  `checksum_algorithm` value is treated the same as having none (no
  comparison attempted) rather than being rejected outright - no
  catalog entry uses a non-SHA-256 algorithm today.
- The disk-space preflight check and the actual write can still race
  (another process fills the disk in between) - the write-time error
  path (already in place before this stage) is the real backstop.
