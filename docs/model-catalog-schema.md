# Model Build Catalog schema

## In plain language

A "catalog" is a small JSON file listing specific downloadable model
files (not model *families* — each entry is one exact quantization of one
exact release) along with enough facts about them to guess whether they'll
run well on a given machine. BRUTE never downloads these files itself; the
catalog is just metadata pointing at an official source.

## File shape

```json
{
  "_notice": "human-readable disclaimer about how curated/verified this data is",
  "schema_version": "stage1-v1",
  "builds": [ { ...one ModelBuild per entry... } ]
}
```

## `ModelBuild` fields

| Field | Meaning |
|---|---|
| `catalog_id` | Stable local identifier, e.g. `qwen2.5-0.5b-instruct-q4_k_m` |
| `family` | Groups quantizations of the same release together |
| `display_name`, `publisher` | Human-facing labels |
| `official_source_url`, `official_repository_id` | Where to get it — data only, never opened automatically by the core engine |
| `filename` | Bare filename, validated to contain no path separators or `..` |
| `architecture`, `parameter_count`, `quantization` | Used directly by the estimator |
| `file_size_bytes`, `estimated_disk_bytes` | Real download size when known |
| `estimated_runtime_memory_bytes` | Optional curator hint, never trusted over `estimator`'s own computed range |
| `min_recommended_ram_bytes`, `min_recommended_vram_bytes` | Curator's own rule-of-thumb minimums (informational; `brute fit` computes its own independent estimate) |
| `supported_backends`, `context_sizes`, `task_categories` | Drive backend/context/task matching in `fit`/`recommend` |
| `short_description`, `strength`, `limitation` | One sentence each — no marketing copy (enforced: capped at 300-500 chars, validated at load time) |
| `license`, `commercial_use`, `gated_access` | See below — the fields this schema is strictest about |
| `metadata_provenance`, `last_reviewed` | Where these numbers came from and when they were last checked |

## License and commercial-use honesty rules

```rust
enum License { Known { identifier: String }, Unknown }
enum CommercialUse { Allowed, Restricted, Unknown }
```

- `CommercialUse::Allowed` may **only** be set when the catalog metadata
  explicitly establishes it — never inferred from a permissive-sounding
  model name or an `Unknown` license.
- `License::Unknown` and `CommercialUse::Unknown` are first-class values,
  not error states. A curator who genuinely doesn't know the license
  writes `Unknown`, not a guess.
- The dev catalog includes `dev-fixture-unknown-license`, a synthetic
  entry that exists specifically to exercise this path in tests
  (`catalog::tests::missing_license_stays_unknown_not_defaulted`,
  `recommend::tests::unknown_license_fixture_still_ranks_but_never_claims_commercial_use_allowed`).

## Safety: catalog files are untrusted input

`catalog::load_catalog` treats every catalog file as untrusted:

- Hard file-size cap (16 MiB) before parsing at all.
- Hard entry-count cap (50,000) after parsing.
- Every entry is structurally validated: non-empty IDs, `filename` must be
  a bare name (no `/`, `\`, or `..`) ending in `.gguf`,
  `official_source_url` must be `http(s)://` (never `file://` or a shell
  command), all counts/sizes must be nonzero, no duplicate `catalog_id`s.
- Nothing in a catalog file is ever interpreted as a path to open, a URL
  to fetch, or a command to run — it is metadata only, displayed as text.

See `src/catalog/mod.rs` for the full validation list and its tests
(`rejects_path_traversal_in_filename`, `rejects_non_http_source_url`,
`rejects_oversized_catalog_file`, `rejects_malformed_json`,
`rejects_duplicate_catalog_ids`, `rejects_zero_parameter_count`).

## The dev catalog

`data/catalog/dev-catalog.json` is explicitly labeled curated development/
test fixture data (its own `_notice` field says so). It contains 7 builds
chosen to cover every case Stage 1's acceptance criteria call for:

| catalog_id | Why it's here |
|---|---|
| `qwen2.5-0.5b-instruct-q4_k_m` | Very small general model. **Independently verified** — every numeric field matches what `brute model inspect` actually measured against the real downloaded file (sha256 in its `metadata_provenance`). |
| `qwen2.5-0.5b-instruct-q8_0`, `qwen2.5-0.5b-instruct-f16` | Same family, multiple quantizations |
| `qwen2.5-coder-1.5b-instruct-q4_k_m` | Small coding model |
| `llama-3.1-8b-instruct-q4_k_m` | Medium general model, gated + license-restricted |
| `llama-3.1-70b-instruct-q4_k_m` | Deliberately oversized negative-test case |
| `dev-fixture-unknown-license` | Synthetic entry with `Unknown` license/commercial-use |

Every non-Qwen-0.5B-Q4_K_M entry's `file_size_bytes` is a rounded,
order-of-magnitude approximation (documented per-entry in
`metadata_provenance`) — not independently verified, and must be
re-checked before any production use.
