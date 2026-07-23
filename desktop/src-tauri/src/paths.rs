//! Resolves the curated seed data (catalog + calibration fixtures) and
//! the bundled llama.cpp runtime the core engine ships with. These are
//! read-only, bundled with the app - never fetched, never written to,
//! never a place user data lives (user state always goes through
//! `brute::identity::default_local_state_dir`, i.e.
//! `%LOCALAPPDATA%\BruteRuntime\` on Windows, unrelated to this).

use std::path::PathBuf;
use tauri::{AppHandle, Manager};

/// The curated catalog file bundled with the app.
pub fn catalog_path(app: &AppHandle) -> PathBuf {
    seed_data_dir(app).join("catalog").join("dev-catalog.json")
}

/// The seed calibration records bundled with the app.
pub fn calibration_path(app: &AppHandle) -> PathBuf {
    seed_data_dir(app)
        .join("calibration")
        .join("seed-calibration.json")
}

/// In a packaged build, bundled resources live under the app's resource
/// directory (configured via `tauri.conf.json`'s `bundle.resources`). In
/// a dev build, no bundling has happened yet, so this falls back to the
/// repository's own `data/` directory two levels up from `src-tauri/`.
/// Either way, a missing file downstream degrades to an honest "not
/// found" (`load_catalog`/`CalibrationStore::load` already handle this),
/// never a fabricated fallback value.
fn seed_data_dir(app: &AppHandle) -> PathBuf {
    if let Ok(resource_dir) = app.path().resource_dir() {
        let candidate = resource_dir.join("data");
        if candidate.is_dir() {
            return candidate;
        }
    }
    dev_repo_data_dir()
}

#[cfg(debug_assertions)]
fn dev_repo_data_dir() -> PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("data")
}

#[cfg(not(debug_assertions))]
fn dev_repo_data_dir() -> PathBuf {
    PathBuf::from("data")
}

/// The bundled CPU-only llama.cpp runtime directory, if this build
/// actually has one (see `tauri.conf.json`'s `bundle.resources`, which
/// maps the pinned, hash-verified `.tools/llama.cpp/b10064/cpu` directory
/// into the packaged app). Populated per-platform from the same manifest:
/// `scripts/fetch-llama-cpp.ps1` on Windows, `scripts/fetch-llama-cpp.sh`
/// on macOS. In a dev build this falls back to the same real repo-relative
/// path those scripts write to.
/// Returns `None` (never a guessed/fabricated path) when neither
/// location actually exists - `commands::runtime::resolve_runtime`
/// treats that as "no bundled runtime available on this build/platform"
/// and continues down the resolution hierarchy.
pub fn bundled_runtime_dir(app: &AppHandle) -> Option<PathBuf> {
    if let Ok(resource_dir) = app.path().resource_dir() {
        let candidate = resource_dir.join("runtime").join("cpu");
        if candidate.is_dir() {
            return Some(candidate);
        }
    }
    let dev_candidate = dev_repo_runtime_dir();
    dev_candidate.is_dir().then_some(dev_candidate)
}

#[cfg(debug_assertions)]
fn dev_repo_runtime_dir() -> PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join(".tools")
        .join("llama.cpp")
        .join("b10064")
        .join("cpu")
}

#[cfg(not(debug_assertions))]
fn dev_repo_runtime_dir() -> PathBuf {
    PathBuf::from(".tools/llama.cpp/b10064/cpu")
}
