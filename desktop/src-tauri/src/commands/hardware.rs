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
#[tauri::command]
pub fn hardware_profile(app: AppHandle) -> Result<HardwareCapabilityProfile, String> {
    let hw = brute::hardware::inspect(None);
    let calibration_store =
        CalibrationStore::load(&paths::calibration_path(&app)).unwrap_or_default();
    Ok(brute::profile::build_profile_for_machine(
        &hw,
        now_rfc3339(),
        &calibration_store,
    ))
}
