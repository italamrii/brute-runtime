# Security model — Stage 0

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

## What Stage 0 explicitly does NOT do

- Does not execute anything from inside a GGUF file, or any file adjacent
  to it. GGUF metadata is treated as inert data (numbers and strings), and
  the tensor payload is never read at all.
- Does not download or redistribute model files, ever.
- Does not run any code with elevated privileges.
- Does not phone home, telemetry-free by construction (no HTTP client
  linked into the `brute` binary itself).
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
