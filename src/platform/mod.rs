//! OS-specific system-integration code, isolated behind this module so
//! the rest of the engine (`hardware`, `identity`, `runtime`, etc.)
//! depends only on stable, platform-neutral function signatures - never
//! a scattered `cfg(target_os = ...)` inside otherwise-shared business
//! logic. See each submodule's own doc comment for exactly what it
//! implements and how honestly it degrades when the expected OS API or
//! system tool isn't present.
//!
//! **Verification status**: `windows` has been built, run, and tested
//! end-to-end on real hardware, including full manual GUI verification
//! (see `docs/stage-4-verification.md` and the Stage 0-4 verification
//! documents). `macos` has been built and run on real Apple Silicon
//! hardware: the full engine and desktop test suites pass there and the
//! bundled llama.cpp runtime is pin-verified and launches (`brute doctor`).
//! Full manual GUI end-to-end sign-off (installing the `.dmg` and running
//! a real model generation) has not been completed, so it is not yet
//! held to the same "release-ready" bar as Windows. `linux` still only
//! compiles and has not been run on real hardware. See
//! `docs/cross-platform.md` for exactly what each level does and does not
//! mean. Never state or imply more than the level a platform has reached.

#[cfg(windows)]
pub mod windows;

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(target_os = "linux")]
pub mod linux;

/// The current build target's platform implementation, re-exported
/// under one stable name so callers never need their own
/// `cfg(target_os = ...)` branch.
#[cfg(windows)]
pub use windows as current;

#[cfg(target_os = "macos")]
pub use macos as current;

#[cfg(target_os = "linux")]
pub use linux as current;

/// Shared storage-free-space query for macOS/Linux, via `sysinfo`'s
/// cross-platform `Disks` API (wraps `statfs`/`statvfs`) rather than a
/// hand-rolled FFI call on either platform. Matches `path` to whichever
/// disk's mount point is the longest real prefix of it - the standard
/// way to resolve "which filesystem is this path actually on" without a
/// direct syscall of our own. Windows keeps its own `GetDiskFreeSpaceExW`
/// implementation (`platform::windows::inspect_storage`) since it
/// already existed, was already verified on real hardware, and changing
/// it isn't warranted.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) fn disk_stats_via_sysinfo(path: &std::path::Path) -> crate::hardware::StorageReport {
    use crate::hardware::HardwareField;

    let query_dir = if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(|| path.to_path_buf())
    };

    let disks = sysinfo::Disks::new_with_refreshed_list();
    let best_match = disks
        .list()
        .iter()
        .filter(|d| query_dir.starts_with(d.mount_point()))
        .max_by_key(|d| d.mount_point().as_os_str().len());

    match best_match {
        Some(disk) => crate::hardware::StorageReport {
            path_queried: query_dir.display().to_string(),
            free_bytes: HardwareField::measured(
                disk.available_space(),
                "sysinfo::Disks (matched mount point)",
            ),
            total_bytes: HardwareField::measured(
                disk.total_space(),
                "sysinfo::Disks (matched mount point)",
            ),
        },
        None => crate::hardware::StorageReport {
            path_queried: query_dir.display().to_string(),
            free_bytes: HardwareField::unavailable(
                "no sysinfo disk entry's mount point contains this path",
            ),
            total_bytes: HardwareField::unavailable(
                "no sysinfo disk entry's mount point contains this path",
            ),
        },
    }
}
