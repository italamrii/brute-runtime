//! Explicit, user-triggered model download (spec: "Do not download
//! anything automatically... Keep all network activity visible and
//! user-triggered. Default application behavior remains offline.").
//!
//! This is the *only* place in the entire codebase that makes a network
//! request - `ureq` is a dependency of `brute-desktop` only, never of
//! the core `brute` engine crate, so the engine's zero-network-crate
//! privacy guarantee (see `docs/privacy-model.md`) is unchanged; a
//! download only ever happens because `download_model` was called,
//! which only ever happens from an explicit frontend button click.
//!
//! Streams to `<destination>.partial`, verifies the real SHA-256 of
//! what was actually written (never an assumed value - the catalog
//! schema has no independently curated expected hash to check against,
//! same honest limitation `library::import` already documents), and
//! only atomically renames to the final destination after a full,
//! uncancelled download. A cancelled or failed download deletes its
//! `.partial` file rather than leaving a truncated file at the real
//! destination path. The downloaded file is only ever written to -
//! never executed.
//!
//! **Known scope cut**: pause/resume (HTTP range-request resumption) is
//! not implemented in this pass - a cancelled download must be
//! restarted from zero. See `docs/known-limitations.md`.

use crate::state::{AppState, CancelFlag, is_cancelled, new_cancel_flag, request_cancel};
use brute::security::hashing::sha256_file;
use serde::Serialize;
use std::io::{Read, Write};
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, State};

/// Only emit a progress event every this many bytes, so a fast local
/// mirror doesn't flood IPC with an event per 64 KiB chunk.
const PROGRESS_EMIT_INTERVAL_BYTES: u64 = 512 * 1024;

#[derive(Serialize, Clone)]
struct DownloadProgressEvent {
    bytes_downloaded: u64,
    total_bytes: Option<u64>,
}

#[derive(Serialize)]
pub struct DownloadOutcome {
    pub succeeded: bool,
    pub cancelled: bool,
    pub final_path: Option<String>,
    pub sha256: Option<String>,
    pub bytes_downloaded: u64,
    pub error: Option<String>,
}

/// Downloads `url` to `destination_path`, which the frontend must have
/// already shown the user before calling this (spec: "Show destination
/// path before download"). Only `http://`/`https://` URLs are accepted.
#[tauri::command(rename_all = "snake_case")]
pub async fn download_model(
    app: AppHandle,
    state: State<'_, AppState>,
    url: String,
    destination_path: String,
) -> Result<DownloadOutcome, String> {
    if !is_supported_url(&url) {
        return Err("only http:// and https:// URLs are supported".to_string());
    }

    let cancel_flag = new_cancel_flag();
    {
        let mut guard = state
            .download_cancel
            .lock()
            .map_err(|_| "download state lock was poisoned".to_string())?;
        *guard = Some(cancel_flag.clone());
    }

    let result = tauri::async_runtime::spawn_blocking(move || {
        download_blocking(&app, &url, &destination_path, &cancel_flag)
    })
    .await
    .map_err(|e| e.to_string())?;

    {
        let mut guard = state
            .download_cancel
            .lock()
            .map_err(|_| "download state lock was poisoned".to_string())?;
        *guard = None;
    }

    Ok(result)
}

