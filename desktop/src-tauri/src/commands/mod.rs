//! The full Tauri command surface - every function the frontend can call
//! is declared here, grouped by the engine area it wraps. Every command
//! returns `Result<_, String>` with a sanitized error message (see
//! docs/tauri-security-boundary.md) and every filesystem-facing
//! parameter is re-validated by the underlying `brute` engine call, not
//! trusted just because it crossed IPC.

pub mod backends;
pub mod catalog;
pub mod conversations;
pub mod discovery;
pub mod download;
pub mod hardware;
pub mod library;
pub mod preferences;
pub mod profiles;
pub mod run;
pub mod runtime;
pub mod tuning;
