//! Local model run workspace (spec section 13) - a single-session local
//! inference run with genuine token streaming and cooperative
//! cancellation. Never persists prompts or outputs; the frontend only
//! ever sees this session's own events. UTF-8 decoding is buffered
//! across chunk boundaries (`on_chunk` below) because llama-cli's raw
//! 4096-byte stdout reads can split a multi-byte character - including
//! Arabic text - across two chunks; emitting each chunk's bytes through
//! `from_utf8_lossy` independently would corrupt exactly that text.

use crate::state::{AppState, CancelFlag, is_cancelled, new_cancel_flag, request_cancel};
use brute::library::LibraryStore;
use brute::runtime::RuntimeConfig;
use brute::runtime::llama_cpp;
use brute::runtime::process::TickAction;
use brute::tuning::runtime_profile;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, State};

const LOCAL_RUN_GEN_TOKENS: u32 = 512;
const LOCAL_RUN_TIMEOUT_SECS: u64 = 300;

/// The run-workspace phase model (spec: "required run states"). Every
/// transition below is driven by a genuine engine signal - never a timer
/// or an animation:
/// - `Preparing`: resolving the library entry + saved profile (local,
///   no process yet).
/// - `ValidatingRuntime`: `verify_llama_binary` running for real against
///   the configured binary directory.
/// - `LoadingModel`: the binary validated; `llama-cli` is being spawned
///   and has not yet produced its first byte of output.
/// - `Generating`: the first stdout chunk has arrived.
/// - `Stopping`: cancellation was requested and the process is being
///   killed/reaped.
/// - `Completed` / `Failed` / `Cancelled`: terminal, matches
///   `LocalRunOutcome`.
#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunPhase {
    Preparing,
    ValidatingRuntime,
    LoadingModel,
    Generating,
    Stopping,
    Completed,
    Failed,
    Cancelled,
}

fn emit_phase(app: &AppHandle, phase: RunPhase) {
    let _ = app.emit("local-run-phase", phase);
}

#[derive(Serialize)]
pub struct LocalRunOutcome {
    pub succeeded: bool,
    pub timed_out: bool,
    pub cancelled: bool,
    pub generation_tokens_per_second: Option<f64>,
    pub prompt_tokens_per_second: Option<f64>,
    pub elapsed_secs: f64,
    pub error: Option<String>,
}

/// Runs one local prompt against a verified library model + saved
/// runtime profile, streaming generated text via the `"local-run-chunk"`
/// window event as it arrives. Never executes a raw file path handed
/// straight from the frontend - `library_id` must resolve through the
/// trusted library index first.
#[tauri::command(rename_all = "snake_case")]
pub async fn local_run_generate(
    app: AppHandle,
    state: State<'_, AppState>,
    library_id: String,
    profile_id: String,
    llama_bin: String,
    prompt: String,
    allow_unverified_binary: bool,
) -> Result<LocalRunOutcome, String> {
    let cancel_flag = new_cancel_flag();
    {
        let mut guard = state
            .run_cancel
            .lock()
            .map_err(|_| "run state lock was poisoned".to_string())?;
        *guard = Some(cancel_flag.clone());
    }

    let result = tauri::async_runtime::spawn_blocking(move || {
        generate_blocking(
            &app,
            &library_id,
            &profile_id,
            &llama_bin,
            &prompt,
            allow_unverified_binary,
            &cancel_flag,
        )
    })
    .await
    .map_err(|e| e.to_string())?;

    {
        let mut guard = state
            .run_cancel
            .lock()
            .map_err(|_| "run state lock was poisoned".to_string())?;
        *guard = None;
    }

    result
}

