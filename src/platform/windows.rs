//! Windows implementation of every OS-specific hardware/system query
//! `brute` needs: OS version + storage free space (registry +
//! `GetDiskFreeSpaceExW`), GPU adapter enumeration (DXGI) + CUDA/Vulkan
//! detection, and power/battery/chassis state (`GetSystemPowerStatus`).
//! This is a straight relocation of Stage 0/1's original Windows-only
//! `hardware::windows`/`hardware::gpu`/`hardware::power` implementations,
//! with no behavior change, verified by the unchanged test suite. See
//! `docs/stage-0-verification.md` through `docs/stage-4-verification.md`
//! for what has actually been run on real Windows hardware.

use crate::hardware::power::{AcLineStatus, ChassisClass, PowerReport};
use crate::hardware::{GpuAdapter, GpuReport, GpuVendor, HardwareField, OsReport, StorageReport};
use std::path::Path;
use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1};
use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
use windows::Win32::System::Power::GetSystemPowerStatus;
use windows::core::HSTRING;
use winreg::RegKey;
use winreg::enums::HKEY_LOCAL_MACHINE;

const CURRENT_VERSION_KEY: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";
const VENDOR_NVIDIA: u32 = 0x10DE;
const VENDOR_AMD: u32 = 0x1002;
const VENDOR_INTEL: u32 = 0x8086;

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

