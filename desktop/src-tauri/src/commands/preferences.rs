//! Local-only user preference profile - get/save. Single JSON file under
//! `%LOCALAPPDATA%\BruteRuntime\preferences.json` (see
//! `brute::preferences`) - never uploaded, never synced, no account, no
//! cloud sync for this or anything else in BRUTE.
//!
//! Every command runs on a blocking worker thread
//! (`tauri::async_runtime::spawn_blocking`), matching every other
//! filesystem-facing command in this codebase - see
//! `docs/architecture.md`'s "Stage 4 responsiveness" note.

use brute::preferences::{self, Preferences};

async fn off_thread<T, F>(f: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
}

/// A fresh install (or a preferences file that has never been saved) has
/// no file yet - `brute::preferences::load_preferences_from` already
/// returns honest defaults for that case, not an error.
#[tauri::command(rename_all = "snake_case")]
pub async fn preferences_get() -> Result<Preferences, String> {
    off_thread(preferences_get_impl).await
}

fn preferences_get_impl() -> Result<Preferences, String> {
    preferences::load_preferences_from(&preferences::default_preferences_path())
        .map_err(|e| e.to_string())
}

/// `updated_at_rfc3339` is always stamped here with the real current
/// time, regardless of what the caller sent.
#[tauri::command(rename_all = "snake_case")]
pub async fn preferences_save(mut prefs: Preferences) -> Result<(), String> {
    off_thread(move || {
        prefs.updated_at_rfc3339 = chrono::Utc::now().to_rfc3339();
        preferences_save_impl(prefs)
    })
    .await
}

fn preferences_save_impl(prefs: Preferences) -> Result<(), String> {
    preferences::save_preferences_to(&preferences::default_preferences_path(), &prefs)
        .map_err(|e| e.to_string())
}

/// Resets to defaults by writing `Preferences::default()` over the saved
/// file - not a delete, so `preferences_get` immediately after still
/// returns a valid, fully-populated object rather than requiring every
/// caller to handle "file briefly doesn't exist."
#[tauri::command(rename_all = "snake_case")]
pub async fn preferences_reset() -> Result<Preferences, String> {
    off_thread(preferences_reset_impl).await
}

fn preferences_reset_impl() -> Result<Preferences, String> {
    let defaults = Preferences {
        updated_at_rfc3339: chrono::Utc::now().to_rfc3339(),
        ..Preferences::default()
    };
    preferences::save_preferences_to(&preferences::default_preferences_path(), &defaults)
        .map_err(|e| e.to_string())?;
    Ok(defaults)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_produces_a_fully_default_object() {
        // Matches commands::conversations's convention of not writing
        // throwaway data into the user's actual real %LOCALAPPDATA% state
        // in unit tests - this only checks the pure construction logic
        // shared by preferences_reset_impl, not the real save.
        let defaults = Preferences::default();
        assert_eq!(defaults.language, preferences::LanguagePreference::Both);
        assert!(defaults.permitted_licenses.is_empty());
        assert!(!defaults.cpu_only);
    }
}
