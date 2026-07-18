# Stage 1 verification log

Real commands, real output, on the same machine as Stage 0's verification
(13th Gen Intel Core i5-13450HX, 16 GB RAM, NVIDIA GeForce RTX 5050 Laptop
GPU, Windows 11 Home build 26200), 2026-07-18.

## 1. Build, format, lint, test

```
cargo build --release      -> succeeds, no warnings
cargo fmt --check          -> clean
cargo clippy --all-targets -- -D warnings   -> clean
cargo test                 -> 125 passed; 0 failed
```

## 2. A real bug found and fixed during live verification

`brute fit --all` was run against real hardware before any Stage 1
integration tests existed. Every single entry came back `Experimental`,
including a tiny 0.5B model on a machine with gigabytes of free RAM. Root
cause: the fit engine sent *any* catalog-only estimate (no real
hyperparameters, since a build hasn't been downloaded) with no calibration
match straight to `Experimental`, regardless of how much headroom existed
— conflating "the estimate is unverified" with "the outcome is uncertain."
A model that clearly fits even under a pessimistic estimate isn't
genuinely uncertain.

Fixed by changing the uncertainty penalty from "jump to Experimental" to
"downgrade one tier" (Excellent→Good, Good→Constrained→Experimental only
when *also* tight). See `docs/model-fit-classification.md` for the full
before/after and `fit::tests::uncalibrated_coarse_estimate_downgrades_excellent_to_good_not_experimental`.

A second real bug: `recommend-model`'s terminal output presented a
`Close`-proximity calibration match's raw tok/s numbers as flat "Expected
performance" for a differently-sized model — exactly the "extrapolate as
if scaling were linear" the brief explicitly forbids. Fixed by adding
`recommend::explain::describe_calibration_performance`, which phrases the
number according to proximity (Exact/Close/Distant) instead of always
claiming it as a direct prediction. See `docs/calibration-methodology.md`.

A third, smaller bug: `calibration_record_count` in the hardware profile
counted every record in the store file regardless of which machine
measured it, contradicting its own doc comment. Fixed to filter by
`machine_id`.

## 3. `brute profile create` (real hardware)

```
Machine ID: machine-4afb786d0b05e66d
Schema version: stage1-v1
CPU: 13th Gen Intel(R) Core(TM) i5-13450HX (10 physical / 16 logical cores)
RAM: 16873545728 bytes total, 8387559424 bytes available
GPU: Intel(R) UHD Graphics (Intel)
GPU: NVIDIA GeForce RTX 5050 Laptop GPU (Nvidia)
GPU: Microsoft Basic Render Driver (Other)
Backends: cpu=true cuda=Some(true) vulkan=Some(true)
Calibration records for this machine: 1
Confidence summary: measured=15 detected=2 inferred=2 unavailable=0 (of 19 fields)
```

## 4. `brute catalog list` / `show`

7 builds loaded from `data/catalog/dev-catalog.json`; the curated-data
notice prints first. `brute catalog show qwen2.5-0.5b-instruct-q4_k_m`
correctly displays the sha256-verified provenance note and
`Commercial use: allowed (per catalog metadata)` — never phrased as if
inferred.

## 5. `brute fit --all --storage-path C:\Models` (real hardware, post-fix)

```
qwen2.5-0.5b-instruct-q4_k_m             excellent       headroom_ratio=0.67
qwen2.5-0.5b-instruct-q8_0               good            headroom_ratio=0.63
qwen2.5-0.5b-instruct-f16                good            headroom_ratio=0.52
qwen2.5-coder-1.5b-instruct-q4_k_m       excellent       headroom_ratio=0.57
llama-3.1-8b-instruct-q4_k_m             not_recommended headroom_ratio=n/a
llama-3.1-70b-instruct-q4_k_m            not_recommended headroom_ratio=n/a
    - insufficient free disk space for this build's file size
dev-fixture-unknown-license               experimental    headroom_ratio=0.50
```

The 70B model is rejected on **disk space** specifically (this machine's
`C:\Models` drive has ~9-10 GB free, nowhere near the 42.5 GB catalog
entry) - a real, independently-triggered rejection reason distinct from
the memory check, proving both gates work.

## 6. `brute recommend-model` across priorities (real hardware)

```
--priority lowest-memory   -> Qwen2.5 0.5B Instruct (Q4_K_M)     score 0.925
--priority highest-quality -> Qwen2.5 Coder 1.5B Instruct (Q4_K_M) score 0.898, safer fallback = the 0.5B model
--task coding --priority coding -> Qwen2.5 Coder 1.5B Instruct (Q4_K_M)
```

Same catalog, same machine, same calibration store — different priority,
different top pick, matching the acceptance criterion "recommendations
change appropriately by task and priority" (also covered by
`recommend::tests::highest_quality_priority_prefers_a_larger_model_than_lowest_memory_priority`
and `recommend::tests::coding_priority_ranks_the_coding_model_above_pure_chat_models_of_similar_size`).

