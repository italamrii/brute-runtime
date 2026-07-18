//! Calibration store: real benchmark measurements, used to ground the
//! estimator's KV-cache/throughput numbers and to tell the fit classifier
//! when an `Experimental` (substantial-uncertainty) verdict is warranted.
//!
//! A calibration record is *always* a real measurement (`brute benchmark`
//! actually ran) - never a projection. Projections happen at lookup time,
//! in `find_nearest`, and every projection is graded by how far the
//! nearest record actually is from what's being estimated. See
//! `docs/calibration-methodology.md`.

use crate::benchmark::metrics::StabilityStatus;
use crate::runtime::Backend;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibrationRecord {
    pub recorded_at_rfc3339: String,
    /// Ties this record to the machine it was measured on (see
    /// `profile::compute_machine_id`) - a future same-machine lookup can
    /// prefer records captured on this exact machine.
    pub machine_id: String,
    pub machine_profile_schema_version: String,
    pub catalog_id: Option<String>,
    pub architecture: String,
    pub quantization: String,
    pub parameter_count: u64,
    pub backend: Backend,
    pub threads: u32,
    pub gpu_layers: u32,
    pub context_size: u32,
    pub batch_size: u32,
    pub prompt_processing_tokens_per_second: f64,
    pub generation_tokens_per_second: f64,
    pub peak_ram_bytes: Option<u64>,
    pub peak_vram_bytes: Option<u64>,
    pub stability: StabilityStatus,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CalibrationStore {
    pub records: Vec<CalibrationRecord>,
}

impl CalibrationStore {
    pub fn load(path: &Path) -> Result<Self, crate::errors::CatalogError> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let contents =
            std::fs::read_to_string(path).map_err(|source| crate::errors::CatalogError::Read {
                path: path.to_path_buf(),
                source,
            })?;
        serde_json::from_str(&contents).map_err(|source| crate::errors::CatalogError::Parse {
            path: path.to_path_buf(),
            source,
        })
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self).expect("CalibrationStore always serializes");
        std::fs::write(path, json)
    }

    pub fn add(&mut self, record: CalibrationRecord) {
        self.records.push(record);
    }
}

/// How close the nearest calibration record is to a target build. There is
/// deliberately no "none" variant here - "no usable match" is represented
/// by `find_nearest` returning `Option::None` at the function level, not
/// by a value inside this enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CalibrationProximity {
    /// Same architecture, same or closely related quantization, backend
    /// match, parameter count within 20%.
    Exact,
    /// Same architecture, param count within 3x, quantization or backend
    /// may differ.
    Close,
    /// Same architecture only, or param count more than 3x away.
    Distant,
}

#[derive(Debug, Clone, Serialize)]
pub struct CalibrationMatch {
    pub record: CalibrationRecord,
    pub proximity: CalibrationProximity,
    pub distance_notes: Vec<String>,
}

/// Finds the closest calibration record to the given target, if any is
/// close enough to be worth reporting at all (same architecture, and
/// parameter count within 10x - beyond that we report no match rather
/// than extrapolate across a gap that wide).
///
/// This is a documented, bounded nearest-neighbor lookup - no machine
/// learning, no hidden weighting. See `docs/calibration-methodology.md`
/// for the exact thresholds and why each one was chosen.
pub fn find_nearest(
    store: &CalibrationStore,
    architecture: &str,
    quantization: &str,
    parameter_count: u64,
    backend: Backend,
) -> Option<CalibrationMatch> {
    let mut best: Option<(CalibrationMatch, f64)> = None;

    for record in &store.records {
        if !record.architecture.eq_ignore_ascii_case(architecture) {
            continue; // never extrapolate across architectures
        }

        let param_ratio = if record.parameter_count == 0 || parameter_count == 0 {
            f64::INFINITY
        } else {
            let (hi, lo) = if record.parameter_count > parameter_count {
                (record.parameter_count, parameter_count)
            } else {
                (parameter_count, record.parameter_count)
            };
            hi as f64 / lo as f64
        };
        if param_ratio > 10.0 {
            continue; // too far to be a meaningful calibration anchor at all
        }

        let quant_match = record.quantization.eq_ignore_ascii_case(quantization);
        let quant_family_match = quant_family(&record.quantization) == quant_family(quantization);
        let backend_match = record.backend == backend;

        let mut notes = Vec::new();
        if !quant_match {
            notes.push(format!(
                "nearest calibration record used quantization {:?}, this build uses {quantization:?}",
                record.quantization
            ));
        }
        if !backend_match {
            notes.push(format!(
                "nearest calibration record used backend {:?}, this run targets {backend:?}",
                record.backend
            ));
        }
        if param_ratio > 1.2 {
            notes.push(format!(
                "nearest calibration record's parameter count differs by {:.1}x",
                param_ratio
            ));
        }

        let proximity = if quant_match && backend_match && param_ratio <= 1.2 {
            CalibrationProximity::Exact
        } else if quant_family_match && param_ratio <= 3.0 {
            CalibrationProximity::Close
        } else {
            CalibrationProximity::Distant
        };

        // Distance score: lower is closer. Used only to pick the single
        // best record among the store, not exposed as a fake precision
        // metric to callers.
        let score = (param_ratio - 1.0).min(9.0)
            + if quant_match {
                0.0
            } else if quant_family_match {
                0.5
            } else {
                1.5
            }
            + if backend_match { 0.0 } else { 1.0 };

        let candidate = CalibrationMatch {
            record: record.clone(),
            proximity,
            distance_notes: notes,
        };

        match &best {
            Some((_, best_score)) if *best_score <= score => {}
            _ => best = Some((candidate, score)),
        }
    }

    best.map(|(m, _)| m)
}

