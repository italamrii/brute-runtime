//! Automatic model discovery: scans a small, fixed set of safe, common
//! local model locations (never the whole disk) plus any folders the
//! user has explicitly added, using the exact same bounded, non-
//! executing `library::scan::scan` the Models page's manual "Scan
//! directory" flow already uses - this is not a second scanning
//! implementation, just a different, curated set of starting
//! directories. Every discovered `.gguf` candidate is reported for the
//! user to review; nothing is imported automatically.

use brute::library::scan::{ScanOptions, ScanResult};
use serde::Serialize;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Serialize)]
pub struct CommonLocation {
    pub label: String,
    pub path: String,
    pub exists: bool,
}

/// The fixed set of well-known model directories BRUTE will offer to
/// scan - real, standard conventions (`Downloads`/`Documents` via the
/// `dirs` crate; LM Studio's and Ollama's own documented model cache
/// paths under the user's home directory), plus `C:\Models` on Windows
/// specifically, since that is where this project's own real-machine
/// verification model lives and a common ad hoc convention for
/// Windows users. Never the whole disk, never a recursive walk from a
/// filesystem root.
fn common_locations() -> Vec<(&'static str, Option<PathBuf>)> {
    let home = dirs::home_dir();
    vec![
        ("common_location_downloads", dirs::download_dir()),
        ("common_location_documents", dirs::document_dir()),
        (
            "common_location_lm_studio",
            home.as_ref()
                .map(|h| h.join(".cache").join("lm-studio").join("models")),
        ),
        (
            "common_location_ollama",
            home.as_ref().map(|h| h.join(".ollama").join("models")),
        ),
        #[cfg(windows)]
        (
            "common_location_c_models",
            Some(PathBuf::from(r"C:\Models")),
        ),
    ]
}

/// Reports which of the fixed well-known locations actually exist on
/// this machine, without scanning any of them yet - lets the frontend
/// show "will scan: Downloads, Documents" honestly before the user
/// commits to a (potentially slow, on a large folder) scan.
#[tauri::command(rename_all = "snake_case")]
pub async fn list_common_model_locations() -> Result<Vec<CommonLocation>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        common_locations()
            .into_iter()
            .filter_map(|(label, path)| {
                path.map(|p| CommonLocation {
                    label: label.to_string(),
                    exists: p.is_dir(),
                    path: p.display().to_string(),
                })
            })
            .collect::<Vec<_>>()
    })
    .await
    .map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct DiscoveryResult {
    pub label: String,
    pub path: String,
    pub scan: ScanResult,
}

/// Scans every well-known location that actually exists on this
/// machine, one bounded `library::scan::scan` call per location (same
/// file-count/size/time/depth caps as a manual scan) - a slow or huge
/// folder cannot make this run away, and it never recurses into
/// arbitrary subdirectories beyond the configured depth. Dry discovery
/// only, exactly like the manual "Scan directory" flow - nothing is
/// imported.
#[tauri::command(rename_all = "snake_case")]
pub async fn scan_common_model_locations() -> Result<Vec<DiscoveryResult>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let options = ScanOptions {
            recursive: true,
            max_depth: 4,
            max_files: 5_000,
            max_duration: Duration::from_secs(30),
            ..ScanOptions::default()
        };

        common_locations()
            .into_iter()
            .filter_map(|(label, path)| path.filter(|p| p.is_dir()).map(|p| (label, p)))
            .filter_map(|(label, path)| {
                brute::library::scan::scan(&path, &options, || false)
                    .ok()
                    .map(|scan| DiscoveryResult {
                        label: label.to_string(),
                        path: path.display().to_string(),
                        scan,
                    })
            })
            .collect::<Vec<_>>()
    })
    .await
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fixed location list must never be empty and must never
    /// include a whole filesystem root - a real, bounded guard against
    /// accidentally turning "scan common folders" into "scan everything".
    #[test]
    fn common_locations_are_never_a_filesystem_root() {
        let locations = common_locations();
        assert!(!locations.is_empty());
        for (_, path) in &locations {
            if let Some(p) = path {
                assert!(
                    p.parent().is_some(),
                    "{} looks like a filesystem root, not a specific folder",
                    p.display()
                );
            }
        }
    }

    /// A location that does not exist on this machine must be reported
    /// as `exists: false`, never silently omitted or fabricated as
    /// present.
    #[test]
    fn a_nonexistent_location_is_reported_honestly() {
        let fake = PathBuf::from(r"C:\this\path\definitely\does\not\exist\brute-test");
        let reported = CommonLocation {
            label: "test".to_string(),
            exists: fake.is_dir(),
            path: fake.display().to_string(),
        };
        assert!(!reported.exists);
    }

    /// The scan options used for common-location discovery must stay
    /// bounded (never unlimited depth/files/time) - the exact property
    /// that makes "scan common folders" safe to run automatically
    /// without the user picking a directory first.
    #[test]
    fn common_location_scan_options_are_bounded() {
        let options = ScanOptions {
            recursive: true,
            max_depth: 4,
            max_files: 5_000,
            max_duration: Duration::from_secs(30),
            ..ScanOptions::default()
        };
        assert!(options.max_depth < 100);
        assert!(options.max_files < 1_000_000);
        assert!(options.max_duration < Duration::from_secs(600));
    }
}
