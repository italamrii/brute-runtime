# Windows packaging

## Build command

```powershell
cd desktop
npm install
npm run tauri build
```

This runs the frontend production build (`tsc && vite build`), compiles
`brute-desktop` in `--release`, and bundles the result via Tauri's
bundler. Bundle targets are configured in
`desktop/src-tauri/tauri.conf.json`'s `bundle.targets`: `msi` (WiX) and
`nsis`.

## What gets bundled

- `desktop/src-tauri/target/release/brute-desktop.exe` - the compiled
  desktop app.
- `desktop/dist/` - the Vite production build of the React frontend,
  embedded via `frontendDist` in `tauri.conf.json`. No dev server URL, no
  remote assets - everything is a local file baked into the binary/
  bundle at build time.
- `bundle.resources` (`tauri.conf.json`) copies the repo's `data/`
  directory (the curated `dev-catalog.json` and `seed-calibration.json`
  fixtures) into the app's resource directory as `data/`, resolved at
  runtime by `desktop/src-tauri/src/paths.rs::seed_data_dir`. Without
  this, a packaged (non-dev) build would have no catalog/calibration
  data available - `paths.rs`'s dev-only fallback
  (`CARGO_MANIFEST_DIR/../../data`) only exists in debug builds, since a
  packaged install has no access to the source repository.
- `bundle.resources` also copies the pinned, hash-verified CPU-only
  llama.cpp runtime (`.tools/llama.cpp/b10064/cpu`, populated by
  `scripts/fetch-llama-cpp.ps1`) into the app's resource directory as
  `runtime/cpu/` - including each binary's sibling `.sha256` pin file,
  so `commands::runtime::resolve_runtime`'s strict (non-lenient)
  verification of the bundled tier actually has something to check
  against. This is what lets an ordinary user run a model without ever
  configuring a runtime path themselves - see
  `docs/architecture.md`'s "trusted runtime resolution" section.
- App icons (`icons/*.png`, `icons/icon.ico`, `icons/icon.icns`) -
  locally provided, no external icon service.

## Version metadata

- Product name: `BRUTE Runtime` (`tauri.conf.json`'s `productName`)
- Version: `0.1.0`
- Identifier: `com.bruteruntime.desktop`
- Publisher (metadata field, not a code-signing identity - see below):
  `BRUTE Runtime`

## Unsigned development build

**No real code-signing certificate was available for this MVP.** The
built installer and executable are unsigned. Per the Stage 4 spec, this
must never be silently hidden: the app's Settings page and this document
both state plainly —

> Development build — publisher signature not yet configured.

This means:

- Windows SmartScreen will show an "unrecognized app" warning the first
  time a user runs the installer or the app. BRUTE does not attempt to
  bypass, suppress, or work around this warning - it is the correct,
  honest behavior for an unsigned binary.
- `npm run tauri build` does not fail or block on the absence of a
  certificate - it produces a genuine, working, unsigned build, since an
  external security audit and paid code signing are both explicitly
  out of scope for this MVP per the release-scope directive.
- If a real certificate becomes available later, signing is a
  `tauri.conf.json`'s `bundle.windows.certificateThumbprint` (or a
  CI-level signing step) addition - no application code changes are
  needed.

## Uninstall and local state

- The MSI/NSIS installer uninstalls the application binary and its
  installed files through the normal Windows "Apps & Features" flow.
