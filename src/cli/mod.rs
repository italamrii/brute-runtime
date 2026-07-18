use crate::catalog::TaskCategory;
use crate::recommend::Priority;
use crate::runtime::Backend;
use crate::tuning::ranking::RankingPriority;
use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

/// Default curated dev catalog, relative to the current working directory
/// (this repo's root when run via `cargo run`). Override with `--catalog`
/// when pointing at a different file.
pub const DEFAULT_CATALOG_PATH: &str = "data/catalog/dev-catalog.json";

/// Default calibration store - the seeded real-benchmark data shipped in
/// this repo. Override with `--calibration` to point at a store you've
/// added your own recorded runs to.
pub const DEFAULT_CALIBRATION_PATH: &str = "data/calibration/seed-calibration.json";

#[derive(Debug, Parser)]
#[command(name = "brute", version, about = "BRUTE Runtime - local-AI hardware intelligence and model fit engine", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Inspect this machine's hardware and print a capability report.
    Inspect {
        /// Print machine-readable JSON instead of a human-readable table.
        #[arg(long)]
        json: bool,
        /// Optional path to check free storage space against (e.g. the
        /// directory you intend to import a model into).
        #[arg(long)]
        storage_path: Option<PathBuf>,
    },
    /// Inspect a GGUF model file without loading it fully into memory.
    Model {
        #[command(subcommand)]
        action: ModelCommands,
    },
    /// Run a controlled CPU/GPU benchmark of a model through llama.cpp.
    Benchmark {
        /// Path to the GGUF model to benchmark.
        #[arg(long)]
        model: PathBuf,
        /// Directory containing the fetched/verified llama.cpp binaries
        /// (llama-cli.exe, llama-bench.exe) - see scripts/fetch-llama-cpp.ps1.
        #[arg(long)]
        llama_bin: PathBuf,
        #[command(flatten)]
        args: BenchmarkArgs,
        /// Print machine-readable JSON instead of a human-readable table.
        #[arg(long)]
        json: bool,
    },
    /// Run a benchmark and derive a runtime profile from the measured results.
    Recommend {
        /// Path to the GGUF model to benchmark.
        #[arg(long)]
        model: PathBuf,
        /// Directory containing the fetched/verified llama.cpp binaries
        /// (llama-cli.exe, llama-bench.exe) - see scripts/fetch-llama-cpp.ps1.
        #[arg(long)]
        llama_bin: PathBuf,
        #[command(flatten)]
        args: BenchmarkArgs,
        /// Print machine-readable JSON instead of a human-readable table.
        #[arg(long)]
        json: bool,
    },
    /// Verify a llama.cpp binary directory is present, hash-pinned, and
    /// actually launchable, without needing a model (`llama-cli --version`).
    Doctor {
        #[arg(long)]
        llama_bin: PathBuf,
        #[arg(long)]
        allow_unverified_binary: bool,
    },
    /// Run hardware inspection (+ optional model/benchmark) and write a full JSON report.
    Report {
        /// Where to write the JSON report.
        #[arg(long)]
        output: PathBuf,
        /// Optional model to include (adds a model-inspect section).
        #[arg(long)]
        model: Option<PathBuf>,
        /// Optional llama.cpp binary directory - if given alongside --model,
        /// a benchmark is run and included too.
        #[arg(long)]
        llama_bin: Option<PathBuf>,
        #[command(flatten)]
        args: BenchmarkArgs,
    },

    /// Hardware Capability Profile - a normalized, UI-stable snapshot of
    /// this machine (see docs/stage-1-hardware-intelligence.md).
    Profile {
        #[command(subcommand)]
        action: ProfileCommands,
    },
    /// Local curated Model Build Catalog (see docs/model-catalog-schema.md).
    Catalog {
        #[command(subcommand)]
        action: CatalogCommands,
    },
    /// Classify how well catalog build(s) fit this machine.
    Fit {
        #[command(flatten)]
        catalog: CatalogArgs,
        /// Classify a single catalog entry by its catalog_id.
        #[arg(long)]
        model: Option<String>,
        /// Classify every entry in the catalog.
        #[arg(long)]
        all: bool,
        #[arg(long)]
        context: Option<u32>,
        #[arg(long, value_enum)]
        backend: Option<Backend>,
        #[arg(long)]
        json: bool,
    },
    /// Rank catalog builds for a task/priority and produce a recommendation.
    RecommendModel {
        #[command(flatten)]
        catalog: CatalogArgs,
        #[arg(long, value_enum)]
        task: Option<TaskCategory>,
        #[arg(long, value_enum, default_value = "balanced")]
        priority: Priority,
        #[arg(long)]
        json: bool,
    },
    /// Explain, in both plain language and technical detail, why a
    /// specific catalog build was (or wasn't) recommended.
    ExplainFit {
        #[command(flatten)]
        catalog: CatalogArgs,
        #[arg(long)]
        model: String,
        #[arg(long, value_enum)]
        task: Option<TaskCategory>,
        #[arg(long, value_enum, default_value = "balanced")]
        priority: Priority,
        #[arg(long)]
        json: bool,
    },
    /// Inspect stored real-benchmark calibration records.
    Calibrations {
        #[command(subcommand)]
        action: CalibrationsCommands,
    },
    /// Manage the random, local-only, resettable instance identifier used
    /// in shareable exports in place of the coarse hardware-derived
    /// machine ID. See docs/privacy-model.md.
    Privacy {
        #[command(subcommand)]
        action: PrivacyCommands,
    },
    /// Backend capability verification (Stage 2) - proves CPU/CUDA/Vulkan
    /// actually work end to end, never just that a driver was detected.
    Backends {
        #[command(subcommand)]
        action: BackendsCommands,
    },
    /// Runtime auto-tuning (Stage 2): safely benchmark bounded candidate
    /// configurations and rank the results.
    Tune {
        #[command(subcommand)]
        action: TuneCommands,
    },
    /// Saved local runtime profiles (Stage 2) - never uploaded anywhere.
    Profiles {
        #[command(subcommand)]
        action: ProfilesCommands,
    },
    /// Trusted local model library (Stage 3): discovers, imports,
    /// verifies, and tracks GGUF models already on disk - never uploads,
    /// executes, or scans in the background.
    Library {
        #[command(subcommand)]
        action: LibraryCommands,
    },
}

