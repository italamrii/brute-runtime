//! Resolves the curated seed data (catalog + calibration fixtures) the
//! core engine ships with. These are read-only, bundled with the app -
//! never fetched, never written to, never a place user data lives (user
//! state always goes through `brute::identity::default_local_state_dir`,
//! i.e. `%LOCALAPPDATA%\BruteRuntime\`, unrelated to this).

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
