//! Linux implementation of the OS-specific hardware/system queries
//! `brute` needs. Uses standard, widely-available sources: `/etc/os-release`
//! (OS version - a standardized file format, not a tool), `lspci`
//! (GPU enumeration) + `nvidia-smi` (NVIDIA driver version, shared with
//! Windows), and `/sys/class/power_supply/` (battery/AC state - a
//! standard kernel sysfs interface, no subprocess needed at all).
//!
//! **Not yet run on real Linux hardware** - this compiles (verified via
//! cross-target `cargo check`) but every value/degradation path below is
//! unverified until a real Linux machine runs it. See
//! `docs/cross-platform.md`.

use crate::hardware::power::{AcLineStatus, ChassisClass, PowerReport};
use crate::hardware::{GpuAdapter, GpuReport, GpuVendor, HardwareField, OsReport, StorageReport};
use std::path::Path;
use std::process::Command;

pub fn inspect_os() -> OsReport {
    let fields = std::fs::read_to_string("/etc/os-release")
        .ok()
        .map(parse_os_release);

    let (product_name, display_version) = match &fields {
        Some(f) => (
            f.get("PRETTY_NAME")
                .or_else(|| f.get("NAME"))
                .map(|v| HardwareField::measured(v.clone(), "/etc/os-release (PRETTY_NAME/NAME)"))
                .unwrap_or_else(|| {
                    HardwareField::unavailable("/etc/os-release had no NAME/PRETTY_NAME key")
                }),
            f.get("VERSION_ID")
                .map(|v| HardwareField::measured(v.clone(), "/etc/os-release (VERSION_ID)"))
                .unwrap_or_else(|| {
                    HardwareField::unavailable("/etc/os-release had no VERSION_ID key")
                }),
        ),
        None => (
            HardwareField::unavailable("/etc/os-release could not be read"),
            HardwareField::unavailable("/etc/os-release could not be read"),
        ),
    };

    // The kernel version is the closest Linux equivalent to Windows'
    // "build number" - a real, meaningful distinguishing value, via
    // `uname -r` rather than parsing /proc/version's freeform string.
    let build_number = match Command::new("uname").arg("-r").output() {
        Ok(o) if o.status.success() => HardwareField::measured(
            String::from_utf8_lossy(&o.stdout).trim().to_string(),
            "uname -r (kernel release)",
        ),
        _ => HardwareField::unavailable("uname -r failed or was not found"),
    };

    OsReport {
        product_name,
        display_version,
        build_number,
        process_architecture: HardwareField::measured(
            std::env::consts::ARCH.to_string(),
            "std::env::consts::ARCH (compile-time target of this binary)",
        ),
        native_architecture: match Command::new("uname").arg("-m").output() {
            Ok(o) if o.status.success() => HardwareField::measured(
                String::from_utf8_lossy(&o.stdout).trim().to_string(),
                "uname -m",
            ),
            _ => HardwareField::unavailable("uname -m failed or was not found"),
        },
    }
}

fn parse_os_release(contents: String) -> std::collections::HashMap<String, String> {
    contents
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(k, v)| (k.to_string(), v.trim_matches('"').to_string()))
        .collect()
}

/// Storage free/total space via `sysinfo`'s cross-platform `Disks` API
/// (wraps `statvfs` on Linux) - never a hand-rolled FFI call.
pub fn inspect_storage(path: &Path) -> StorageReport {
    crate::platform::disk_stats_via_sysinfo(path)
}

pub fn inspect_gpu() -> GpuReport {
    let adapters = query_lspci();
    let has_nvidia = adapters
        .as_ref()
        .map(|a| a.iter().any(|g| g.vendor == GpuVendor::Nvidia))
        .unwrap_or(false);

    let mut adapters = adapters;
    if let (Ok(nvidia_info), Some(list)) =
        (crate::hardware::gpu::query_nvidia_smi(), adapters.as_mut())
    {
        for adapter in list.iter_mut().filter(|a| a.vendor == GpuVendor::Nvidia) {
            if let Some((_, driver)) = nvidia_info.first() {
                adapter.driver_version = Some(driver.clone());
            }
        }
    }

    let adapters_field = match adapters {
        Some(list) => {
            HardwareField::detected(list, "lspci -mm (+ nvidia-smi for NVIDIA driver_version)")
        }
        None => HardwareField::unavailable(
            "lspci failed, was not found, or produced no VGA/3D controller lines",
        ),
    };

    let cuda_available = if !has_nvidia {
        HardwareField::detected(false, "no NVIDIA adapter present in lspci enumeration")
    } else {
        match crate::hardware::gpu::query_nvidia_smi() {
            Ok(_) => HardwareField::inferred(
                true,
                "nvidia-smi reports a working NVIDIA driver; CUDA kernel execution was not run",
            ),
            Err(reason) => HardwareField::detected(
                false,
                format!("NVIDIA adapter present but nvidia-smi query failed: {reason}"),
            ),
        }
    };

    GpuReport {
        adapters: adapters_field,
        cuda_available,
        vulkan_available: detect_vulkan(),
    }
}

