//! Hardware inspection commands - thin, validated wrappers around
//! `brute::hardware`/`brute::profile`. No logic is duplicated here; this
//! module only shapes the real engine output for IPC and sanitizes
//! errors before they reach the frontend.

use crate::paths;
use brute::calibration::CalibrationStore;
use brute::profile::HardwareCapabilityProfile;
use tauri::AppHandle;

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// The normalized hardware capability profile - identical data to
/// `brute profile create --json`, including confidence-tagged fields
/// (measured/detected/inferred/unavailable) for every value, and a real
/// `calibration_record_count` for this machine. Never a fabricated
/// universal score.
///
/// Runs on a blocking worker thread, not the IPC/UI thread: hardware
/// inspection shells out to `nvidia-smi` and reads several Win32 APIs
/// that are not guaranteed to return instantly (a slow/hung driver call
/// must not freeze the window). See `docs/architecture.md`'s "Stage 4
/// responsiveness" note.
#[tauri::command(rename_all = "snake_case")]
pub async fn hardware_profile(app: AppHandle) -> Result<HardwareCapabilityProfile, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let hw = brute::hardware::inspect(None);
        let calibration_store =
            CalibrationStore::load(&paths::calibration_path(&app)).unwrap_or_default();
        Ok(brute::profile::build_profile_for_machine(
            &hw,
            now_rfc3339(),
            &calibration_store,
        ))
    })
    .await
    .map_err(|e| e.to_string())?
}
