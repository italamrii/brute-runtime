# Model file change detection

Two deliberately separate checks, both in `library::verify`:

| | `quick_file_status` (`brute library refresh`) | `verify_entry` (`brute library verify`) |
|---|---|---|
| Cost | Cheap - one `stat()`, no file contents read | Expensive - full GGUF re-parse + streaming SHA-256 |
| Confirms | Size/mtime match what was last recorded | Content hash actually matches |
| Result | `FileStatus` (heuristic) | `GgufVerification` (authoritative) + updated `FileStatus`/`trust` |

A `Modified` result from the cheap check is a **heuristic** - size or
mtime changed, which usually but not always means content changed (a
`touch` with no real edit would also trip it). Only the full verify
pass, which recomputes the hash and compares it to what's on record,
can confirm the file was genuinely `Replaced` with different content
versus merely `Unchanged` despite a metadata blip (in which case the
recorded size/mtime baseline is refreshed so it stops tripping the
cheap heuristic going forward).

## `FileStatus` states

- `Unchanged` - matches what was last recorded.
- `Modified` - cheap check flagged a size/mtime difference (not yet
  hash-confirmed).
- `Replaced` - full verify confirmed the hash actually differs.
- `Missing` - the file cannot be found at its recorded path.
- `Inaccessible` - exists but couldn't be opened/read (permissions,
  a transient lock).
- `Moved` - the *reported* outcome of a successful `brute library
  locate` call; the persisted entry immediately normalizes to
  `Unchanged` once the move is hash-confirmed, since from that point on
  it's simply the current valid location - see below.

## Invalidation

A changed file (`Modified`/`Replaced`) or a missing one immediately:

- Suspends `trust` to `ModifiedSinceVerification` (or `Missing`) - see
  `docs/trust-and-provenance.md`.
- Flags in `brute library audit`'s `stale_profiles`/`stale_calibrations`
  lists any Stage 2 runtime profile or calibration record that was
  keyed to this entry's *previously recorded* hash - see
  `docs/runtime-profile-association.md`. Verified end-to-end with a
  real saved runtime profile:
  `library::audit::tests::a_changed_entry_with_an_associated_runtime_profile_is_flagged_stale`.

**Historical metadata is never deleted immediately.** A changed or
missing entry stays in the library, visibly flagged, until the user
explicitly runs `brute library forget` (metadata only) or resolves it
via `brute library locate`/re-import.

## `brute library locate` - recovery

```
brute library locate <library-id> <new-path>
```

1. Validates `new-path` (`security::validate_regular_file`).
2. Computes its SHA-256.
3. Compares against the entry's recorded hash - **exact match
   required**.
4. On match: rebinds `current_path`, then runs a full `verify_entry`
   pass (confirming structure too, not just the hash) and returns
   `reported_status: Moved`.
5. On mismatch: rejected with `LibraryError::RelocationHashMismatch`,
   and **the entry is left completely untouched** - no partial update,
   no path change, nothing.

Verified: `library::verify::tests::locate_rebinds_the_path_when_the_hash_matches`,
`locate_rejects_a_mismatched_replacement_and_leaves_the_entry_untouched`.

Optional recovery search only ever scans a directory the user names
explicitly (there is no `locate`-driven whole-disk search) - consistent
with `docs/stage-3-trusted-local-library.md`'s "no automatic whole-disk
scan" rule.

## Real result on this machine

A copy of the real model was imported at a scratch path, the copy file
was then moved to a second scratch path (never the real model), and
`brute library locate` correctly recovered it: `verify` at the old path
first reported `trust: Missing`, then `locate` at the new path reported
`overall_integrity: Verified` and confirmed the associated Stage 2
runtime profile remained valid throughout, since association is by
content hash, never by path. See `docs/stage-3-verification.md` for the
full transcript.