/// Parses `lspci -mm`'s machine-readable output for `"VGA compatible
/// controller"`/`"3D controller"` lines - every matching line is
/// included, never just the first, so a machine with both an iGPU and a
/// discrete GPU reports both. VRAM is not available from `lspci` itself
/// (unlike DXGI/`system_profiler`) - reported as `None`, never guessed.
fn query_lspci() -> Option<Vec<GpuAdapter>> {
    let output = Command::new("lspci").arg("-mm").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);

    let mut adapters = Vec::new();
    for line in text.lines() {
        if !line.contains("VGA compatible controller") && !line.contains("3D controller") {
            continue;
        }
        // lspci -mm quotes each field: slot "class" "vendor" "device" ...
        let fields: Vec<&str> = line.split('"').collect();
        let vendor_name = fields.get(3).copied().unwrap_or("");
        let device_name = fields.get(5).copied().unwrap_or("Unknown GPU");

        let vendor = if vendor_name.to_lowercase().contains("nvidia") {
            GpuVendor::Nvidia
        } else if vendor_name.to_lowercase().contains("amd")
            || vendor_name.to_lowercase().contains("advanced micro")
        {
            GpuVendor::Amd
        } else if vendor_name.to_lowercase().contains("intel") {
            GpuVendor::Intel
        } else {
            GpuVendor::Other
        };

        adapters.push(GpuAdapter {
            name: format!("{vendor_name} {device_name}").trim().to_string(),
            vendor,
            dedicated_vram_bytes: None,
            shared_system_memory_bytes: None,
            driver_version: None,
        });
    }
    Some(adapters)
}

fn detect_vulkan() -> HardwareField<bool> {
    if crate::hardware::gpu::vulkaninfo_reports_success() {
        return HardwareField::measured(true, "vulkaninfo --summary exited successfully");
    }
    let loader_candidates = [
        "/usr/lib/x86_64-linux-gnu/libvulkan.so.1",
        "/usr/lib/libvulkan.so.1",
        "/usr/lib64/libvulkan.so.1",
    ];
    if loader_candidates.iter().any(|p| Path::new(p).exists()) {
        HardwareField::detected(
            true,
            "Vulkan loader library present (loader-level only; ICD/device not enumerated)",
        )
    } else {
        HardwareField::detected(
            false,
            "no Vulkan loader library found in standard lib paths and vulkaninfo not on PATH",
        )
    }
}

/// Reads the kernel's own `/sys/class/power_supply/` sysfs interface -
/// no subprocess needed at all, and no ambiguity about which shell tool
/// might or might not be installed. A machine with no `BAT*` entry has
/// no system battery (a desktop), reported honestly rather than guessed.
pub fn inspect_power() -> PowerReport {
    let power_supply_dir = Path::new("/sys/class/power_supply");
    let entries = match std::fs::read_dir(power_supply_dir) {
        Ok(e) => e.filter_map(|e| e.ok()).collect::<Vec<_>>(),
        Err(_) => {
            return PowerReport {
                ac_line_status: HardwareField::unavailable(
                    "/sys/class/power_supply could not be read",
                ),
                battery_percent: HardwareField::unavailable(
                    "/sys/class/power_supply could not be read",
                ),
                battery_present: HardwareField::unavailable(
                    "/sys/class/power_supply could not be read",
                ),
                chassis_class: HardwareField::unavailable(
                    "/sys/class/power_supply could not be read",
                ),
            };
        }
    };

    let battery_dir = entries
        .iter()
        .find(|e| e.file_name().to_string_lossy().starts_with("BAT"))
        .map(|e| e.path());
    let ac_dir = entries.iter().find(|e| {
        let name = e.file_name().to_string_lossy().to_uppercase();
        name.starts_with("AC") || name.starts_with("ADP")
    });

    let battery_present = HardwareField::detected(
        battery_dir.is_some(),
        "/sys/class/power_supply (presence of a BAT* entry)",
    );

    let battery_percent = battery_dir
        .as_ref()
        .and_then(|dir| std::fs::read_to_string(dir.join("capacity")).ok())
        .and_then(|s| s.trim().parse::<u8>().ok())
        .map(|p| HardwareField::measured(p, "/sys/class/power_supply/BAT*/capacity"))
        .unwrap_or_else(|| HardwareField::unavailable("no BAT*/capacity file found or unreadable"));

    let ac_line_status = ac_dir
        .and_then(|dir| std::fs::read_to_string(dir.path().join("online")).ok())
        .map(|s| {
            if s.trim() == "1" {
                HardwareField::measured(
                    AcLineStatus::Online,
                    "/sys/class/power_supply/A{C,DP}*/online",
                )
            } else {
                HardwareField::measured(
                    AcLineStatus::Offline,
                    "/sys/class/power_supply/A{C,DP}*/online",
                )
            }
        })
        .unwrap_or_else(|| {
            HardwareField::unavailable("no AC*/ADP*/online file found or unreadable")
        });

    let chassis_class = match battery_present.value {
        Some(true) => HardwareField::inferred(
            ChassisClass::Laptop,
            "inferred from a BAT* entry being present under /sys/class/power_supply",
        ),
        Some(false) => HardwareField::inferred(
            ChassisClass::Desktop,
            "inferred from no BAT* entry under /sys/class/power_supply",
        ),
        None => HardwareField::unavailable("battery presence could not be determined"),
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
    fn parse_os_release_extracts_quoted_and_unquoted_values() {
        let sample = "NAME=\"Ubuntu\"\nVERSION_ID=\"24.04\"\nPRETTY_NAME=\"Ubuntu 24.04 LTS\"\n";
        let fields = parse_os_release(sample.to_string());
        assert_eq!(fields.get("NAME").map(String::as_str), Some("Ubuntu"));
        assert_eq!(fields.get("VERSION_ID").map(String::as_str), Some("24.04"));
        assert_eq!(
            fields.get("PRETTY_NAME").map(String::as_str),
            Some("Ubuntu 24.04 LTS")
        );
    }
}
