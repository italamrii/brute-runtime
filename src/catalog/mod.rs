//! Model Build Catalog: local, curated metadata about exact downloadable
//! model artifacts ("builds"). BRUTE never hosts, redistributes, scrapes,
//! or downloads model files - a catalog entry is a pointer to an official
//! source plus enough metadata to estimate whether it fits this machine
//! *before* the user downloads it.
//!
//! Catalog files are treated as untrusted input: bounded file size, bounded
//! entry count, and every entry is structurally and semantically validated
//! (no path traversal in filenames, only http(s) source URLs, no silent
//! coercion of missing license/commercial-use data into an assumed-safe
//! default). See `docs/model-catalog-schema.md`.

pub mod schema;

use crate::errors::CatalogError;
pub use schema::{
    CapabilityLevel, CommercialUse, EvidenceSource, License, ModelBuild, SpeedCategory,
    TaskCategory, VerificationStatus,
};
use std::path::Path;

/// Hard ceiling on catalog file size before we even attempt to parse it.
/// Generous for "hundreds or thousands" of entries (a few KB each) while
/// still rejecting a hostile or corrupted multi-GB file outright.
const MAX_CATALOG_FILE_BYTES: u64 = 16 * 1024 * 1024; // 16 MiB

/// Hard ceiling on the number of entries a single catalog file may declare.
const MAX_CATALOG_ENTRIES: usize = 50_000;

#[derive(Debug, Clone, Default, serde::Deserialize)]
struct CatalogFile {
    #[serde(default)]
    _notice: Option<String>,
    #[serde(default)]
    schema_version: Option<String>,
    #[serde(default)]
    builds: Vec<ModelBuild>,
}

#[derive(Debug, Clone)]
pub struct Catalog {
    pub notice: Option<String>,
    pub schema_version: Option<String>,
    pub builds: Vec<ModelBuild>,
}

impl Catalog {
    pub fn get(&self, catalog_id: &str) -> Option<&ModelBuild> {
        self.builds.iter().find(|b| b.catalog_id == catalog_id)
    }

    pub fn require(&self, catalog_id: &str) -> Result<&ModelBuild, CatalogError> {
        self.get(catalog_id)
            .ok_or_else(|| CatalogError::NotFound(catalog_id.to_string()))
    }
}

/// Loads and validates a catalog file. Every failure mode is a distinct,
/// named `CatalogError` variant - a malformed catalog is always rejected
/// loudly, never silently skipped or partially trusted.
pub fn load_catalog(path: &Path) -> Result<Catalog, CatalogError> {
    let metadata = std::fs::metadata(path).map_err(|source| CatalogError::Read {
        path: path.to_path_buf(),
        source,
    })?;

    if metadata.len() > MAX_CATALOG_FILE_BYTES {
        return Err(CatalogError::FileTooLarge {
            path: path.to_path_buf(),
            actual_bytes: metadata.len(),
            limit_bytes: MAX_CATALOG_FILE_BYTES,
        });
    }

    let contents = std::fs::read_to_string(path).map_err(|source| CatalogError::Read {
        path: path.to_path_buf(),
        source,
    })?;

    let file: CatalogFile =
        serde_json::from_str(&contents).map_err(|source| CatalogError::Parse {
            path: path.to_path_buf(),
            source,
        })?;

    if file.builds.len() > MAX_CATALOG_ENTRIES {
        return Err(CatalogError::TooManyEntries {
            path: path.to_path_buf(),
            actual: file.builds.len(),
            limit: MAX_CATALOG_ENTRIES,
        });
    }

    let mut seen_ids = std::collections::HashSet::new();
    for build in &file.builds {
        validate_entry(build)?;
        if !seen_ids.insert(build.catalog_id.clone()) {
            return Err(CatalogError::DuplicateId(build.catalog_id.clone()));
        }
    }

    Ok(Catalog {
        notice: file._notice,
        schema_version: file.schema_version,
        builds: file.builds,
    })
}

fn invalid(build: &ModelBuild, reason: impl Into<String>) -> CatalogError {
    CatalogError::InvalidEntry {
        catalog_id: build.catalog_id.clone(),
        reason: reason.into(),
    }
}

