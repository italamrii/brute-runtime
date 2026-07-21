# Model comparison plain-language summary (Stage B.9)

`buildCompareSummaries` in `desktop/src/pages/Discover.tsx`. Adds a
plain-language section beneath Stage B.6's raw side-by-side compare
table, shown only when 2 or more models are selected.

## Why per-model summaries, not pairwise sentences

The mission's example phrasing was pairwise ("Model A is faster and
lighter than Model B because..."), but the compare set can hold up to
4 models - pairwise sentences over 4 models means 6 pairs, which reads
as a wall of near-duplicate text. Instead, `buildCompareSummaries`
computes one summary per model: for each real, already-displayed
comparison dimension, it finds which model(s) hold the best (or, for
size/RAM, the smallest) value *within the compared set*, and lists
those dimension labels next to that model's name - e.g. "Model A —
Leads in: fastest, uses the least RAM." This scales linearly with the
number of compared models instead of combinatorially, and never
requires template-string interpolation (`I18nContext.t()` only maps a
key to a string - see its own doc comment) since every dynamic part
(the model's real display name, the list of translated dimension
words) is composed directly in JSX rather than inside a translated
sentence template.

## Dimensions compared

Speed, quality, Arabic support (only entries with `arabic > 0`
participate - a model with zero real Arabic signal is never credited
with "best Arabic support" by default), device fit, trust, download
size (smaller wins), and minimum recommended RAM (smaller wins) - the
same `component_scores` and `build` fields the table above it already
shows. No new computation, no fabricated numbers.

## Honesty rules

- **Ties are never broken arbitrarily.** If two compared models have
  the exact same value for a dimension, both are credited as leading
  it - never an arbitrary pick to manufacture a single "winner."
- **No standout dimension is a real, shown answer, not a blank.** A
  model that doesn't lead in anything gets "No standout dimension
  compared to the other selected models" instead of an empty summary
  or a silently omitted entry.
- **Single-model selection shows no summary section at all** - there's
  nothing to compare yet; the raw table's `discover_compare_empty`
  state already covers the zero-selection case.

## Tests

`desktop/src/pages/Discover.test.tsx`: a real speed/size difference is
reflected in the correct "fastest"/"smallest download" labels; an
exact tie on every dimension credits both models rather than picking
one; a single selected model shows no summary section.
