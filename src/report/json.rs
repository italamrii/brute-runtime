use super::CapabilityReport;
use crate::errors::ReportError;
use std::path::Path;

pub fn to_pretty_string(report: &CapabilityReport) -> Result<String, ReportError> {
    serde_json::to_string_pretty(report).map_err(ReportError::from)
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
}
