use crate::runtime::Backend;
use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "brute", version, about = "BRUTE Runtime - Stage 0 local-AI capability inspector", long_about = None)]
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
}
