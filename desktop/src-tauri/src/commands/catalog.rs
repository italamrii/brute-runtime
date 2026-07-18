//! Catalog browsing, fit classification, and recommendation commands -
//! thin wrappers around `brute::catalog`/`brute::recommend`. All fit and
//! recommendation results are derived, never fabricated; the response
//! always carries the same confidence/limitation data the CLI prints.

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

#[tauri::command]
pub fn catalog_list(app: AppHandle) -> Result<Vec<ModelBuild>, String> {
    Ok(load_catalog(&app)?.builds)
}

#[tauri::command]
pub fn catalog_show(app: AppHandle, catalog_id: String) -> Result<ModelBuild, String> {
    let catalog = load_catalog(&app)?;
    catalog
        .require(&catalog_id)
        .cloned()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn calibrations_list(app: AppHandle) -> Result<CalibrationStore, String> {
    Ok(load_calibration(&app))
}

/// Fit classification for one catalog build against this machine - spec
/// section 10's "fit and recommendation workspace" data source.
#[tauri::command]
pub fn fit_evaluate(
    app: AppHandle,
    catalog_id: String,
    context: Option<u32>,
    backend: Option<brute::runtime::Backend>,
) -> Result<BuildEvaluation, String> {
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
}

#[derive(Serialize)]
pub struct RecommendationDto {
    pub recommendation: Option<Recommendation>,
    pub ranking_formula_version: String,
}

#[tauri::command]
pub fn recommend_model(
    app: AppHandle,
    task: Option<TaskCategory>,
    priority: Priority,
) -> Result<RecommendationDto, String> {
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
}

#[derive(Serialize)]
pub struct ExplainFitDto {
    pub evaluation: BuildEvaluation,
    pub explanation: Explanation,
}

/// The Simple + Technical explanation pair for one catalog build (spec
/// section 10).
#[tauri::command]
pub fn explain_fit(
    app: AppHandle,
    catalog_id: String,
    task: Option<TaskCategory>,
    priority: Priority,
) -> Result<ExplainFitDto, String> {
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
}