- **Local state is never touched by uninstall.** Everything BRUTE
  persists lives under `%LOCALAPPDATA%\BruteRuntime\` (the model library
  index, saved runtime profiles, calibration records, the local instance
  ID) - this directory is intentionally left in place after an uninstall
  so a reinstall picks up exactly where the user left off. A user who
  wants a full clean removal should delete
  `%LOCALAPPDATA%\BruteRuntime\` manually after uninstalling; BRUTE does
  not do this automatically to avoid any chance of silent data loss on a
  routine uninstall/reinstall.
- Settings → "Reset local state" only clears the desktop app's own UI
  preferences (`localStorage`: selected language, configured llama.cpp
  binary path, onboarding-seen flag) - it does not touch
  `%LOCALAPPDATA%\BruteRuntime\` either.

## Portable vs. installed

This MVP ships installer-based distribution (MSI/NSIS) only. A portable
(no-installer, run-from-anywhere) build was not produced - `bundle.targets`
would need `"targets": "all"` or an explicit portable target added, which
was judged out of scope for the initial release per the Stage 4 scope
narrowing (focus on one verified, working distribution path rather than
several partially-verified ones).

## Release checksums

After a successful `npm run tauri build`, compute SHA-256 checksums for
every distributable artifact and publish them alongside the release:

```powershell
Get-FileHash desktop\src-tauri\target\release\bundle\msi\*.msi -Algorithm SHA256
Get-FileHash desktop\src-tauri\target\release\bundle\nsis\*.exe -Algorithm SHA256
```

### Real build produced in this repository's verification session

Both bundle targets built successfully end-to-end on the real
verification machine (WiX 3.14.1 and NSIS 3.11, both auto-downloaded by
Tauri's bundler on first use - no manual toolchain installation was
required):

| Artifact | Size | SHA-256 |
|---|---|---|
| `BRUTE Runtime_0.1.0_x64_en-US.msi` | 3,649,536 bytes | `9CBE2D04CBE85FA3A9DB67C760FBF0E7181366B8EB9689C2FF0CF32BD535DB48` |
| `BRUTE Runtime_0.1.0_x64-setup.exe` | 2,398,684 bytes | `5CDD91B8152485F97914E963F3A1D62001339511EF42764709DC09843B089FE8` |

The MSI was independently verified to contain the expected payload via
an administrative extraction (`msiexec /a ... TARGETDIR=...`, no
install performed): `brute-desktop.exe`, `brute_desktop_lib.dll`, and
`data\catalog\dev-catalog.json` / `data\calibration\seed-calibration.json`
under `Program Files\BRUTE Runtime\` - confirming the `bundle.resources`
fix (see below) actually ships the seed data a packaged install needs,
not just the dev build's repo-relative fallback.

### Rebuild after adding trusted runtime auto-resolution + cross-platform architecture

A later pass in the same overall effort added the bundled-runtime
resolution hierarchy, model auto-discovery, the Discover Models catalog
page, the explicit user-triggered download flow, and cross-platform
(compile-only) hardware detection. The full production build was
re-run afterward to confirm packaging still succeeds with these
changes (new `ureq`/`dirs` dependencies, the additional bundled
`runtime/cpu/` resource):

| Artifact | Size | SHA-256 |
|---|---|---|
| `BRUTE Runtime_0.1.0_x64_en-US.msi` | 23,535,616 bytes | `C75228F50E5692ABACFFDB7E6C647DDFB8E21654DE7B54EF42F34DA40531EEEA` |
| `BRUTE Runtime_0.1.0_x64-setup.exe` | 13,180,013 bytes | `47D24FE563765674D9C2AA43BDA52BBFD025D79B7E44C6F1C9927234FD58806D` |

The size increase versus the first build (~20 MB) is the bundled
CPU-only llama.cpp runtime (`.tools/llama.cpp/b10064/cpu`, ~45 MB
uncompressed) now shipping inside the installer. A second
administrative MSI extraction confirmed `runtime\cpu\llama-cli.exe`,
`runtime\cpu\llama-bench.exe`, and their sibling `.sha256` pin files
are genuinely present in the installed payload (53 files under
`runtime\cpu\` total) - not just the raw executables, but the pin
files `commands::runtime::resolve_runtime`'s strict bundled-tier
verification actually depends on.

### A real gap this build caught and fixed

The first build attempt in this session produced a working `.exe` but
`bundle.resources` was not yet configured in `tauri.conf.json` - a
packaged install would have had no catalog/calibration data available
at all (`paths.rs`'s dev-only fallback path only resolves inside this
source repository). This was caught by inspecting the release build's
warnings (`unused import: Path` in the `not(debug_assertions)` branch of
`paths.rs` - a real signal that the debug-only code path, and therefore
the packaged-build code path it's paired with, needed attention), fixed
by adding `"resources": { "../../data": "data" }` to `tauri.conf.json`,
and confirmed by the MSI content extraction above.

### A real disk-space failure encountered and resolved

The first full `npm run tauri build` after the resources fix failed with
`error: failed to build archive ... There is not enough space on the
disk. (os error 112)` - the machine's C: drive had only ~128 MB free at
that point. This was resolved by removing the `target/debug/` directories
under both the root crate and `desktop/src-tauri/` (regenerable build
cache, ~10 GB combined, unrelated to any committed source or user data),
which freed enough space for the release build to complete. This is a
real machine-level disk-space constraint worth being aware of when
running `cargo tauri build` repeatedly during development - not a defect
in BRUTE itself.
