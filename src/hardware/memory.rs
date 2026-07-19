//! System memory via `sysinfo` - a cross-platform crate that wraps each
//! OS's own native memory query (`GlobalMemoryStatusEx` on Windows,
//! `host_statistics64`/`sysctl` on macOS, `/proc/meminfo` on Linux), so
//! this needs no platform-specific code of its own. Always `Measured`
//! when the OS call succeeds - never interpreted or estimated.

use super::{HardwareField, MemoryReport};
use sysinfo::System;

pub fn inspect_memory() -> MemoryReport {
    let mut sys = System::new_all();
    sys.refresh_memory();

    let total = sys.total_memory();
    let available = sys.available_memory();

    if total == 0 {
        return MemoryReport {
            total_bytes: HardwareField::unavailable("sysinfo::System::total_memory reported 0"),
            available_bytes: HardwareField::unavailable(
                "sysinfo::System::available_memory reported 0",
            ),
        };
    }

    MemoryReport {
        total_bytes: HardwareField::measured(total, "sysinfo::System::total_memory"),
        available_bytes: HardwareField::measured(available, "sysinfo::System::available_memory"),
    }
}

/// Snapshot of just the two numbers, used for before/during/after sampling
/// around a benchmark run without re-deriving the full `MemoryReport`.
pub fn sample_bytes() -> Option<(u64, u64)> {
    let mut sys = System::new_all();
    sys.refresh_memory();
    let total = sys.total_memory();
    if total == 0 {
        return None;
    }
    Some((total, sys.available_memory()))
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
