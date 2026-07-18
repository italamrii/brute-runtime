use crate::catalog::TaskCategory;
use crate::recommend::Priority;
use crate::runtime::Backend;
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
