# Runtime profile and calibration association

`library::associations` connects a library entry to the Stage 2 runtime
profiles and Stage 1 calibration records that apply to it - **computed
fresh on every call, never persisted** on `LibraryEntry`.

## Why not persisted

Spec section 1 lists "associated runtime profiles" and "associated
calibration records" as fields to track. Storing a list of profile/
calibration IDs directly on the entry would create a second source of
truth that can drift: a profile deleted straight out of
`%LOCALAPPDATA%\BruteRuntime\profiles\` would leave a dangling reference
behind in the library index, silently wrong until something noticed.
Computing the association live from the model's SHA-256
(`associations::find_runtime_profiles_for`) or architecture/
quantization/parameter-count
(`associations::find_calibration_matches_for`) costs almost nothing -
both Stage 2's profile store and Stage 1's calibration store are small,
flat, locally-loaded JSON - and it can never be stale, by construction.

## How association works

- **Runtime profiles**: exact match on `RuntimeProfile.model_sha256 ==
  LibraryEntry.sha256`. Nothing fuzzy - either a saved profile was
  tuned for exactly this artifact, or it wasn't.
- **Calibration records**: reuses Stage 1's exact
  `calibration::find_nearest` lookup (the same bounded nearest-neighbor
  match `brute fit`/`brute recommend-model` already use), gated
  additionally on the *returned* record's own `backend` field matching
  the backend being asked about - `find_nearest` itself doesn't filter
  by backend (a mismatch only lowers its proximity score), so naively
  trusting its result per-backend would report the same single CPU
  record as a false match for CUDA and Vulkan too. This was a real bug
  caught during Stage 3 development - see `docs/known-limitations.md`.

## Moves preserve associations automatically

Since the association key is content hash, not path, a model relocated
via `brute library locate` (which only rebinds the path after
confirming the hash is unchanged) keeps exactly the same associated
profiles and calibration records before and after the move - nothing in
`associations` needs to know a move happened at all. Verified:
`library::associations::tests::a_moved_model_keeps_the_same_associated_profiles_since_association_is_by_hash`.

## Content changes invalidate associations

When a library entry's file actually changes (`FileStatus::Modified`/
`Replaced`), any profile/calibration record still keyed to its
*previously recorded* hash no longer describes the current file - see
`docs/model-file-change-detection.md` for the `stale_profiles`/
`stale_calibrations` audit flags this triggers, verified end-to-end with
a real saved profile:
`library::audit::tests::a_changed_entry_with_an_associated_runtime_profile_is_flagged_stale`.

## Duplicate artifacts

A profile associated with a given SHA-256 applies to *every* library
entry sharing that hash - `find_runtime_profiles_for` looks up by hash
alone, with no path involved, so all exact-hash copies are equally
"covered" by the same saved profile. Which physical file path actually
gets launched at runtime is resolved separately, at launch time, by
whichever future integration consumes a library entry - not a concern
this association layer needs to solve.

## Real result on this machine

`brute library show` on the real imported Qwen model reports
**"Associated runtime profiles: 1"** (the profile saved during Stage 2's
real `brute tune run`) and **"Associated calibration records: 1"** (the
seed calibration record from Stage 1) - both found live, by content
hash, with no manual linking step. See `docs/stage-3-verification.md`
for the full transcript.
