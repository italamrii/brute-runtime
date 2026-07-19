//! macOS implementation of the OS-specific hardware/system queries
//! `brute` needs. Uses standard macOS command-line tools (`sw_vers`,
//! `system_profiler`, `pmset`) rather than native Cocoa/IOKit bindings,
//! matching the same "shell out to a stable, documented system tool"
//! pattern Windows detection already uses for `nvidia-smi`/`vulkaninfo`.
//! This keeps the module dependency-free (no `objc`/`core-foundation`
//! crates) and each tool's exact output is easy to audit.
//!
//! **Not yet run on real macOS hardware** - this compiles (verified via
//! cross-target `cargo check`) but every value/degradation path below is
//! unverified until a real Mac runs it. See `docs/cross-platform.md`.

use crate::hardware::power::{AcLineStatus, ChassisClass, PowerReport};
use crate::hardware::{GpuAdapter, GpuReport, GpuVendor, HardwareField, OsReport, StorageReport};
use std::path::Path;
use std::process::Command;

pub fn inspect_os() -> OsReport {
    let sw_vers = |flag: &str| -> Option<String> {
        Command::new("sw_vers")
            .arg(flag)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|s| !s.is_empty())
    };

    let product_name = match sw_vers("-productName") {
        Some(v) => HardwareField::measured(v, "sw_vers -productName"),
        None => HardwareField::unavailable("sw_vers -productName failed or returned nothing"),
    };
    let display_version = match sw_vers("-productVersion") {
        Some(v) => HardwareField::measured(v, "sw_vers -productVersion"),
        None => HardwareField::unavailable("sw_vers -productVersion failed or returned nothing"),
    };
    let build_number = match sw_vers("-buildVersion") {
        Some(v) => HardwareField::measured(v, "sw_vers -buildVersion"),
        None => HardwareField::unavailable("sw_vers -buildVersion failed or returned nothing"),
    };

    OsReport {
        product_name,
        display_version,
        build_number,
        process_architecture: HardwareField::measured(
            std::env::consts::ARCH.to_string(),
            "std::env::consts::ARCH (compile-time target of this binary)",
        ),
        // Rosetta 2 lets an x86_64 binary run on Apple Silicon - the
        // `arch` command reports the CPU's real architecture regardless
        // of which slice of this binary is executing, unlike `uname -m`
        // under emulation.
        native_architecture: match Command::new("arch").output() {
            Ok(o) if o.status.success() => HardwareField::detected(
                String::from_utf8_lossy(&o.stdout).trim().to_string(),
                "`arch` command (reports true native architecture, including under Rosetta 2)",
            ),
            _ => HardwareField::unavailable("`arch` command failed or was not found"),
        },
    }
}

/// Storage free/total space via `sysinfo`'s cross-platform `Disks` API
/// (wraps `statfs` on macOS) - never a hand-rolled FFI call - matched
/// against the disk whose mount point is the longest real prefix of
/// `path`, the standard way to resolve "which filesystem is this path
/// on" without a direct `statfs` syscall.
pub fn inspect_storage(path: &Path) -> StorageReport {
    crate::platform::disk_stats_via_sysinfo(path)
}

pub fn inspect_gpu() -> GpuReport {
    let adapters = query_system_profiler_displays();
    let adapters_field = match adapters {
        Some(list) => HardwareField::detected(
            list,
            "system_profiler SPDisplaysDataType -json (subprocess, parsed JSON)",
        ),
        None => HardwareField::unavailable(
            "system_profiler SPDisplaysDataType failed or returned unparseable output",
        ),
    };

    // NVIDIA dropped macOS driver support after 10.13 - CUDA is not a
    // realistic backend on any currently-supported macOS version.
    let cuda_available = HardwareField::detected(
        false,
        "NVIDIA/CUDA has not shipped a supported macOS driver since 10.13 - never reported available",
    );

    GpuReport {
        adapters: adapters_field,
        cuda_available,
        vulkan_available: detect_vulkan(),
    }
}

