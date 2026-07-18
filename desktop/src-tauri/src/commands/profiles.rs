//! Saved runtime profile management - list/show/verify/export/delete.
//! Verification always launches a real short confirmation run through
//! `tuning::apply::apply_and_verify` (spec section 12) - never a bare
//! flag flip. Export always sanitizes `machine_id` before it touches
//! disk.

use brute::models;
use brute::profile::HardwareCapabilityProfile;
use brute::tuning::apply::{ApplyRequest, ApplyResult};
use brute::tuning::runtime_profile::{self, RuntimeProfile};
use std::path::{Path, PathBuf};
use std::time::Duration;

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn binary_hash(path: &Path, allow_unverified_binary: bool) -> Option<String> {
    brute::runtime::llama_cpp::verify_llama_binary(path, allow_unverified_binary)
        .ok()
        .map(|c| c.sha256)
}

#[tauri::command]
pub fn profiles_list() -> Result<Vec<String>, String> {
    runtime_profile::list_profile_ids_in(&runtime_profile::default_profiles_dir())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn profiles_show(profile_id: String) -> Result<RuntimeProfile, String> {
    runtime_profile::load_profile_from(&runtime_profile::default_profiles_dir(), &profile_id)
        .map_err(|e| e.to_string())
}

/// Launches a real short verification run and reports whether the saved
/// settings still actually work on this machine/model/binaries - never
/// a cached or assumed result.
#[tauri::command]
pub fn profiles_verify(
    profile_id: String,
    model: String,
    llama_bin: String,
    allow_unverified_binary: bool,
    timeout_secs: u64,
) -> Result<ApplyResult, String> {
    let profile =
        runtime_profile::load_profile_from(&runtime_profile::default_profiles_dir(), &profile_id)
            .map_err(|e| e.to_string())?;
    let model_path = PathBuf::from(&model);
    let llama_bin_path = PathBuf::from(&llama_bin);
    let model_report = models::inspect_model(&model_path).map_err(|e| e.to_string())?;
    let hw = brute::hardware::inspect(model_path.parent());
    let machine_profile: HardwareCapabilityProfile =
        brute::profile::build_profile(&hw, now_rfc3339(), 0);

    let cli_hash = binary_hash(
        &brute::runtime::llama_cpp::llama_cli_path(&llama_bin_path),
        allow_unverified_binary,
    );
    let bench_hash = binary_hash(
        &brute::runtime::llama_cpp::llama_bench_path(&llama_bin_path),
        allow_unverified_binary,
    );

    Ok(brute::tuning::apply::apply_and_verify(ApplyRequest {
        profile: &profile,
        binary_dir: &llama_bin_path,
        model: &model_report,
        machine_profile: &machine_profile,
        llama_cli_sha256: cli_hash.as_deref(),
        llama_bench_sha256: bench_hash.as_deref(),
        allow_unverified_binary,
        timeout: Duration::from_secs(timeout_secs),
    }))
}

/// Writes a sanitized (machine ID redacted) copy of the profile to
/// `output_path`. Never includes a username or local path.
#[tauri::command]
pub fn profiles_export(profile_id: String, output_path: String) -> Result<(), String> {
    let profile =
        runtime_profile::load_profile_from(&runtime_profile::default_profiles_dir(), &profile_id)
            .map_err(|e| e.to_string())?;
    let sanitized = runtime_profile::sanitize_for_export(&profile);
    let json = serde_json::to_string_pretty(&sanitized).map_err(|e| e.to_string())?;
    std::fs::write(&output_path, json).map_err(|e| e.to_string())
}

/// Deletes only the local profile metadata file - the model and any
/// runtime binary are never touched.
#[tauri::command]
pub fn profiles_delete(profile_id: String) -> Result<(), String> {
    runtime_profile::delete_profile_from(&runtime_profile::default_profiles_dir(), &profile_id)
        .map_err(|e| e.to_string())
}
