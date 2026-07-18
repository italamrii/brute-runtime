# Library security

Extends `docs/security-model.md` for Stage 3. Every model file,
directory, catalog entry, library import, and metadata field is treated
as untrusted input.

## Path traversal and reparse-point escapes

`import_model`/`locate` both go through `security::validate_regular_file`
- the same Stage 0 primitive used for llama.cpp binary launches:
canonicalizes (resolving symlinks/junctions/reparse points to their
*real* target) and re-checks the resolved path, so a reparse point
redirecting outside an intended directory is rejected on the resolved
path, not the original one.

`remove_managed`'s managed-root boundary check
(`LibraryStore::remove_managed`) canonicalizes the entry's path and
verifies it starts with the managed library root **before** ever
calling `std::fs::remove_file` - tested with a synthetic
`managed_copy: true` entry pointing outside the root, confirming the
guard rejects it before any deletion is attempted:
`library::tests::remove_managed_rejects_a_path_outside_the_managed_root_even_if_flagged_managed`.

## Directory scan loop prevention

`library::scan::scan` tracks canonical directory paths already
descended into (`visited_dirs: HashSet<PathBuf>`) - a directory
reachable twice (e.g. via a junction looping back to an ancestor) is
only ever visited once, preventing infinite recursion regardless of how
the cycle was introduced. Verified live with a real Windows junction
(`mklink /J`), not just a hypothetical:
`library::scan::tests::directory_loop_via_junction_does_not_infinite_loop`.

Symlinked *files* are resolved through `metadata()` (which follows the
link) before being treated as regular files, and a symlink pointing at
a directory is never treated as something to descend into during file
discovery - only the explicit directory-recursion path (already
loop-guarded) descends into directories at all.

## Bounded, hostile-input-safe parsing

Reused unchanged from Stage 0: `models::gguf::inspect` caps KV count,
tensor count, string length, and array length before acting on any of
them, and never reads the tensor data blob. `library::scan`'s magic-byte
check reads exactly 4 bytes per candidate file, never more, before
classifying it. No file discovered by a scan or presented to import is
ever executed.

## No command injection surface

Nothing in the `library` module shells out to anything - there is no
subprocess launch anywhere in `library::*`. The only process-launching
code Stage 3 touches indirectly (via `associations`) is Stage 2's
already-audited `runtime::process`/`llama_cpp` argv-based launcher (see
`docs/security-model.md`), and only to *read* saved profile files, never
to launch anything itself.

## No privilege elevation, no antivirus/security-setting changes

Consistent with every prior stage: `library` never elevates, never
touches Windows Defender or any other security product, never modifies
registry security/performance settings, and never executes anything a
model file or catalog entry might embed.

## Unsafe metadata is contained

`alias`/`notes` are plain user-supplied strings, stored and round-tripped
through JSON exactly as given - never interpreted as a path, a command,
or executable content anywhere. Verified with deliberately unusual text
(quotes, backslashes, embedded newlines, a NUL byte, and a
path-traversal-*looking* string in `notes`):
`library::import::tests::alias_and_notes_with_unusual_characters_round_trip_safely`
confirms the text round-trips exactly and the entry's real
`current_path` is completely unaffected by whatever text ended up in
`notes`.

## Race conditions (TOCTOU)

`verify_entry`/`locate` re-derive everything (existence, size, hash)
from a single fresh read at the moment of the check, rather than
trusting a stale cached fact from an earlier step - the same discipline
Stage 0 established for binary hash verification immediately before
each launch. A file that changes between two separate `brute library`
invocations is caught by the *next* invocation's own fresh check, never
silently trusted from a prior one.