/// Parses `system_profiler SPDisplaysDataType -json`'s
/// `SPDisplaysDataType[].sppci_model`/`spdisplays_vram*` fields. Every
/// GPU entry `system_profiler` reports is included - integrated and
/// discrete alike - never just the first.
fn query_system_profiler_displays() -> Option<Vec<GpuAdapter>> {
    let output = Command::new("system_profiler")
        .args(["SPDisplaysDataType", "-json"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    let entries = json.get("SPDisplaysDataType")?.as_array()?;

    let mut adapters = Vec::new();
    for entry in entries {
        let name = entry
            .get("sppci_model")
            .or_else(|| entry.get("_name"))
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown GPU")
            .to_string();

        let vendor = if name.to_lowercase().contains("apple") {
            GpuVendor::Other // Apple Silicon integrated GPU - no existing enum variant fits cleanly
        } else if name.to_lowercase().contains("amd") || name.to_lowercase().contains("radeon") {
            GpuVendor::Amd
        } else if name.to_lowercase().contains("intel") {
            GpuVendor::Intel
        } else if name.to_lowercase().contains("nvidia") {
            GpuVendor::Nvidia
        } else {
            GpuVendor::Other
        };

        let vram_bytes = entry
            .get("sppci_vram")
            .or_else(|| entry.get("spdisplays_vram"))
            .and_then(|v| v.as_str())
            .and_then(parse_vram_string);

        adapters.push(GpuAdapter {
            name,
            vendor,
            dedicated_vram_bytes: vram_bytes,
            shared_system_memory_bytes: None,
            driver_version: None,
        });
    }
    Some(adapters)
}

/// `system_profiler` reports VRAM as a human string like `"8 GB"` or
/// `"1536 MB"` - parsed conservatively, `None` (never a guessed number)
/// on any format that doesn't match exactly.
fn parse_vram_string(s: &str) -> Option<u64> {
    let s = s.trim();
    let (number, unit) = s.split_once(' ')?;
    let value: f64 = number.parse().ok()?;
    match unit.to_uppercase().as_str() {
        "GB" => Some((value * 1024.0 * 1024.0 * 1024.0) as u64),
        "MB" => Some((value * 1024.0 * 1024.0) as u64),
        _ => None,
    }
}

fn detect_vulkan() -> HardwareField<bool> {
    // MoltenVK provides a Vulkan-over-Metal loader on macOS when the
    // Vulkan SDK is installed - `vulkaninfo` is the same real
    // device-enumeration signal used on Windows/Linux.
    if crate::hardware::gpu::vulkaninfo_reports_success() {
        return HardwareField::measured(
            true,
            "vulkaninfo --summary exited successfully (via MoltenVK)",
        );
    }
    HardwareField::detected(
        false,
        "vulkaninfo not on PATH - the Vulkan SDK/MoltenVK does not appear to be installed",
    )
}

pub fn inspect_power() -> PowerReport {
    let output = match Command::new("pmset").args(["-g", "batt"]).output() {
        Ok(o) if o.status.success() => o,
        _ => {
            return PowerReport {
                ac_line_status: HardwareField::unavailable("pmset -g batt failed or was not found"),
                battery_percent: HardwareField::unavailable(
                    "pmset -g batt failed or was not found",
                ),
                battery_present: HardwareField::unavailable(
                    "pmset -g batt failed or was not found",
                ),
                chassis_class: HardwareField::unavailable("pmset -g batt failed or was not found"),
            };
        }
    };
    let text = String::from_utf8_lossy(&output.stdout);

    // Typical output: "Now drawing from 'AC Power' ... -InternalBattery-0 (id=...)\t95%; charged; ..."
    // A Mac with no battery line at all (Mac mini/Studio/Pro) has no
    // system battery - reported honestly, never guessed as a laptop.
    let ac_line_status = if text.contains("AC Power") {
        HardwareField::measured(
            AcLineStatus::Online,
            "pmset -g batt (\"Now drawing from 'AC Power'\")",
        )
    } else if text.contains("Battery Power") {
        HardwareField::measured(
            AcLineStatus::Offline,
            "pmset -g batt (\"Now drawing from 'Battery Power'\")",
        )
    } else {
        HardwareField::unavailable(
            "pmset -g batt output did not contain a recognized power-source line",
        )
    };

    let has_battery_line = text.contains("InternalBattery");
    let battery_present = HardwareField::detected(
        has_battery_line,
        "pmset -g batt (presence of an InternalBattery line)",
    );

    let battery_percent = has_battery_line
        .then(|| {
            text.split('\t').find_map(|part| {
                part.split('%')
                    .next()
                    .and_then(|n| n.trim().parse::<u8>().ok())
            })
        })
        .flatten()
        .map(|p| HardwareField::measured(p, "pmset -g batt (percentage field)"))
        .unwrap_or_else(|| {
            HardwareField::unavailable("battery percentage not found in pmset output")
        });

    let chassis_class = if has_battery_line {
        HardwareField::inferred(
            ChassisClass::Laptop,
            "inferred from an InternalBattery line being present in pmset -g batt",
        )
    } else {
        HardwareField::inferred(
            ChassisClass::Desktop,
            "inferred from no InternalBattery line in pmset -g batt",
        )
    };

    PowerReport {
        ac_line_status,
        battery_percent,
        battery_present,
        chassis_class,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_vram_string_handles_gb_and_mb() {
        assert_eq!(parse_vram_string("8 GB"), Some(8 * 1024 * 1024 * 1024));
        assert_eq!(parse_vram_string("1536 MB"), Some(1536 * 1024 * 1024));
        assert_eq!(parse_vram_string("garbage"), None);
    }
}
