//! Auto-tune workflow commands (spec section 11). `tune_dry_run` never
//! launches a process. `tune_run` launches real, bounded benchmark
//! repetitions and reports progress via the `"tune-progress"` window
//! event as each candidate completes, honoring cooperative cancellation
//! through `AppState.tune_cancel` (`tune_cancel` sets the flag;
//! `tuning::runner::execute_plan` polls it between candidates/
//! repetitions - the same mechanism `brute tune cancel`'s flag file
//! provides for the CLI, just in-process instead of cross-process).

use crate::state::{AppState, is_cancelled, new_cancel_flag, request_cancel};
use brute::backends::{self, BackendVerification};
use brute::models;
use brute::profile::HardwareCapabilityProfile;
use brute::runtime::Backend;
use brute::tuning::candidates::TuningPlan;
use brute::tuning::ranking::{RankingPriority, RankingResult};
use brute::tuning::runner::{self, TUNE_BACKEND_VERIFY_TIMEOUT_SECS, TuningRunSummary};
use serde::Serialize;
use std::path::PathBuf;
use std::time::Duration;
use tauri::{AppHandle, Emitter, State};

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

#[derive(Serialize)]
pub struct TunePlanDto {
    pub plan: TuningPlan,
    pub backend_verifications: Vec<BackendVerification>,
}

/// Builds the candidate plan and shows the verified-GPU-backend decision,
/// without launching any benchmark. Spec section 11's mandatory dry-run
/// preview before a real tuning run.
///
/// Runs on a blocking worker thread, not the IPC/UI thread: despite
/// never launching a benchmark, this still calls
/// `determine_verified_gpu_backend`, which launches real short
/// verification subprocesses for CUDA/Vulkan - not instant.
#[tauri::command(rename_all = "snake_case")]
pub async fn tune_dry_run(
    model: String,
    llama_bin: String,
    backend: Option<Backend>,
    allow_unverified_binary: bool,
) -> Result<TunePlanDto, String> {
    tauri::async_runtime::spawn_blocking(move || {
        tune_dry_run_impl(model, llama_bin, backend, allow_unverified_binary)
    })
    .await
    .map_err(|e| e.to_string())?
}

fn tune_dry_run_impl(
    model: String,
    llama_bin: String,
    backend: Option<Backend>,
    allow_unverified_binary: bool,
) -> Result<TunePlanDto, String> {
    let model_path = PathBuf::from(&model);
    let llama_bin_path = PathBuf::from(&llama_bin);
    let model_report = models::inspect_model(&model_path).map_err(|e| e.to_string())?;
    let hw = brute::hardware::inspect(model_path.parent());
    let machine_profile: HardwareCapabilityProfile =
        brute::profile::build_profile(&hw, now_rfc3339(), 0);

    let (verified_gpu_backend, backend_verifications) = backends::determine_verified_gpu_backend(
        &model_path,
        &llama_bin_path,
        &machine_profile,
        backend,
        allow_unverified_binary,
        Duration::from_secs(TUNE_BACKEND_VERIFY_TIMEOUT_SECS),
    );

    let plan = brute::tuning::candidates::generate_plan(
        &model_report,
        &machine_profile,
        verified_gpu_backend,
    );

    Ok(TunePlanDto {
        plan,
        backend_verifications,
    })
}

#[derive(Clone, Serialize)]
struct TuneProgressEvent {
    total_candidates: usize,
    completed_candidates: usize,
    current_candidate_id: Option<String>,
}

#[derive(Serialize)]
pub struct TuneRunDto {
    pub backend_verifications: Vec<BackendVerification>,
    pub summary_cancelled: bool,
    pub wall_time_secs: f64,
    pub candidate_results: Vec<brute::tuning::runner::CandidateResult>,
    pub ranking: RankingResult,
    pub saved_profile_id: Option<String>,
}

