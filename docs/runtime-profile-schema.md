# Runtime profile schema

`tuning::runtime_profile::RuntimeProfile` (`PROFILE_SCHEMA_VERSION =
"stage2-profile-v1"`) is what a tuning run's winning configuration
becomes once saved. Stored as one plain JSON file per profile at
`%LOCALAPPDATA%\BruteRuntime\profiles\<profile_id>.json` — see
`docs/privacy-model.md` for why this location and never anywhere else.

## Fields

```json
{
  "schema_version": "stage2-profile-v1",
  "profile_id": "profile-instance-f052be34d631ff2889844a7551eea020",
  "tuning_date": "2026-07-18T12:29:57.569156100+00:00",

  "model_sha256": "74a4da8c9fdbcd15bd1f6d01d621410d31c6fc00986f5eb687824e7b93d7a9db",
  "model_architecture": "qwen2",
  "model_quantization": "MOSTLY_Q4_K_M",
  "model_parameter_count": 630167424,

  "machine_profile_schema_version": "stage1-v1",
  "machine_id": "omitted-from-export",

  "backend": "cpu",
  "llama_cli_sha256": "9f89e2ca70026abed6ec2561ab33e5dcb080581b9af7a061b404a13f88155a90",
  "llama_bench_sha256": "a869f27e08ac6c2f326eee1bd1a33c37dcfe5d75d41424fbf1a2573b09a88784",

  "threads": 16,
  "gpu_layers": 0,
  "context_size": 1024,
  "batch_size": 128,

  "mean_generation_tokens_per_second": 75.456,
  "mean_prompt_tokens_per_second": 512.356,
  "predicted_ram_bytes": 2414595680,
  "predicted_vram_bytes": null,

  "stability": "stable",
  "stability_formula_version": "stage2-stability-v1",
  "ranking_formula_version": "stage2-ranking-v1",
  "tuning_formula_version": "stage2-v1",
  "confidence": "high",

  "source_repetitions_succeeded": 3,
  "source_repetitions_requested": 3
}
```

This is a real profile from a live tuning run on this machine (see
`docs/stage-2-verification.md`) — `machine_id` is shown redacted, as it
always is in exported output (`brute profiles export`), and the model
path (`C:\Models\qwen2.5-0.5b-instruct-q4_k_m.gguf`) never appears
anywhere in this file at all — the model is identified purely by
`model_sha256`.

## Why binaries are identified by their own hash, not a version string

`llama_cli_sha256`/`llama_bench_sha256` are the SHA-256 of the actual
binary files (from `llama_cpp::verify_llama_binary`'s own `BinaryCheck`),
not a parsed `--version` output string. A version string varies across
llama.cpp forks/builds and is not a reliable equality check; the
binary's own hash is exactly what changed if the binary changed — and
it's the same hash already used for the Stage 0 tamper-detection pin
(`docs/security-model.md`), so no new trust mechanism was introduced.

## Invalidation — when a saved profile stops being trustworthy

`tuning::runtime_profile::check_still_valid` compares the profile
against the *current* model/machine/binaries and returns a list of
`InvalidationReason`s (empty = still valid):

| Reason | Triggered when |
|---|---|
| `ModelHashMismatch` | `model_sha256` no longer matches the model being pointed at |
| `BinaryHashMismatch` | `llama_cli_sha256` or `llama_bench_sha256` no longer matches the current binaries |
| `MachineProfileSchemaMismatch` | The machine profile schema version changed (a `profile::SCHEMA_VERSION` bump) |
| `MachineIdMismatch` | The coarse `machine_id` no longer matches — this profile was tuned on a different machine |
| `ProfileSchemaOutdated` | `schema_version` doesn't match the current `PROFILE_SCHEMA_VERSION` |

Verified: `tuning::runtime_profile::tests::check_still_valid_flags_a_changed_model_hash`,
`_a_changed_binary_hash`, `_a_changed_machine_id`, `_a_stale_schema_version`,
and the negative case `check_still_valid_reports_no_reasons_when_nothing_changed`.

Backend driver-version invalidation (spec section 9's "backend driver
changes, when detectable") is not separately tracked — a driver change
that actually affects behavior would change what `brute backends verify`
reports on the next `brute profiles verify`/`brute tune run`, which
already re-verifies rather than trusting a stale flag; see
`docs/known-limitations.md`.

## Sanity checking — rejecting a manipulated or corrupted profile

Distinct from invalidation (environment *compatibility*),
`tuning::runtime_profile::sanity_check` rejects a profile with
internally impossible values — the defense spec section 13 asks for
against "manipulated numeric values/impossible settings" in a
hand-edited or corrupted profile JSON file on disk: zero threads, an
implausibly large thread count, zero context/batch, batch exceeding
context, a CPU backend paired with nonzero GPU layers, or a
malformed (non-64-hex-char) `model_sha256`. `tuning::apply::apply_and_verify`
runs this check *before* `check_still_valid`, and before launching any
process. Verified: `tuning::runtime_profile::tests::sanity_check_*` (5
tests) and
`tuning::apply::tests::a_manipulated_profile_with_impossible_settings_is_rejected_before_any_process_is_launched`.

## Apply and verify — see `docs/cancellation-and-process-safety.md`'s sibling doc

Loading a profile and confirming it is honestly detailed in
`tuning::apply` — see that module's doc comment and
`docs/backend-verification.md`'s anti-silent-fallback discussion, which
`build_verified_result` reuses the same principle from.

## CLI

```
brute profiles list [--json]
brute profiles show <profile-id> [--json]
brute profiles verify <profile-id> --model <path> --llama-bin <dir> [--allow-unverified-binary] [--timeout-secs 60] [--json]
brute profiles export <profile-id> --output <file>
```

`export` always sanitizes (`sanitize_for_export`) before writing — the
real `machine_id` never reaches an exported file.
