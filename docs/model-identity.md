# Model identity

**A local file path is not a model identity.** Two files at different
paths can be byte-identical; one file renamed is still the same model;
two files that happen to share a filename can be completely different
artifacts. Stage 3 identifies models by content, not by where they
happen to sit on disk.

## The identity hierarchy

| Level | What distinguishes it | Example |
|---|---|---|
| **Model family** | A human grouping of related releases | "Qwen2.5 0.5B Instruct" |
| **Exact model build** | Architecture + quantization + parameter count (a specific catalog entry, when curated) | `qwen2.5-0.5b-instruct-q4_k_m` |
| **Exact local file artifact** | Full SHA-256 of the actual bytes on disk - the authoritative identity | `74a4da8c9f...93d7a9db` |
| **Local library entry** | One tracked `(artifact, path)` binding - `library_id` | `model-instance-12ddb13f...` |

The **artifact** (full SHA-256) is authoritative. Everything else -
GGUF architecture/quantization/parameter count, a catalog match, a
user-supplied alias - is metadata *about* that artifact, never a
substitute for it. `library::LibraryEntry.sha256` is computed by
streaming the whole file through SHA-256
(`security::hashing::sha256_file` - the same Stage 0 primitive used for
llama.cpp binary verification), never inferred from a name, a size
alone, or a directory location.

## What this means in practice

- **Renamed files are not new models.** `brute library import` on a
  file whose content hash already exists under a different path reports
  it as a duplicate of the existing entry (`ImportOutcome.duplicate_of`)
  - it does not silently create an unrelated-looking second identity.
  Verified: `library::import::tests::importing_a_copy_at_a_different_path_reports_it_as_a_duplicate`.
- **Same-name, different-content files are never merged.** Two files
  both called `model.gguf` in different directories get two separate
  library entries with two different hashes the moment their bytes
  differ even slightly. Verified:
  `library::import::tests::same_filename_different_content_are_never_merged`.
- **A moved file keeps its identity.** `brute library locate` only ever
  rebinds a tracked entry's path after confirming the candidate file's
  hash matches the *original* artifact's hash exactly - see
  `docs/model-file-change-detection.md`.
- **Local library entries never leak into shareable output as raw
  paths.** `LibraryEntry.current_path` is real, unredacted local state
  (BRUTE needs it to actually locate the file); `brute library export`
  always replaces it with the literal placeholder `<LOCAL_MODEL_PATH>`
  first - see `docs/library-privacy.md`.

## Real example

On this machine, `C:\Models\qwen2.5-0.5b-instruct-q4_k_m.gguf` has
artifact identity `74a4da8c9fdbcd15bd1f6d01d621410d31c6fc00986f5eb687824e7b93d7a9db`
- the exact same hash Stage 0's `brute model inspect`, Stage 1's
`brute fit`, and Stage 2's `brute tune run` all independently computed
against this same file, confirmed identical across every stage (see
`docs/stage-3-verification.md`). Its library entry ID,
`model-instance-12ddb13fc87b9f539bd5d87baab1c840`, is a separate,
random, non-identifying local label (same generation mechanism as
Stage 2's `local_instance_id` - see `identity::generate_random_id`) -
never derived from the content hash, the path, or anything about the
machine.
