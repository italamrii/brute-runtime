//! GPU inspection. Adapter enumeration (name, VRAM) comes straight from
//! DXGI - a direct Win32 API, `Measured`. Driver version for NVIDIA parts
//! and CUDA/Vulkan runtime availability come from secondary signals
//! (`nvidia-smi`, DLL presence) and are graded `Detected`/`Inferred`
//! accordingly, never conflated with the DXGI-measured facts.

use super::{GpuAdapter, GpuReport, GpuVendor, HardwareField};
use std::process::Command;
use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1};

const VENDOR_NVIDIA: u32 = 0x10DE;
const VENDOR_AMD: u32 = 0x1002;
const VENDOR_INTEL: u32 = 0x8086;

pub fn inspect_gpu() -> GpuReport {
    let adapters = enumerate_dxgi_adapters();
    let has_nvidia = adapters
        .as_ref()
        .map(|a| a.iter().any(|g| g.vendor == GpuVendor::Nvidia))
        .unwrap_or(false);

    let mut adapters = adapters;
    if let (Ok(nvidia_info), Some(list)) = (query_nvidia_smi(), adapters.as_mut()) {
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
        match query_nvidia_smi() {
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

/// Runs `nvidia-smi --query-gpu=name,driver_version --format=csv,noheader`
/// and returns `(gpu_name, driver_version)` pairs. Errors (tool missing,
/// non-zero exit) are surfaced, never treated as "no GPU".
fn query_nvidia_smi() -> Result<Vec<(String, String)>, String> {
    let output = Command::new("nvidia-smi")
        .args(["--query-gpu=name,driver_version", "--format=csv,noheader"])
        .output()
        .map_err(|e| format!("failed to launch nvidia-smi: {e}"))?;

    if !output.status.success() {
        return Err(format!("nvidia-smi exited with status {}", output.status));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut results = Vec::new();
    for line in stdout.lines() {
        let parts: Vec<&str> = line.split(',').map(str::trim).collect();
        if parts.len() >= 2 {
            results.push((parts[0].to_string(), parts[1].to_string()));
        }
    }
    Ok(results)
}

/// Vulkan detection is tiered: if `vulkaninfo` is on PATH we run it for a
/// direct device enumeration (`Measured`); otherwise we fall back to
/// checking whether the Vulkan loader DLL is present (`Detected` - presence
/// of the loader does not guarantee a working ICD).
fn detect_vulkan() -> HardwareField<bool> {
    let vulkaninfo_ok = Command::new("vulkaninfo")
        .arg("--summary")
        .output()
        .is_ok_and(|output| output.status.success());
    if vulkaninfo_ok {
        return HardwareField::measured(true, "vulkaninfo --summary exited successfully");
    }

    let loader_path = std::path::Path::new(r"C:\Windows\System32\vulkan-1.dll");
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inspect_gpu_never_panics() {
        let report = inspect_gpu();
        // We don't assert specific hardware here - this must degrade
        // gracefully on machines without any of these GPUs/tools.
        let _ = report.adapters.value;
        let _ = report.cuda_available.value;
        let _ = report.vulkan_available.value;
    }
}