/// Runs a real tuning session: launches bounded `llama-bench` repetitions
/// for every candidate in the plan, emitting `"tune-progress"` after each
/// candidate. Never claims GPU tuning succeeded unless backend
/// verification (not mere driver detection) proved the GPU backend
/// actually works first.
#[allow(clippy::too_many_arguments)]
#[tauri::command(rename_all = "snake_case")]
pub async fn tune_run(
    app: AppHandle,
    state: State<'_, AppState>,
    model: String,
    llama_bin: String,
    priority: RankingPriority,
    backend: Option<Backend>,
    max_duration_secs: Option<u64>,
    allow_unverified_binary: bool,
    save_profile: bool,
) -> Result<TuneRunDto, String> {
    let cancel_flag = new_cancel_flag();
    {
        let mut guard = state
            .tune_cancel
            .lock()
            .map_err(|_| "tuning state lock was poisoned".to_string())?;
        *guard = Some(cancel_flag.clone());
    }

    let result = tauri::async_runtime::spawn_blocking(move || {
        run_tuning_blocking(
            &app,
            &model,
            &llama_bin,
            priority,
            backend,
            max_duration_secs,
            allow_unverified_binary,
            save_profile,
            &cancel_flag,
        )
    })
    .await
    .map_err(|e| e.to_string())?;

    {
        let mut guard = state
            .tune_cancel
            .lock()
            .map_err(|_| "tuning state lock was poisoned".to_string())?;
        *guard = None;
    }

    result
}

