//! OS version and storage-free-space dispatcher. The actual query is
//! genuinely OS-specific (Windows registry + `GetDiskFreeSpaceExW`,
//! macOS `sw_vers` + `statvfs`, Linux `/etc/os-release` + `statvfs`) and
//! lives in `crate::platform::{windows,macos,linux}`. Named `system`
//! rather than `windows` specifically so this module can dispatch to any
//! platform's implementation without a misleading name - see
//! `docs/architecture.md`'s cross-platform section for why the rename
//! happened and what depended on the old `hardware::windows` path.

use super::{OsReport, StorageReport};
use std::path::Path;

pub fn inspect_os() -> OsReport {
    crate::platform::current::inspect_os()
}

pub fn inspect_storage(path: &Path) -> StorageReport {
    crate::platform::current::inspect_storage(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inspect_os_reports_something_on_real_hardware() {
        let report = inspect_os();
        // Every platform must report at least one of these - never a
        // fully-empty report on a real, supported machine.
        assert!(
            report.product_name.value.is_some()
                || report.display_version.value.is_some()
                || report.build_number.value.is_some()
        );
    }

    #[test]
    fn inspect_storage_on_temp_dir_reports_nonzero_total() {
        let report = inspect_storage(&std::env::temp_dir());
        assert!(report.total_bytes.value.unwrap_or(0) > 0);
    }
}
