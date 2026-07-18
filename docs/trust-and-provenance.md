# Trust and provenance

`library::TrustStatus` never claims "official" status from a filename
resembling a known model, publisher metadata alone, or the directory
the user happened to put a file in. Official-source status requires
curated local catalog evidence to even approach, and even then is
capped honestly - see below.

## States

| State | Means |
|---|---|
| `LocalUnverifiedSource` | Structurally valid GGUF, hash computed, no catalog match strong enough to say more |
| `UserConfirmedSource` | Reserved for a future explicit "I confirm this is X" user action - not set by any current code path |
| `CatalogMetadataMatched` | A catalog entry's architecture/quantization/parameter-count/file-size all agree (`CatalogMatchConfidence::Exact` or `Strong`) |
| `ExpectedHashMatched` | The computed hash matches an independently curated *expected* hash - see the honest gap below |
| `RuntimeVerified` | Reserved for when a runtime load/benchmark check (not yet wired into the library layer) confirms the model actually loads |
| `ModifiedSinceVerification` | The file's current hash no longer matches what was last recorded for it |
| `Missing` | The file cannot be found at its recorded path |
| `Corrupt` | Structurally invalid (bad magic, or a size-consistency check fails) |
| `Unsupported` | Magic is valid GGUF, but the version isn't one this codebase parses |
| `Unknown` | Verification itself could not be completed (e.g. an I/O error mid-hash) |

Never collapsed into a boolean "trusted: true/false" - see
`docs/model-import-and-verification.md` for the full `GgufVerification`
structure this is one field of.

## The honest gap: `ExpectedHashMatched` cannot fire today

Spec section 6 asks for an `ExpectedHashMatched` trust tier - "the
computed hash matches a known, curated expected hash." Stage 1's
catalog schema (`catalog::ModelBuild`) has no field for an independently
curated expected SHA-256 of the artifact, only descriptive metadata
(architecture, quantization, parameter count, file size). So this
tier's precondition can never be satisfied by the current catalog data
- `import::match_against_catalog`'s strongest reachable result is
`CatalogMatchConfidence::Strong` (file size and GGUF metadata all
agree), which maps to `CatalogMetadataMatched`, one tier below what a
real hash match would justify. This is not a shortfall in the matching
logic - it accurately reflects what the catalog can actually prove
today. Adding a per-entry `expected_sha256` to `ModelBuild` (verified
against a real downloaded file, the way Stage 1's dev catalog already
does for one entry) is the natural way to close this gap in a future
stage; see `docs/known-limitations.md`.

## "Source link matched, artifact hash not independently verified"

When a catalog entry's `official_source_url` describes where a model
*should* come from but nothing in the catalog independently confirms
*this specific file's* hash, that is exactly what `CatalogMatchResult`
communicates through its `confidence` field rather than a plain
"matched" boolean - a `Weak` or `Probable` match still surfaces the
matched `catalog_id` so a user can see the candidate, while `notes`
spells out exactly what wasn't confirmed (e.g. "file size differs from
the catalog's recorded value"). Never upgraded to `CatalogMetadataMatched`
trust on the strength of a weak match - verified:
`library::import::tests::weak_catalog_match_is_never_upgraded_to_a_strong_claim`.

## What never counts as evidence

- A filename that *looks like* an official release name.
- Publisher metadata claiming a known name.
- The directory a file was found in during a scan.

None of these appear anywhere in `TrustStatus`'s derivation
(`verify::verify_artifact`) or catalog matching
(`import::match_against_catalog`) - only structural verification,
content hash, and curated catalog metadata actually move the needle.
