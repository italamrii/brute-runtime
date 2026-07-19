//! GPU inspection dispatcher. The actual adapter-enumeration API is
//! genuinely OS-specific (DXGI on Windows, IOKit/`system_profiler` on
//! macOS, sysfs/`lspci`/Vulkan on Linux) and lives in
//! `crate::platform::{windows,macos,linux}`; this module only holds the
//! cross-platform dispatch plus two small helpers (`nvidia-smi` and
//! `vulkaninfo` subprocess queries) that are genuinely reusable across
//! more than one platform, so they exist exactly once rather than being
//! copy-pasted into every `platform::*` module.

use super::GpuReport;
use std::process::Command;

pub fn inspect_gpu() -> GpuReport {
    crate::platform::current::inspect_gpu()
}

/// Runs `nvidia-smi --query-gpu=name,driver_version --format=csv,noheader`
/// and returns `(gpu_name, driver_version)` pairs. Errors (tool missing,
/// non-zero exit) are surfaced, never treated as "no GPU". `nvidia-smi`
/// ships with the NVIDIA driver on both Windows and Linux (not macOS,
/// where NVIDIA dropped driver support - `platform::macos` never calls
/// this, hence the `allow(dead_code)` on that target only), so this is
/// shared rather than duplicated per platform.
#[cfg_attr(target_os = "macos", allow(dead_code))]
pub(crate) fn query_nvidia_smi() -> Result<Vec<(String, String)>, String> {
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

/// `true` only if the `vulkaninfo` CLI (bundled with the Vulkan SDK, or
/// commonly available via the platform's Vulkan loader package) is on
/// `PATH` and exits successfully - a real device-enumeration signal, not
/// mere loader-library presence. Shared across platforms; each
/// `platform::*::detect_vulkan` falls back to its own OS-specific
/// loader-presence check when this returns `false`.
pub(crate) fn vulkaninfo_reports_success() -> bool {
    Command::new("vulkaninfo")
        .arg("--summary")
        .output()
        .is_ok_and(|output| output.status.success())
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
