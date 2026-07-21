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
//! Streams to `<destination>.partial`, always computes the real SHA-256
//! of what was actually written (never an assumed value), and only
//! atomically renames to the final destination after a full,
//! uncancelled download. When the caller supplies the catalog's own
//! curated `checksum_value` (Stage B.1's `ChecksumVerified`+ builds -
//! see `docs/model-catalog-schema.md`), that hash is compared *before*
//! the rename; a mismatch deletes the `.partial` file and reports
//! failure - the real destination path is never left holding content
//! that didn't match. When no expected checksum is available (most
//! builds today), the download still succeeds, but `checksum_verified`
//! is honestly `None`, never a fabricated `true`. A cancelled or failed
//! download always deletes its `.partial` file rather than leaving a
//! truncated file at the real destination path. The downloaded file is
//! only ever written to - never executed.
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
    /// `Some(true)`/`Some(false)` only when the caller supplied an
    /// expected checksum to compare against (the catalog's own curated
    /// `checksum_value`); `None` when no expected value was available -
    /// never upgraded to `true` just because the download completed.
    pub checksum_verified: Option<bool>,
    pub bytes_downloaded: u64,
    pub error: Option<String>,
}

/// Downloads `url` to `destination_path`, which the frontend must have
/// already shown the user before calling this (spec: "Show destination
/// path before download"). Only `http://`/`https://` URLs are accepted.
/// `expected_sha256`, when given, must match the catalog's own curated
/// `checksum_value` for this exact artifact - never a guess.
#[tauri::command(rename_all = "snake_case")]
pub async fn download_model(
    app: AppHandle,
    state: State<'_, AppState>,
    url: String,
    destination_path: String,
    expected_sha256: Option<String>,
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
        // `download_blocking` itself takes no `AppHandle` - a real one
        // cannot be constructed in a unit test outside a running Tauri
        // app (see the same constraint documented in
        // `commands/runtime.rs`), so the emit is done via this closure
        // instead, keeping the actual download/streaming/checksum logic
        // directly unit-testable with a plain in-memory callback.
        let mut on_progress = |bytes_downloaded: u64, total_bytes: Option<u64>| {
            let _ = app.emit(
                "download-progress",
                DownloadProgressEvent {
                    bytes_downloaded,
                    total_bytes,
                },
            );
        };
        download_blocking(
            &url,
            &destination_path,
            expected_sha256.as_deref(),
            &cancel_flag,
            &mut on_progress,
        )
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
    url: &str,
    destination_path: &str,
    expected_sha256: Option<&str>,
    cancel_flag: &CancelFlag,
    on_progress: &mut dyn FnMut(u64, Option<u64>),
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
                checksum_verified: None,
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
            on_progress(downloaded, total_bytes);
        }
    }
    drop(file);

    on_progress(downloaded, total_bytes);

    let sha256 = match sha256_file(&partial) {
        Ok(h) => h,
        Err(e) => {
            let _ = std::fs::remove_file(&partial);
            return download_error(format!("could not hash downloaded file: {e}"));
        }
    };

    if expected_sha256.is_some_and(|expected| !checksum_matches(expected, &sha256)) {
        let _ = std::fs::remove_file(&partial);
        return DownloadOutcome {
            succeeded: false,
            cancelled: false,
            final_path: None,
            sha256: Some(sha256),
            checksum_verified: Some(false),
            bytes_downloaded: downloaded,
            error: Some(
                "the downloaded file's checksum did not match the expected value - \
                     it was deleted rather than saved, to avoid silently trusting \
                     unverified or tampered content"
                    .to_string(),
            ),
        };
    }

    if let Err(e) = std::fs::rename(&partial, &destination) {
        return download_error(format!("could not move completed download into place: {e}"));
    }

    DownloadOutcome {
        succeeded: true,
        cancelled: false,
        final_path: Some(destination.display().to_string()),
        sha256: Some(sha256),
        checksum_verified: expected_sha256.map(|_| true),
        bytes_downloaded: downloaded,
        error: None,
    }
}

/// Case-insensitive - published checksums are conventionally lowercase
/// hex but not universally, and `sha256_file` itself always lowercases.
fn checksum_matches(expected: &str, actual: &str) -> bool {
    expected.eq_ignore_ascii_case(actual)
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
        checksum_verified: None,
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

/// A small fixed safety margin on top of the expected download size, so
/// a download that would land exactly at the reported free-space
/// boundary is flagged as insufficient rather than starving the OS/
/// other apps down to zero free space.
const DOWNLOAD_SPACE_SAFETY_MARGIN_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Serialize)]
pub struct DiskSpaceCheck {
    /// `None` only when the OS free-space query itself failed (see
    /// `hardware::system::inspect_storage`) - never a guessed number.
    pub available_bytes: Option<u64>,
    pub required_bytes: u64,
    /// `None` when `available_bytes` is `None` - there is nothing
    /// honest to compare against.
    pub sufficient: Option<bool>,
}

