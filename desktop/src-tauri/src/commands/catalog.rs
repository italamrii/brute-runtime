//! Catalog browsing, fit classification, and recommendation commands -
//! thin wrappers around `brute::catalog`/`brute::recommend`. All fit and
//! recommendation results are derived, never fabricated; the response
//! always carries the same confidence/limitation data the CLI prints.
//!
//! `fit_evaluate`/`recommend_model`/`explain_fit` all call
//! `build_machine_profile`, which calls `brute::hardware::inspect` - the
//! same routine that shells out to `nvidia-smi` in `hardware_profile`
//! (see commands/hardware.rs) and is not guaranteed to return instantly.
//! Every command here therefore runs on a blocking worker thread via
//! `tauri::async_runtime::spawn_blocking`, never directly on the IPC
//! thread. See `docs/architecture.md`'s "Stage 4 responsiveness" note.

use crate::paths;
use brute::calibration::CalibrationStore;
use brute::catalog::{self, Catalog, ModelBuild, TaskCategory};
use brute::profile::HardwareCapabilityProfile;
use brute::recommend::{self, BuildEvaluation, Priority, Recommendation, explain::Explanation};
use serde::Serialize;
use tauri::AppHandle;

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn load_catalog(app: &AppHandle) -> Result<Catalog, String> {
    catalog::load_catalog(&paths::catalog_path(app)).map_err(|e| e.to_string())
}

fn load_calibration(app: &AppHandle) -> CalibrationStore {
    CalibrationStore::load(&paths::calibration_path(app)).unwrap_or_default()
}

fn build_machine_profile(app: &AppHandle) -> HardwareCapabilityProfile {
    let hw = brute::hardware::inspect(None);
    brute::profile::build_profile_for_machine(&hw, now_rfc3339(), &load_calibration(app))
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
pub async fn catalog_list(app: AppHandle) -> Result<Vec<ModelBuild>, String> {
    off_thread(move || Ok(load_catalog(&app)?.builds)).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn catalog_show(app: AppHandle, catalog_id: String) -> Result<ModelBuild, String> {
    off_thread(move || {
        let catalog = load_catalog(&app)?;
        catalog
            .require(&catalog_id)
            .cloned()
            .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn calibrations_list(app: AppHandle) -> Result<CalibrationStore, String> {
    off_thread(move || Ok(load_calibration(&app))).await
}

/// Fit classification for one catalog build against this machine - spec
/// section 10's "fit and recommendation workspace" data source.
#[tauri::command(rename_all = "snake_case")]
pub async fn fit_evaluate(
    app: AppHandle,
    catalog_id: String,
    context: Option<u32>,
    backend: Option<brute::runtime::Backend>,
) -> Result<BuildEvaluation, String> {
    off_thread(move || {
        let catalog = load_catalog(&app)?;
        let calibration_store = load_calibration(&app);
        let machine_profile = build_machine_profile(&app);
        let build = catalog.require(&catalog_id).map_err(|e| e.to_string())?;
        Ok(recommend::evaluate_build(
            build,
            &machine_profile,
            &calibration_store,
            None,
            Priority::Balanced,
            context,
            backend,
        ))
    })
    .await
}

#[derive(Serialize)]
pub struct RecommendationDto {
    pub recommendation: Option<Recommendation>,
    pub ranking_formula_version: String,
}

#[tauri::command(rename_all = "snake_case")]
pub async fn recommend_model(
    app: AppHandle,
    task: Option<TaskCategory>,
    priority: Priority,
) -> Result<RecommendationDto, String> {
    off_thread(move || {
        let catalog = load_catalog(&app)?;
        let calibration_store = load_calibration(&app);
        let machine_profile = build_machine_profile(&app);

        let ranked = recommend::rank(
            &catalog,
            &machine_profile,
            &calibration_store,
            task,
            priority,
        );
        let recommendation = recommend::recommend(&ranked, &machine_profile);

        Ok(RecommendationDto {
            recommendation,
            ranking_formula_version: ranked.ranking_formula_version,
        })
    })
    .await
}

#[derive(Serialize)]
pub struct ExplainFitDto {
    pub evaluation: BuildEvaluation,
    pub explanation: Explanation,
}

/// The Simple + Technical explanation pair for one catalog build (spec
/// section 10).
#[tauri::command(rename_all = "snake_case")]
pub async fn explain_fit(
    app: AppHandle,
    catalog_id: String,
    task: Option<TaskCategory>,
    priority: Priority,
) -> Result<ExplainFitDto, String> {
    off_thread(move || {
        let catalog = load_catalog(&app)?;
        let calibration_store = load_calibration(&app);
        let machine_profile = build_machine_profile(&app);
        let build = catalog.require(&catalog_id).map_err(|e| e.to_string())?;

        let evaluation = recommend::evaluate_build(
            build,
            &machine_profile,
            &calibration_store,
            task,
            priority,
            None,
            None,
        );
        let explanation =
            recommend::explain::build_explanation(&evaluation, &None, &None, &machine_profile);

        Ok(ExplainFitDto {
            evaluation,
            explanation,
        })
    })
    .await
}
