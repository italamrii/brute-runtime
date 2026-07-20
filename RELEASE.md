# Release process

This document describes how a BRUTE Runtime Windows release is built,
packaged, and verified. It's aimed at maintainers cutting a release, not
end users (who want the [README's download section](README.md#download-windows)
instead).

## 1. Build and verify the installer

```powershell
cd desktop
npm install
npm run tauri build
```

This produces `desktop/src-tauri/target/release/bundle/msi/*.msi` and
`.../bundle/nsis/*-setup.exe`. Before packaging for release:

- Run every check in [`BUILDING.md`](BUILDING.md#desktop-verification-checks)
  — frontend tests/lint/typecheck/build, desktop Rust tests/fmt/clippy —
  and the core engine's equivalents (`cargo test`/`fmt`/`clippy` at the
  repo root).
- Manually test the **installed** NSIS build, not just `npm run tauri
  dev` — install, launch, exercise hardware detection/model
  discovery/a real local generation/stop, uninstall. See
  [`docs/stage-4-verification.md`](docs/stage-4-verification.md) and
  [`docs/known-limitations.md`](docs/known-limitations.md) for the kind
  of real, dynamic verification this has caught real bugs before
  (console-window flashing, a crash in Discover Models) that static
  checks alone missed.

## 2. Package the end-user release folder + ZIP

```powershell
pwsh scripts/package-windows-release.ps1
```

This copies the already-built, verified NSIS installer (never modifies
its bytes) into `release/BRUTE-Runtime-v<version>-Windows-x64/` as
`BRUTE-Runtime-Setup.exe`, alongside bilingual READMEs, a SHA-256
checksum file, the official icon, and third-party license notices for
the bundled llama.cpp runtime — then zips the folder with the installer
at the archive's top level. See the script's own comments and
[`docs/windows-packaging.md`](docs/windows-packaging.md) for exactly
what's included.

The `release/` directory is gitignored (regenerable build output) —
running this script never requires a commit.

## 3. Verify the release package before publishing

```powershell
# Checksum matches what's in the release folder
Get-FileHash release\BRUTE-Runtime-v0.1.0-Windows-x64\BRUTE-Runtime-Setup.exe -Algorithm SHA256

# The packaged installer is byte-identical to the raw build artifact
Get-FileHash "desktop\src-tauri\target\release\bundle\nsis\BRUTE Runtime_0.1.0_x64-setup.exe" -Algorithm SHA256

# Inspect ZIP contents - installer must be at the top level, no source
# code, no developer paths, no model files
Expand-Archive release\BRUTE-Runtime-v0.1.0-Windows-x64.zip -DestinationPath <temp-dir>
```

## 4. Rename assets for the GitHub Release

The packaging script produces `BRUTE-Runtime-Setup.exe` inside the
release folder/ZIP (a clean name for someone who already knows what
they downloaded). For the GitHub Release itself, attach these three
files with version-qualified names so they're unambiguous outside the
ZIP's own context:

| File | Source |
|---|---|
| `BRUTE-Runtime-v<version>-Windows-x64.zip` | `release/BRUTE-Runtime-v<version>-Windows-x64.zip`, as produced |
| `BRUTE-Runtime-v<version>-Windows-x64-Setup.exe` | `release/BRUTE-Runtime-v<version>-Windows-x64/BRUTE-Runtime-Setup.exe`, copied and renamed (bytes untouched) |
| `SHA256SUMS.txt` | `release/BRUTE-Runtime-v<version>-Windows-x64/SHA256SUMS.txt`, copied as-is |

## 5. Write release notes

Create `docs/releases/v<version>.md` (see
[`docs/releases/v0.1.0-beta.1.md`](docs/releases/v0.1.0-beta.1.md) for
the format) covering what's new, known limitations, the unsigned-beta
warning, and platform support status. Never claim every model runs on
every device, and never claim macOS/Linux are release-ready ahead of
real verification on those platforms.

## 6. Publish

Create the GitHub Release manually (this repository's CI does **not**
auto-publish releases - see `.github/workflows/ci.yml`), attach the
three files from step 4, and paste in the release notes from step 5.

## Versioning

Version numbers live in `desktop/src-tauri/tauri.conf.json`
(`"version"`) — bump that before packaging. Pre-1.0 releases use a
`v<version>-beta.<n>` tag/title (e.g. `v0.1.0-beta.1`) to keep beta
status unambiguous; this project has not yet reached a stable 1.0.
