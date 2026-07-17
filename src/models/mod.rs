pub mod gguf;
pub mod validation;

use crate::errors::BruteError;
use crate::security;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
pub struct ModelReport {
    pub path: PathBuf,
    pub file_size_bytes: u64,
    pub sha256: String,
    pub gguf_version: u32,
    pub tensor_count: u64,
    pub kv_count: u64,
    pub architecture: Option<String>,
    pub name: Option<String>,
    pub quantization: Option<String>,
    pub dominant_tensor_type: Option<String>,
    pub parameter_count: Option<u64>,
    pub alignment: u32,
    pub size_consistency_checked: bool,
    pub kv_preview: BTreeMap<String, String>,
}

/// Validates the path, hashes the file, and parses GGUF metadata. Every step
/// can fail independently and each failure is reported with its real cause -
/// nothing here is allowed to silently produce a partial/fake report.
pub fn inspect_model(path: &Path) -> Result<ModelReport, BruteError> {
    let canonical = validation::validate_model_path(path)?;

    let sha256 = security::sha256_file(&canonical).map_err(|source| BruteError::Io {
        context: format!("hashing {}", canonical.display()),
        source,
    })?;

    let summary = gguf::inspect(&canonical).map_err(BruteError::Gguf)?;

    Ok(ModelReport {
        path: canonical,
        file_size_bytes: summary.file_size,
        sha256,
        gguf_version: summary.version,
        tensor_count: summary.tensor_count,
        kv_count: summary.kv_count,
        architecture: summary.architecture,
        name: summary.name,
        quantization: summary.quantization,
        dominant_tensor_type: summary.dominant_tensor_type,
        parameter_count: summary.parameter_count,
        alignment: summary.alignment,
        size_consistency_checked: summary.size_consistency_checked,
        kv_preview: summary.kv_preview,
    })
}