fn space_is_sufficient(available_bytes: Option<u64>, required_bytes: u64) -> Option<bool> {
    available_bytes.map(|available| {
        available >= required_bytes.saturating_add(DOWNLOAD_SPACE_SAFETY_MARGIN_BYTES)
    })
}

/// Checks free disk space at `destination_path`'s drive/volume before a
/// download starts (spec: "check disk space before downloading"). Safe
/// to call with a path that doesn't exist yet - only the parent
/// directory/drive needs to be resolvable.
#[tauri::command(rename_all = "snake_case")]
pub async fn check_download_space(
    destination_path: String,
    required_bytes: u64,
) -> Result<DiskSpaceCheck, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let report = brute::hardware::system::inspect_storage(&PathBuf::from(&destination_path));
        let available_bytes = report.free_bytes.value;
        DiskSpaceCheck {
            sufficient: space_is_sufficient(available_bytes, required_bytes),
            available_bytes,
            required_bytes,
        }
    })
    .await
    .map_err(|e| e.to_string())
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

    #[test]
    fn checksum_matches_is_case_insensitive() {
        let hash = "a".repeat(64);
        assert!(checksum_matches(&hash, &hash));
        assert!(checksum_matches(&hash.to_uppercase(), &hash));
        assert!(checksum_matches(&hash, &hash.to_uppercase()));
    }

    #[test]
    fn checksum_matches_rejects_a_real_mismatch() {
        assert!(!checksum_matches(&"a".repeat(64), &"b".repeat(64)));
    }

    #[test]
    fn space_is_sufficient_is_honestly_unknown_when_free_space_could_not_be_queried() {
        assert_eq!(space_is_sufficient(None, 1_000_000), None);
    }

    #[test]
    fn space_is_sufficient_requires_the_safety_margin_on_top_of_the_required_bytes() {
        let required = 10_000_000_000u64;
        // Exactly enough for the file but nothing else - not sufficient.
        assert_eq!(space_is_sufficient(Some(required), required), Some(false));
        // Enough for the file plus the safety margin - sufficient.
        assert_eq!(
            space_is_sufficient(
                Some(required + DOWNLOAD_SPACE_SAFETY_MARGIN_BYTES),
                required
            ),
            Some(true)
        );
    }

    #[test]
    fn space_is_sufficient_flags_a_clearly_too_small_disk() {
        assert_eq!(
            space_is_sufficient(Some(1_000_000), 10_000_000_000),
            Some(false)
        );
    }
}

#[cfg(test)]
mod download_blocking_tests {
    //! Real end-to-end coverage of `download_blocking` itself - the
    //! streaming/checksum/cancellation logic above only had its pure
    //! helper functions (`checksum_matches`, `space_is_sufficient`)
    //! tested directly until now. A minimal, hand-rolled HTTP/1.1
    //! server bound to an ephemeral loopback port stands in for a real
    //! artifact host: it is test-only infrastructure spun up and torn
    //! down entirely within this process, never shipped, never
    //! reachable from outside this machine - it does not weaken the
    //! "the shipped app makes no network requests except an explicit,
    //! user-triggered download" guarantee, since nothing here ships.
    use super::*;
    use std::net::TcpListener;

