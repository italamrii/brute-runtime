use super::CapabilityReport;
use crate::errors::ReportError;
use crate::security::redact_username_for_report;
use std::path::{Path, PathBuf};

/// Every path that could carry a real Windows username (the model file,
/// the storage path checked, the llama.cpp binary directory) is redacted
/// before a report leaves this process as JSON - see
/// `docs/security-model.md`. This never touches the paths `brute` itself
/// operates on; it only affects the serialized copy.
fn sanitize(report: &CapabilityReport) -> CapabilityReport {
    let mut r = report.clone();

    if let Some(model) = &mut r.model {
        model.path = PathBuf::from(redact_username_for_report(&model.path));
    }
    if let Some(storage) = &mut r.hardware.storage {
        storage.path_queried = redact_username_for_report(Path::new(&storage.path_queried));
    }
    if let Some(bench) = &mut r.benchmark {
        bench.model_path = PathBuf::from(redact_username_for_report(&bench.model_path));
        bench.config.binary_dir =
            PathBuf::from(redact_username_for_report(&bench.config.binary_dir));
    }

    r
}

pub fn to_pretty_string(report: &CapabilityReport) -> Result<String, ReportError> {
    serde_json::to_string_pretty(&sanitize(report)).map_err(ReportError::from)
}

pub fn write_to_file(report: &CapabilityReport, path: &Path) -> Result<(), ReportError> {
    let json = to_pretty_string(report)?;
    std::fs::write(path, json).map_err(|source| ReportError::Write {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware;

    #[test]
    fn round_trips_through_serde_json_value() {
        let report = CapabilityReport {
            generated_at_rfc3339: "2026-07-18T00:00:00Z".to_string(),
            hardware: hardware::inspect(None),
            model: None,
            benchmark: None,
            recommendation: None,
        };

        let s = to_pretty_string(&report).expect("should serialize");
        let value: serde_json::Value = serde_json::from_str(&s).expect("should be valid json");
        assert!(value.get("hardware").is_some());
        assert!(value.get("generated_at_rfc3339").is_some());
    }

    #[test]
    fn writes_to_file_and_is_readable() {
        let report = CapabilityReport {
            generated_at_rfc3339: "2026-07-18T00:00:00Z".to_string(),
            hardware: hardware::inspect(None),
            model: None,
            benchmark: None,
            recommendation: None,
        };

        let dir = std::env::temp_dir().join(format!("brute-report-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("report.json");

        write_to_file(&report, &path).expect("should write");
        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.contains("generated_at_rfc3339"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn exported_report_never_contains_the_real_username_from_a_model_path() {
        let mut hw = hardware::inspect(None);
        hw.storage = Some(hardware::StorageReport {
            path_queried: r"C:\Users\RealUserName\Downloads".to_string(),
            free_bytes: hardware::HardwareField::measured(1, "test"),
            total_bytes: hardware::HardwareField::measured(2, "test"),
        });

        let report = CapabilityReport {
            generated_at_rfc3339: "2026-07-18T00:00:00Z".to_string(),
            hardware: hw,
            model: Some(crate::models::ModelReport {
                path: PathBuf::from(r"C:\Users\RealUserName\Downloads\model.gguf"),
                file_size_bytes: 1,
                sha256: "test".to_string(),
                gguf_version: 3,
                tensor_count: 0,
                kv_count: 0,
                architecture: None,
                name: None,
                quantization: None,
                dominant_tensor_type: None,
                parameter_count: None,
                alignment: 32,
                size_consistency_checked: false,
                kv_preview: Default::default(),
                hyperparameters: Default::default(),
            }),
            benchmark: None,
            recommendation: None,
        };

        let s = to_pretty_string(&report).expect("should serialize");
        assert!(
            !s.contains("RealUserName"),
            "sanitized report leaked a real username:\n{s}"
        );
        assert!(
            s.contains("<redacted>"),
            "sanitized report should show the redaction placeholder:\n{s}"
        );
    }

    /// Every field path a Stage 0 consumer of `brute report`'s JSON could
    /// have depended on must still exist, unchanged in shape, after the
    /// Stage 1 additions (`power`, new hardware fields, etc. are additive
    /// only - see docs/architecture.md).
    #[test]
    fn stage0_json_field_paths_remain_present_after_stage1_additions() {
        let report = CapabilityReport {
            generated_at_rfc3339: "2026-07-18T00:00:00Z".to_string(),
            hardware: hardware::inspect(None),
            model: None,
            benchmark: None,
            recommendation: None,
        };
        let s = to_pretty_string(&report).expect("should serialize");
        let value: serde_json::Value = serde_json::from_str(&s).expect("should be valid json");

        for pointer in [
            "/generated_at_rfc3339",
            "/hardware/os/product_name/value",
            "/hardware/os/product_name/confidence",
            "/hardware/os/product_name/source",
            "/hardware/cpu/vendor",
            "/hardware/cpu/brand",
            "/hardware/cpu/physical_cores",
            "/hardware/cpu/logical_cores",
            "/hardware/cpu/instruction_sets",
            "/hardware/memory/total_bytes",
            "/hardware/memory/available_bytes",
            "/hardware/gpu/adapters",
            "/hardware/gpu/cuda_available",
            "/hardware/gpu/vulkan_available",
            "/model",
            "/benchmark",
            "/recommendation",
        ] {
            assert!(
                value.pointer(pointer).is_some(),
                "Stage 0 JSON field path {pointer} is missing after Stage 1 changes"
            );
        }
    }
}
