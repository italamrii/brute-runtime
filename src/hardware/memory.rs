//! System memory via `GlobalMemoryStatusEx` - a direct Win32 API call, no
//! interpretation, so this is always `Measured` when it succeeds.

use super::{HardwareField, MemoryReport};
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

pub fn inspect_memory() -> MemoryReport {
    let mut status = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };

    match unsafe { GlobalMemoryStatusEx(&mut status) } {
        Ok(()) => MemoryReport {
            total_bytes: HardwareField::measured(
                status.ullTotalPhys,
                "Win32 GlobalMemoryStatusEx.ullTotalPhys",
            ),
            available_bytes: HardwareField::measured(
                status.ullAvailPhys,
                "Win32 GlobalMemoryStatusEx.ullAvailPhys",
            ),
        },
        Err(e) => MemoryReport {
            total_bytes: HardwareField::unavailable(format!("GlobalMemoryStatusEx failed: {e}")),
            available_bytes: HardwareField::unavailable(format!(
                "GlobalMemoryStatusEx failed: {e}"
            )),
        },
    }
}

/// Snapshot of just the two numbers, used for before/during/after sampling
/// around a benchmark run without re-deriving the full `MemoryReport`.
pub fn sample_bytes() -> Option<(u64, u64)> {
    let mut status = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    unsafe { GlobalMemoryStatusEx(&mut status) }
        .ok()
        .map(|()| (status.ullTotalPhys, status.ullAvailPhys))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_nonzero_total_memory_on_real_hardware() {
        let report = inspect_memory();
        assert!(report.total_bytes.value.unwrap_or(0) > 0);
    }
}