    fn spawn_test_server(body: &'static [u8]) -> (String, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind an ephemeral loopback port");
        let port = listener.local_addr().unwrap().port();
        let handle = std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf);
                let header = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(header.as_bytes());
                let _ = stream.write_all(body);
                let _ = stream.flush();
            }
        });
        (format!("http://127.0.0.1:{port}/test.gguf"), handle)
    }

    /// A real, currently-closed loopback port: bound once to claim a
    /// free ephemeral port number, then immediately dropped so nothing
    /// is listening on it - deterministic "connection refused" without
    /// depending on any specific well-known port being closed on the
    /// machine running the test.
    fn unreachable_url() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        format!("http://127.0.0.1:{port}/unreachable.gguf")
    }

    fn temp_destination(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "brute-download-blocking-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    fn partial_path_for(destination: &std::path::Path) -> PathBuf {
        PathBuf::from(format!("{}.partial", destination.display()))
    }

    #[test]
    fn streams_the_real_body_writes_it_and_computes_a_real_sha256() {
        const BODY: &[u8] = b"fake gguf bytes for a real streamed download test";
        let (url, server) = spawn_test_server(BODY);
        let destination = temp_destination("download-ok.gguf");
        let cancel_flag = new_cancel_flag();
        let mut progress_events: Vec<(u64, Option<u64>)> = Vec::new();

        let outcome = download_blocking(
            &url,
            destination.to_str().unwrap(),
            None,
            &cancel_flag,
            &mut |d, t| {
                progress_events.push((d, t));
            },
        );
        server.join().ok();

        assert!(outcome.succeeded);
        assert!(!outcome.cancelled);
        assert!(outcome.error.is_none());
        assert_eq!(outcome.bytes_downloaded, BODY.len() as u64);
        assert_eq!(outcome.checksum_verified, None);
        assert_eq!(std::fs::read(&destination).unwrap(), BODY);
        assert_eq!(outcome.sha256, Some(sha256_file(&destination).unwrap()));
        assert!(!partial_path_for(&destination).exists());
        // The final flush-at-completion emit always fires, even for a
        // body smaller than PROGRESS_EMIT_INTERVAL_BYTES.
        assert!(!progress_events.is_empty());
        assert_eq!(
            progress_events.last(),
            Some(&(BODY.len() as u64, Some(BODY.len() as u64)))
        );

        let _ = std::fs::remove_file(&destination);
    }

    #[test]
    fn a_matching_expected_checksum_is_verified_and_the_file_is_kept() {
        const BODY: &[u8] = b"checksum-verified content";
        let scratch = temp_destination("checksum-scratch.bin");
        std::fs::write(&scratch, BODY).unwrap();
        let expected = sha256_file(&scratch).unwrap();
        let _ = std::fs::remove_file(&scratch);

        let (url, server) = spawn_test_server(BODY);
        let destination = temp_destination("download-checksum-ok.gguf");
        let cancel_flag = new_cancel_flag();

        let outcome = download_blocking(
            &url,
            destination.to_str().unwrap(),
            Some(&expected),
            &cancel_flag,
            &mut |_, _| {},
        );
        server.join().ok();

        assert!(outcome.succeeded);
        assert_eq!(outcome.checksum_verified, Some(true));
        assert_eq!(
            outcome.final_path.as_deref(),
            Some(destination.to_str().unwrap())
        );
        assert!(destination.exists());

        let _ = std::fs::remove_file(&destination);
    }

    #[test]
    fn a_checksum_mismatch_deletes_everything_and_never_saves_unverified_content() {
        const BODY: &[u8] = b"this body will not match the expected hash";
        let (url, server) = spawn_test_server(BODY);
        let destination = temp_destination("download-checksum-bad.gguf");
        let cancel_flag = new_cancel_flag();
        let wrong_hash = "0".repeat(64);

        let outcome = download_blocking(
            &url,
            destination.to_str().unwrap(),
            Some(&wrong_hash),
            &cancel_flag,
            &mut |_, _| {},
        );
        server.join().ok();

        assert!(!outcome.succeeded);
        assert!(!outcome.cancelled);
        assert_eq!(outcome.checksum_verified, Some(false));
        assert!(outcome.final_path.is_none());
        assert!(outcome.error.is_some());
        assert!(!destination.exists());
        assert!(!partial_path_for(&destination).exists());
    }

    #[test]
    fn a_pre_cancelled_flag_deletes_the_partial_file_and_saves_nothing() {
        const BODY: &[u8] = b"content that must never reach the final destination once cancelled";
        let (url, server) = spawn_test_server(BODY);
        let destination = temp_destination("download-cancelled.gguf");
        let cancel_flag = new_cancel_flag();
        request_cancel(&cancel_flag);

        let outcome = download_blocking(
            &url,
            destination.to_str().unwrap(),
            None,
            &cancel_flag,
            &mut |_, _| {},
        );
        server.join().ok();

        assert!(outcome.cancelled);
        assert!(!outcome.succeeded);
        assert!(outcome.final_path.is_none());
        assert!(!destination.exists());
        assert!(!partial_path_for(&destination).exists());
    }

    #[test]
    fn an_unreachable_server_reports_a_clear_error_and_writes_nothing() {
        let destination = temp_destination("download-unreachable.gguf");
        let cancel_flag = new_cancel_flag();

        let outcome = download_blocking(
            &unreachable_url(),
            destination.to_str().unwrap(),
            None,
            &cancel_flag,
            &mut |_, _| {},
        );

        assert!(!outcome.succeeded);
        assert!(!outcome.cancelled);
        assert!(outcome.error.is_some());
        assert!(!destination.exists());
        assert!(!partial_path_for(&destination).exists());
    }
}
