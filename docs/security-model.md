# Security model — Stage 0, Stage 1, and Stage 2

## Threats considered

1. **Command injection via model path, prompt, or binary path.**
   Every external process is launched with `std::process::Command::new(bin).args([...])`
   — an explicit argv, never a shell string (`cmd /C "..."` with
   interpolation). Windows' `CreateProcess` receives a program path and an
   argument array; there is no shell parsing step for either llama.cpp
   arguments or file paths to be misinterpreted through. This is verified
   directly: `runtime::llama_cpp::tests::build_bench_args_puts_model_path_as_a_single_argv_element`
   builds a model path containing `; rm -rf / && calc.exe` and asserts it
   survives as one literal argv element.

2. **Tampered or substituted llama.cpp binary.**
   `brute` never downloads anything itself (see below). Before every launch
   it computes the SHA-256 of the exact binary path it was given and
   compares it against a sibling `<binary>.sha256` pin file written by
   `scripts/fetch-llama-cpp.ps1` immediately after that script verified the
   download against the hash pinned in `scripts/llama-cpp-manifest.json`.
   - **Hash matches pin:** proceeds.
   - **Hash mismatch:** hard error, always, not bypassable by any flag. A
     mismatch means the file changed since the trusted fetch.
   - **No pin file found:** refuses to run unless `--allow-unverified-binary`
     is explicitly passed (for binaries the user built or obtained
     themselves outside the fetch script).

   This protects against the binary being altered *after* a trusted fetch.
   It is not a signature scheme and does not, by itself, prove the original
   download was authentic — that assurance comes from step 3.

3. **Supply-chain integrity of the llama.cpp download itself.**
   `scripts/llama-cpp-manifest.json` pins an exact release tag, asset name,
   and SHA-256 **read from GitHub's own release-asset digest field** (the
   GitHub API's `digest` property on each release asset), not computed
   locally from a file we already trusted. `fetch-llama-cpp.ps1` downloads
   the asset, hashes it, and **deletes the file and aborts before
   extracting or executing anything** if the hash doesn't match. `brute`
   itself has no HTTP client and cannot download anything — the only
   network-touching code in this repository is that one script, so the
   entire download/verify/extract path is auditable in one file.

4. **Path safety for imported models and binaries.**
   `security::paths::validate_regular_file` canonicalizes the path (which
   resolves symlinks/junctions/reparse points to their real target) and
   then re-checks the *resolved* path is a regular, non-empty file. A
   reparse point that redirects to something other than a plain file is
   rejected by the `is_file()` check on the canonicalized path, not the
   original one. Paths containing spaces and non-ASCII characters (e.g.
   Arabic) are handled as opaque `OsString`/`PathBuf` data throughout — no
   byte-level ASCII assumptions are made anywhere in the path or argv
   construction code.

5. **Resource exhaustion from a hostile or corrupt GGUF file.**
   The parser never allocates memory proportional to an attacker-controlled
   length field without first bounding it against both a fixed sanity cap
   and the actual remaining file size (see `models/gguf.rs`, and
   [`measurement-methodology.md`](measurement-methodology.md)). The tensor
   data blob is never read.

6. **A hung or runaway llama.cpp process.**
   Every launch has an enforced timeout (`--timeout-secs`, default 120s).
   `runtime::process::run` polls the child with `try_wait()` and calls
   `kill()` on expiry, then reaps the process. stdout/stderr are drained on
   background threads so a chatty process cannot deadlock us by filling an
   OS pipe buffer while we're blocked waiting for exit.

7. **Untrusted catalog input (Stage 1).** `catalog::load_catalog` treats
   every catalog JSON file as untrusted: a hard 16 MiB file-size cap and
   50,000-entry cap are enforced before/after parsing, every entry is
   structurally validated (`filename` must be a bare name with no path
   separators or `..`, `official_source_url` must be `http(s)://`, no
   duplicate IDs, all counts/sizes nonzero), and no catalog string is ever
   interpreted as a path to open, a URL to fetch, or a shell command - see
   `docs/model-catalog-schema.md` and `src/catalog/mod.rs`'s validation
   tests (`rejects_path_traversal_in_filename`, `rejects_non_http_source_url`,
   `rejects_oversized_catalog_file`, `rejects_malformed_json`).

8. **Privacy-sensitive fields in exported reports (Stage 1).** Every path
   that could carry a real Windows username (a model's file path, a
   storage path checked for free space, a llama.cpp binary directory) is
   redacted before it leaves the process as JSON -
   `security::redact_username_for_report` replaces
   `C:\Users\<name>\...` with `C:\Users\<redacted>\...`, applied in
   `report::json::to_pretty_string`/`write_to_file` (Stage 0's
   `brute report`) and `brute profile create`'s JSON/file output. Verified
   with both a Latin and an Arabic username
   (`security::paths::tests::redact_username_handles_arabic_usernames_too`)
   and end-to-end
   (`report::json::tests::exported_report_never_contains_the_real_username_from_a_model_path`).
   This is a display-time transform only - `brute` always operates on the
   real, unredacted path internally.