fn download_blocking(
    app: &AppHandle,
    url: &str,
    destination_path: &str,
    cancel_flag: &CancelFlag,
) -> DownloadOutcome {
    let destination = PathBuf::from(destination_path);
    let partial = PathBuf::from(format!("{destination_path}.partial"));

    let response = match ureq::get(url).call() {
        Ok(r) => r,
        Err(e) => return download_error(format!("request failed: {e}")),
    };
    let total_bytes = response
        .header("Content-Length")
        .and_then(|s| s.parse::<u64>().ok());

    let mut file = match std::fs::File::create(&partial) {
        Ok(f) => f,
        Err(e) => return download_error(format!("could not create {}: {e}", partial.display())),
    };

    let mut reader = response.into_reader();
    let mut buf = [0u8; 65536];
    let mut downloaded: u64 = 0;
    let mut last_emitted: u64 = 0;

    loop {
        if is_cancelled(cancel_flag) {
            drop(file);
            let _ = std::fs::remove_file(&partial);
            return DownloadOutcome {
                succeeded: false,
                cancelled: true,
                final_path: None,
                sha256: None,
                bytes_downloaded: downloaded,
                error: None,
            };
        }

        let n = match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => {
                drop(file);
                let _ = std::fs::remove_file(&partial);
                return download_error(format!("read failed after {downloaded} bytes: {e}"));
            }
        };

        if let Err(e) = file.write_all(&buf[..n]) {
            drop(file);
            let _ = std::fs::remove_file(&partial);
            return download_error(format!("write failed after {downloaded} bytes: {e}"));
        }

        downloaded += n as u64;
        if downloaded - last_emitted >= PROGRESS_EMIT_INTERVAL_BYTES {
            last_emitted = downloaded;
            let _ = app.emit(
                "download-progress",
                DownloadProgressEvent {
                    bytes_downloaded: downloaded,
                    total_bytes,
                },
            );
        }
    }
    drop(file);

    let _ = app.emit(
        "download-progress",
        DownloadProgressEvent {
            bytes_downloaded: downloaded,
            total_bytes,
        },
    );

    let sha256 = match sha256_file(&partial) {
        Ok(h) => h,
        Err(e) => {
            let _ = std::fs::remove_file(&partial);
            return download_error(format!("could not hash downloaded file: {e}"));
        }
    };

    if let Err(e) = std::fs::rename(&partial, &destination) {
        return download_error(format!("could not move completed download into place: {e}"));
    }

    DownloadOutcome {
        succeeded: true,
        cancelled: false,
        final_path: Some(destination.display().to_string()),
        sha256: Some(sha256),
        bytes_downloaded: downloaded,
        error: None,
    }
}

/// Only `http://`/`https://` are ever passed to `ureq` - rejects
/// anything else (a `file://` URL, a bare path, `javascript:`, etc.)
/// before any network or filesystem action is attempted.
fn is_supported_url(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

fn download_error(message: String) -> DownloadOutcome {
    DownloadOutcome {
        succeeded: false,
        cancelled: false,
        final_path: None,
        sha256: None,
        bytes_downloaded: 0,
        error: Some(message),
    }
}

/// Requests cancellation of the currently running download, if any. A
/// no-op (not an error) when nothing is downloading.
#[tauri::command(rename_all = "snake_case")]
pub fn cancel_download(state: State<'_, AppState>) -> Result<(), String> {
    let guard = state
        .download_cancel
        .lock()
        .map_err(|_| "download state lock was poisoned".to_string())?;
    if let Some(flag) = guard.as_ref() {
        request_cancel(flag);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_supported_url_accepts_only_http_and_https() {
        assert!(is_supported_url("https://example.com/model.gguf"));
        assert!(is_supported_url("http://example.com/model.gguf"));
        assert!(!is_supported_url("file:///etc/passwd"));
        assert!(!is_supported_url("javascript:alert(1)"));
        assert!(!is_supported_url("C:\\Models\\model.gguf"));
        assert!(!is_supported_url(""));
    }

    /// A cancelled/failed download must never leave a truncated file at
    /// the *real* destination path - only ever at the `.partial` path,
    /// which the caller is expected to clean up (and which
    /// `download_blocking` already does on every error/cancel branch).
    #[test]
    fn partial_file_suffix_never_collides_with_a_real_gguf_extension() {
        let destination = "C:\\Models\\example.gguf";
        let partial = format!("{destination}.partial");
        assert_ne!(destination, partial);
        assert!(partial.ends_with(".gguf.partial"));
    }
}