fn generate_blocking(
    app: &AppHandle,
    library_id: &str,
    profile_id: &str,
    llama_bin: &str,
    prompt: &str,
    allow_unverified_binary: bool,
    cancel_flag: &CancelFlag,
) -> Result<LocalRunOutcome, String> {
    emit_phase(app, RunPhase::Preparing);

    let store = LibraryStore::load_from(&brute::library::default_index_path())
        .map_err(|e| e.to_string())?;
    let model_path = store
        .require(library_id)
        .map_err(|e| e.to_string())?
        .current_path
        .clone();

    let profile =
        runtime_profile::load_profile_from(&runtime_profile::default_profiles_dir(), profile_id)
            .map_err(|e| e.to_string())?;

    let config = RuntimeConfig {
        backend: profile.backend,
        binary_dir: PathBuf::from(llama_bin),
        threads: profile.threads,
        gpu_layers: profile.gpu_layers,
        context_size: profile.context_size,
        batch_size: profile.batch_size,
        prompt_tokens: 0,
        gen_tokens: LOCAL_RUN_GEN_TOKENS,
        repetitions: 1,
        timeout_secs: LOCAL_RUN_TIMEOUT_SECS,
    };

    // A real validation pass against the exact binary that will be
    // launched - not a label, an actual hash check with an actual
    // failure mode, run before anything is spawned. `run_cli_streaming`
    // below re-validates internally too (its own safety invariant,
    // unconditional regardless of caller) - the cost is one cheap hash
    // read, not a second process launch.
    emit_phase(app, RunPhase::ValidatingRuntime);
    if let Err(e) = llama_cpp::verify_llama_binary(
        &llama_cpp::llama_cli_path(&config.binary_dir),
        allow_unverified_binary,
    ) {
        emit_phase(app, RunPhase::Failed);
        return Ok(LocalRunOutcome {
            succeeded: false,
            timed_out: false,
            cancelled: false,
            generation_tokens_per_second: None,
            prompt_tokens_per_second: None,
            elapsed_secs: 0.0,
            error: Some(e.to_string()),
        });
    }

    emit_phase(app, RunPhase::LoadingModel);

    let app_for_stream = app.clone();
    let leftover: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let generating_announced = Arc::new(AtomicBool::new(false));
    let on_chunk = move |bytes: &[u8]| {
        if !generating_announced.swap(true, Ordering::SeqCst) {
            emit_phase(&app_for_stream, RunPhase::Generating);
        }
        let Ok(mut buf) = leftover.lock() else {
            return;
        };
        buf.extend_from_slice(bytes);
        if let Some(text) = drain_utf8_prefix(&mut buf) {
            let _ = app_for_stream.emit("local-run-chunk", text);
        }
    };

    let flag_for_tick = cancel_flag.clone();
    let app_for_tick = app.clone();
    let stopping_announced = AtomicBool::new(false);
    let start = std::time::Instant::now();
    let result = llama_cpp::run_cli_streaming(
        &config.binary_dir,
        &model_path,
        &config,
        prompt,
        allow_unverified_binary,
        move |_| {
            if is_cancelled(&flag_for_tick) {
                if !stopping_announced.swap(true, Ordering::SeqCst) {
                    emit_phase(&app_for_tick, RunPhase::Stopping);
                }
                TickAction::Cancel
            } else {
                TickAction::Continue
            }
        },
        on_chunk,
    );
    let elapsed_secs = start.elapsed().as_secs_f64();

    Ok(match result {
        Ok((metrics, run)) => {
            emit_phase(
                app,
                if run.cancelled {
                    RunPhase::Cancelled
                } else if run.succeeded() {
                    RunPhase::Completed
                } else {
                    RunPhase::Failed
                },
            );
            LocalRunOutcome {
                succeeded: run.succeeded(),
                timed_out: run.timed_out,
                cancelled: run.cancelled,
                generation_tokens_per_second: metrics.eval_tokens_per_second,
                prompt_tokens_per_second: metrics.prompt_eval_tokens_per_second,
                elapsed_secs,
                error: None,
            }
        }
        Err(e) => {
            let cancelled = is_cancelled(cancel_flag);
            emit_phase(
                app,
                if cancelled {
                    RunPhase::Cancelled
                } else {
                    RunPhase::Failed
                },
            );
            LocalRunOutcome {
                succeeded: false,
                timed_out: false,
                cancelled,
                generation_tokens_per_second: None,
                prompt_tokens_per_second: None,
                elapsed_secs,
                error: Some(e.to_string()),
            }
        }
    })
}

