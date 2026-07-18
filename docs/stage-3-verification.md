# Stage 3 verification — what was actually run and observed

Real machine: 13th Gen Intel Core i5-13450HX, 16 GB RAM, NVIDIA GeForce
RTX 5050 Laptop GPU, Windows 11 Home build 26200 - same machine used for
every prior stage's real-machine validation. Model:
`C:\Models\qwen2.5-0.5b-instruct-q4_k_m.gguf`.

## 1. A real bug found and fixed during development

`library::associations::find_calibration_matches_for` originally iterated
over all three backends (CPU/CUDA/Vulkan) and trusted whatever
`calibration::find_nearest` returned for each. `find_nearest` itself
doesn't filter by backend - a backend mismatch only lowers its proximity
*score*, so asking it "nearest calibration for CUDA" against a CPU-only
store still returns the CPU record (with a mismatch note in
`distance_notes`), not `None`. The first implementation of this function
naively reported that one CPU record as a match for all three backends -
a live test with one CPU calibration record asserted 1 match and got 3.

Fixed by additionally requiring the *returned* record's own `backend`
field to equal the backend being asked about:

```rust
crate::calibration::find_nearest(store, architecture, quantization, parameter_count, backend)
    .filter(|m| m.record.backend == backend)
    .map(|m| (backend, m))
```

Regression test:
`library::associations::tests::finds_calibration_matches_for_a_known_architecture`
now correctly asserts exactly 1 match (CPU), not 3. See
`docs/runtime-profile-association.md`.

## 2. `brute library scan` / `import` - real transcript

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
```

The catalog match is honestly `Weak`, not `Strong`: the GGUF's own
`general.file_type`-derived quantization label is `MOSTLY_Q4_K_M`, while
the curated catalog's label for this exact build is `Q4_K_M` - a real
naming-convention difference (the "MOSTLY_" prefix is how llama.cpp's
legacy file-type enum names itself), not a bug. The matcher correctly
refuses to claim a `Strong` match on a string comparison that doesn't
actually agree - see `docs/trust-and-provenance.md` and
`docs/known-limitations.md`.

## 3. `brute library show` confirms identity is consistent across every stage

```
$ brute library show model-instance-12ddb13fc87b9f539bd5d87baab1c840
Library entry: model-instance-12ddb13fc87b9f539bd5d87baab1c840
  Alias: Qwen2.5 0.5B Instruct
  qwen2, 0.6B params, MOSTLY_Q4_K_M
  Path: C:\Models\qwen2.5-0.5b-instruct-q4_k_m.gguf
  SHA-256: 74a4da8c9fdbcd15bd1f6d01d621410d31c6fc00986f5eb687824e7b93d7a9db
  File status: Unchanged  Trust: LocalUnverifiedSource
  Catalog match: Some("qwen2.5-0.5b-instruct-q4_k_m") (confidence: Weak)
  License metadata available: apache-2.0
  Commercial use: allowed per catalog metadata - review official license before deployment
  Associated runtime profiles: 1
  Associated calibration records: 1
```

The SHA-256 (`74a4da8c9f...93d7a9db`) is byte-identical to what Stage 0's
`model inspect`, Stage 1's `fit`, and Stage 2's `tune run` independently
computed against this same file in prior sessions - confirming content
identity is stable across every stage's tooling. **Associated runtime
profiles: 1** and **Associated calibration records: 1** were found
live, by hash, with zero manual linking - the exact Stage 2 profile and
Stage 1 seed calibration record already on this machine.

## 4. `brute library verify` and quarantine workflow

```
$ brute library verify model-instance-12ddb13fc87b9f539bd5d87baab1c840
model-instance-...: overall_integrity=Verified trust=LocalUnverifiedSource

$ brute library quarantine model-instance-... --reason "smoke test"
Quarantined model-instance-.... It will not be benchmarked or launched until cleared.

$ brute library verify model-instance-...
model-instance-...: skipped (quarantined - run `brute library unquarantine` first)

$ brute library unquarantine model-instance-...
Quarantine lifted for model-instance-... after a passing re-verification.
overall_integrity=Verified trust=LocalUnverifiedSource
```

Unquarantine genuinely re-ran the full verification pipeline before
lifting the hold - not a bare flag flip.

## 5. Duplicate detection and relocation recovery - real, isolated from the real model

A byte-identical **copy** of the real model was imported at a scratch
path (never the real file):

```
$ brute library import <scratch>/relocated-test-copy.gguf
Imported a new library entry.
  Library ID: model-instance-6be14ff8c0a8d97758b22ec9c6b94d23
  Integrity: Verified  Trust: LocalUnverifiedSource
  Duplicate of: model-instance-12ddb13fc87b9f539bd5d87baab1c840
