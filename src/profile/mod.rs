//! Hardware Capability Profile: a normalized, UI-stable view over the raw
//! Stage 0 `hardware::HardwareReport`, plus a machine identity and a
//! confidence roll-up. This is the input the Stage 1 estimator, fit
//! classifier, and recommendation engine consume - they never re-derive
//! hardware facts themselves, only read this profile.
//!
//! Deliberately does not collect anything identifying: no serial numbers,
//! no MAC/GUID device IDs, no usernames. `machine_id` is a hash of already
//! non-sensitive aggregate specs (CPU brand, core counts, rounded RAM, OS
//! build) - stable across runs on the same machine, meaningless off it,
//! and not traceable to a real hardware identity.

use crate::hardware::power::PowerReport;
use crate::hardware::{
    Confidence, CpuReport, GpuAdapter, HardwareField, HardwareReport, MemoryReport, OsReport,
    StorageReport,
};
use serde::Serialize;
use sha2::{Digest, Sha256};

pub const SCHEMA_VERSION: &str = "stage1-v1";

#[derive(Debug, Clone, Serialize)]
pub struct BackendAvailability {
    /// CPU inference is always structurally available in Stage 0/1 (llama.cpp
    /// CPU builds run everywhere) - not a `HardwareField` because there is
    /// nothing to measure or infer, it's a constant of this codebase.
    pub cpu: bool,
    pub cuda: HardwareField<bool>,
    pub vulkan: HardwareField<bool>,
}

#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct ConfidenceSummary {
    pub measured_count: usize,
    pub detected_count: usize,
    pub inferred_count: usize,
    pub unavailable_count: usize,
}

