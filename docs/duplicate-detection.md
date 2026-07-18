# Duplicate detection

`library::duplicates::find_duplicate_groups` groups library entries by
**SHA-256 only**. Filename similarity is never treated as evidence of
identity - see `docs/model-identity.md`.

## What counts as a duplicate

Two (or more) library entries whose `sha256` fields are byte-identical
- regardless of filename, directory, or import time. This automatically
covers every case the spec asks for:

- **Exact duplicates**: same bytes, different paths.
- **Renamed identical files**: same bytes under a name that doesn't
  match the original - identity follows content, so this is just
  another instance of "same hash, different path."
- **Probable variants from the same family**: *not* grouped as
  duplicates unless the bytes are actually identical - a Q4_K_M and a
  Q5_K_M build of the same model are different artifacts with different
  hashes, correctly kept as separate entries, not conflated.
- **Incomplete temporary duplicates**: a partially-copied file has
  different bytes (and usually fails GGUF parsing outright, so it never
  even reaches the point of having a comparable hash) - never grouped
  with the complete original.

## Output shape

```json
{
  "sha256": "74a4da8c9f...93d7a9db",
  "canonical_library_id": "model-instance-...",
  "members": [
    { "library_id": "...", "path": "...", "size_bytes": 491400032, "file_status": "unchanged", "available": true }
  ],
  "suggested_action": "2 identical copies found (sha256 74a4da8c9f8b...). Consider keeping model-instance-... and running `brute library forget <id>` on the others once you've confirmed no external workflow depends on the copy - BRUTE never deletes or removes duplicates automatically."
}
```

`canonical_library_id` is a **suggestion**, never an automatic action:
it prefers a currently-available (`FileStatus::Unchanged`) member, and
otherwise the earliest imported. `suggested_action` always names the
exact `brute library forget <id>` command a user would run - never
"and BRUTE deleted the rest," because it never does.

## Determinism

Group membership and ordering are fully deterministic: members within a
group are sorted by import time then library ID, and groups themselves
are sorted by hash. Two calls against the same store produce
byte-identical output. Verified:
`library::duplicates::tests::grouping_is_deterministic_across_repeated_calls`.

## Real result on this machine

Live-testing imported a byte-identical copy of the real Qwen model at a
second path and confirmed it was immediately reported as a duplicate of
the original entry (`ImportOutcome.duplicate_of`), then confirmed
`brute library duplicates` grouped both under one hash with the
original correctly picked as canonical. The test copy and its (`forget`
-only, never delete) entry were removed afterward - see
`docs/stage-3-verification.md`.