/// Requests cancellation of the currently streaming local run, if any.
/// A no-op (not an error) when nothing is running.
#[tauri::command(rename_all = "snake_case")]
pub fn local_run_cancel(state: State<'_, AppState>) -> Result<(), String> {
    let guard = state
        .run_cancel
        .lock()
        .map_err(|_| "run state lock was poisoned".to_string())?;
    if let Some(flag) = guard.as_ref() {
        request_cancel(flag);
    }
    Ok(())
}

/// Removes and returns the longest valid-UTF-8 prefix of `buf`, leaving
/// any trailing incomplete multi-byte sequence in place for the next
/// chunk to complete. llama-cli's raw stdout reads are 4096-byte chunks
/// with no regard for character boundaries, so a naive
/// `from_utf8_lossy` per chunk would corrupt any multi-byte character -
/// including Arabic text - that happens to straddle a chunk boundary.
/// Returns `None` when nothing new and complete is available yet.
fn drain_utf8_prefix(buf: &mut Vec<u8>) -> Option<String> {
    let valid_len = match std::str::from_utf8(buf) {
        Ok(_) => buf.len(),
        Err(e) => e.valid_up_to(),
    };
    if valid_len == 0 {
        return None;
    }
    let text = String::from_utf8_lossy(&buf[..valid_len]).into_owned();
    buf.drain(..valid_len);
    Some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drain_utf8_prefix_returns_none_for_an_empty_buffer() {
        let mut buf = Vec::new();
        assert_eq!(drain_utf8_prefix(&mut buf), None);
    }

    #[test]
    fn drain_utf8_prefix_returns_the_whole_chunk_when_it_is_valid_utf8() {
        let mut buf = "hello".as_bytes().to_vec();
        assert_eq!(drain_utf8_prefix(&mut buf), Some("hello".to_string()));
        assert!(buf.is_empty());
    }

    /// The exact scenario the doc comment warns about: a multi-byte
    /// Arabic character's bytes split across two chunk boundaries must
    /// reassemble correctly rather than being corrupted or dropped.
    #[test]
    fn drain_utf8_prefix_holds_back_a_split_multibyte_character_across_chunks() {
        let word = "بيانات"; // "data" - multi-byte Arabic, each codepoint 2 bytes in UTF-8
        let bytes = word.as_bytes();
        assert!(bytes.len() > 2, "test needs a genuinely multi-byte string");
        let split_point = 3; // guaranteed to land mid-character for this word

        let mut buf = bytes[..split_point].to_vec();
        let first = drain_utf8_prefix(&mut buf);
        // Whatever prefix was already complete came through; the
        // incomplete trailing bytes must remain in `buf`, not be lost.
        let first_text = first.unwrap_or_default();
        assert!(
            !buf.is_empty(),
            "the incomplete trailing byte(s) must be held back"
        );

        buf.extend_from_slice(&bytes[split_point..]);
        let second = drain_utf8_prefix(&mut buf).unwrap_or_default();

        assert_eq!(format!("{first_text}{second}"), word);
        assert!(buf.is_empty());
    }

    #[test]
    fn drain_utf8_prefix_never_panics_on_truncated_trailing_bytes() {
        // A lone continuation byte can never become valid on its own -
        // draining must not panic or loop forever.
        let mut buf = vec![0xE2, 0x82]; // incomplete 3-byte sequence (would be part of e.g. '€')
        assert_eq!(drain_utf8_prefix(&mut buf), None);
        assert_eq!(buf.len(), 2);
    }
}
