# Local user preference profile (Stage B.3)

## In plain language

A single local file, `%LOCALAPPDATA%\BruteRuntime\preferences.json`, that
records what BRUTE should optimize for when recommending/ranking models.
There is no account and nothing here is ever uploaded, synced, or shared -
same guarantee as every other piece of BRUTE's local state.

بياناتك ما تطلع من جهازك.
Your data never leaves your device.

## File shape

Rust: `preferences::Preferences` (`src/preferences/mod.rs`). Serialized as
plain JSON, written via write-to-temp-then-rename (the same atomic
pattern as `conversations`, not `runtime_profile`'s plain `fs::write` -
see that module's own doc comment for why).

A brand-new install has no file yet - `load_preferences_from` returns
`Preferences::default()` in that case, not an error. A file that exists
but fails to parse, or fails validation, *is* a real error (never a
silent fallback to defaults) - see `Preferences::validate`.

## Fields

**Simple, always-visible** (the only three most users should ever need):

| Field | Values | Default |
|---|---|---|
| `language` | `arabic` / `english` / `both` | `both` |
| `use_case` | `general_assistant` / `coding` / `documents` / `writing` / `summarization` / `reasoning` | `general_assistant` |
| `priority` | `fastest` / `balanced` / `best_quality` | `balanced` |

**Advanced, hidden behind a toggle in Settings** - every one of these
defaults to "no constraint" (`false` / empty list / `null`), never a
guessed limit:

| Field | Meaning |
|---|---|
| `arabic_priority`, `english_priority` | Weigh that language's quality more heavily even when `language` is `both` |
| `memory_conservative_mode` | Prefer smaller/lower-memory models |
| `cpu_only` | Never recommend a GPU-only model |
| `gpu_preference` | `no_preference` / `prefer_gpu` / `require_gpu` |
| `offline_only` | Hides/disables anything that would suggest a network action (Discover's Download flow, "View official source") - independent of the fact that inference itself is always offline regardless of this setting |
| `permitted_licenses` | Empty = no restriction. Non-empty = only these license identifiers (e.g. `apache-2.0`) are acceptable |
| `commercial_use_required` | Only recommend `commercial_use: allowed` builds |
| `preferred_families`, `excluded_families` | `family_id` values (see `docs/model-catalog-schema.md`) |
| `max_download_size_bytes`, `max_ram_bytes`, `max_vram_bytes` | Hard ceilings, `null` = unbounded |

## Validation

`Preferences::validate` runs on every load and every save:

- `permitted_licenses`/`preferred_families`/`excluded_families` are each
  capped at 200 entries - defends a corrupted or hand-edited file from
  turning every future recommendation pass into an unbounded scan.
- The same family may never appear in both `preferred_families` and
  `excluded_families` - an invalid preferences file is never written in
  the first place (`save_preferences_to` validates before writing).

## UI

`desktop/src/pages/Settings.tsx`'s `PreferencesPanel`: the three simple
selects are always visible; every advanced field lives behind a
"Show advanced preferences" toggle, collapsed by default. Every change
saves immediately (no separate Save button, matching the existing
display-language selector's behavior) via `preferences_save`; a
`Reset preferences to defaults` button (behind a confirmation dialog)
calls `preferences_reset`.

## Network

None. `preferences_get`/`preferences_save`/`preferences_reset` are pure
local file I/O - grepped clean of `fetch`/`axios`/`XMLHttpRequest`/
`WebSocket`/any URL literal.

## What this feeds (Step 5, not yet built)

The recommendation engine (`recommend`) will read this profile to choose
weighting/filters, mapping the three simple choices onto the richer
internal `Priority`/`TaskCategory` vocabulary the ranking formula already
uses (see `docs/recommendation-methodology.md`) and applying the advanced
constraints as hard filters (license, family, memory ceilings) before
scoring. Not implemented yet - this module only owns what the user
actually chose.
