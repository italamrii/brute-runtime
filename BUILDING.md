# Building BRUTE Runtime from source

There are two things you can build from this repository: the **desktop
app** (what most people want) and the **`brute` CLI engine** it's built
on (useful for scripting/automation). The desktop app depends on the
engine crate at the repo root — you don't need to build the CLI
separately to build the desktop app.

## Prerequisites

- Windows 10/11, x86_64. (macOS/Linux: compile-only today — see
  [`docs/cross-platform.md`](docs/cross-platform.md).)
- Rust (tested with `rustc 1.98.0-nightly`; any recent stable toolchain
  should also work — `rustup default stable`).
- Node.js + npm (for the desktop app's frontend).
- Visual Studio Build Tools with the MSVC C++ toolset (needed to compile
  Win32 API bindings) — already satisfied if you installed Rust via the
  standard Windows installer.
- **Not required**: CMake, a CUDA toolkit, or the Vulkan SDK. llama.cpp
  is used as a prebuilt official binary, never built from source by this
  project.

## Desktop app

```powershell
cd desktop
npm install
npm run tauri dev      # run in development mode
npm run tauri build    # build a production installer (MSI + NSIS)
```

`npm run tauri build` also runs the frontend production build and
compiles the Rust backend in release mode as part of the same command.
See [`docs/windows-packaging.md`](docs/windows-packaging.md) for exactly
what gets bundled, checksums, and the unsigned-build notice.

### Desktop verification checks

```powershell
cd desktop
npm run lint                          # eslint
npx tsc --noEmit                      # TypeScript type-check
npm test                              # vitest (frontend unit tests)
npm run build                          # production frontend build

cd src-tauri
cargo test                             # desktop backend Rust tests
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

## Core engine / CLI

```powershell
cargo build --release
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

Full CLI usage: [`docs/cli-reference.md`](docs/cli-reference.md).

## Regenerating branding assets

If the official logo/icon files (`branding/brute-app-icon.png`,
`branding/brute-logo.png`) are ever replaced, regenerate every derived
icon format and in-app asset with:

```powershell
pwsh desktop/scripts/generate-icons.ps1
```

See [`branding/README.md`](branding/README.md) for exact requirements
and what this script produces.

## Packaging an end-user release

See [`RELEASE.md`](RELEASE.md).

## Architecture

If you're planning to make non-trivial changes, read
[`docs/architecture.md`](docs/architecture.md) first — it explains the
engine/desktop split, the command-boundary pattern, and why the engine
crate has zero network dependencies by design.