#[derive(Debug, Subcommand)]
pub enum BackendsCommands {
    /// Verify a backend (or all of CPU/CUDA/Vulkan) actually launches,
    /// loads the model, and completes a tiny benchmark - never reports
    /// success from driver detection alone.
    Verify {
        #[arg(long)]
        model: PathBuf,
        #[arg(long)]
        llama_bin: PathBuf,
        /// Verify only this backend. Default: verify CPU plus whatever
        /// GPU backends were detected.
        #[arg(long, value_enum)]
        backend: Option<Backend>,
        #[arg(long)]
        allow_unverified_binary: bool,
        #[arg(long, default_value_t = 60)]
        timeout_secs: u64,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum TuneCommands {
    /// Generate the bounded candidate search space and, unless
    /// `--dry-run` is given, safely benchmark and rank it.
    Run {
        #[arg(long)]
        model: PathBuf,
        #[arg(long)]
        llama_bin: PathBuf,
        #[arg(long, value_enum, default_value = "balanced")]
        priority: RankingPriority,
        /// Restrict tuning to one backend. A GPU backend is only used for
        /// GPU-offload candidates once it passes `brute backends verify`
        /// internally - never assumed from detection alone. Default:
        /// auto-detect and verify CUDA/Vulkan opportunistically.
        #[arg(long, value_enum)]
        backend: Option<Backend>,
        /// Total tuning time budget, in seconds. Default: 30 minutes.
        #[arg(long)]
        max_duration_secs: Option<u64>,
        /// Show the planned, pruned candidates without launching any
        /// benchmarks.
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        allow_unverified_binary: bool,
        /// Save the winning configuration as a local runtime profile.
        #[arg(long)]
        save_profile: bool,
        #[arg(long)]
        json: bool,
    },
    /// Show the status of the most recent (or currently running) tuning run.
    Status {
        #[arg(long)]
        json: bool,
    },
    /// Request cancellation of a currently running `brute tune run`. Takes
    /// effect at its next safe checkpoint (between repetitions/candidates),
    /// never mid-process.
    Cancel,
}

#[derive(Debug, Subcommand)]
pub enum ProfilesCommands {
    /// List saved local runtime profile IDs.
    List {
        #[arg(long)]
        json: bool,
    },
    /// Show a saved profile's full details.
    Show {
        profile_id: String,
        #[arg(long)]
        json: bool,
    },
    /// Validate a saved profile against the current model/machine/binaries
    /// and launch a short run confirming it actually applies.
    Verify {
        profile_id: String,
        #[arg(long)]
        model: PathBuf,
        #[arg(long)]
        llama_bin: PathBuf,
        #[arg(long)]
        allow_unverified_binary: bool,
        #[arg(long, default_value_t = 60)]
        timeout_secs: u64,
        #[arg(long)]
        json: bool,
    },
    /// Export a saved profile to a file with the machine ID redacted.
    Export {
        profile_id: String,
        #[arg(long)]
        output: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
pub enum LibraryCommands {
    /// Discover GGUF files under a directory. Never imports, never
    /// executes anything found, never scans automatically or in the
    /// background - always one explicit, user-requested pass. Not
    /// recursive unless `--recursive` is given.
    Scan {
        path: PathBuf,
        #[arg(long)]
        recursive: bool,
        #[arg(long)]
        max_depth: Option<u32>,
        #[arg(long)]
        max_files: Option<usize>,
        #[arg(long)]
        max_total_bytes: Option<u64>,
        #[arg(long)]
        max_duration_secs: Option<u64>,
        #[arg(long)]
        json: bool,
    },
    /// Explicitly import one model file - reads and hashes it, never
    /// executes, uploads, copies, or modifies it.
    Import {
        path: PathBuf,
        /// A display name for this entry - purely local metadata.
        #[arg(long)]
        alias: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Scans a directory and imports every genuine GGUF candidate found.
    ImportDirectory {
        path: PathBuf,
        #[arg(long)]
        recursive: bool,
        #[arg(long)]
        max_depth: Option<u32>,
        #[arg(long)]
        max_files: Option<usize>,
        #[arg(long)]
        json: bool,
    },
    /// List every library entry.
    List {
        #[arg(long)]
        json: bool,
    },
    /// Show one library entry's full details.
    Show {
        library_id: String,
        #[arg(long)]
        json: bool,
    },
    /// Full verification pass (recomputes the hash) against one entry,
    /// or every entry with `--all`.
    Verify {
        library_id: Option<String>,
        #[arg(long)]
        all: bool,
        #[arg(long)]
        json: bool,
    },
    /// Cheap size/mtime-only refresh (no hashing) against one entry, or
    /// every entry with `--all`.
    Refresh {
        library_id: Option<String>,
        #[arg(long)]
        all: bool,
        #[arg(long)]
        json: bool,
    },
    /// Library-wide health report - missing/modified/corrupt entries,
    /// duplicates, stale profiles/calibrations, privacy concerns.
    Audit {
        #[arg(long)]
        json: bool,
    },
    /// Report groups of entries sharing an identical content hash.
    Duplicates {
        #[arg(long)]
        json: bool,
    },
    /// Storage usage summary - totals, duplicates, largest models,
    /// breakdown by architecture/quantization.
    Storage {
        #[arg(long)]
        json: bool,
    },
    /// Recover a moved/renamed model by pointing at its new path - only
    /// rebinds the entry if the new file's hash matches exactly.
    Locate {
        library_id: String,
        new_path: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Set a display name for an entry (local metadata only).
    Alias { library_id: String, name: String },
    /// Attach a free-text note to an entry (local metadata only).
    Note { library_id: String, text: String },
    /// Remove the library metadata entry - never deletes the underlying
    /// file.
    Forget { library_id: String },
    /// Delete a BRUTE-managed copy from disk (Stage 3 does not create
    /// any managed copies - see docs/model-import-and-verification.md -
    /// so this always reports there is nothing to remove).
    RemoveManaged {
        library_id: String,
        #[arg(long)]
        confirm: bool,
    },
    /// Hold an entry back from benchmarking/launch pending
    /// re-verification.
    Quarantine {
        library_id: String,
        #[arg(long)]
        reason: String,
    },
    /// Lift quarantine - only succeeds after a fresh verification pass
    /// actually passes; never bypasses validation.
    Unquarantine { library_id: String },
    /// List currently quarantined entries.
    Quarantined {
        #[arg(long)]
        json: bool,
    },
    /// Export the full library index as sanitized JSON - no local paths,
    /// no machine ID.
    Export {
        #[arg(long)]
        output: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
pub enum PrivacyCommands {
    /// Print the current local instance ID (creating one if none exists).
    ShowId,
    /// Delete the local instance ID so a brand new, unrelated one is
    /// generated next time it's needed.
    ResetId,
}

#[derive(Debug, Clone, Args)]
pub struct CatalogArgs {
    #[arg(long, default_value = DEFAULT_CATALOG_PATH)]
    pub catalog: PathBuf,
    #[arg(long, default_value = DEFAULT_CALIBRATION_PATH)]
    pub calibration: PathBuf,
    /// Directory to check free disk space against (e.g. where you intend
    /// to store downloaded models). Without this, disk-space fit checks
    /// are skipped and assumed sufficient - always reported honestly as
    /// such, never silently treated as "checked."
    #[arg(long)]
    pub storage_path: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
pub enum ProfileCommands {
    /// Build and print/save the normalized hardware capability profile.
    Create {
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long, default_value = DEFAULT_CALIBRATION_PATH)]
        calibration: PathBuf,
        #[arg(long)]
        storage_path: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum CatalogCommands {
    /// List every build in the catalog.
    List {
        #[arg(long, default_value = DEFAULT_CATALOG_PATH)]
        catalog: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Show full details for one catalog entry.
    Show {
        #[arg(long, default_value = DEFAULT_CATALOG_PATH)]
        catalog: PathBuf,
        catalog_id: String,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum CalibrationsCommands {
    /// List every stored calibration record.
    List {
        #[arg(long, default_value = DEFAULT_CALIBRATION_PATH)]
        calibration: PathBuf,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum ModelCommands {
    /// Parse and validate a GGUF file's metadata.
    Inspect {
        #[arg(long)]
        path: PathBuf,
        /// Print machine-readable JSON instead of a human-readable table.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Clone, Args)]
pub struct BenchmarkArgs {
    #[arg(long, value_enum, default_value = "cpu")]
    pub backend: Backend,

    /// 0 = use all logical cores detected on this machine.
    #[arg(long, default_value_t = 0)]
    pub threads: u32,

    #[arg(long, default_value_t = 0)]
    pub gpu_layers: u32,

    #[arg(long, default_value_t = 2048)]
    pub context_size: u32,

    #[arg(long, default_value_t = 512)]
    pub batch_size: u32,

    #[arg(long, default_value_t = 512)]
    pub prompt_tokens: u32,

    #[arg(long, default_value_t = 128)]
    pub gen_tokens: u32,

    /// Number of repeated samples llama-bench takes per test.
    #[arg(long, default_value_t = 3)]
    pub repetitions: u32,

    #[arg(long, default_value_t = 120)]
    pub timeout_secs: u64,

    /// Allow launching a llama.cpp binary with no local SHA-256 pin on
    /// record (e.g. one you built yourself rather than fetched via
    /// scripts/fetch-llama-cpp.ps1). A hash *mismatch* against an existing
    /// pin is always rejected regardless of this flag.
    #[arg(long)]
    pub allow_unverified_binary: bool,

    /// After a successful benchmark, append a real calibration record to
    /// this JSON file (created if absent) - see `brute calibrations list`
    /// and docs/calibration-methodology.md. Off by default.
    #[arg(long)]
    pub save_calibration: Option<PathBuf>,
}
