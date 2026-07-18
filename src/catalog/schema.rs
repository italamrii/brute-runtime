//! Catalog entry schema. One `ModelBuild` = one exact downloadable
//! artifact (a specific quantization of a specific model release), not a
//! model family. See `docs/model-catalog-schema.md` for the field-by-field
//! rationale.

use crate::runtime::Backend;
use serde::{Deserialize, Serialize};

/// A model's license status. `Unknown` is a first-class, structurally
/// distinct value - never collapsed into a guessed identifier. Curators
/// must be able to say "we don't know" without the schema forcing a
/// fabricated answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum License {
    Known { identifier: String },
    Unknown,
}

/// Whether commercial use is established as allowed by the catalog
/// metadata. `Unknown` and `Restricted` are both distinct from `Allowed` -
/// `Allowed` must never be inferred, only stated because the source
/// material explicitly establishes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommercialUse {
    Allowed,
    Restricted,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum TaskCategory {
    GeneralChat,
    Coding,
    ArabicChat,
    Reasoning,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelBuild {
    pub catalog_id: String,
    pub family: String,
    pub display_name: String,
    pub publisher: String,
    /// Official model page/repository - data only. The core engine never
    /// opens this automatically; see `docs/security-model.md`.
    pub official_source_url: String,
    pub official_repository_id: String,
    /// Bare filename the official source uses for this exact artifact -
    /// never a path (validated at load time in `catalog::validate_entry`).
    pub filename: String,
    pub architecture: String,
    pub parameter_count: u64,
    pub quantization: String,
    pub file_size_bytes: u64,
    /// Usually equal to `file_size_bytes`; separate field because some
    /// distributions ship extra files (tokenizer, mmproj) alongside the
    /// weights.
    pub estimated_disk_bytes: Option<u64>,
    /// Optional curator-provided rough runtime memory estimate, kept
    /// distinct from - and cross-checked against, never trusted blindly
    /// over - `estimator`'s own computed estimate.
    pub estimated_runtime_memory_bytes: Option<u64>,
    pub min_recommended_ram_bytes: u64,
    pub min_recommended_vram_bytes: Option<u64>,
    pub supported_backends: Vec<Backend>,
    pub context_sizes: Vec<u32>,
    pub task_categories: Vec<TaskCategory>,
    pub short_description: String,
    pub strength: String,
    pub limitation: String,
    pub license: License,
    pub commercial_use: CommercialUse,
    pub gated_access: Option<bool>,
    pub metadata_provenance: String,
    pub last_reviewed: String,
}
