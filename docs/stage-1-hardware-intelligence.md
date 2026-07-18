# Stage 1 — Hardware Intelligence & Model Fit Engine

## In plain language

Stage 0 could inspect your machine and benchmark *one model you already
downloaded*. Stage 1 answers a different question: **before you download
anything, which of several candidate models will actually run well on
your machine, and why?**

It does this by combining four things:

1. A **normalized snapshot of your hardware** (`brute profile create`).
2. A **local, curated list of model builds** with their real specs
   (`brute catalog list`/`show`) — BRUTE never downloads or hosts models.
3. A **memory/disk estimate** for each build on your specific machine
   (`brute fit`).
4. A **ranked recommendation** for a task and priority you choose
   (`brute recommend-model`, `brute explain-fit`).

Every number is labeled with where it came from: measured on your
machine, detected via a tool, inferred from a formula, taken from curated
catalog metadata, or genuinely unknown. Nothing is ever presented as more
certain than it is, and BRUTE never invents a single "AI quality score."

## Architecture

See `docs/architecture.md` for the full module map. In one sentence: raw
Stage 0 `hardware::HardwareReport` facts flow into a normalized
`profile::HardwareCapabilityProfile`; a local `catalog::Catalog` supplies
model-build metadata; `estimator` turns (profile, build) into a memory/
disk range; `fit` turns (estimate, calibration match) into one of six
states; `recommend` ranks builds for a task/priority and produces a
two-tier (simple + technical) explanation; `calibration` stores real
`brute benchmark` measurements and grounds/degrades the estimator and fit
engine's confidence.

## Commands

```
brute profile create [--output <path>] [--storage-path <dir>] [--json]
brute catalog list [--catalog <path>]
brute catalog show <catalog-id> [--catalog <path>]
brute fit (--model <catalog-id> | --all) [--context N] [--backend cpu|cuda|vulkan] [--storage-path <dir>]
brute recommend-model [--task <category>] [--priority <priority>] [--storage-path <dir>]
brute explain-fit --model <catalog-id> [--task <category>] [--priority <priority>]
brute calibrations list [--calibration <path>]
```

`--catalog` defaults to `data/catalog/dev-catalog.json`; `--calibration`
defaults to `data/calibration/seed-calibration.json`. Both are plain,
reviewable JSON files you can point elsewhere.

## Real output on this machine

```
$ brute profile create
Machine ID: machine-4afb786d0b05e66d
Schema version: stage1-v1
CPU: 13th Gen Intel(R) Core(TM) i5-13450HX (10 physical / 16 logical cores)
RAM: 16873545728 bytes total, 8387559424 bytes available
GPU: Intel(R) UHD Graphics (Intel)
GPU: NVIDIA GeForce RTX 5050 Laptop GPU (Nvidia)
Backends: cpu=true cuda=Some(true) vulkan=Some(true)
Calibration records for this machine: 1
Confidence summary: measured=15 detected=2 inferred=2 unavailable=0 (of 19 fields)
```

```
$ brute recommend-model --priority balanced --storage-path C:\Models
Recommended: Qwen2.5 0.5B Instruct (Q4_K_M) (qwen2.5-0.5b-instruct-q4_k_m)
  Fit: excellent
  Estimated RAM: 620024037-1105896052 bytes
  Expected performance (measured on this exact build): 425.1 tok/s prompt, 44.1 tok/s generation
  Score: 0.919 (ranking formula stage1-v1)
Stronger optional: Qwen2.5 Coder 1.5B Instruct (Q4_K_M) (qwen2.5-coder-1.5b-instruct-q4_k_m)

Fast and comfortable on your machine. A larger build (Qwen2.5 Coder 1.5B Instruct (Q4_K_M)) may give better quality but will use significantly more memory.
```

See `docs/stage-1-verification.md` for the complete real transcript across
every command, and `docs/model-memory-estimation.md`,
`docs/model-fit-classification.md`, `docs/recommendation-methodology.md`,
`docs/calibration-methodology.md` for how each of these numbers is
actually computed.

## What Stage 1 does not do

- Does not download, host, or redistribute model files.
- Does not claim a universal "AI quality" score.
- Does not scrape the internet for catalog data - the catalog is a small,
  hand-curated, explicitly-labeled development fixture (see
  `docs/model-catalog-schema.md`).
- Does not extrapolate a small model's benchmark directly onto a much
  larger one and present it as measured fact - see the "calibration
  proximity" wording rules in `docs/recommendation-methodology.md`.
- Does not build the desktop UI - CLI/JSON only, same as Stage 0.