#[allow(clippy::too_many_arguments)]
fn run_tuning_blocking(
    app: &AppHandle,
    model: &str,
    llama_bin: &str,
    priority: RankingPriority,
    backend: Option<Backend>,
    max_duration_secs: Option<u64>,
    allow_unverified_binary: bool,
    save_profile: bool,
    cancel_flag: &crate::state::CancelFlag,
) -> Result<TuneRunDto, String> {
    let model_path = PathBuf::from(model);
    let llama_bin_path = PathBuf::from(llama_bin);
    let model_report = models::inspect_model(&model_path).map_err(|e| e.to_string())?;
    let hw = brute::hardware::inspect(model_path.parent());
    let machine_profile: HardwareCapabilityProfile =
        brute::profile::build_profile(&hw, now_rfc3339(), 0);

    let (verified_gpu_backend, backend_verifications) = backends::determine_verified_gpu_backend(
        &model_path,
        &llama_bin_path,
        &machine_profile,
        backend,
        allow_unverified_binary,
        Duration::from_secs(TUNE_BACKEND_VERIFY_TIMEOUT_SECS),
    );

    let plan = brute::tuning::candidates::generate_plan(
        &model_report,
        &machine_profile,
        verified_gpu_backend,
    );

    let runner_config = runner::RunnerConfig {
        total_timeout: max_duration_secs
            .map(Duration::from_secs)
            .unwrap_or(runner::DEFAULT_TOTAL_TIMEOUT),
        ..runner::RunnerConfig::default()
    };

    let total = plan.candidates.len();
    let model_path_for_bench = model_report.path.clone();

    let summary: TuningRunSummary = runner::execute_plan(
        &plan,
        &runner_config,
        || brute::hardware::memory::sample_bytes().map(|(_, avail)| avail),
        || {
            brute::hardware::windows::inspect_storage(&model_path_for_bench)
                .free_bytes
                .value
        },
        || is_cancelled(cancel_flag),
        |candidate| {
            let candidate_index = plan
                .candidates
                .iter()
                .position(|c| c.id == candidate.id)
                .unwrap_or(0);
            let _ = app.emit(
                "tune-progress",
                TuneProgressEvent {
                    total_candidates: total,
                    completed_candidates: candidate_index,
                    current_candidate_id: Some(candidate.id.clone()),
                },
            );
            let flag_for_tick = cancel_flag.clone();
            runner::run_candidate_repetition(
                &llama_bin_path,
                &model_path_for_bench,
                candidate,
                allow_unverified_binary,
                move |_| {
                    if is_cancelled(&flag_for_tick) {
                        brute::runtime::process::TickAction::Cancel
                    } else {
                        brute::runtime::process::TickAction::Continue
                    }
                },
            )
        },
    );

    let _ = app.emit(
        "tune-progress",
        TuneProgressEvent {
            total_candidates: total,
            completed_candidates: total,
            current_candidate_id: None,
        },
    );

    let ranking_result = brute::tuning::ranking::rank(&plan, &summary, priority);

    let mut saved_profile_id = None;
    if save_profile && let Some(winner) = &ranking_result.winner {
        let stability = summary
            .candidate_results
            .iter()
            .find(|r| r.candidate_id == winner.candidate_id)
            .map(|r| &r.stability);
        if let Some(stability) = stability {
            let profile_id = brute::tuning::runtime_profile::generate_profile_id();
            let cli_hash = brute::runtime::llama_cpp::verify_llama_binary(
                &brute::runtime::llama_cpp::llama_cli_path(&llama_bin_path),
                allow_unverified_binary,
            )
            .ok()
            .map(|c| c.sha256);
            let bench_hash = brute::runtime::llama_cpp::verify_llama_binary(
                &brute::runtime::llama_cpp::llama_bench_path(&llama_bin_path),
                allow_unverified_binary,
            )
            .ok()
            .map(|c| c.sha256);

            let runtime_profile = brute::tuning::runtime_profile::build_profile(
                brute::tuning::runtime_profile::ProfileInputs {
                    plan: &plan,
                    winner,
                    stability,
                    confidence: ranking_result.confidence,
                    machine_profile: &machine_profile,
                    model: &model_report,
                    llama_cli_sha256: cli_hash,
                    llama_bench_sha256: bench_hash,
                    tuning_date: now_rfc3339(),
                },
                profile_id.clone(),
            );

            let dir = brute::tuning::runtime_profile::default_profiles_dir();
            brute::tuning::runtime_profile::save_profile_to(&dir, &runtime_profile)
                .map_err(|e| e.to_string())?;
            saved_profile_id = Some(profile_id);
        }
    }

    Ok(TuneRunDto {
        backend_verifications,
        summary_cancelled: summary.cancelled,
        wall_time_secs: summary.wall_time.as_secs_f64(),
        candidate_results: summary.candidate_results,
        ranking: ranking_result,
        saved_profile_id,
    })
}

/// Requests cancellation of the currently running tuning session, if
/// any. A no-op (not an error) when no tuning run is active.
#[tauri::command(rename_all = "snake_case")]
pub fn tune_cancel(state: State<'_, AppState>) -> Result<(), String> {
    let guard = state
        .tune_cancel
        .lock()
        .map_err(|_| "tuning state lock was poisoned".to_string())?;
    if let Some(flag) = guard.as_ref() {
        request_cancel(flag);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A dry-run preview never launches a process, so a bad model path
    /// must fail during the model-inspection step and never reach
    /// candidate generation or panic.
    #[test]
    fn tune_dry_run_rejects_a_nonexistent_model_path() {
        let result = tune_dry_run_impl(
            "C:\\this\\path\\does\\not\\exist.gguf".to_string(),
            "C:\\also\\does\\not\\exist".to_string(),
            None,
            true,
        );
        assert!(result.is_err());
    }

    /// A fresh `AppState` has no active tuning session - `tune_cancel`'s
    /// no-op branch (guard is `None`) is exactly this state, so the
    /// frontend can call it defensively without first checking whether a
    /// run is active.
    #[test]
    fn app_state_has_no_active_tuning_session_before_any_run_starts() {
        let state = AppState::default();
        let flag_before = state.tune_cancel.lock().unwrap().clone();
        assert!(flag_before.is_none());
    }
}
