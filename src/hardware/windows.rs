//! Windows OS version (registry), process/native architecture, and storage
//! free space for a given path.

use super::{HardwareField, OsReport, StorageReport};
use std::path::Path;
use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
use windows::core::HSTRING;
use winreg::RegKey;
use winreg::enums::HKEY_LOCAL_MACHINE;

const CURRENT_VERSION_KEY: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";

pub fn inspect_os() -> OsReport {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let key = hklm.open_subkey(CURRENT_VERSION_KEY);

    let (product_name, display_version, build_number) = match key {
        Ok(k) => {
            let product_name = k
                .get_value::<String, _>("ProductName")
                .map(|v| {
                    HardwareField::measured(
                        v,
                        format!(r"registry HKLM\{CURRENT_VERSION_KEY}\ProductName"),
                    )
                })
                .unwrap_or_else(|e| {
                    HardwareField::unavailable(format!("ProductName read failed: {e}"))
                });

            let display_version = k
                .get_value::<String, _>("DisplayVersion")
                .map(|v| {
                    HardwareField::measured(
                        v,
                        format!(r"registry HKLM\{CURRENT_VERSION_KEY}\DisplayVersion"),
                    )
                })
                .unwrap_or_else(|e| {
                    HardwareField::unavailable(format!("DisplayVersion read failed: {e}"))
                });

            // CurrentBuildNumber + UBR together give the full build (e.g. 26200.1000).
            let build = k.get_value::<String, _>("CurrentBuildNumber").ok();
            let ubr = k.get_value::<u32, _>("UBR").ok();
            let build_number = match (build, ubr) {
                (Some(b), Some(u)) => HardwareField::measured(
                    format!("{b}.{u}"),
                    format!(r"registry HKLM\{CURRENT_VERSION_KEY}\CurrentBuildNumber+UBR"),
                ),
                (Some(b), None) => HardwareField::measured(
                    b,
                    format!(r"registry HKLM\{CURRENT_VERSION_KEY}\CurrentBuildNumber"),
                ),
                _ => HardwareField::unavailable("CurrentBuildNumber read failed"),
            };

            (product_name, display_version, build_number)
        }
        Err(e) => {
            let unavailable =
                || HardwareField::unavailable(format!("registry key open failed: {e}"));
            (unavailable(), unavailable(), unavailable())
        }
    };

    OsReport {
        product_name,
        display_version,
        build_number,
        process_architecture: HardwareField::measured(
            std::env::consts::ARCH.to_string(),
            "std::env::consts::ARCH (compile-time target of this binary)",
        ),
        native_architecture: detect_native_architecture(),
    }
}

/// The OS loader sets `PROCESSOR_ARCHITEW6432` on a 32-bit process running
/// under WOW64 to the *true* native architecture, and always sets
/// `PROCESSOR_ARCHITECTURE` to the architecture as seen by the current
/// process. Preferring the former (when present) avoids misreporting a
/// 64-bit machine as 32-bit just because this binary happened to be x86.
fn detect_native_architecture() -> HardwareField<String> {
    if let Ok(v) = std::env::var("PROCESSOR_ARCHITEW6432") {
        return HardwareField::detected(v, "env var PROCESSOR_ARCHITEW6432 (set under WOW64)");
    }
    if let Ok(v) = std::env::var("PROCESSOR_ARCHITECTURE") {
        return HardwareField::detected(v, "env var PROCESSOR_ARCHITECTURE");
    }
    HardwareField::unavailable("neither PROCESSOR_ARCHITEW6432 nor PROCESSOR_ARCHITECTURE set")
}

pub fn inspect_storage(path: &Path) -> StorageReport {
    let query_dir = if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| path.to_path_buf())
    };

    let wide = HSTRING::from(query_dir.as_os_str());
    let mut free_available: u64 = 0;
    let mut total: u64 = 0;

    let result =
        unsafe { GetDiskFreeSpaceExW(&wide, Some(&mut free_available), Some(&mut total), None) };

    match result {
        Ok(()) => StorageReport {
            path_queried: query_dir.display().to_string(),
            free_bytes: HardwareField::measured(free_available, "Win32 GetDiskFreeSpaceExW"),
            total_bytes: HardwareField::measured(total, "Win32 GetDiskFreeSpaceExW"),
        },
        Err(e) => StorageReport {
            path_queried: query_dir.display().to_string(),
            free_bytes: HardwareField::unavailable(format!("GetDiskFreeSpaceExW failed: {e}")),
            total_bytes: HardwareField::unavailable(format!("GetDiskFreeSpaceExW failed: {e}")),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inspect_os_reports_a_product_name_on_real_windows() {
        let report = inspect_os();
        assert!(report.product_name.value.is_some());
    }

    #[test]
    fn inspect_storage_on_temp_dir_reports_nonzero_total() {
        let report = inspect_storage(&std::env::temp_dir());
        assert!(report.total_bytes.value.unwrap_or(0) > 0);
    }
}