fn quant_family(quantization: &str) -> String {
    // "Q4_K_M" -> "Q4", "Q8_0" -> "Q8", "F16" -> "F16" - groups
    // quantizations that share the same bit-width family together.
    let q = quantization.to_uppercase();
    q.split('_').next().unwrap_or(&q).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(
        architecture: &str,
        quantization: &str,
        parameter_count: u64,
        backend: Backend,
    ) -> CalibrationRecord {
        CalibrationRecord {
            recorded_at_rfc3339: "2026-07-18T00:00:00Z".to_string(),
            machine_id: "machine-test".to_string(),
            machine_profile_schema_version: "stage1-v1".to_string(),
            catalog_id: None,
            architecture: architecture.to_string(),
            quantization: quantization.to_string(),
            parameter_count,
            backend,
            threads: 16,
            gpu_layers: 0,
            context_size: 2048,
            batch_size: 512,
            prompt_processing_tokens_per_second: 425.15,
            generation_tokens_per_second: 44.09,
            peak_ram_bytes: Some(542_609_408),
            peak_vram_bytes: None,
            stability: StabilityStatus::Stable,
        }
    }

    #[test]
    fn exact_match_when_everything_lines_up() {
        let mut store = CalibrationStore::default();
        store.add(record("qwen2", "Q4_K_M", 630_167_424, Backend::Cpu));

        let m = find_nearest(&store, "qwen2", "Q4_K_M", 630_167_424, Backend::Cpu).unwrap();
        assert_eq!(m.proximity, CalibrationProximity::Exact);
        assert!(m.distance_notes.is_empty());
    }

    #[test]
    fn never_matches_a_different_architecture() {
        let mut store = CalibrationStore::default();
        store.add(record("qwen2", "Q4_K_M", 630_167_424, Backend::Cpu));

        let m = find_nearest(&store, "llama", "Q4_K_M", 630_167_424, Backend::Cpu);
        assert!(m.is_none());
    }

    #[test]
    fn close_match_for_moderately_different_param_count_same_family() {
        let mut store = CalibrationStore::default();
        store.add(record("qwen2", "Q4_K_M", 630_167_424, Backend::Cpu));

        // ~2.4x larger, same architecture, same quant family.
        let m = find_nearest(&store, "qwen2", "Q4_K_S", 1_500_000_000, Backend::Cpu).unwrap();
        assert_eq!(m.proximity, CalibrationProximity::Close);
        assert!(!m.distance_notes.is_empty());
    }

    #[test]
    fn distant_match_for_far_apart_param_count() {
        let mut store = CalibrationStore::default();
        store.add(record("llama", "Q4_K_M", 630_167_424, Backend::Cpu));

        // ~7x larger - still within the 10x cutoff, but far.
        let m = find_nearest(&store, "llama", "Q4_K_M", 4_500_000_000, Backend::Cpu).unwrap();
        assert_eq!(m.proximity, CalibrationProximity::Distant);
    }

    #[test]
    fn no_match_beyond_the_ten_x_cutoff() {
        let mut store = CalibrationStore::default();
        store.add(record("llama", "Q4_K_M", 500_000_000, Backend::Cpu));

        let m = find_nearest(&store, "llama", "Q4_K_M", 70_000_000_000, Backend::Cpu);
        assert!(m.is_none());
    }

    #[test]
    fn empty_store_never_matches() {
        let store = CalibrationStore::default();
        let m = find_nearest(&store, "qwen2", "Q4_K_M", 630_167_424, Backend::Cpu);
        assert!(m.is_none());
    }

    #[test]
    fn picks_the_closest_of_multiple_records() {
        let mut store = CalibrationStore::default();
        store.add(record("qwen2", "Q4_K_M", 100_000_000, Backend::Cpu));
        store.add(record("qwen2", "Q4_K_M", 630_167_424, Backend::Cpu)); // closest
        store.add(record("qwen2", "Q4_K_M", 8_000_000_000, Backend::Cpu));

        let m = find_nearest(&store, "qwen2", "Q4_K_M", 630_167_424, Backend::Cpu).unwrap();
        assert_eq!(m.record.parameter_count, 630_167_424);
        assert_eq!(m.proximity, CalibrationProximity::Exact);
    }

    #[test]
    fn store_round_trips_through_json() {
        let dir = std::env::temp_dir().join(format!(
            "brute-calib-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("calibration.json");

        let mut store = CalibrationStore::default();
        store.add(record("qwen2", "Q4_K_M", 630_167_424, Backend::Cpu));
        store.save(&path).unwrap();

        let loaded = CalibrationStore::load(&path).unwrap();
        assert_eq!(loaded.records.len(), 1);
        assert_eq!(loaded.records[0].architecture, "qwen2");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn loading_a_missing_file_returns_an_empty_store_not_an_error() {
        let store = CalibrationStore::load(Path::new(r"C:\nonexistent\calibration.json")).unwrap();
        assert!(store.records.is_empty());
    }

    #[test]
    fn seed_calibration_file_loads_and_matches_the_qwen_build_exactly() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("data/calibration/seed-calibration.json");
        let store = CalibrationStore::load(&path).expect("seed calibration file must be valid");
        assert!(!store.records.is_empty());

        let m = find_nearest(&store, "qwen2", "Q4_K_M", 630_167_424, Backend::Cpu)
            .expect("seed record should exactly match the real Qwen2.5-0.5B build");
        assert_eq!(m.proximity, CalibrationProximity::Exact);
        assert_eq!(m.record.prompt_processing_tokens_per_second, 425.15);
        assert_eq!(m.record.generation_tokens_per_second, 44.09);
        assert_eq!(m.record.stability, StabilityStatus::Stable);
    }
}
