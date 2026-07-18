pub mod llama_cpp;
pub mod process;

use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum Backend {
    Cpu,
    Cuda,
    Vulkan,
}

impl Backend {
    pub fn as_str(&self) -> &'static str {
        match self {
            Backend::Cpu => "cpu",
            Backend::Cuda => "cuda",
            Backend::Vulkan => "vulkan",
        }
    }
}

/// The exact runtime configuration used for a launch - recorded verbatim
/// into the report so results are reproducible and never presented as
/// "the" universal number for a model.
#[derive(Debug, Clone, Serialize)]
pub struct RuntimeConfig {
    pub backend: Backend,
    pub binary_dir: PathBuf,
    pub threads: u32,
    pub gpu_layers: u32,
    pub context_size: u32,
    pub batch_size: u32,
    pub prompt_tokens: u32,
    pub gen_tokens: u32,
    pub repetitions: u32,
    /// Stored as seconds (not `Duration`) so `RuntimeConfig` can derive
    /// `Serialize` without a wrapper crate; converted at call sites.
    pub timeout_secs: u64,
}

impl RuntimeConfig {
    pub fn timeout(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.timeout_secs)
    }
}