/// Structural and semantic validation. Every check here defends against a
/// concrete failure mode - not aesthetic linting.
fn validate_entry(build: &ModelBuild) -> Result<(), CatalogError> {
    if build.catalog_id.trim().is_empty() {
        return Err(invalid(build, "catalog_id must not be empty"));
    }
    if build.catalog_id.len() > 256 {
        return Err(invalid(build, "catalog_id exceeds 256 characters"));
    }
    if build.filename.trim().is_empty() {
        return Err(invalid(build, "filename must not be empty"));
    }
    // The filename is metadata describing what to expect after a manual
    // download, never a path we open ourselves - reject anything that
    // looks like a path (separators, traversal, drive letters) so it can
    // never later be misused as one.
    if build.filename.contains(['/', '\\']) || build.filename.contains("..") {
        return Err(invalid(
            build,
            "filename must be a bare file name, not a path",
        ));
    }
    if !build.filename.to_lowercase().ends_with(".gguf") {
        return Err(invalid(build, "filename must end in .gguf"));
    }
    if !(build.official_source_url.starts_with("https://")
        || build.official_source_url.starts_with("http://"))
    {
        return Err(invalid(
            build,
            "official_source_url must be an http(s) URL - data only, never executed",
        ));
    }
    if build.parameter_count == 0 {
        return Err(invalid(build, "parameter_count must be nonzero"));
    }
    if build.file_size_bytes == 0 {
        return Err(invalid(build, "file_size_bytes must be nonzero"));
    }
    if build.min_recommended_ram_bytes == 0 {
        return Err(invalid(build, "min_recommended_ram_bytes must be nonzero"));
    }
    if build.context_sizes.is_empty() {
        return Err(invalid(build, "context_sizes must list at least one value"));
    }
    if build.context_sizes.contains(&0) {
        return Err(invalid(build, "context_sizes must not contain 0"));
    }
    if build.task_categories.is_empty() {
        return Err(invalid(
            build,
            "task_categories must list at least one category",
        ));
    }
    if build.supported_backends.is_empty() {
        return Err(invalid(
            build,
            "supported_backends must list at least one backend",
        ));
    }
    if build.short_description.len() > 500 {
        return Err(invalid(build, "short_description exceeds 500 characters"));
    }
    if build.strength.len() > 300 || build.limitation.len() > 300 {
        return Err(invalid(
            build,
            "strength/limitation statements must stay under 300 characters (no marketing copy)",
        ));
    }

    // Stage B.1 consistency checks: a verification status may never claim
    // more evidence than the fields it depends on actually back up. These
    // guard the exact promise the mission makes - "never treat a landing
    // page as a direct artifact URL" and "never show Download unless the
    // exact artifact URL is verified" - at the data layer, not just the UI.
    if matches!(
        build.artifact_verification,
        schema::VerificationStatus::ArtifactUrlVerified
            | schema::VerificationStatus::ChecksumVerified
            | schema::VerificationStatus::Downloaded
            | schema::VerificationStatus::IntegrityVerified
            | schema::VerificationStatus::RuntimeCompatible
            | schema::VerificationStatus::Benchmarked
            | schema::VerificationStatus::DeviceVerified
    ) && build.exact_artifact_url.is_none()
    {
        return Err(invalid(
            build,
            "artifact_verification claims a verified artifact but exact_artifact_url is unset",
        ));
    }
    if let Some(url) = &build.exact_artifact_url
        && !(url.starts_with("https://") || url.starts_with("http://"))
    {
        return Err(invalid(
            build,
            "exact_artifact_url must be an http(s) URL when present",
        ));
    }
    if matches!(
        build.artifact_verification,
        schema::VerificationStatus::ChecksumVerified
            | schema::VerificationStatus::IntegrityVerified
    ) && (build.checksum_algorithm.is_none() || build.checksum_value.is_none())
    {
        return Err(invalid(
            build,
            "checksum-related artifact_verification requires both checksum_algorithm and checksum_value",
        ));
    }
    if build.checksum_value.is_some() && build.checksum_algorithm.is_none() {
        return Err(invalid(
            build,
            "checksum_value is set but checksum_algorithm is missing",
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tmp_path(name: &str) -> std::path::PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "brute-catalog-test-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    fn write_json(path: &Path, json: &str) {
        std::fs::File::create(path)
            .unwrap()
            .write_all(json.as_bytes())
            .unwrap();
    }

    fn minimal_valid_build_json(catalog_id: &str) -> String {
        format!(
            r#"{{
                "catalog_id": "{catalog_id}",
                "family": "test-family",
                "display_name": "Test Model",
                "publisher": "Test Publisher",
                "official_source_url": "https://huggingface.co/test/test-model",
                "official_repository_id": "test/test-model",
                "filename": "test-model.Q4_K_M.gguf",
                "architecture": "llama",
                "parameter_count": 500000000,
                "quantization": "Q4_K_M",
                "file_size_bytes": 400000000,
                "estimated_disk_bytes": 400000000,
                "estimated_runtime_memory_bytes": null,
                "min_recommended_ram_bytes": 2000000000,
                "min_recommended_vram_bytes": null,
                "supported_backends": ["cpu"],
                "context_sizes": [2048, 4096],
                "task_categories": ["general_chat"],
                "short_description": "A small test model.",
                "strength": "Fast on modest hardware.",
                "limitation": "Limited reasoning depth.",
                "license": {{"status": "known", "identifier": "apache-2.0"}},
                "commercial_use": "allowed",
                "gated_access": false,
                "metadata_provenance": "test fixture",
                "last_reviewed": "2026-07-18"
            }}"#
        )
    }

    #[test]
    fn loads_a_minimal_valid_catalog() {
        let path = tmp_path("valid.json");
        let json = format!(
            r#"{{"_notice": "test", "schema_version": "stage1-v1", "builds": [{}]}}"#,
            minimal_valid_build_json("test-1")
        );
        write_json(&path, &json);

        let catalog = load_catalog(&path).expect("should load");
        assert_eq!(catalog.builds.len(), 1);
        assert_eq!(catalog.get("test-1").unwrap().family, "test-family");

        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn rejects_duplicate_catalog_ids() {
        let path = tmp_path("dup.json");
        let entry = minimal_valid_build_json("dup-id");
        let json = format!(r#"{{"builds": [{entry}, {entry}]}}"#);
        write_json(&path, &json);

        let err = load_catalog(&path).unwrap_err();
        assert!(matches!(err, CatalogError::DuplicateId(id) if id == "dup-id"));

        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn rejects_path_traversal_in_filename() {
        let path = tmp_path("traversal.json");
        let mut entry: serde_json::Value =
            serde_json::from_str(&minimal_valid_build_json("evil")).unwrap();
        entry["filename"] = serde_json::json!("../../etc/passwd.gguf");
        let json = format!(r#"{{"builds": [{entry}]}}"#);
        write_json(&path, &json);

        let err = load_catalog(&path).unwrap_err();
        assert!(matches!(err, CatalogError::InvalidEntry { .. }));

        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn rejects_non_http_source_url() {
        let path = tmp_path("badurl.json");
        let mut entry: serde_json::Value =
            serde_json::from_str(&minimal_valid_build_json("evil-url")).unwrap();
        entry["official_source_url"] = serde_json::json!("file:///etc/passwd");
        let json = format!(r#"{{"builds": [{entry}]}}"#);
        write_json(&path, &json);

        let err = load_catalog(&path).unwrap_err();
        assert!(matches!(err, CatalogError::InvalidEntry { .. }));

        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn rejects_oversized_catalog_file() {
        let path = tmp_path("huge.json");
        // Don't actually allocate 16MiB+ of real content for a fast test -
        // write a file just over the limit with padding inside a string.
        let padding = "x".repeat((MAX_CATALOG_FILE_BYTES as usize) + 1024);
        let json = format!(r#"{{"_notice": "{padding}", "builds": []}}"#);
        write_json(&path, &json);

        let err = load_catalog(&path).unwrap_err();
        assert!(matches!(err, CatalogError::FileTooLarge { .. }));

        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn rejects_malformed_json() {
        let path = tmp_path("malformed.json");
        write_json(&path, "{ this is not valid json");

        let err = load_catalog(&path).unwrap_err();
        assert!(matches!(err, CatalogError::Parse { .. }));

        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn missing_license_stays_unknown_not_defaulted() {
        let path = tmp_path("unknown_license.json");
        let mut entry: serde_json::Value =
            serde_json::from_str(&minimal_valid_build_json("unknown-lic")).unwrap();
        entry["license"] = serde_json::json!({"status": "unknown"});
        entry["commercial_use"] = serde_json::json!("unknown");
        let json = format!(r#"{{"builds": [{entry}]}}"#);
        write_json(&path, &json);

        let catalog = load_catalog(&path).expect("should load");
        let build = catalog.get("unknown-lic").unwrap();
        assert_eq!(build.license, License::Unknown);
        assert_eq!(build.commercial_use, CommercialUse::Unknown);

        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn dev_fixture_catalog_loads_and_validates() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("data/catalog/dev-catalog.json");
        let catalog = load_catalog(&path).expect("dev-catalog.json must be valid");

        assert!(catalog.builds.len() >= 7);
        assert!(
            catalog.notice.is_some(),
            "dev catalog must carry its curated-data notice"
        );

        // Required representative entry types from the Stage 1 brief.
        let very_small = catalog.get("qwen2.5-0.5b-instruct-q4_k_m").unwrap();
        assert!(very_small.parameter_count < 1_000_000_000);

        let coding = catalog.get("qwen2.5-coder-1.5b-instruct-q4_k_m").unwrap();
        assert!(coding.task_categories.contains(&TaskCategory::Coding));

        let same_family: Vec<_> = catalog
            .builds
            .iter()
            .filter(|b| b.family == "qwen2.5-0.5b-instruct")
            .collect();
        assert!(
            same_family.len() >= 3,
            "expected multiple quantizations of one family"
        );

        let oversized = catalog.get("llama-3.1-70b-instruct-q4_k_m").unwrap();
        assert!(oversized.min_recommended_ram_bytes > 32_000_000_000);

        // Stage B.1 multi-family entries: at least one Arabic-first family
        // with an officially-verified artifact, one Arabic-first family
        // with no first-party artifact yet (curated_metadata only, no
        // Download-eligible URL), and one non-Qwen/Llama family entirely.
        let jais = catalog.get("jais-2-8b-chat-q4_k_m").unwrap();
        assert!(jais.task_categories.contains(&TaskCategory::ArabicChat));
        assert_eq!(
            jais.artifact_verification,
            schema::VerificationStatus::ArtifactUrlVerified
        );
        assert!(jais.exact_artifact_url.is_some());

        let allam = catalog.get("allam-7b-instruct-preview-q4_k_m").unwrap();
        assert!(allam.task_categories.contains(&TaskCategory::ArabicChat));
        assert_eq!(
            allam.artifact_verification,
            schema::VerificationStatus::CuratedMetadata
        );
        assert!(
            allam.exact_artifact_url.is_none(),
            "no first-party GGUF exists for ALLaM - must never claim a Download-eligible URL"
        );

        let gemma = catalog.get("gemma-3-4b-it-qat-q4_0").unwrap();
        assert_eq!(gemma.family_id.as_deref(), Some("gemma-3"));
    }

    #[test]
    fn test_fixtures_file_contains_the_unknown_license_case() {
        // Synthetic/test-only catalog data lives in a file production code
        // never loads (see desktop/src-tauri/src/paths.rs and
        // src/cli/mod.rs::DEFAULT_CATALOG_PATH, both of which point at
        // dev-catalog.json, never this file) - the fake entry must never
        // appear in what a real user's Discover Models page shows.
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("data/catalog/test-fixtures.json");
        let catalog = load_catalog(&path).expect("test-fixtures.json must be valid");

        let unknown_license = catalog.get("dev-fixture-unknown-license").unwrap();
        assert_eq!(unknown_license.license, License::Unknown);
        assert_eq!(unknown_license.commercial_use, CommercialUse::Unknown);
    }

    #[test]
    fn production_catalog_never_contains_the_test_fixture_entry() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("data/catalog/dev-catalog.json");
        let catalog = load_catalog(&path).expect("dev-catalog.json must be valid");
        assert!(
            catalog.get("dev-fixture-unknown-license").is_none(),
            "the synthetic test fixture must never ship in the file production code actually loads"
        );
    }

    #[test]
    fn stage_b1_fields_default_to_unknown_when_absent_from_json() {
        // A catalog entry written before Stage B.1 (no new fields at all)
        // must still load, with every new field reporting the honest
        // "nobody has entered this yet" answer - never a guess.
        let path = tmp_path("legacy_entry.json");
        let json = format!(r#"{{"builds": [{}]}}"#, minimal_valid_build_json("legacy"));
        write_json(&path, &json);

        let catalog = load_catalog(&path).expect("legacy-shaped entry must still load");
        let build = catalog.get("legacy").unwrap();
        assert_eq!(build.family_id, None);
        assert_eq!(build.artifact_id, None);
        assert_eq!(build.exact_artifact_url, None);
        assert_eq!(
            build.source_verification,
            schema::VerificationStatus::Unknown
        );
        assert_eq!(
            build.artifact_verification,
            schema::VerificationStatus::Unknown
        );
        assert_eq!(build.arabic_capability, schema::CapabilityLevel::Unknown);
        assert_eq!(build.evidence_source, schema::EvidenceSource::Unknown);

        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn rejects_verified_artifact_status_with_no_exact_artifact_url() {
        // The data layer must never let a claim of "verified artifact"
        // exist without the URL it's supposedly verifying - this is the
        // schema-level backstop for "never show Download unless the exact
        // artifact URL is verified."
        let path = tmp_path("claims_verified_no_url.json");
        let mut entry: serde_json::Value =
            serde_json::from_str(&minimal_valid_build_json("claims-verified")).unwrap();
        entry["artifact_verification"] = serde_json::json!("artifact_url_verified");
        let json = format!(r#"{{"builds": [{entry}]}}"#);
        write_json(&path, &json);

        let err = load_catalog(&path).unwrap_err();
        assert!(matches!(err, CatalogError::InvalidEntry { .. }));

        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn accepts_verified_artifact_status_when_the_url_is_present() {
        let path = tmp_path("claims_verified_with_url.json");
        let mut entry: serde_json::Value =
            serde_json::from_str(&minimal_valid_build_json("claims-verified-ok")).unwrap();
        entry["artifact_verification"] = serde_json::json!("artifact_url_verified");
        entry["exact_artifact_url"] = serde_json::json!(
            "https://huggingface.co/test/test-model/resolve/main/test-model.Q4_K_M.gguf"
        );
        let json = format!(r#"{{"builds": [{entry}]}}"#);
        write_json(&path, &json);

        let catalog = load_catalog(&path).expect("should load");
        assert_eq!(
            catalog
                .get("claims-verified-ok")
                .unwrap()
                .artifact_verification,
            schema::VerificationStatus::ArtifactUrlVerified
        );

        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn rejects_checksum_value_without_an_algorithm() {
        let path = tmp_path("checksum_no_algo.json");
        let mut entry: serde_json::Value =
            serde_json::from_str(&minimal_valid_build_json("checksum-no-algo")).unwrap();
        entry["checksum_value"] = serde_json::json!("a".repeat(64));
        let json = format!(r#"{{"builds": [{entry}]}}"#);
        write_json(&path, &json);

        let err = load_catalog(&path).unwrap_err();
        assert!(matches!(err, CatalogError::InvalidEntry { .. }));

        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn rejects_non_http_exact_artifact_url() {
        let path = tmp_path("bad_exact_url.json");
        let mut entry: serde_json::Value =
            serde_json::from_str(&minimal_valid_build_json("bad-exact-url")).unwrap();
        entry["exact_artifact_url"] = serde_json::json!("file:///etc/passwd");
        let json = format!(r#"{{"builds": [{entry}]}}"#);
        write_json(&path, &json);

        let err = load_catalog(&path).unwrap_err();
        assert!(matches!(err, CatalogError::InvalidEntry { .. }));

        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn rejects_zero_parameter_count() {
        let path = tmp_path("zero_params.json");
        let mut entry: serde_json::Value =
            serde_json::from_str(&minimal_valid_build_json("zero-params")).unwrap();
        entry["parameter_count"] = serde_json::json!(0);
        let json = format!(r#"{{"builds": [{entry}]}}"#);
        write_json(&path, &json);

        let err = load_catalog(&path).unwrap_err();
        assert!(matches!(err, CatalogError::InvalidEntry { .. }));

        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }
}
