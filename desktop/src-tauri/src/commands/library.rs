//! Trusted local model library commands. Every path argument coming
//! from the frontend (typically via a native file/folder dialog) is
//! re-validated by the underlying `brute::library`/`brute::security`
//! functions exactly as the CLI does - never trusted just because it
//! arrived over IPC. See docs/tauri-security-boundary.md.
//!
//! Every command here runs its real work on a blocking worker thread via
//! `tauri::async_runtime::spawn_blocking`, never directly on the thread
//! that dispatches IPC messages. Import/scan/verify/audit all do real
//! disk I/O, hashing, and GGUF parsing that can legitimately take
//! seconds on a large model or a large library - a synchronous Tauri
//! command doing that work directly would freeze the whole window for
//! that entire time (a real, previously observed "Not Responding"
//! symptom). See `docs/architecture.md`'s "Stage 4 responsiveness" note.
//! Each command is a thin `async fn` wrapper around a private, plain
//! `_impl` function so the existing synchronous unit tests below need no
//! async test runtime.

use crate::paths;
use brute::library::audit::AuditReport;
use brute::library::duplicates::DuplicateGroup;
use brute::library::import::{ImportDirectoryOutcome, ImportOutcome};
use brute::library::scan::{ScanOptions, ScanResult};
use brute::library::storage::StorageSummary;
use brute::library::verify::LocateOutcome;
use brute::library::{self, GgufVerification, LibraryEntry, LibraryStore};
use brute::tuning::runtime_profile::RuntimeProfile;
use serde::Serialize;
use std::path::Path;
use std::time::Duration;
use tauri::AppHandle;

fn load_store() -> Result<LibraryStore, String> {
    LibraryStore::load_from(&library::default_index_path()).map_err(|e| e.to_string())
}

fn save_store(store: &LibraryStore) -> Result<(), String> {
    store
        .save_to(&library::default_index_path())
        .map_err(|e| e.to_string())
}

fn try_load_catalog(app: &AppHandle) -> Option<brute::catalog::Catalog> {
    brute::catalog::load_catalog(&paths::catalog_path(app)).ok()
}

fn try_load_calibration(app: &AppHandle) -> brute::calibration::CalibrationStore {
    brute::calibration::CalibrationStore::load(&paths::calibration_path(app)).unwrap_or_default()
}

async fn off_thread<T, F>(f: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command(rename_all = "snake_case")]
pub async fn library_list() -> Result<Vec<LibraryEntry>, String> {
    off_thread(library_list_impl).await
}

