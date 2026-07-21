# Discover Models redesign (Stage B.6)

`desktop/src/pages/Discover.tsx`. Replaces the page's previous data
source (`listCatalog()` + a per-build v1 `evaluateFit()` call, neither
preferences-aware) with a single `recommendV2()` call
(`docs/recommendation-methodology-v2.md`) plus `listLibrary()`, used
only to cross-reference `LibraryEntry.catalog_match` for the
"Installed" badge. The Optimize page is untouched and still uses v1.

## Filters

Eighteen filter dimensions, each backed by a real catalog/library field
- never a fabricated or estimated one:

| Dimension | Field |
|---|---|
| Arabic capability | `task_categories` includes `arabic_chat`, or `arabic_capability != unknown` |
| Coding / general use / documents / vision | `task_categories` membership |
| Family | `family_id ?? family` (`family_key`, matches the Rust-side helper of the same name in `recommend/v2.rs`) |
| Model size | `parameter_count` bucketed at ≤3B/≤8B/≤15B/>15B |
| Quantization | `quantization`, exact match |
| Max download size / RAM / VRAM | GB-denominated inputs converted to bytes, compared against `file_size_bytes`/`min_recommended_ram_bytes`/`min_recommended_vram_bytes` |
| CPU / GPU compatible | `supported_backends` membership |
| License known | `license.status !== "unknown"` (unchanged from the pre-redesign "Verified source only" filter) |
| Source verified | `source_verification !== "unknown"` |
| Artifact verified | `exact_artifact_url` is set |
| Installed only | `library.catalog_match.catalog_id` matches and `confidence !== "none"` |
| Recommended only | `fit_state` is `good` or `excellent` |

## Sorting

Seven options, all backed by `recommend_v2`'s real component scores or
the catalog's own declared fields - never a synthesized number:

`best_match` (`overall_score`), `best_arabic`/`fastest`/`best_quality`/
`most_trusted` (the matching `component_scores` field), `smallest_download`
(`file_size_bytes`), `lowest_memory` (`min_recommended_ram_bytes` - the
catalog's own declared minimum, since `BuildRecommendationV2` does not
carry a raw byte estimate). Every sort breaks ties on `catalog_id`
ascending, matching v2's own determinism guarantee.

## Cards and details dialog

Cards show the fit-state badge, task-category chips, curated
speed/quality badges (only when curated, never a default-`unknown`
placeholder), the model's `RecommendationCategory` tags (e.g. "Best
match for you", "Best Arabic support"), and an "Installed" badge when
the library cross-reference matches. The details dialog adds source/
artifact verification status, checksum presence, confidence level, the
real `explanation` string, and - when present - `rejection_reasons`
(a hard-rejected build is never silently hidden; it still appears,
tagged `unsupported`, with the reason shown).

## Two explicit scope decisions

- **No "Download" button in this stage.** Deferred to Phase B Step 7
  and delivered there - see `docs/safe-download-flow.md`. Shipping a
  button ahead of the real progress/checksum/cancel wiring would have
  either done nothing or bypassed the verification this project's
  privacy/security model requires.
- **Comparison without plain-language summaries.** Selecting up to 4
  models (a checkbox per card, capped client-side) opens a raw side-by-
  side table (family, Arabic/coding/reasoning/document capability,
  parameters, quantization, size, RAM/VRAM, speed, context length,
  license, trust, device fit) - all real fields, no generated prose.
  Plain-language comparison summaries ("Model A is faster and lighter
  than Model B because...") are Phase B Step 9's own increment, built
  on top of this selection/table mechanism rather than duplicating it.

## Tests

`desktop/src/pages/Discover.test.tsx` - all pre-existing navigation-
safety tests (internal-modal-only card clicks, Escape/Close/Back never
trapping the user, localhost/malformed/`javascript:`-URL rejection,
external opens going only through `openUrl` to the system browser,
double-click/StrictMode dialog-duplication guards) were ported to the
new `recommendV2`/`listLibrary` data source unchanged in intent. New
tests cover the installed badge, the installed-only and recommended-
only filters, the family filter, sorting, the compare selection cap and
side-by-side table, the verified-download informational note, the
explanation text, and hard-rejection reasons.