## 7. `brute explain-fit` (technical explanation, real numbers)

```
Memory calculation: formula stage1-v1: weights=491400032 bytes (catalog)
  + KV cache 78624005-314496020 bytes (CoarseApproximation)
  + runtime overhead 50000000-300000000 bytes + OS reserve 1610612736 bytes
  = estimated total 620024037-1105896052 bytes;
  available RAM 8735199232 bytes -> headroom 6018690444 bytes
Calibration used: nearest calibration record (Close match): 425.15 prompt tok/s,
  44.09 gen tok/s, Stable stability, recorded 2026-07-17T23:45:24Z
  - differences: nearest calibration record used backend Cpu, this run targets Cuda
```

## 8. `brute calibrations list`

```
1 calibration record(s):
  qwen2 Q4_K_M 630167424 backend=Cpu 425.1/44.1 tok/s (prompt/gen) Stable recorded=2026-07-17T23:45:24Z
```

## 9. Performance measurements

Isolated internal timings (`cargo test measure_stage1_performance -- --nocapture`),
separate from Stage 0's hardware-detection subprocess overhead
(`nvidia-smi`/`vulkaninfo`, which dominates end-to-end CLI wall time at
~0.68-0.71s regardless of any Stage 1 work - confirmed by timing
`brute inspect`, which does zero Stage 1 work, at the same ~0.71s):

| Operation | Real measured time |
|---|---|
| Catalog parse (7 entries) | 783.1 µs |
| Profile build from already-inspected hardware | 82.6 µs |
| Single build fit evaluation | 60 µs |
| Full ranking pass (7 builds) | 56.5 µs |

All sub-millisecond. Linearly extrapolating catalog parse time to 1,000
entries would suggest roughly 100-150 ms (untested at that scale - a
projection, not a measurement) - comfortably within the "remain fast for
hundreds or thousands of entries" requirement, though this repo has not
independently verified behavior at that scale.

## 10. Privacy/network audit

`Cargo.toml` has zero network-capable dependencies. `grep -rniE
"reqwest|hyper|tokio::net|TcpStream|geoip|country|locale|telemetry|
analytics|track_event|phone.?home"` across `src/` returns no matches
(the only hits are the unrelated word "hyperparameter"). Stage 1 performs
no network I/O of any kind - catalog and calibration data are local JSON
files only.

## 11. Backward compatibility and privacy tests

`report::json::tests::stage0_json_field_paths_remain_present_after_stage1_additions`
confirms every Stage 0 JSON field path is still present, unchanged, in
`CapabilityReport` output. `report::json::tests::exported_report_never_contains_the_real_username_from_a_model_path`
confirms a model path under `C:\Users\RealUserName\...` is redacted to
`C:\Users\<redacted>\...` in every exported JSON report (`brute report`
and the Stage 1 `brute profile create --output`), verified with both a
synthetic Latin username and an Arabic one
(`security::paths::tests::redact_username_handles_arabic_usernames_too`).