pub fn inspect_gpu() -> GpuReport {
    let adapters = enumerate_dxgi_adapters();
    let has_nvidia = adapters
        .as_ref()
        .map(|a| a.iter().any(|g| g.vendor == GpuVendor::Nvidia))
        .unwrap_or(false);

    let mut adapters = adapters;
    if let (Ok(nvidia_info), Some(list)) =
        (crate::hardware::gpu::query_nvidia_smi(), adapters.as_mut())
    {
        for adapter in list.iter_mut().filter(|a| a.vendor == GpuVendor::Nvidia) {
            if let Some((_, driver)) = nvidia_info.iter().find(|(name, _)| {
                adapter.name.contains(name.as_str()) || name.contains(&adapter.name)
            }) {
                adapter.driver_version = Some(driver.clone());
            } else if let Some((_, driver)) = nvidia_info.first() {
                adapter.driver_version = Some(driver.clone());
            }
        }
    }

    let adapters_field = match adapters {
        Some(list) => HardwareField::measured(
            list,
            "Win32 DXGI IDXGIFactory1::EnumAdapters1 (+ nvidia-smi for NVIDIA driver_version)",
        ),
        None => HardwareField::unavailable("DXGI factory/adapter enumeration failed"),
    };

    let cuda_available = if !has_nvidia {
        HardwareField::detected(false, "no NVIDIA adapter present in DXGI enumeration")
    } else {
        match crate::hardware::gpu::query_nvidia_smi() {
            Ok(_) => HardwareField::inferred(
                true,
                "nvidia-smi reports a working NVIDIA driver; CUDA kernel execution was not run in Stage 0",
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

/// Enumerates *every* DXGI adapter (`EnumAdapters1(0)`, `(1)`, `(2)`, ...
/// until `DXGI_ERROR_NOT_FOUND`) - never just the first, so a machine
/// with both an integrated and a discrete GPU (e.g. Intel UHD + NVIDIA)
/// reports both.
fn enumerate_dxgi_adapters() -> Option<Vec<GpuAdapter>> {
    let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1() }.ok()?;
    let mut adapters = Vec::new();

    for i in 0.. {
        let adapter = match unsafe { factory.EnumAdapters1(i) } {
            Ok(a) => a,
            Err(_) => break, // DXGI_ERROR_NOT_FOUND: end of list
        };
        let desc = match unsafe { adapter.GetDesc1() } {
            Ok(d) => d,
            Err(_) => continue,
        };

        let name = String::from_utf16_lossy(&desc.Description)
            .trim_end_matches('\0')
            .to_string();

        let vendor = match desc.VendorId {
            VENDOR_NVIDIA => GpuVendor::Nvidia,
            VENDOR_AMD => GpuVendor::Amd,
            VENDOR_INTEL => GpuVendor::Intel,
            _ => GpuVendor::Other,
        };

        adapters.push(GpuAdapter {
            name,
            vendor,
            dedicated_vram_bytes: Some(desc.DedicatedVideoMemory as u64),
            shared_system_memory_bytes: Some(desc.SharedSystemMemory as u64),
            driver_version: None,
        });
    }

    Some(adapters)
}

/// Vulkan detection is tiered: if `vulkaninfo` is on PATH we run it for a
/// direct device enumeration (`Measured`); otherwise we fall back to
/// checking whether the Vulkan loader DLL is present (`Detected` - presence
/// of the loader does not guarantee a working ICD).
fn detect_vulkan() -> HardwareField<bool> {
    if crate::hardware::gpu::vulkaninfo_reports_success() {
        return HardwareField::measured(true, "vulkaninfo --summary exited successfully");
    }

    let loader_path = Path::new(r"C:\Windows\System32\vulkan-1.dll");
    if loader_path.exists() {
        HardwareField::detected(
            true,
            "vulkan-1.dll present in System32 (loader-level only; ICD/device not enumerated)",
        )
    } else {
        HardwareField::detected(
            false,
            "vulkan-1.dll not found in System32 and vulkaninfo not on PATH",
        )
    }
}

pub fn inspect_power() -> PowerReport {
    let mut status = windows::Win32::System::Power::SYSTEM_POWER_STATUS::default();
    let ok = unsafe { GetSystemPowerStatus(&mut status) };

    if ok.is_err() {
        return PowerReport {
            ac_line_status: HardwareField::unavailable("GetSystemPowerStatus failed"),
            battery_percent: HardwareField::unavailable("GetSystemPowerStatus failed"),
            battery_present: HardwareField::unavailable("GetSystemPowerStatus failed"),
            chassis_class: HardwareField::unavailable("GetSystemPowerStatus failed"),
        };
    }

    let ac_line_status = match status.ACLineStatus {
        0 => HardwareField::measured(
            AcLineStatus::Offline,
            "Win32 GetSystemPowerStatus.ACLineStatus",
        ),
        1 => HardwareField::measured(
            AcLineStatus::Online,
            "Win32 GetSystemPowerStatus.ACLineStatus",
        ),
        _ => HardwareField::measured(
            AcLineStatus::Unknown,
            "Win32 GetSystemPowerStatus.ACLineStatus",
        ),
    };

    // BatteryFlag bit 128 means "no system battery"; 255 means "unknown
    // status" (which we treat as unavailable rather than guessing either
    // way).
    let battery_flag = status.BatteryFlag;
    let battery_present = if battery_flag == 255 {
        HardwareField::unavailable("BatteryFlag reported unknown (0xFF)")
    } else {
        HardwareField::detected(
            battery_flag & 128 == 0,
            "Win32 GetSystemPowerStatus.BatteryFlag bit 7 (no-system-battery)",
        )
    };

    let battery_percent = if status.BatteryLifePercent == 255 {
        HardwareField::unavailable("BatteryLifePercent reported unknown (0xFF)")
    } else {
        HardwareField::measured(
            status.BatteryLifePercent,
            "Win32 GetSystemPowerStatus.BatteryLifePercent",
        )
    };

    let chassis_class = match battery_present.value {
        Some(true) => HardwareField::inferred(
            ChassisClass::Laptop,
            "inferred from a system battery being present (GetSystemPowerStatus); not a direct chassis-type query",
        ),
        Some(false) => HardwareField::inferred(
            ChassisClass::Desktop,
            "inferred from no system battery being present (GetSystemPowerStatus); not a direct chassis-type query",
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
    fn inspect_os_reports_a_product_name_on_real_windows() {
        let report = inspect_os();
        assert!(report.product_name.value.is_some());
    }

    #[test]
    fn inspect_storage_on_temp_dir_reports_nonzero_total() {
        let report = inspect_storage(&std::env::temp_dir());
        assert!(report.total_bytes.value.unwrap_or(0) > 0);
    }

    #[test]
    fn inspect_gpu_never_panics() {
        let report = inspect_gpu();
        let _ = report.adapters.value;
        let _ = report.cuda_available.value;
        let _ = report.vulkan_available.value;
    }

    #[test]
    fn inspect_power_never_panics() {
        let report = inspect_power();
        let _ = report.chassis_class.value;
    }
}
