# Phase B installed Windows build verification (Stage B.12)

Real acceptance pass against the actual NSIS/MSI installers built from
the tip of Phase B (commit `4fcc299`, Stage B.11), not just `cargo
test`/`vitest run`/dev-mode. Performed on this repository's own
Windows 11 development machine.

## Build

```
npx tauri build
```

- `npm run build` (frontend `tsc && vite build`) succeeded first,
  confirming the exact bundle shipped is the one `vitest`/`tsc` had
  already verified in-repo.
- `cargo build --release` for both `brute` and `brute-desktop`
  succeeded with no warnings.
- Two installers were produced:

| Artifact | Path (under `desktop/src-tauri/target/release/bundle/`) | Size | SHA-256 |
|---|---|---|---|
| MSI | `msi/BRUTE Runtime_0.1.0_x64_en-US.msi` | 24,252,416 bytes | `b96622dfcd331df4725a2d08554935929d3207150d09ffbaa7d25695fd2de7f8` |
| NSIS | `nsis/BRUTE Runtime_0.1.0_x64-setup.exe` | 13,854,033 bytes | `0a528abd53a46d44d459f6f767068bc2be84e56cca9079fc0e719665fe0eff40` |

Neither installer, nor anything under `target/`, is committed to git -
`.gitignore` already excludes `/target`, `*.msi`, `*-setup.exe`, and
`/release` (defense in depth beyond the root-anchored rule, since
`desktop/src-tauri/.gitignore` also has its own `/target/`).

## WebView2 dependency check

`tauri.conf.json`'s `bundle.windows.webviewInstallMode` is
`downloadBootstrapper`, which would fetch the WebView2 Runtime
bootstrapper *during install* if it were missing - the one path in the
whole install/build/run lifecycle that could reach the network without
being the explicit, user-triggered `download_model` flow. Verified
before installing that WebView2 Runtime was already present system-
wide (`HKLM\...\EdgeUpdate\Clients\{F3017226-...}`, version
`150.0.4078.83` - standard on Windows 11), so the install performed in
this pass made no network request of any kind.

## Install

```
"BRUTE Runtime_0.1.0_x64-setup.exe" /S
```

- Silent per-user install completed with no prompts.
- Actual install location (confirmed via the real
  `HKCU\...\Uninstall\BRUTE Runtime` registry entry, not assumed):
  `%LOCALAPPDATA%\BRUTE Runtime`.
- Verified on disk: `brute-desktop.exe`, `uninstall.exe`, the bundled
  `data/catalog/dev-catalog.json` and `data/catalog/test-fixtures.json`
  (the latter ships but is never loaded by production code - confirmed
  by `production_catalog_never_contains_the_test_fixture_entry`, see
  `docs/model-catalog-schema.md`), `data/calibration/seed-calibration.json`,
  and the full bundled CPU llama.cpp runtime tree under `runtime/cpu/`
  (`llama-cli.exe`, `llama-cli.exe.sha256`, `llama-bench.exe`,
  `llama-bench.exe.sha256`, and their shared `.dll`s) - all present and
  matching what `tauri.conf.json`'s `bundle.resources` maps in.

## Launch

- The installer's own silent-mode auto-launch and a second explicit
  launch both produced a real, stable `brute-desktop.exe` process with
  window title `BRUTE Runtime` (confirmed via `Get-Process`, not
  assumed) - no crash, no console window, ~45 MB working set 5 seconds
  after start.
- Two independent process instances coexisting (the installer's auto-
  launch plus the explicit one) is expected, ordinary behavior - this
  app does not enforce single-instance, and the "exactly one window"
  guarantee (`structuralGuards.test.ts`,
  `no_window_flash_guard_test`) is about *one process never opening a
  second WebView window*, not about the OS refusing to run two
  separate copies of the executable.
- Both instances were terminated cleanly after verification.

## Scope and honest limits of this pass

This is a real build/install/launch/uninstall acceptance pass, run
non-interactively via PowerShell/CLI tooling - it verifies the
installer produces a working, correctly-bundled, network-silent
application that starts and stays running. It does **not** include a
manual, visual click-through of every page (Chat send/receive, Discover
filters, the download flow's progress UI, Settings) inside the
installed build, since this environment has no interactive display
session to drive one. That deeper walkthrough remains a real,
documented gap - not silently assumed to have passed - and should be
done by a human (or a UI-automation harness this environment doesn't
have) before a real release is cut. The dev-mode equivalent of most of
these flows *is* covered by `vitest run`'s 156 frontend tests, which
exercise the same React components the installed build ships.

## Uninstall

```
uninstall.exe /S
```

- Removed `%LOCALAPPDATA%\BRUTE Runtime` entirely and the
  `HKCU\...\Uninstall\BRUTE Runtime` registry entry - confirmed absent
  afterward, not assumed. The machine was left in the same state it
  was in before this verification pass.
- One real, worth-recording snag: invoking `uninstall.exe /S` from Git
  Bash did not reliably wait for/complete the uninstall (the directory
  was still present immediately after). Re-running it via PowerShell's
  `Start-Process -ArgumentList "/S" -Wait` completed correctly (exit
  code `0`, directory and registry entry both gone). Likely a
  Bash/argument-quoting or process-detachment issue with this
  particular NSIS uninstaller stub, not a defect in BRUTE Runtime
  itself - noted here in case it recurs in a future verification pass.
