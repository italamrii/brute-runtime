# Calibration methodology

## In plain language

A "calibration record" is a real benchmark BRUTE actually ran — never a
projection. When you ask about a model that hasn't been benchmarked yet,
BRUTE looks for the closest real measurement it has and tells you exactly
how close it is, rather than pretending a different model's numbers are a
direct prediction.

## Record shape (`calibration::CalibrationRecord`)

Exact model build, backend, thread count, GPU layers, context, batch,
measured prompt/generation throughput, peak RAM/VRAM, stability
classification, timestamp, and the machine profile schema version. See
`src/calibration/mod.rs`.

## How a record gets created

`brute benchmark --save-calibration <path>` (also available on
`recommend`/`report`) appends a record after a **real** run completes,
using the model's actual parsed architecture/quantization/parameter_count
and the actual measured throughput/memory/stability. Recording is skipped
(with a warning, not a silent no-op or a fabricated entry) if any of
those identity fields couldn't be determined from the file.

## Seed data

`data/calibration/seed-calibration.json` ships with exactly one record:
the real Qwen2.5-0.5B-Instruct Q4_K_M CPU benchmark from Stage 0 (425.15
prompt tok/s, 44.09 generation tok/s, 542,609,408 bytes peak RAM, Stable,
recorded 2026-07-17T23:45:24Z, on `machine-4afb786d0b05e66d`).

## Nearest-match lookup (`calibration::find_nearest`)

A documented, bounded nearest-neighbor search — no machine learning:

1. **Architecture must match exactly** (case-insensitive). BRUTE never
   extrapolates a Qwen2 measurement onto a Llama build, or vice versa.
2. **Parameter count beyond 10x apart is excluded entirely** — too wide a
   gap to be a meaningful anchor at all.
3. Among the remaining candidates, the **closest** by a combined distance
   score (parameter-count ratio, quantization match, backend match) wins.
4. The match is classified into one of three proximities:

| Proximity | Condition |
|---|---|
| `Exact` | Same quantization, same backend, parameter count within 20% |
| `Close` | Same quantization *family* (e.g. Q4_K_M and Q4_K_S both "Q4"), parameter count within 3x |
| `Distant` | Same architecture only, or parameter count more than 3x (but ≤10x) away |

`Close` counts as "calibration support" for the fit engine's uncertainty
downgrade (see `docs/model-fit-classification.md`); `Distant` does not.

## Confidence degrades honestly at every reporting layer

`recommend::explain::describe_calibration_performance` phrases the
numbers according to proximity — never presents a different-sized or
different-quantized build's real measurement as if it directly predicted
the build in hand:

- **Exact**: *"Expected performance (measured on this exact build): …"*
- **Close**: *"Reference performance from a similar but not identical
  calibrated build (…differences…): … — a rough guide only, not a
  measurement of this build"*
- **Distant**: *"Only a distant calibration reference exists (…): … — not
  a meaningful prediction for this one"*

This wording was added after a real bug was caught during live testing:
an earlier version of `recommend-model`'s terminal output printed the
0.5B model's real 425 tok/s as the flat "expected performance" for the
1.5B Coder model on a `Close` match, with no qualifier — exactly the kind
of scaling-as-if-linear extrapolation the Stage 1 brief explicitly
forbids. Fixed in the same session; see
`docs/stage-1-verification.md` for the before/after transcript.

## Growing the store

Multiple records accumulate in the same JSON file as more real benchmarks
are run (`CalibrationStore::add` + `::save`). `find_nearest` always
prefers the closest match among however many records exist; nothing about
the lookup changes as the store grows beyond a single seed entry.
