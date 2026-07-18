# Stage 3 — Trusted Local Model Library & Lifecycle

## In plain language

Stages 0-2 could inspect, fit, tune, and profile *one model you point
BRUTE at by path*. Stage 3 answers a different, longer-lived question:
**what models do I actually have, are they still what I think they are,
and what do I know about each one?**

It does this without ever copying, moving, executing, or uploading your
models. BRUTE reads a file once (to hash and parse its GGUF metadata),
records what it found locally, and from then on tracks that recorded
identity against whatever is actually on disk - flagging drift honestly
rather than assuming nothing changed.

Four ideas underpin everything in this stage:

1. **Identity is content, not location.** A file's SHA-256 is the
   authoritative fact; its path is just where you can currently find it.
   See `docs/model-identity.md`.
2. **Nothing is trusted more than the evidence earns.** A structurally
   valid GGUF isn't automatically an "official" model; a catalog match
   isn't automatically a hash match. See `docs/trust-and-provenance.md`.
3. **Every destructive-sounding operation is actually safe.** "Forget"
   never deletes a file; "remove-managed" only ever touches files BRUTE
   itself is responsible for (which is none, today); nothing is
   quarantined or unquarantined without real verification. See
   `docs/quarantine-and-recovery.md`.
4. **Discovery is always explicit.** No background scanning, no
   whole-disk scans, no automatic imports - every scan and import is a
   command the user typed. See below.

## Architecture

See `docs/architecture.md` for the full module map. In one sentence:
`library::scan` discovers `.gguf` candidates in a user-named directory;
`library::import` validates/hashes/parses one and creates or updates a
`library::LibraryEntry`; `library::verify` runs the full structural +
hash pipeline and the cheap file-change heuristic, and powers
`library::locate`'s move-recovery workflow; `library::duplicates` groups
entries by identical hash; `library::associations` connects an entry to
Stage 2 runtime profiles and Stage 1 calibration records live, by
content hash; `library::storage` and `library::audit` are read-only
analyses over the whole `library::LibraryStore`.

## Commands

```
brute library scan <path> [--recursive] [--max-depth N] [--max-files N] [--max-total-bytes N] [--max-duration-secs N] [--json]
brute library import <path> [--alias <name>] [--json]
brute library import-directory <path> [--recursive] [--json]
brute library list [--json]
brute library show <library-id> [--json]
brute library verify <library-id> | --all [--json]
brute library refresh <library-id> | --all [--json]
brute library audit [--json]
brute library duplicates [--json]
brute library storage [--json]
brute library locate <library-id> <new-path> [--json]
brute library alias <library-id> <name>
brute library note <library-id> <text>
brute library forget <library-id>
brute library remove-managed <library-id> --confirm
brute library quarantine <library-id> --reason <reason>
brute library unquarantine <library-id>
brute library quarantined [--json]
brute library export --output <file>
```

`scan` and `import`/`import-directory` are deliberately separate
commands, not flags on one command - `scan` alone never imports
anything, matching the spec's "default behavior should be dry
discovery, not import."

## Real output on this machine

```
$ brute library scan C:\Models
Scanned C:/Models (recursive=false): 1 candidate(s) found across 1 directory
  [GgufCandidate] C:/Models\qwen2.5-0.5b-instruct-q4_k_m.gguf (491400032 bytes)

This was a scan only - nothing was imported. Use `brute library import`.

$ brute library import C:\Models\qwen2.5-0.5b-instruct-q4_k_m.gguf --alias "Qwen2.5 0.5B Instruct"
Imported a new library entry.
  Library ID: model-instance-12ddb13fc87b9f539bd5d87baab1c840
  Integrity: Verified  Trust: LocalUnverifiedSource
  Catalog match: Some("qwen2.5-0.5b-instruct-q4_k_m") (confidence: Weak)

$ brute library show model-instance-12ddb13fc87b9f539bd5d87baab1c840
Library entry: model-instance-12ddb13fc87b9f539bd5d87baab1c840
  Alias: Qwen2.5 0.5B Instruct
  qwen2, 0.6B params, MOSTLY_Q4_K_M
  SHA-256: 74a4da8c9fdbcd15bd1f6d01d621410d31c6fc00986f5eb687824e7b93d7a9db
  File status: Unchanged  Trust: LocalUnverifiedSource
  License metadata available: apache-2.0
  Commercial use: allowed per catalog metadata - review official license before deployment
  Associated runtime profiles: 1
  Associated calibration records: 1
```

See `docs/stage-3-verification.md` for the complete transcript across
every command, including a real bug found and fixed during development.

## What Stage 3 explicitly does NOT do

- No automatic, hidden, or background scanning - see
  `docs/library-security.md`.
- No telemetry, no network requests, no cloud database - see
  `docs/library-privacy.md`.
- No managed copying (`--copy-into-library`) - see
  `docs/model-import-and-verification.md`.
- No external file deletion, ever - see
  `docs/quarantine-and-recovery.md`.
- Never infers official/legal status from a filename or directory - see
  `docs/trust-and-provenance.md`.
- Does not build the polished desktop UI - still CLI-only, same as
  every prior stage.
