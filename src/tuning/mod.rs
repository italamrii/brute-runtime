//! Runtime Auto-Tuning: safely benchmarks a bounded set of candidate
//! configurations for a real, already-inspected model on this machine,
//! and selects the best *verified* setup - never a config that merely
//! "should" work on paper. See `docs/stage-2-runtime-auto-tuning.md`.

pub mod apply;
pub mod candidates;
pub mod ranking;
pub mod runner;
pub mod runtime_profile;
pub mod stability;

pub const TUNING_FORMULA_VERSION: &str = "stage2-v1";