impl ConfidenceSummary {
    pub fn total(&self) -> usize {
        self.measured_count + self.detected_count + self.inferred_count + self.unavailable_count
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct HardwareCapabilityProfile {
    pub schema_version: String,
    pub machine_id: String,
    pub captured_at_rfc3339: String,
    pub os: OsReport,
    pub cpu: CpuReport,
    pub memory: MemoryReport,
    pub gpus: Vec<GpuAdapter>,
    pub backends: BackendAvailability,
    pub storage: Option<StorageReport>,
    pub power: PowerReport,
    /// Number of calibration records on file that apply to *this* machine
    /// (see `calibration` module) - a count, not a copy, so the profile
    /// stays small and the calibration store remains the single source of
    /// truth for calibration data.
    pub calibration_record_count: usize,
    pub confidence_summary: ConfidenceSummary,
}

/// Builds the normalized profile from a completed hardware inspection.
/// `calibration_record_count` is supplied by the caller (typically the
/// calibration store, loaded independently) rather than looked up here, so
/// this module has no dependency on the calibration store and can be
/// tested/used in isolation.
pub fn build_profile(
    hardware: &HardwareReport,
    captured_at_rfc3339: String,
    calibration_record_count: usize,
) -> HardwareCapabilityProfile {
    let gpus = hardware.gpu.adapters.value.clone().unwrap_or_default();

    let backends = BackendAvailability {
        cpu: true,
        cuda: clone_field(&hardware.gpu.cuda_available),
        vulkan: clone_field(&hardware.gpu.vulkan_available),
    };

    let machine_id = compute_machine_id(hardware);
    let confidence_summary = summarize_confidence(hardware);

    HardwareCapabilityProfile {
        schema_version: SCHEMA_VERSION.to_string(),
        machine_id,
        captured_at_rfc3339,
        os: hardware.os.clone(),
        cpu: hardware.cpu.clone(),
        memory: hardware.memory.clone(),
        gpus,
        backends,
        storage: hardware.storage.clone(),
        power: hardware.power.clone(),
        calibration_record_count,
        confidence_summary,
    }
}

/// Placeholder written in place of `machine_id` in any shareable export
/// (JSON/file output, never the local terminal display). `machine_id` is
/// coarse and collision-prone by design (see `compute_machine_id`), but
/// it's still fully deterministic from hardware, so Stage 2's privacy
/// review requires it never leave the machine as-is. Use
/// `identity::load_or_create_local_instance_id` for a value that *is*
/// safe to export - random, resettable, and carrying no hardware meaning.
pub const REDACTED_MACHINE_ID_PLACEHOLDER: &str = "omitted-from-export";

/// Returns a copy of `profile` with `machine_id` replaced by
/// [`REDACTED_MACHINE_ID_PLACEHOLDER`] - for building shareable (JSON/
/// file) output. Never used for the local terminal display, where the
/// real coarse ID is still useful for the user's own reference.
pub fn redact_machine_id_for_export(
    profile: &HardwareCapabilityProfile,
) -> HardwareCapabilityProfile {
    let mut redacted = profile.clone();
    redacted.machine_id = REDACTED_MACHINE_ID_PLACEHOLDER.to_string();
    redacted
}

fn clone_field<T: Clone>(field: &HardwareField<T>) -> HardwareField<T> {
    HardwareField {
        value: field.value.clone(),
        confidence: field.confidence,
        source: field.source.clone(),
    }
}

/// Hashes a canonical string built from already non-sensitive, coarse
/// aggregate specs. Two different machines with identical CPU brand,
/// core counts, RAM (rounded to the nearest GiB), and OS build could in
/// principle collide - that's an accepted tradeoff for staying far away
/// from anything that resembles a real hardware identifier.
fn compute_machine_id(hardware: &HardwareReport) -> String {
    let cpu_brand = hardware.cpu.brand.value.as_deref().unwrap_or("unknown-cpu");
    let physical_cores = hardware.cpu.physical_cores.value.unwrap_or(0);
    let logical_cores = hardware.cpu.logical_cores.value.unwrap_or(0);
    let ram_gib_rounded = hardware
        .memory
        .total_bytes
        .value
        .map(|b| b / (1024 * 1024 * 1024))
        .unwrap_or(0);
    let os_build = hardware
        .os
        .build_number
        .value
        .as_deref()
        .unwrap_or("unknown-build");

    let canonical =
        format!("{cpu_brand}|{physical_cores}|{logical_cores}|{ram_gib_rounded}|{os_build}");

    let digest = Sha256::digest(canonical.as_bytes());
    let hex: String = digest.iter().take(8).map(|b| format!("{b:02x}")).collect();
    format!("machine-{hex}")
}

fn summarize_confidence(hardware: &HardwareReport) -> ConfidenceSummary {
    let mut counts = ConfidenceSummary::default();
    let mut tally = |c: Confidence| match c {
        Confidence::Measured => counts.measured_count += 1,
        Confidence::Detected => counts.detected_count += 1,
        Confidence::Inferred => counts.inferred_count += 1,
        Confidence::Unavailable => counts.unavailable_count += 1,
    };

    tally(hardware.os.product_name.confidence);
    tally(hardware.os.display_version.confidence);
    tally(hardware.os.build_number.confidence);
    tally(hardware.os.process_architecture.confidence);
    tally(hardware.os.native_architecture.confidence);
    tally(hardware.cpu.vendor.confidence);
    tally(hardware.cpu.brand.confidence);
    tally(hardware.cpu.physical_cores.confidence);
    tally(hardware.cpu.logical_cores.confidence);
    tally(hardware.cpu.instruction_sets.confidence);
    tally(hardware.memory.total_bytes.confidence);
    tally(hardware.memory.available_bytes.confidence);
    tally(hardware.gpu.adapters.confidence);
    tally(hardware.gpu.cuda_available.confidence);
    tally(hardware.gpu.vulkan_available.confidence);
    if let Some(storage) = &hardware.storage {
        tally(storage.free_bytes.confidence);
        tally(storage.total_bytes.confidence);
    }
    tally(hardware.power.ac_line_status.confidence);
    tally(hardware.power.battery_present.confidence);
    tally(hardware.power.battery_percent.confidence);
    tally(hardware.power.chassis_class.confidence);

    counts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn machine_id_is_stable_across_two_builds_of_the_same_report() {
        let hw = crate::hardware::inspect(None);
        let id1 = compute_machine_id(&hw);
        let id2 = compute_machine_id(&hw);
        assert_eq!(id1, id2);
        assert!(id1.starts_with("machine-"));
    }

    #[test]
    fn confidence_summary_totals_match_the_number_of_fields_tallied() {
        let hw = crate::hardware::inspect(None);
        let summary = summarize_confidence(&hw);
        // 15 always-present fields + power (4) = 19 when storage is None.
        assert_eq!(summary.total(), 19);
    }

    #[test]
    fn profile_never_panics_and_carries_the_requested_calibration_count() {
        let hw = crate::hardware::inspect(None);
        let profile = build_profile(&hw, "2026-01-01T00:00:00Z".to_string(), 3);
        assert_eq!(profile.calibration_record_count, 3);
        assert_eq!(profile.schema_version, SCHEMA_VERSION);
        assert!(profile.backends.cpu);
    }

    #[test]
    fn redact_machine_id_for_export_replaces_the_real_id_and_nothing_else() {
        let hw = crate::hardware::inspect(None);
        let profile = build_profile(&hw, "2026-01-01T00:00:00Z".to_string(), 0);
        let real_id = profile.machine_id.clone();

        let redacted = redact_machine_id_for_export(&profile);
        assert_eq!(redacted.machine_id, REDACTED_MACHINE_ID_PLACEHOLDER);
        assert_ne!(redacted.machine_id, real_id);
        // Everything else must be untouched.
        assert_eq!(redacted.schema_version, profile.schema_version);
        assert_eq!(redacted.captured_at_rfc3339, profile.captured_at_rfc3339);
    }
}