```

The copy was then moved (still only within scratch space) and recovered:

```
$ brute library verify model-instance-6be14ff8c0a8d97758b22ec9c6b94d23
model-instance-...: overall_integrity=Unknown trust=Missing

$ brute library locate model-instance-6be14ff8c0a8d97758b22ec9c6b94d23 <scratch>/relocated/moved-copy.gguf
Located: model-instance-... -> <scratch>/relocated/moved-copy.gguf
Verification: overall_integrity=Verified trust=LocalUnverifiedSource
Runtime profiles/calibration records remain valid - they're keyed to content hash, not path.
```

The test entry was then `forget`-ed (metadata only) and the scratch
files deleted - the real model and its real library entry were
untouched throughout.

## 6. Audit, storage, export

```
$ brute library audit
Library audit:
  Healthy: 1  Missing: 0  Modified: 0  Corrupt: 0
  Unknown provenance: 0  Unsupported: 0  Duplicate groups: 0
  Stale runtime profiles: 0  Stale calibrations: 0  Privacy concerns: 0
  Entries needing schema migration: 0

$ brute library storage
Total entries: 1  Total bytes: 491400032  Available bytes: 491400032
Potentially reclaimable duplicate bytes: 0
```

Export sanitization, checked directly against the written file:

```
$ brute library export --output library-export.json
Library exported to library-export.json (1 entries, paths and machine identifiers redacted).

$ grep -c "asdks" library-export.json            # 0 - no username
$ grep -c "machine_id\|local_instance_id" library-export.json   # 0 - these fields don't exist on LibraryEntry at all
$ grep -c "C:\\\\Models" library-export.json      # 0 - no raw path
```

## 7. No network required

Confirmed by the same dependency audit maintained since Stage 1: no
network crate appears in `Cargo.toml` (`grep -iE "reqwest|hyper|tokio.*net|curl|ureq" Cargo.toml` matches nothing), so there is no code path
in the binary that *could* make a network request during any of the
above - not a runtime policy, a structural fact.

## 8. Real model confirmed unchanged

`C:\Models\qwen2.5-0.5b-instruct-q4_k_m.gguf`: 491,400,032 bytes,
modification timestamp unchanged across the entire session - never
renamed, moved, or modified by any command above.

## 9. Performance (spec section 21)

All real measurements, `cargo test stage3_performance -- --nocapture`
(release build):

| Entries | Save | Load | Duplicate grouping | Storage summary | Audit | Export |
|---|---|---|---|---|---|---|
| 10 | 0.69ms | 17.16ms | 13.5µs | 188.8µs | 155.4µs | 393.5µs |
| 100 | 0.59ms | 17.57ms | 24.6µs | 1.38ms | 1.41ms | 566µs |
| 1,000 | 1.93ms | 17.55ms | 226.4µs | 12.90ms | 13.12ms | 2.00ms |
| 10,000 | 15.37ms | 31.71ms | 2.40ms | 129.52ms | 136.29ms | 16.18ms |

Additional real measurements:

- **Streaming SHA-256 throughput**, real 491,400,032-byte model file:
  337.16ms (1,457.5 MB/s).
- **Scan planning**, 200 synthetic files across 10 directories,
  recursive: 25.97ms.

`load` time stays roughly flat (~17ms) from 10 to 1,000 entries, then
rises at 10,000 - consistent with fixed per-call OS/file-open overhead
dominating at small scale and JSON deserialization cost only becoming
visible at the higher end. `storage summary`/`audit` scale roughly
linearly with entry count, dominated by one real filesystem `stat()`
call per entry (`quick_file_status`) - genuine I/O cost, not an
artifact of the measurement.

## 10. Test suite, formatting, lint

```
$ cargo test
test result: ok. 299 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

$ cargo fmt --check
(clean)

$ cargo clippy --all-targets -- -D warnings
Finished `dev` profile [unoptimized + debuginfo] target(s)
(zero warnings, zero errors)
```

## 11. Honest gaps in this verification pass

- The catalog match for the real model is `Weak`, not `Strong`/`Exact`,
  due to the quantization-string naming mismatch documented above - a
  real, observed limitation, not a hypothetical one.
- `ExpectedHashMatched` trust can never be reached with the current
  catalog schema (no curated expected-hash field) - see
  `docs/trust-and-provenance.md`.
- Managed-copy mode (`--copy-into-library`) was not exercised, since it
  isn't implemented - see `docs/model-import-and-verification.md`.
- "Inaccessible file" (a real permission-denied scenario, distinct from
  "missing") was not separately live-tested - Windows ACL manipulation
  in an automated/interactive session is impractical to set up
  reliably; the code path (`quick_file_status`'s `Inaccessible` branch)
  is exercised by construction, not by a dedicated permission-denial
  fixture.
