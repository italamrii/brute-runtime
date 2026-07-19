# Cross-platform status

BRUTE Runtime's core engine (`src/`) now compiles for Windows, macOS,
and Linux. This document states exactly what that does and does not
mean - evidence-based, not aspirational.

## What "compiles" means here

Every platform-specific piece of hardware/system detection lives behind
`src/platform/{windows,macos,linux}.rs`, dispatched through stable,
OS-neutral functions in `hardware::{gpu,power,system}` (see
`docs/architecture.md`'s cross-platform section for the module
boundary). Compilation was verified from this Windows development
machine via real cross-target `cargo check`/`cargo clippy` runs:

```powershell
rustup target add x86_64-unknown-linux-gnu x86_64-apple-darwin aarch64-apple-darwin
cargo check --lib --target x86_64-unknown-linux-gnu
cargo check --lib --target x86_64-apple-darwin
cargo check --lib --target aarch64-apple-darwin
cargo clippy --lib --target x86_64-unknown-linux-gnu -- -D warnings
cargo clippy --lib --target aarch64-apple-darwin -- -D warnings
```

All five checks passed with zero errors and zero warnings. This proves
the code is syntactically and type-correct for each target and that
every macOS/Linux code path (registry-free OS version detection, GPU
enumeration via `system_profiler`/`lspci`, power state via
`pmset`/`sysfs`) at least *builds*.

## What "compiles" does not mean

**No macOS or Linux hardware has run this code.** Every value and every
degradation path in `platform/macos.rs` and `platform/linux.rs` -
whether `sw_vers`/`system_profiler`/`pmset` or `/etc/os-release`/
`lspci`/`/sys/class/power_supply` actually produce the expected output
format on a real, current macOS/Linux install - is unverified. A
subprocess's exact output format, a sysfs path's exact layout, or a
JSON key's exact name can differ across OS versions in ways that only
running the real binary on the real OS would catch.

**Only Windows is release-verified.** See
`docs/stage-0-verification.md` through `docs/stage-4-verification.md`
for the full real-hardware verification history - all of it is Windows.
Do not present macOS or Linux as "supported" in any release
communication until a native machine has actually run the built app,
imported a real model, verified a real backend, and completed a real
local generation - the same bar Windows already cleared.

## Per-platform detection method

| Fact | Windows | macOS | Linux |
|---|---|---|---|
| OS version | Registry (`HKLM\...\CurrentVersion`) | `sw_vers` | `/etc/os-release` + `uname -r` |
| CPU | `sysinfo` + `raw_cpuid` (x86 only) | `sysinfo` + `raw_cpuid` (x86 only) | `sysinfo` + `raw_cpuid` (x86 only) |
| Memory | `sysinfo` | `sysinfo` | `sysinfo` |
| Storage | `GetDiskFreeSpaceExW` | `sysinfo::Disks` | `sysinfo::Disks` |
| GPU adapters | DXGI (`EnumAdapters1`, every adapter) | `system_profiler SPDisplaysDataType -json` | `lspci -mm` |
| GPU driver (NVIDIA) | `nvidia-smi` | not applicable (no NVIDIA macOS driver since 10.13) | `nvidia-smi` |
| Vulkan | `vulkaninfo` or loader DLL presence | `vulkaninfo` (via MoltenVK) or none | `vulkaninfo` or loader `.so` presence |
| Power/battery | `GetSystemPowerStatus` | `pmset -g batt` | `/sys/class/power_supply/` |
| Local state root | `%LOCALAPPDATA%\BruteRuntime\` | `~/Library/Application Support/BruteRuntime/` | `$XDG_DATA_HOME/BruteRuntime/` or `~/.local/share/BruteRuntime/` |
| Random ID generation | `getrandom` crate (wraps `BCryptGenRandom`) | `getrandom` crate (wraps `getentropy`) | `getrandom` crate (wraps `getrandom(2)`/`/dev/urandom`) |
| Runtime binary name | `llama-cli.exe`/`llama-bench.exe` | `llama-cli`/`llama-bench` | `llama-cli`/`llama-bench` |

Windows keeps its original, already-verified direct Win32 API calls for
memory/storage rather than being switched to the shared `sysinfo`-based
path other platforms use for those two facts - not a correctness
concern (the numbers agree), just "don't change verified code without a
measured reason."

## Known architectural gaps not addressed in this pass

- **Per-process peak memory sampling** (used during benchmark/tuning
  runs) has a real implementation on Windows (`GetProcessMemoryInfo`)
  and Linux (`/proc/<pid>/status` `VmHWM`), but not macOS - it would
  need `libproc`/`task_info` FFI, which was judged out of scope for
  this pass. Returns `None` honestly on macOS rather than substituting
  a mislabeled current-RSS reading.
- **The Rust test suite itself is not yet cross-platform.**
  `runtime::process`'s tests still launch `cmd.exe` directly rather than
  a per-OS shell - they will fail to *run* (not fail to compile) on a
  Linux/macOS CI runner. This is a test-harness gap, not a shipped-code
  gap; fixing it is tracked as follow-up work for the CI pipeline
  (`docs/known-limitations.md`).
- **ARM64 CPU instruction-set detection.** `raw_cpuid` is x86/x86_64-only;
  `hardware::cpu::detect_instruction_sets` honestly reports an empty
  instruction-set list on ARM (Apple Silicon, ARM Linux) rather than
  guessing - llama.cpp's NEON/ARM SIMD kernel dispatch is a separate,
  not-yet-modeled concern.
- **No macOS/Linux packaging has been attempted or verified** - see
  `docs/windows-packaging.md` for the Windows MSI/NSIS pipeline, which
  has no macOS/Linux equivalent yet.
