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
    // Added Stage B.1 (multi-family intelligence) - additive, existing
    // serialized values above are unchanged.
    English,
    Multilingual,
    Writing,
    Summarization,
    DocumentAnalysis,
    Vision,
    ToolUse,
    Embeddings,
    Reranking,
    Speech,
}

/// How far a catalog entry's metadata has actually been checked, in strict
/// increasing order of evidence. `Unknown`/`CuratedMetadata` never imply a
/// verified artifact exists - only `ArtifactUrlVerified` and above do, and
/// the frontend must never show a Download button below that threshold
/// (spec: "never treat a landing page as a direct artifact URL").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum VerificationStatus {
    /// No field below this line has independent evidence backing it yet.
    #[default]
    Unknown,
    /// Hand-entered by a curator from the publisher's own listing, not
    /// independently re-checked.
    CuratedMetadata,
    /// The publisher/repository itself was confirmed to exist and match.
    SourceVerified,
    /// The exact artifact URL was confirmed to resolve to a real file
    /// (not just the landing page).
    ArtifactUrlVerified,
    /// A checksum for the exact artifact was confirmed against a source
    /// independent of BRUTE's own download.
    ChecksumVerified,
    /// BRUTE has actually downloaded this exact artifact at least once.
    Downloaded,
    /// The downloaded file's checksum matched after download.
    IntegrityVerified,
    /// The downloaded file was successfully loaded by the runtime.
    RuntimeCompatible,
    /// A real benchmark run completed against the downloaded artifact.
    Benchmarked,
    /// Benchmarked specifically on the current device, not just some
    /// device.
    DeviceVerified,
    /// Actively known not to work (e.g. runtime load failure observed).
    Unsupported,
}

/// A curator's honest, coarse capability estimate. `Unknown` is the
/// default and must never be silently upgraded - see
/// `docs/model-catalog-schema.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityLevel {
    #[default]
    Unknown,
    Basic,
    Good,
    Strong,
    Excellent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SpeedCategory {
    #[default]
    Unknown,
    Slow,
    Moderate,
    Fast,
}

/// Where a quality/speed claim's evidence actually comes from. Mirrors the
/// mission's explicit distinction between measured-on-this-device,
/// measured-on-similar-hardware, and estimated-only - never conflated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceSource {
    #[default]
    Unknown,
    EstimatedOnly,
    MeasuredOnSimilarHardware,
    MeasuredOnThisDevice,
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

    // ---- Stage B.1: multi-family identity/trust/evidence fields -------
    // All additive and `#[serde(default)]` so every existing catalog file
    // and test fixture keeps loading unchanged - a build with none of
    // these fields simply reports Unknown/None for all of them, which is
    // the honest answer for metadata nobody has entered yet (spec: "mark
    // incomplete metadata as unknown rather than guessing").
    /// Stable slug identifying the model family independent of a specific
    /// size/variant/quantization, e.g. `"qwen2.5"`. Distinct from the
    /// human-readable `family` display string above.
    #[serde(default)]
    pub family_id: Option<String>,
    /// Identifies one model+variant (a specific parameter count and
    /// instruction-tuning, but not a specific quantization), e.g.
    /// `"qwen2.5-0.5b-instruct"`.
    #[serde(default)]
    pub model_id: Option<String>,
    /// Identifies this exact artifact (model + quantization + file). If
    /// unset, `catalog_id` already serves this purpose.
    #[serde(default)]
    pub artifact_id: Option<String>,
    /// The publisher's own exact model name string, verbatim, distinct
    /// from BRUTE's `display_name`.
    #[serde(default)]
    pub exact_model_name: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    /// The single largest context length this artifact supports -
    /// distinct from `context_sizes`, which lists the specific sizes
    /// BRUTE has evaluated/tuned against.
    #[serde(default)]
    pub context_length: Option<u32>,
    /// Artifact file format, e.g. `"gguf"`.
    #[serde(default)]
    pub file_format: Option<String>,
    /// The runtime this artifact targets, e.g. `"llama.cpp"`.
    #[serde(default)]
    pub runtime_provider: Option<String>,
    #[serde(default)]
    pub minimum_runtime_version: Option<String>,
    #[serde(default)]
    pub license_url: Option<String>,
    #[serde(default)]
    pub source_verification: VerificationStatus,
    #[serde(default)]
    pub artifact_verification: VerificationStatus,
    /// The exact, direct, resolvable download URL for this artifact -
    /// deliberately separate from `official_source_url` (which may only
    /// be a landing/repository page). The frontend must never offer a
    /// Download button when this is `None` (spec: "never show Download
    /// unless the exact artifact URL is verified").
    #[serde(default)]
    pub exact_artifact_url: Option<String>,
    #[serde(default)]
    pub checksum_algorithm: Option<String>,
    #[serde(default)]
    pub checksum_value: Option<String>,
    /// Where the checksum came from (e.g. "publisher-published manifest",
    /// "computed by BRUTE after download") - never fabricated, and this
    /// field exists so a checksum's provenance is always inspectable.
    #[serde(default)]
    pub checksum_source: Option<String>,
    /// Free-form curator commentary, distinct from the structured
    /// `metadata_provenance`/`last_reviewed` audit trail above.
    #[serde(default)]
    pub curator_notes: Option<String>,
    #[serde(default)]
    pub arabic_capability: CapabilityLevel,
    #[serde(default)]
    pub coding_capability: CapabilityLevel,
    #[serde(default)]
    pub reasoning_capability: CapabilityLevel,
    #[serde(default)]
    pub general_quality: CapabilityLevel,
    #[serde(default)]
    pub speed_category: SpeedCategory,
    #[serde(default)]
    pub evidence_source: EvidenceSource,
    /// Free-form confidence note for the capability/speed levels above
    /// (e.g. "based on 3 real conversations, not a formal benchmark").
    #[serde(default)]
    pub benchmark_confidence: Option<String>,
}
