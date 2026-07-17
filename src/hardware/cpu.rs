//! CPU inspection: brand/vendor via `sysinfo` (wraps OS-native queries),
//! core counts via `sysinfo` + `std::thread::available_parallelism`, and
//! instruction-set flags relevant to llama.cpp's CPU (ggml) kernels via
//! direct `cpuid` through the `raw_cpuid` crate.

use super::{CpuReport, HardwareField};
use raw_cpuid::CpuId;
use sysinfo::System;

pub fn inspect_cpu() -> CpuReport {
    let mut sys = System::new_all();
    sys.refresh_cpu_all();

    let cpus = sys.cpus();
    let brand = cpus.first().map(|c| c.brand().trim().to_string());
    let vendor = cpus.first().map(|c| c.vendor_id().trim().to_string());

    let brand_field = match brand {
        Some(b) if !b.is_empty() => HardwareField::measured(b, "sysinfo::Cpu::brand"),
        _ => HardwareField::unavailable("sysinfo returned an empty CPU brand string"),
    };

    let vendor_field = match vendor {
        Some(v) if !v.is_empty() => HardwareField::measured(v, "sysinfo::Cpu::vendor_id"),
        _ => HardwareField::unavailable("sysinfo returned an empty CPU vendor string"),
    };

    let physical_cores = match System::physical_core_count() {
        Some(n) => HardwareField::measured(n, "sysinfo::System::physical_core_count"),
        None => HardwareField::unavailable("OS did not report physical core topology"),
    };

    let logical_cores = match std::thread::available_parallelism() {
        Ok(n) => HardwareField::measured(n.get(), "std::thread::available_parallelism"),
        Err(e) => HardwareField::unavailable(format!("available_parallelism failed: {e}")),
    };

    CpuReport {
        vendor: vendor_field,
        brand: brand_field,
        physical_cores,
        logical_cores,
        instruction_sets: HardwareField::measured(detect_instruction_sets(), "cpuid (raw_cpuid)"),
    }
}

/// Instruction-set extensions that matter for llama.cpp's ggml CPU backend
/// (it dispatches SIMD kernels based on these at runtime). Reads real
/// `cpuid` leaves - never a heuristic.
fn detect_instruction_sets() -> Vec<String> {
    let cpuid = CpuId::new();
    let mut sets = Vec::new();

    if let Some(fi) = cpuid.get_feature_info() {
        if fi.has_sse3() {
            sets.push("SSE3".to_string());
        }
        if fi.has_ssse3() {
            sets.push("SSSE3".to_string());
        }
        if fi.has_sse41() {
            sets.push("SSE4.1".to_string());
        }
        if fi.has_sse42() {
            sets.push("SSE4.2".to_string());
        }
        if fi.has_fma() {
            sets.push("FMA".to_string());
        }
        if fi.has_avx() {
            sets.push("AVX".to_string());
        }
        if fi.has_f16c() {
            sets.push("F16C".to_string());
        }
    }

    if let Some(ext) = cpuid.get_extended_feature_info() {
        if ext.has_avx2() {
            sets.push("AVX2".to_string());
        }
        if ext.has_avx512f() {
            sets.push("AVX512F".to_string());
        }
        if ext.has_avx512vnni() {
            sets.push("AVX512VNNI".to_string());
        }
        if ext.has_avx512bw() {
            sets.push("AVX512BW".to_string());
        }
        if ext.has_bmi1() {
            sets.push("BMI1".to_string());
        }
        if ext.has_bmi2() {
            sets.push("BMI2".to_string());
        }
    }

    let has_fma4 = cpuid
        .get_extended_processor_and_feature_identifiers()
        .is_some_and(|ext| ext.has_fma4());
    if has_fma4 {
        sets.push("FMA4".to_string());
    }

    sets
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inspect_cpu_never_panics_and_reports_something() {
        let report = inspect_cpu();
        // On any real machine logical cores must be measurable; this is the
        // one field we can assert on without hardware-specific assumptions.
        assert!(report.logical_cores.value.is_some());
    }

    #[test]
    fn instruction_set_detection_is_deterministic() {
        let a = detect_instruction_sets();
        let b = detect_instruction_sets();
        assert_eq!(a, b);
    }
}
