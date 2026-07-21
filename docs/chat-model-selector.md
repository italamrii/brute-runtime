# Chat model selector integration (Stage B.8)

`desktop/src/pages/Chat.tsx`. Adds an 8-mode chooser (Auto, Fastest,
Balanced, Best quality, Arabic, Coding, Documents, Vision) above the
message list that uses Recommendation Engine v2
(`docs/recommendation-methodology-v2.md`) to auto-select the best
already-*installed* model for the chosen mode - never a model that
isn't actually on disk.

## Why it only ranks installed models

Chat can only run a model that has already been imported into the
local library (`local_run_generate` needs a real `library_id` and
`profile_id`). `recommend_v2` ranks the full *catalog*, most of which
the user may not have downloaded. `pickModelForMode` (in `Chat.tsx`)
bridges the two: it only considers `LibraryEntry` rows whose
`catalog_match.catalog_id` is set with a non-`"none"` confidence, looks
up that catalog build's `BuildRecommendationV2` from the same
`recommend_v2` call Discover Models uses, and scores only that
installed subset. An installed model with no catalog match is honestly
excluded from mode-based ranking (it can still be picked manually via
the existing dropdown) rather than guessed at.

## Mode scoring

| Mode | Ranks installed+matched models by |
|---|---|
| Auto | `overall_score` |
| Fastest | `component_scores.speed` |
| Best quality | `component_scores.quality` |
| Arabic | `component_scores.arabic`, restricted to entries with `arabic > 0` |
| Balanced | `min(device_fit, task_fit, quality, speed)` - "no glaring weakness," matching v2's own `balanced` category definition |
| Coding | `overall_score`, restricted to builds whose `task_categories` includes `coding` |
| Documents | `overall_score`, restricted to `task_categories` including `document_analysis` |
| Vision | `overall_score`, restricted to `task_categories` including `vision` |

An entry with any `rejection_reasons` is never eligible for mode-based
selection. Ties break on `library_id` ascending - deterministic, same
guarantee v2 itself makes for the catalog-wide ranking.

## Behavior

- Picking a mode immediately selects the best-matching installed model
  in the existing model dropdown (which in turn triggers the existing
  runtime-profile lookup, unchanged) and shows the picked build's real
  `explanation` string underneath - never a bare score.
- No installed model fits the mode (e.g. "Vision" with no
  vision-capable model installed) → an honest note: "No installed
  model matches this mode yet. Visit Discover Models to find and
  download one." The current selection is left untouched, never
  cleared or replaced with a wrong guess.
- Manually changing the model dropdown clears the active mode
  indicator (back to "manual") so the UI never claims a mode is active
  when the user has actually overridden it by hand.
- Mode selection never re-runs automatically when the library list
  changes in the background - it's a one-shot pick, not a live-pinned
  state, so a model mid-conversation is never silently swapped out from
  under the user.

## Tests

`desktop/src/pages/Chat.test.tsx`, "model mode selector" describe
block: Fastest picks the higher-speed installed model over a
lower-speed one; a mode with no eligible installed model shows the
"no match" note and leaves the current selection unchanged; Coding
only considers coding-tagged installed models; manually changing the
dropdown clears the active mode's highlighted state; the picked
model's explanation text renders.
