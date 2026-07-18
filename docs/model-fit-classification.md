# Fit Classification Engine

## In plain language

For every model build, BRUTE answers "will this probably fit?" with one
of six words, each with a precise, tested meaning. `Unknown` and
`Not recommended` are **never the same thing** — one means "we don't have
enough information," the other means "we checked, and it doesn't fit."

## The six states (`src/fit/mod.rs`, `FitState`)

| State | Meaning |
|---|---|
| `Excellent` | ≥50% RAM headroom after the OS reserve, supported backend available, requested context realistic, and either the estimate is backed by real hyperparameters/calibration or the margin is so large the uncertainty doesn't matter. |
| `Good` | 20–50% headroom under the same conditions, **or** an Excellent-tier headroom that got downgraded one notch for being an uncalibrated estimate. |
| `Constrained` | <20% headroom, or the requested context exceeds what the build documents support. |
| `Experimental` | Technically fits (headroom ≥0) but the estimate is both uncalibrated **and** tight-margin — the combination that actually creates real uncertainty about whether it'll work well. |
| `Not recommended` | Headroom is negative, no supported backend is available, or insufficient disk space. |
| `Unknown` | A required input (available RAM, backend availability, or the headroom calculation itself) could not be determined. Checked **before** any of the above — never collapsed into `Not recommended`. |

## Rule evaluation order (`fit::evaluate`)

1. Backend availability unknown, or available RAM unknown → `Unknown`.
2. No supported backend available → `Not recommended`.
3. Disk known to be insufficient → `Not recommended`.
4. Headroom calculation impossible → `Unknown`.
5. Headroom negative → `Not recommended`.
6. Context not realistic → `Constrained`.
7. Otherwise: headroom-ratio tier (Excellent/Good/Constrained), then the
   uncertainty downgrade below.

## The uncertainty downgrade — a real design correction

An earlier version of this engine sent *every* uncalibrated catalog-only
estimate straight to `Experimental`, regardless of headroom. Testing
against a 64 GB workstation fixture caught this: a tiny 0.5B model with
enormous headroom was being called "substantial uncertainty" just because
no calibration record existed yet — which is the wrong signal. The margin
itself already absorbs a wide-range coarse estimate's uncertainty.

The fix: uncertainty (`CoarseApproximation` quality with no calibration
support) downgrades the headroom-ratio tier by exactly one step:

```
Excellent + uncertain   -> Good
Good + uncertain        -> Experimental
Constrained + uncertain -> Experimental
```

So `Experimental` now means what the brief actually asked for: *tight
margin and unverified estimate, compounding*. A comfortably-fitting but
uncalibrated build is `Good`, not `Experimental`. See
`fit::tests::uncalibrated_coarse_estimate_downgrades_excellent_to_good_not_experimental`
and `fit::tests::uncalibrated_coarse_estimate_with_tight_headroom_is_experimental`.

## Real examples on this machine (`brute fit --all --storage-path C:\Models`)

```
qwen2.5-0.5b-instruct-q4_k_m             excellent       headroom_ratio=0.67
qwen2.5-0.5b-instruct-q8_0               good            headroom_ratio=0.63
    - no real per-file hyperparameters or nearby calibration data back this estimate - reduced confidence, downgraded one tier
qwen2.5-0.5b-instruct-f16                good            headroom_ratio=0.52
    - no real per-file hyperparameters or nearby calibration data back this estimate - reduced confidence, downgraded one tier
qwen2.5-coder-1.5b-instruct-q4_k_m       excellent       headroom_ratio=0.57
llama-3.1-8b-instruct-q4_k_m             not_recommended headroom_ratio=n/a
    - the high end of the estimated memory requirement exceeds available RAM after the OS safety reserve
llama-3.1-70b-instruct-q4_k_m            not_recommended headroom_ratio=n/a
    - insufficient free disk space for this build's file size
dev-fixture-unknown-license               experimental    headroom_ratio=0.50
    - adequate headroom; some compromise on context/batch size may help stability
    - no real per-file hyperparameters or nearby calibration data back this estimate - reduced confidence, downgraded one tier
```

Why `qwen2.5-0.5b-instruct-q8_0` and `-f16` are `Good` rather than
`Excellent`: their headroom ratios (0.63, 0.52) fall in the Excellent tier
on their own, but the only real calibration record on file is for the
Q4_K_M quantization of this model - Q8_0/F16 are a *different quant
family*, so the nearest match is `Distant` (`has_calibration_support =
false`), and an Excellent-tier build downgrades exactly one step to Good.
`qwen2.5-coder-1.5b-instruct-q4_k_m` stays `Excellent` even though it's
also uncalibrated for its own exact size, because its calibration match to
the 0.5B record is `Close` (same quant family, parameter count within
3x) - see `docs/calibration-methodology.md`.
`dev-fixture-unknown-license` (architecture `llama`, no calibration data
at all for that architecture) lands one tier further down: `Good` tier
downgraded to `Experimental`, since its headroom (0.50) sits right at the
Excellent/Good boundary.

## Fit is deterministic

Identical inputs (build, estimate, profile, calibration support) always
produce the identical `FitResult` — no randomness, no time-of-day
dependence. See `recommend::tests::ranking_is_deterministic`.
