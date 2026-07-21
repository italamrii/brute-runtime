//! BRUTE Runtime core engine - the single source of truth for hardware
//! inspection, GGUF/model handling, backend verification, runtime
//! tuning, the trusted local model library, and reporting.
//!
//! Both the `brute` CLI binary (`src/main.rs`) and the Tauri desktop
//! backend (`src-tauri/`) depend on this library crate rather than
//! duplicating any logic - see `docs/desktop-architecture.md` for why
//! this split exists and what it does and does not change about
//! Stage 0-3 behavior (nothing: every module below is unchanged from
//! before the split, only its crate membership moved).

pub mod backends;
pub mod benchmark;
pub mod calibration;
pub mod catalog;
pub mod cli;
pub mod conversations;
pub mod errors;
pub mod estimator;
pub mod fit;
pub mod hardware;
pub mod identity;
pub mod library;
pub mod models;
pub mod platform;
pub mod preferences;
pub mod profile;
pub mod provenance;
pub mod recommend;
pub mod report;
pub mod runtime;
pub mod security;
pub mod tuning;

#[cfg(test)]
mod no_window_flash_guard_test;
#[cfg(test)]
mod stage1_fixtures_test;
#[cfg(test)]
mod stage3_performance_test;
