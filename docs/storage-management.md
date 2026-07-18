# Storage management

`library::storage::summarize` is a **read-only analysis** - nothing in
this module deletes, moves, or reclaims anything.

## What's reported

```
brute library storage
```

- **Total entries / total bytes** - sum across every tracked artifact.
- **Available bytes** - sum for entries currently present and matching
  their last-recorded size/mtime (`FileStatus::Unchanged`).
- **Potentially reclaimable duplicate bytes** - sum, across every
  duplicate group, of every member's size *except* the one suggested as
  canonical. See below for why this is never presented as safe to
  reclaim outright.
- **Missing / corrupt counts.**
- **Largest models** - top 10 by size, deterministically ordered (ties
  broken by library ID).
- **Storage by architecture / by quantization** - byte totals grouped
  by GGUF metadata.
- **Managed vs external bytes** - always `managed_bytes: 0` today, since
  Stage 3 doesn't implement managed copying (`docs/model-import-and-verification.md`).

## Duplicate space is a potential estimate, never a safe-to-reclaim claim

The spec is explicit: "Do not present duplicate space as safely
reclaimable until the user confirms no external workflow depends on the
copy." `duplicate_bytes` is computed the same way
`docs/duplicate-detection.md` computes duplicate groups - it's a real
number (what you'd get back if you forgot every non-canonical copy),
but the CLI's human-readable output always pairs it with an explicit
caveat rather than a bare "reclaimable" label:

> Note: duplicate bytes are a potential reclaim estimate only - confirm
> no external workflow depends on a copy before running `brute library
> forget`.

Forgetting a library entry never deletes the underlying file anyway
(see `docs/quarantine-and-recovery.md`) - "reclaiming" duplicate library
*storage-tracking* bytes and actually freeing *disk* space are two
different things, and this command only ever does the former.

## Real result on this machine

```
$ brute library storage
Total entries: 1
Total bytes: 491400032
Available bytes: 491400032
Potentially reclaimable duplicate bytes: 0
Missing: 0  Corrupt: 0

Largest models:
  491400032 bytes - C:\Models\qwen2.5-0.5b-instruct-q4_k_m.gguf (model-instance-...)

By architecture:
  qwen2: 491400032 bytes

By quantization:
  MOSTLY_Q4_K_M: 491400032 bytes
```

Performance at scale (10,000 synthetic entries, measured directly - see
`docs/stage-3-verification.md`): storage summary in ~130ms, dominated by
one filesystem `stat()` per entry to check current availability - real
I/O cost, not fabricated.
