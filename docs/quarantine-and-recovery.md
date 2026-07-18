# Quarantine and recovery, and the three ways to remove an entry

## Three separate operations, deliberately never conflated

| Command | What it does | What it never does |
|---|---|---|
| `brute library forget <id>` | Removes the metadata entry | Touch the underlying file |
| `brute library remove-managed <id> --confirm` | Deletes a BRUTE-*managed* copy from disk and its entry together | Delete anything not inside the managed library root; delete an *external* file |
| *(external model deletion)* | **Not implemented** | — |

"Remove from library" is never "delete from disk." Verified:
`library::tests::forget_removes_the_entry_but_never_touches_the_file`
(writes a real file, forgets its entry, asserts the file is still
there).

### `remove-managed` today

Stage 3 does not implement managed copying (`--copy-into-library` is
skipped - see `docs/model-import-and-verification.md`), so
`LibraryEntry.managed_copy` is `false` for every entry this codebase
creates, and `remove-managed` always returns
`LibraryError::NotManaged` - honestly reporting "nothing to remove"
rather than a permanently-dead success path. The path-traversal guard
it would need is still real and tested: given a synthetic entry with
`managed_copy: true` pointing outside
`%LOCALAPPDATA%\BruteRuntime\library\`, the boundary check rejects it
with `PathEscapesManagedRoot` *before* attempting any deletion.
Verified: `library::tests::remove_managed_rejects_a_path_outside_the_managed_root_even_if_flagged_managed`.

### External model deletion is out of scope by design

The spec is explicit that Stage 3 must not implement deleting a
model the user manages themselves. There is no `brute library delete`
or equivalent command anywhere in this codebase - only `forget`
(metadata-only) and `remove-managed` (managed copies only, which don't
exist yet).

## Quarantine

A **metadata-only** hold - `brute library quarantine <id> --reason
<reason>` never moves the file, it only sets
`LibraryEntry.quarantine = Some(QuarantineInfo)`. Reasons include
malformed GGUF, truncated files, impossible tensor offsets, unsafe
metadata limits exceeded, a hash mismatch discovered by verification,
or an unsupported version - any concrete finding, supplied by the
caller as free text.

A quarantined entry is refused for further verification by the CLI
layer (`cmd_library_verify` skips it, reporting "skipped (quarantined -
run `brute library unquarantine` first)") - the intended integration
point for any future "benchmark"/"launch" command to consult before
acting on a library entry.

### Unquarantine can never bypass validation

```
brute library unquarantine <id>
```

**Always** runs a full `verify_entry` pass first, and only clears the
quarantine flag if `overall_integrity` comes back `Verified`. A
manual "just trust me" unquarantine does not exist - if the file is
still missing, corrupt, or the hash still doesn't match, the command
fails with `LibraryError::Quarantined` and the entry stays held.
Verified:
`library::tests::unquarantine_succeeds_only_after_a_passing_reverification`,
`unquarantine_refuses_to_clear_when_reverification_fails`.

## Real result on this machine

The real imported Qwen model was quarantined, confirmed excluded from
`brute library verify` while held, then unquarantined - which itself
ran a real re-verification (`overall_integrity=Verified`) before
clearing the flag. See `docs/stage-3-verification.md` for the full
transcript.
