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
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, State};

const LOCAL_RUN_GEN_TOKENS: u32 = 512;
const LOCAL_RUN_TIMEOUT_SECS: u64 = 300;

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
#[tauri::command]
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

    let app_for_stream = app.clone();
    let leftover: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let on_chunk = move |bytes: &[u8]| {
        let Ok(mut buf) = leftover.lock() else {
            return;
        };
        buf.extend_from_slice(bytes);
        let valid_len = match std::str::from_utf8(&buf) {
            Ok(_) => buf.len(),
            Err(e) => e.valid_up_to(),
        };
        if valid_len > 0 {
            let text = String::from_utf8_lossy(&buf[..valid_len]).into_owned();
            let _ = app_for_stream.emit("local-run-chunk", text);
            buf.drain(..valid_len);
        }
    };

    let flag_for_tick = cancel_flag.clone();
    let start = std::time::Instant::now();
    let result = llama_cpp::run_cli_streaming(
        &config.binary_dir,
        &model_path,
        &config,
        prompt,
        allow_unverified_binary,
        move |_| {
            if is_cancelled(&flag_for_tick) {
                TickAction::Cancel
            } else {
                TickAction::Continue
            }
        },
        on_chunk,
    );
    let elapsed_secs = start.elapsed().as_secs_f64();

    Ok(match result {
        Ok((metrics, run)) => LocalRunOutcome {
            succeeded: run.succeeded(),
            timed_out: run.timed_out,
            cancelled: run.cancelled,
            generation_tokens_per_second: metrics.eval_tokens_per_second,
            prompt_tokens_per_second: metrics.prompt_eval_tokens_per_second,
            elapsed_secs,
            error: None,
        },
        Err(e) => LocalRunOutcome {
            succeeded: false,
            timed_out: false,
            cancelled: is_cancelled(cancel_flag),
            generation_tokens_per_second: None,
            prompt_tokens_per_second: None,
            elapsed_secs,
            error: Some(e.to_string()),
        },
    })
}

/// Requests cancellation of the currently streaming local run, if any.
/// A no-op (not an error) when nothing is running.
#[tauri::command]
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
