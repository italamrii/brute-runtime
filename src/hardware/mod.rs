//! Hardware inspection. Every fact we report carries a `Confidence` so a
//! downstream reader (human or the recommendation engine) can tell the
//! difference between "the OS told us this directly" and "we guessed."

pub mod cpu;
pub mod gpu;
pub mod memory;
pub mod power;
pub mod system;

use serde::Serialize;

/// How sure we are about a reported value.
///
/// - `Measured`: read directly from an OS API or CPU instruction with no
///   interpretation (e.g. `GlobalMemoryStatusEx`, `cpuid`).
/// - `Detected`: obtained from an external tool or indirect signal we trust
///   but do not control (e.g. `nvidia-smi` output, a DLL's presence).
/// - `Inferred`: derived via heuristic from other facts, not observed
///   directly (e.g. "CUDA is probably usable" from GPU vendor + driver
///   presence, without having actually run a CUDA kernel).
/// - `Unavailable`: we tried and could not determine the value. Never
///   silently defaulted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    Measured,
    Detected,
    Inferred,
    Unavailable,
}

/// A single reported fact, always paired with its confidence and the exact
/// source that produced it (an API name, a tool invocation, or the reason it
/// is unavailable).
#[derive(Debug, Clone, Serialize)]
pub struct HardwareField<T> {
    pub value: Option<T>,
    pub confidence: Confidence,
    pub source: String,
}

impl<T> HardwareField<T> {
    pub fn measured(value: T, source: impl Into<String>) -> Self {
        Self {
            value: Some(value),
            confidence: Confidence::Measured,
            source: source.into(),
        }
    }

    pub fn detected(value: T, source: impl Into<String>) -> Self {
        Self {
            value: Some(value),
            confidence: Confidence::Detected,
            source: source.into(),
        }
    }

    pub fn inferred(value: T, source: impl Into<String>) -> Self {
        Self {
            value: Some(value),
            confidence: Confidence::Inferred,
            source: source.into(),
        }
    }

    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self {
            value: None,
            confidence: Confidence::Unavailable,
            source: reason.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CpuReport {
    pub vendor: HardwareField<String>,
    pub brand: HardwareField<String>,
    pub physical_cores: HardwareField<usize>,
    pub logical_cores: HardwareField<usize>,
    pub instruction_sets: HardwareField<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryReport {
    pub total_bytes: HardwareField<u64>,
    pub available_bytes: HardwareField<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GpuAdapter {
    pub name: String,
    pub vendor: GpuVendor,
    pub dedicated_vram_bytes: Option<u64>,
    /// System RAM the OS may lend to this adapter (e.g. for an iGPU) - only
    /// meaningful as a ceiling, not a guarantee of availability at any
    /// given moment, since it's shared with everything else running.
    pub shared_system_memory_bytes: Option<u64>,
    pub driver_version: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GpuVendor {
    Nvidia,
    Amd,
    Intel,
    Other,
}

#[derive(Debug, Clone, Serialize)]
pub struct GpuReport {
    pub adapters: HardwareField<Vec<GpuAdapter>>,
    pub cuda_available: HardwareField<bool>,
    pub vulkan_available: HardwareField<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OsReport {
    pub product_name: HardwareField<String>,
    pub display_version: HardwareField<String>,
    pub build_number: HardwareField<String>,
    pub process_architecture: HardwareField<String>,
    pub native_architecture: HardwareField<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StorageReport {
    pub path_queried: String,
    pub free_bytes: HardwareField<u64>,
    pub total_bytes: HardwareField<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HardwareReport {
    pub os: OsReport,
    pub cpu: CpuReport,
    pub memory: MemoryReport,
    pub gpu: GpuReport,
    pub storage: Option<StorageReport>,
    pub power: power::PowerReport,
}

/// Runs every detector and assembles the full report. `storage_path` is the
/// directory a model would be imported from/into; pass `None` to skip the
/// storage check (e.g. when no model path is known yet).
pub fn inspect(storage_path: Option<&std::path::Path>) -> HardwareReport {
    HardwareReport {
        os: system::inspect_os(),
        cpu: cpu::inspect_cpu(),
        memory: memory::inspect_memory(),
        gpu: gpu::inspect_gpu(),
        storage: storage_path.map(system::inspect_storage),
        power: power::inspect_power(),
    }
}
