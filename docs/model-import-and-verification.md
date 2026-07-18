# Model import and verification

## The import pipeline (`library::import::import_model`)

1. **Validate the path** - `models::inspect_model` delegates to
   `security::validate_regular_file` (the same Stage 0 primitive used
   for llama.cpp binary launches): canonicalizes, resolves reparse
   points, rejects directories/empty files.
2. **Bounded GGUF parse** - `models::gguf::inspect` streams only the
   header/metadata/tensor-info tables, with hostile-input sanity caps
   (max KV count, max tensor count, max string/array length) already
   established in Stage 0 - never the tensor data blob.
3. **Streaming SHA-256** - `security::hashing::sha256_file`, 1 MiB
   chunks, never loads the whole file into memory.
4. **Catalog match** (`import::match_against_catalog`) - see
   `docs/trust-and-provenance.md` for the confidence tiers.
5. **Full verification** (`verify::verify_artifact`) - header/structure/
   hash checks, producing the `GgufVerification` result recorded on the
   entry.
6. **Create or update the library entry** - a fresh path creates a new
   entry; re-importing an already-tracked path updates it in place
   (same `library_id`), never duplicating.
7. **Report exactly what happened** - `ImportOutcome` always states
   whether the entry was new, what it duplicates (if anything), and the
   full verification/catalog-match results - never a bare "imported."

## What import never does

Execute, upload, copy, modify, rename, or move the file; download
anything; call any remote API. Verified directly:
`library::import::tests::import_never_modifies_the_source_file` (a
byte-for-byte comparison of the file before and after import).

## Managed-copy mode is not implemented

The spec offers `--copy-into-library` as an optional feature, explicitly
permitting it to be skipped "if it adds unnecessary complexity." Stage 3
skips it: every entry operates on the file's existing location, and
`LibraryEntry.managed_copy` is `false` for every entry this codebase
creates. `brute library remove-managed` and the path-traversal guard it
would need both still exist (see `docs/quarantine-and-recovery.md`) -
honestly reporting "not a managed entry" today rather than a dead
success path, and ready without restructuring if managed copying is
added later.

## `GgufVerification` - never one boolean

```json
{
  "header": "verified",
  "structure": "verified",
  "sha256": "verified",
  "runtime_load": "not_tested",
  "benchmark": "not_tested",
  "catalog_match": "weak",
  "overall_integrity": "verified",
  "trust": "local_unverified_source"
}
```

Each field is independently one of `verified`/`failed`/`not_tested`/
`unavailable` (`CheckState`), except `catalog_match` (its own 5-tier
confidence) and `overall_integrity`/`trust` (their own vocabularies -
see `docs/trust-and-provenance.md`). `runtime_load` and `benchmark` are
always `not_tested` at the library layer - the library never launches
llama.cpp itself; that remains Stage 2's `backends`/`tuning` machinery,
associated after the fact by content hash (`docs/runtime-profile-association.md`).

### Error-path honesty

| `gguf::inspect` result | `header` | `structure` | `overall_integrity` | `trust` |
|---|---|---|---|---|
| Bad magic | `failed` | `failed` | `corrupt` | `Corrupt` |
| Unsupported version | `verified` (magic *was* valid) | `unavailable` | `unknown` | `Unsupported` |
| Any other parse error (truncated, hostile count, malformed) | `verified` | `failed` | `corrupt` | `Corrupt` |
| Parses cleanly, hash matches (or no expected hash yet) | `verified` | `verified` | `verified` | `CatalogMetadataMatched` or `LocalUnverifiedSource` |
| Parses cleanly, hash *mismatches* an expected value | `verified` | `verified` | `suspect` | `ModifiedSinceVerification` |

An "unsupported version" is deliberately never reported as `corrupt` -
the file may be perfectly intact, just a GGUF revision this codebase
doesn't parse yet. Verified:
`library::verify::tests::unsupported_version_is_distinguished_from_corrupt`.

## Real result on this machine

Importing `C:\Models\qwen2.5-0.5b-instruct-q4_k_m.gguf` produced
`overall_integrity: Verified`, `trust: LocalUnverifiedSource`, and a
`Weak` catalog match - see `docs/trust-and-provenance.md` for exactly
why (a real quantization-string naming difference between the GGUF's
own `general.file_type` value and the catalog's label), and
`docs/stage-3-verification.md` for the full transcript.