fn library_list_impl() -> Result<Vec<LibraryEntry>, String> {
    Ok(load_store()?.entries)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn library_show(library_id: String) -> Result<LibraryEntry, String> {
    off_thread(move || library_show_impl(library_id)).await
}

fn library_show_impl(library_id: String) -> Result<LibraryEntry, String> {
    let store = load_store()?;
    store
        .require(&library_id)
        .cloned()
        .map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct AssociationsDto {
    pub runtime_profiles: Vec<RuntimeProfile>,
    pub calibration_record_count: usize,
}

#[tauri::command(rename_all = "snake_case")]
pub async fn library_associations(
    app: AppHandle,
    library_id: String,
) -> Result<AssociationsDto, String> {
    off_thread(move || library_associations_impl(&app, library_id)).await
}

fn library_associations_impl(
    app: &AppHandle,
    library_id: String,
) -> Result<AssociationsDto, String> {
    let store = load_store()?;
    let entry = store.require(&library_id).map_err(|e| e.to_string())?;

    let profiles_dir = brute::tuning::runtime_profile::default_profiles_dir();
    let runtime_profiles =
        brute::library::associations::find_runtime_profiles_for(&entry.sha256, &profiles_dir);

    let calibration_store = try_load_calibration(app);
    let calibration_record_count =
        brute::library::associations::find_calibration_matches_for(&calibration_store, entry).len();

    Ok(AssociationsDto {
        runtime_profiles,
        calibration_record_count,
    })
}

#[tauri::command(rename_all = "snake_case")]
pub async fn library_import(
    app: AppHandle,
    path: String,
    alias: Option<String>,
) -> Result<ImportOutcome, String> {
    off_thread(move || library_import_impl(&app, path, alias)).await
}

fn library_import_impl(
    app: &AppHandle,
    path: String,
    alias: Option<String>,
) -> Result<ImportOutcome, String> {
    let mut store = load_store()?;
    let catalog = try_load_catalog(app);
    let outcome =
        library::import::import_model(Path::new(&path), alias, &mut store, catalog.as_ref())
            .map_err(|e| e.to_string())?;
    save_store(&store)?;
    Ok(outcome)
}

#[derive(serde::Deserialize)]
pub struct ScanOptionsDto {
    pub recursive: bool,
    pub max_depth: Option<u32>,
    pub max_files: Option<usize>,
    pub max_total_bytes: Option<u64>,
    pub max_duration_secs: Option<u64>,
}

fn build_scan_options(dto: &ScanOptionsDto) -> ScanOptions {
    let mut options = ScanOptions {
        recursive: dto.recursive,
        ..ScanOptions::default()
    };
    if let Some(d) = dto.max_depth {
        options.max_depth = d;
    }
    if let Some(f) = dto.max_files {
        options.max_files = f;
    }
    if let Some(b) = dto.max_total_bytes {
        options.max_total_bytes = b;
    }
    if let Some(s) = dto.max_duration_secs {
        options.max_duration = Duration::from_secs(s);
    }
    options
}

/// Dry discovery only - never imports anything. Matches
/// `brute library scan`'s default behavior exactly.
#[tauri::command(rename_all = "snake_case")]
pub async fn library_scan(root: String, options: ScanOptionsDto) -> Result<ScanResult, String> {
    off_thread(move || library_scan_impl(root, options)).await
}

fn library_scan_impl(root: String, options: ScanOptionsDto) -> Result<ScanResult, String> {
    let scan_options = build_scan_options(&options);
    library::scan::scan(Path::new(&root), &scan_options, || false).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn library_import_directory(
    app: AppHandle,
    root: String,
    options: ScanOptionsDto,
) -> Result<ImportDirectoryOutcome, String> {
    off_thread(move || library_import_directory_impl(&app, root, options)).await
}

fn library_import_directory_impl(
    app: &AppHandle,
    root: String,
    options: ScanOptionsDto,
) -> Result<ImportDirectoryOutcome, String> {
    let mut store = load_store()?;
    let catalog = try_load_catalog(app);
    let scan_options = build_scan_options(&options);
    let outcome = library::import::import_directory(
        Path::new(&root),
        &scan_options,
        &mut store,
        catalog.as_ref(),
        || false,
    )
    .map_err(|e| e.to_string())?;
    save_store(&store)?;
    Ok(outcome)
}

#[derive(Serialize)]
pub struct VerifyOutcomeDto {
    pub library_id: String,
    pub verification: Option<GgufVerification>,
    pub skipped_reason: Option<String>,
}

#[tauri::command(rename_all = "snake_case")]
pub async fn library_verify(
    library_id: Option<String>,
    all: bool,
) -> Result<Vec<VerifyOutcomeDto>, String> {
    off_thread(move || library_verify_impl(library_id, all)).await
}

fn library_verify_impl(
    library_id: Option<String>,
    all: bool,
) -> Result<Vec<VerifyOutcomeDto>, String> {
    if !all && library_id.is_none() {
        return Err("either a library_id or all=true is required".to_string());
    }
    let mut store = load_store()?;
    let ids: Vec<String> = if all {
        store.entries.iter().map(|e| e.library_id.clone()).collect()
    } else {
        vec![library_id.unwrap()]
    };

    let mut results = Vec::new();
    for id in &ids {
        let confidence = store
            .require(id)
            .map_err(|e| e.to_string())?
            .catalog_match
            .confidence;
        let entry = store
            .find_by_id_mut(id)
            .ok_or_else(|| format!("no library entry with id {id:?}"))?;
        if entry.is_quarantined() {
            results.push(VerifyOutcomeDto {
                library_id: id.clone(),
                verification: None,
                skipped_reason: Some("quarantined - unquarantine first".to_string()),
            });
            continue;
        }
        let result = library::verify::verify_entry(entry, confidence);
        results.push(VerifyOutcomeDto {
            library_id: id.clone(),
            verification: Some(result),
            skipped_reason: None,
        });
    }
    save_store(&store)?;
    Ok(results)
}

#[derive(Serialize)]
pub struct RefreshOutcomeDto {
    pub library_id: String,
    pub file_status: brute::library::FileStatus,
}

#[tauri::command(rename_all = "snake_case")]
pub async fn library_refresh(
    library_id: Option<String>,
    all: bool,
) -> Result<Vec<RefreshOutcomeDto>, String> {
    off_thread(move || library_refresh_impl(library_id, all)).await
}

fn library_refresh_impl(
    library_id: Option<String>,
    all: bool,
) -> Result<Vec<RefreshOutcomeDto>, String> {
    if !all && library_id.is_none() {
        return Err("either a library_id or all=true is required".to_string());
    }
    let mut store = load_store()?;
    let ids: Vec<String> = if all {
        store.entries.iter().map(|e| e.library_id.clone()).collect()
    } else {
        vec![library_id.unwrap()]
    };

    let mut results = Vec::new();
    for id in &ids {
        let entry = store
            .find_by_id_mut(id)
            .ok_or_else(|| format!("no library entry with id {id:?}"))?;
        let status = library::verify::refresh_entry(entry);
        results.push(RefreshOutcomeDto {
            library_id: id.clone(),
            file_status: status,
        });
    }
    save_store(&store)?;
    Ok(results)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn library_audit(app: AppHandle) -> Result<AuditReport, String> {
    off_thread(move || library_audit_impl(&app)).await
}

fn library_audit_impl(app: &AppHandle) -> Result<AuditReport, String> {
    let store = load_store()?;
    let calibration_store = try_load_calibration(app);
    let profiles_dir = brute::tuning::runtime_profile::default_profiles_dir();
    Ok(library::audit::audit(
        &store,
        &profiles_dir,
        Some(&calibration_store),
    ))
}

#[tauri::command(rename_all = "snake_case")]
pub async fn library_duplicates() -> Result<Vec<DuplicateGroup>, String> {
    off_thread(library_duplicates_impl).await
}

fn library_duplicates_impl() -> Result<Vec<DuplicateGroup>, String> {
    Ok(library::duplicates::find_duplicate_groups(&load_store()?))
}

#[tauri::command(rename_all = "snake_case")]
pub async fn library_storage() -> Result<StorageSummary, String> {
    off_thread(library_storage_impl).await
}

fn library_storage_impl() -> Result<StorageSummary, String> {
    Ok(library::storage::summarize(&load_store()?))
}

#[tauri::command(rename_all = "snake_case")]
pub async fn library_locate(library_id: String, new_path: String) -> Result<LocateOutcome, String> {
    off_thread(move || library_locate_impl(library_id, new_path)).await
}

fn library_locate_impl(library_id: String, new_path: String) -> Result<LocateOutcome, String> {
    let mut store = load_store()?;
    let outcome = library::verify::locate(&mut store, &library_id, Path::new(&new_path))
        .map_err(|e| e.to_string())?;
    save_store(&store)?;
    Ok(outcome)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn library_alias(library_id: String, name: String) -> Result<(), String> {
    off_thread(move || library_alias_impl(library_id, name)).await
}

fn library_alias_impl(library_id: String, name: String) -> Result<(), String> {
    let mut store = load_store()?;
    let entry = store
        .find_by_id_mut(&library_id)
        .ok_or_else(|| format!("no library entry with id {library_id:?}"))?;
    entry.alias = Some(name);
    save_store(&store)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn library_note(library_id: String, text: String) -> Result<(), String> {
    off_thread(move || library_note_impl(library_id, text)).await
}

fn library_note_impl(library_id: String, text: String) -> Result<(), String> {
    let mut store = load_store()?;
    let entry = store
        .find_by_id_mut(&library_id)
        .ok_or_else(|| format!("no library entry with id {library_id:?}"))?;
    entry.notes = Some(text);
    save_store(&store)
}

/// Removes tracking metadata only - the underlying model file is never
/// touched. See docs/quarantine-and-recovery.md.
#[tauri::command(rename_all = "snake_case")]
pub async fn library_forget(library_id: String) -> Result<(), String> {
    off_thread(move || library_forget_impl(library_id)).await
}

fn library_forget_impl(library_id: String) -> Result<(), String> {
    let mut store = load_store()?;
    store.forget(&library_id).map_err(|e| e.to_string())?;
    save_store(&store)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn library_quarantine(library_id: String, reason: String) -> Result<(), String> {
    off_thread(move || library_quarantine_impl(library_id, reason)).await
}

fn library_quarantine_impl(library_id: String, reason: String) -> Result<(), String> {
    let mut store = load_store()?;
    store
        .quarantine(&library_id, reason)
        .map_err(|e| e.to_string())?;
    save_store(&store)
}

/// Always re-runs a full verification pass before clearing quarantine -
/// never a bare flag flip. See docs/quarantine-and-recovery.md.
#[tauri::command(rename_all = "snake_case")]
pub async fn library_unquarantine(library_id: String) -> Result<GgufVerification, String> {
    off_thread(move || library_unquarantine_impl(library_id)).await
}

fn library_unquarantine_impl(library_id: String) -> Result<GgufVerification, String> {
    let mut store = load_store()?;
    let result = store.unquarantine(&library_id).map_err(|e| e.to_string());
    save_store(&store)?;
    result
}

#[tauri::command(rename_all = "snake_case")]
pub async fn library_quarantined() -> Result<Vec<LibraryEntry>, String> {
    off_thread(library_quarantined_impl).await
}

fn library_quarantined_impl() -> Result<Vec<LibraryEntry>, String> {
    Ok(load_store()?
        .entries
        .into_iter()
        .filter(|e| e.is_quarantined())
        .collect())
}

/// Writes the full library index to `output_path`, sanitized
/// (`library::sanitize_entry_for_export`) - no local paths, no machine
/// identifiers. Returns the number of entries written.
#[tauri::command(rename_all = "snake_case")]
pub async fn library_export(output_path: String) -> Result<usize, String> {
    off_thread(move || library_export_impl(output_path)).await
}

fn library_export_impl(output_path: String) -> Result<usize, String> {
    let store = load_store()?;
    let sanitized: Vec<LibraryEntry> = store
        .entries
        .iter()
        .map(library::sanitize_entry_for_export)
        .collect();
    let json = serde_json::to_string_pretty(&sanitized).map_err(|e| e.to_string())?;
    std::fs::write(&output_path, json).map_err(|e| e.to_string())?;
    Ok(sanitized.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_scan_options() -> ScanOptionsDto {
        ScanOptionsDto {
            recursive: true,
            max_depth: None,
            max_files: None,
            max_total_bytes: None,
            max_duration_secs: None,
        }
    }

    /// A nonexistent directory must come back as a sanitized `Err`, never
    /// a panic - the frontend hands this command whatever a native
    /// folder-picker or a stale saved path returns, which is not
    /// guaranteed to still exist.
    #[test]
    fn library_scan_rejects_a_nonexistent_root_without_panicking() {
        let result = library_scan_impl(
            "C:\\this\\path\\does\\not\\exist\\brute-test".to_string(),
            default_scan_options(),
        );
        assert!(result.is_err());
    }

    /// Scanning never imports anything by itself - a directory containing
    /// no `.gguf` files must report zero discovered candidates, not
    /// error, and must not create a library index as a side effect.
    #[test]
    fn library_scan_on_an_empty_directory_reports_zero_candidates() {
        let dir = std::env::temp_dir().join(format!("brute-desktop-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let result = library_scan_impl(dir.display().to_string(), default_scan_options());

        std::fs::remove_dir_all(&dir).ok();

        let scan = result.expect("scanning an existing empty directory must succeed");
        assert_eq!(scan.discovered.len(), 0);
        assert!(!scan.cancelled);
    }

    /// Live real-machine acceptance check (spec section 24): when this
    /// machine's already-imported Qwen2.5-0.5B model is present in the
    /// real local library, `library_list` must show it with its real,
    /// previously-verified SHA-256 - not a placeholder or fabricated
    /// value. Skipped, not failed, on a machine without that local
    /// state, matching the core crate's own real-model test convention
    /// (see stage3_performance_test.rs).
    #[test]
    fn library_list_shows_the_real_imported_model_with_its_real_hash_if_present() {
        const KNOWN_SHA256: &str =
            "74a4da8c9fdbcd15bd1f6d01d621410d31c6fc00986f5eb687824e7b93d7a9db";
        if !std::path::Path::new("C:\\Models\\qwen2.5-0.5b-instruct-q4_k_m.gguf").exists() {
            eprintln!("skipping: real model fixture not present on this machine");
            return;
        }

        let entries = library_list_impl().expect("the real local library index must load");
        let real_entry = entries
            .iter()
            .find(|e| e.sha256 == KNOWN_SHA256)
            .expect("the real imported model must be tracked in the local library");
        assert_eq!(real_entry.architecture.as_deref(), Some("qwen2"));
        assert!(
            real_entry.current_path.is_file(),
            "the tracked path must point at a real file, never a fabricated one"
        );
    }
}