9. **Never claiming a GPU backend works from detection alone (Stage 2).**
   `backends::verify_backend` requires a real process launch, a real
   model load, and llama-bench's own `gpu_info` field non-empty before
   ever reporting `Verified` - never Stage 1's driver-detection signal
   alone. This caught a real false positive live: see
   `docs/backend-verification.md` and `docs/known-limitations.md`.
   `tuning::candidates::generate_plan` only generates GPU-offload
   candidates for a backend that passed this pipeline.

10. **A saved runtime profile can be a hand-edited or corrupted file on
    disk (Stage 2).** `tuning::runtime_profile::sanity_check` rejects
    internally impossible values (zero/implausible thread counts, batch
    exceeding context, a CPU backend paired with nonzero GPU layers, a
    malformed model hash) before `tuning::apply::apply_and_verify` ever
    launches a process against it - checked before, and independently
    of, environment-compatibility checks (`check_still_valid`). See
    `docs/runtime-profile-schema.md`.

11. **Cooperative cancellation never leaves an orphaned process (Stage
    2).** `runtime::process::run`'s `TickAction::Cancel` path kills and
    reaps the child exactly like a timeout does, recorded as a distinct
    `cancelled` flag rather than misreported as `timed_out`. Every layer
    above it (llama.cpp wrappers, the benchmark runner, the tuner)
    threads the same guarantee through - there is no code path that
    spawns a `llama-cli`/`llama-bench` process without eventually
    waiting on it. See `docs/cancellation-and-process-safety.md`.

## What Stage 0/1/2 explicitly do NOT do

- Does not modify BIOS, drivers, power limits, voltage, clocks, fan
  curves, registry performance settings, or Windows security settings
  (Stage 2). Tuning only ever changes runtime parameters passed to
  llama.cpp (threads, GPU layers, context, batch) - see
  `docs/cancellation-and-process-safety.md`.
- Does not persist any Stage 2 state (saved profiles, tune status)
  anywhere but `%LOCALAPPDATA%\BruteRuntime\`, never uploaded or synced
  anywhere - see `docs/privacy-model.md`, which is Stage 2's dedicated
  privacy document (the mandatory no-telemetry/no-analytics/no-cloud-
  database list lives there in full).
- Does not execute anything from inside a GGUF file, or any file adjacent
  to it. GGUF metadata is treated as inert data (numbers and strings), and
  the tensor payload is never read at all.
- Does not download or redistribute model files, ever. The Stage 1
  catalog stores only metadata and official source URLs - `brute` never
  opens a catalog URL automatically.
- Does not scrape, crawl, or auto-update catalog data from the internet.
  The catalog is a local, hand-curated, explicitly-labeled fixture file.
- Does not run any code with elevated privileges.
- Does not phone home, telemetry-free by construction (no HTTP client
  linked into the `brute` binary itself - confirmed by dependency and
  source audit, see `docs/stage-1-verification.md` §10: zero network
  crates in `Cargo.toml`, no telemetry/analytics/country-detection code
  anywhere in `src/`).
- Does not collect hardware serial numbers, MAC addresses, or other
  precise device identifiers. `profile::HardwareCapabilityProfile`'s
  `machine_id` is a hash of already non-sensitive, coarse aggregate specs
  (CPU brand string, core counts, RAM rounded to the nearest GiB, OS
  build number) - stable on one machine, not traceable to a real hardware
  identity, and two different machines with identical coarse specs could
  in principle collide (an accepted tradeoff, documented in
  `profile::compute_machine_id`'s doc comment).
- Does not silently downgrade a security check into a warning. A hash
  mismatch is always fatal; an unavailable hardware measurement is always
  reported as `unavailable`, never defaulted to a plausible-looking value.

## Residual risk / honest limitations

- The GitHub API digest pinned in the manifest is trusted as the root of
  trust for the llama.cpp binary. If GitHub's release infrastructure or
  the upstream `ggml-org/llama.cpp` account were compromised, this scheme
  would not detect it — it detects *tampering after the fact*, not
  malicious-but-correctly-signed upstream releases.
- `--allow-unverified-binary` exists deliberately (Stage 0 needs to support
  a self-built llama.cpp) and is an explicit, logged trust decision the
  user makes per invocation, not a default.
- Vulkan/CUDA *detection* is implemented; Stage 0 does not attempt to
  sandbox or otherwise constrain what a GPU-backend llama.cpp binary does
  once launched — the same subprocess trust boundary applies to it as to
  the CPU backend.
