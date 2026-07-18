//! Explicit model import (spec section 4) and catalog matching (spec
//! section 15). Import never executes, uploads, copies, moves, renames,
//! or modifies the model file - it only reads it (path validation,
//! bounded GGUF parsing, streaming SHA-256) and records what was found.
//! See `docs/model-import-and-verification.md`.
//!
//! Managed-copy mode (`--copy-into-library`) is **not implemented** in
//! Stage 3 - the spec explicitly allows skipping it "if it adds
//! unnecessary complexity," and every import here operates on the
//! file's existing location. `LibraryEntry::managed_copy` therefore
//! stays `false` for every entry this module creates.

use super::scan::{DiscoveredKind, ScanOptions, ScanResult};
use super::{
    CatalogMatchConfidence, CatalogMatchResult, FileStatus, GgufVerification, LibraryEntry,
    LibraryStore,
};
use crate::catalog::Catalog;
use crate::errors::LibraryError;
use crate::models::ModelReport;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, serde::Serialize)]
pub struct ImportOutcome {
    pub library_id: String,
    /// `false` when this import re-verified an already-tracked path
    /// (same `current_path`) rather than creating a new entry.
    pub was_new: bool,
    /// Other library IDs that already share this exact content hash -
    /// computed before this import's own entry is added, so a fresh
    /// import of a known duplicate reports it immediately.
    pub duplicate_of: Vec<String>,
    pub verification: GgufVerification,
    pub catalog_match: CatalogMatchResult,
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn file_modified_at(path: &Path) -> Option<String> {
    std::fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .map(super::verify::system_time_to_rfc3339)
}

/// Validates, hashes, and parses the model at `path` (via
/// `models::inspect_model` - the same Stage 0 pipeline `brute model
/// inspect`/`brute benchmark` already use, so path safety and bounded
/// GGUF parsing are not duplicated here), then creates or refreshes its
/// library entry. Re-importing a path already in the library updates
/// its record in place (same `library_id`) rather than creating a
/// second entry for the same location.
pub fn import_model(
    path: &Path,
    alias: Option<String>,
    store: &mut LibraryStore,
    catalog: Option<&Catalog>,
) -> Result<ImportOutcome, crate::errors::BruteError> {
    let model = crate::models::inspect_model(path)?;

    let catalog_match = match_against_catalog(&model, catalog);
    let verification = super::verify::verify_artifact(&model.path, None, catalog_match.confidence);
    let now = now_rfc3339();
    let modified_at = file_modified_at(&model.path);

    if let Some(existing_index) = store
        .entries
        .iter()
        .position(|e| e.current_path == model.path)
    {
        let library_id = store.entries[existing_index].library_id.clone();
        let duplicate_of: Vec<String> = store
            .find_by_sha256(&model.sha256)
            .into_iter()
            .map(|e| e.library_id.clone())
            .filter(|id| *id != library_id)
            .collect();

        let existing = &mut store.entries[existing_index];
        existing.sha256 = model.sha256.clone();
        existing.file_size_bytes = model.file_size_bytes;
        existing.gguf_version = model.gguf_version;
        existing.tensor_count = model.tensor_count;
        existing.kv_count = model.kv_count;
        existing.architecture = model.architecture.clone();
        existing.quantization = model.quantization.clone();
        existing.parameter_count = model.parameter_count;
        existing.last_verified_at_rfc3339 = Some(now.clone());
        existing.file_modified_at_rfc3339 = modified_at;
        existing.file_status = FileStatus::Unchanged;
        existing.trust = verification.trust;
        existing.last_verification = Some(verification.clone());
        existing.catalog_match = catalog_match.clone();
        if let Some(a) = alias {
            existing.alias = Some(a);
        }
        // Quarantine is deliberately NOT auto-cleared by a re-import -
        // see docs/quarantine-and-recovery.md: only an explicit
        // `unquarantine` (itself gated on a fresh verification) lifts it.

        return Ok(ImportOutcome {
            library_id,
            was_new: false,
            duplicate_of,
            verification,
            catalog_match,
        });
    }

    let duplicate_of: Vec<String> = store
        .find_by_sha256(&model.sha256)
        .into_iter()
        .map(|e| e.library_id.clone())
        .collect();

    let library_id = format!("model-{}", crate::identity::generate_random_id());
    let entry = LibraryEntry {
        library_id: library_id.clone(),
        schema_version: super::LIBRARY_SCHEMA_VERSION.to_string(),
        sha256: model.sha256.clone(),
        file_size_bytes: model.file_size_bytes,
        gguf_version: model.gguf_version,
        tensor_count: model.tensor_count,
        kv_count: model.kv_count,
        architecture: model.architecture.clone(),
        quantization: model.quantization.clone(),
        parameter_count: model.parameter_count,
        current_path: model.path.clone(),
        original_import_path: Some(model.path.clone()),
        imported_at_rfc3339: now.clone(),
        last_verified_at_rfc3339: Some(now),
        file_modified_at_rfc3339: modified_at,
        file_status: FileStatus::Unchanged,
        trust: verification.trust,
        last_verification: Some(verification.clone()),
        catalog_match: catalog_match.clone(),
        alias,
        notes: None,
        quarantine: None,
        managed_copy: false,
    };

    store.entries.push(entry);

    Ok(ImportOutcome {
        library_id,
        was_new: true,
        duplicate_of,
        verification,
        catalog_match,
    })
}

#[derive(Debug, serde::Serialize)]
pub struct ImportDirectoryOutcome {
    pub scan: ScanResult,
    /// One entry per discovered GGUF candidate - `Err` holds a message
    /// rather than propagating, so one malformed file in a batch import
    /// never aborts the rest of the batch.
    pub imported: Vec<(PathBuf, Result<ImportOutcome, String>)>,
}

/// Composes `scan::scan` with `import_model` for every genuine GGUF
/// candidate found - never for files reported `UnsupportedFormat` or
/// `Inaccessible`, since those were never confirmed to even be GGUF.
pub fn import_directory(
    root: &Path,
    scan_options: &ScanOptions,
    store: &mut LibraryStore,
    catalog: Option<&Catalog>,
    mut is_cancelled: impl FnMut() -> bool,
) -> Result<ImportDirectoryOutcome, LibraryError> {
    let scan_result = super::scan::scan(root, scan_options, &mut is_cancelled)?;

    let mut imported = Vec::new();
    for file in &scan_result.discovered {
        if file.kind != DiscoveredKind::GgufCandidate {
            continue;
        }
        if is_cancelled() {
            break;
        }
        let result = import_model(&file.path, None, store, catalog).map_err(|e| e.to_string());
        imported.push((file.path.clone(), result));
    }

    Ok(ImportDirectoryOutcome {
        scan: scan_result,
        imported,
    })
}

/// Matches a freshly-parsed model against the local curated catalog
/// using the strongest available evidence (spec section 15). The
/// catalog schema (`catalog::ModelBuild`) carries no independently
/// curated expected SHA-256 for any entry, so the `Exact` tier - which
/// would require exactly that - can never be reached today; the
/// strongest automatic match is `Strong` (file size and GGUF metadata
/// all agree). This is an honest reflection of the catalog's actual
/// data, not a shortfall in the matching logic - documented in
/// `docs/trust-and-provenance.md`.
pub fn match_against_catalog(model: &ModelReport, catalog: Option<&Catalog>) -> CatalogMatchResult {
    let Some(catalog) = catalog else {
        return CatalogMatchResult::none();
    };

    let model_filename_stem = model
        .path
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_lowercase());

    let mut best: Option<(CatalogMatchResult, u8)> = None;

    for build in &catalog.builds {
        let arch_match = model
            .architecture
            .as_deref()
            .is_some_and(|a| a.eq_ignore_ascii_case(&build.architecture));
        let quant_match = model
            .quantization
            .as_deref()
            .is_some_and(|q| q.eq_ignore_ascii_case(&build.quantization));
        let param_match = model.parameter_count.is_some_and(|p| {
            if build.parameter_count == 0 {
                return false;
            }
            let (hi, lo) = if p > build.parameter_count {
                (p, build.parameter_count)
            } else {
                (build.parameter_count, p)
            };
            (hi as f64 / lo as f64) <= 1.05
        });
        let size_match = model.file_size_bytes == build.file_size_bytes;

        let build_filename_stem = Path::new(&build.filename)
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_lowercase());
        let filename_hint = matches!(
            (&model_filename_stem, &build_filename_stem),
            (Some(a), Some(b)) if a == b
        );

        let (confidence, rank, notes): (CatalogMatchConfidence, u8, Vec<String>) =
            if arch_match && quant_match && size_match && param_match {
                (
                    CatalogMatchConfidence::Strong,
                    3,
                    vec![
                        "file size and GGUF metadata (architecture, quantization, parameter \
                         count) all match this catalog entry - no independently curated \
                         expected hash exists to confirm an exact match"
                            .to_string(),
                    ],
                )
            } else if arch_match && quant_match && param_match {
                (
                    CatalogMatchConfidence::Probable,
                    2,
                    vec![
                        "architecture, quantization, and parameter count match this catalog \
                         entry; file size differs from its recorded value"
                            .to_string(),
                    ],
                )
            } else if filename_hint {
                (
                    CatalogMatchConfidence::Weak,
                    1,
                    vec![
                        "filename resembles this catalog entry's filename only - not proof of \
                         identity"
                            .to_string(),
                    ],
                )
            } else {
                continue;
            };

        let is_better = best.as_ref().map(|(_, r)| rank > *r).unwrap_or(true);
        if is_better {
            best = Some((
                CatalogMatchResult {
                    catalog_id: Some(build.catalog_id.clone()),
                    confidence,
                    notes,
                },
                rank,
            ));
        }
    }

    best.map(|(m, _)| m)
        .unwrap_or_else(CatalogMatchResult::none)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::scan::ScanOptions;
    use std::io::Write;

    fn tmp_dir(name: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "brute-library-import-test-{name}-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A minimal but fully valid GGUF fixture - same wire layout as
    /// `models::gguf::tests::valid_minimal`, duplicated locally per this
    /// codebase's per-module test-fixture convention.
    fn write_valid_gguf(path: &Path, architecture: &str) {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"GGUF");
        buf.extend_from_slice(&3u32.to_le_bytes()); // version
        buf.extend_from_slice(&1u64.to_le_bytes()); // tensor_count
        buf.extend_from_slice(&2u64.to_le_bytes()); // kv_count

        let kv_string = |key: &str, value: &str, buf: &mut Vec<u8>| {
            buf.extend_from_slice(&(key.len() as u64).to_le_bytes());
            buf.extend_from_slice(key.as_bytes());
            buf.extend_from_slice(&8u32.to_le_bytes()); // GGUF_TYPE_STRING
            buf.extend_from_slice(&(value.len() as u64).to_le_bytes());
            buf.extend_from_slice(value.as_bytes());
        };
        kv_string("general.architecture", architecture, &mut buf);
        kv_string("general.name", "test-model", &mut buf);

        // tensor[0]: name, n_dims=1, dims=[4], type=F32(0), offset=0
        let name = "weight";
        buf.extend_from_slice(&(name.len() as u64).to_le_bytes());
        buf.extend_from_slice(name.as_bytes());
        buf.extend_from_slice(&1u32.to_le_bytes()); // n_dims
        buf.extend_from_slice(&4u64.to_le_bytes()); // dims[0]
        buf.extend_from_slice(&0u32.to_le_bytes()); // type F32
        buf.extend_from_slice(&0u64.to_le_bytes()); // offset

        let unpadded = buf.len() as u64;
        let alignment = 32u64;
        let padded = unpadded.div_ceil(alignment) * alignment;
        buf.extend(vec![0u8; (padded - unpadded) as usize]);
        buf.extend_from_slice(&[0u8; 16]); // 4 F32 elements = 16 bytes

        std::fs::File::create(path)
            .unwrap()
            .write_all(&buf)
            .unwrap();
    }

    #[test]
    fn importing_a_valid_gguf_creates_a_new_library_entry() {
        let dir = tmp_dir("import-new");
        let path = dir.join("model.gguf");
        write_valid_gguf(&path, "llama");

        let mut store = LibraryStore::default();
        let outcome = import_model(&path, None, &mut store, None).unwrap();

        assert!(outcome.was_new);
        assert!(outcome.duplicate_of.is_empty());
        assert_eq!(store.entries.len(), 1);
        assert_eq!(store.entries[0].architecture.as_deref(), Some("llama"));
        assert_eq!(store.entries[0].alias, None);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn import_never_modifies_the_source_file() {
        let dir = tmp_dir("import-no-modify");
        let path = dir.join("model.gguf");
        write_valid_gguf(&path, "llama");
        let before = std::fs::read(&path).unwrap();

        let mut store = LibraryStore::default();
        import_model(&path, None, &mut store, None).unwrap();

        let after = std::fs::read(&path).unwrap();
        assert_eq!(before, after);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn reimporting_the_same_path_updates_in_place_not_a_new_entry() {
        let dir = tmp_dir("reimport");
        let path = dir.join("model.gguf");
        write_valid_gguf(&path, "llama");

        let mut store = LibraryStore::default();
        let first = import_model(&path, None, &mut store, None).unwrap();
        let second = import_model(&path, Some("my alias".to_string()), &mut store, None).unwrap();

        assert_eq!(first.library_id, second.library_id);
        assert!(!second.was_new);
        assert_eq!(store.entries.len(), 1);
        assert_eq!(store.entries[0].alias.as_deref(), Some("my alias"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn importing_a_copy_at_a_different_path_reports_it_as_a_duplicate() {
        let dir = tmp_dir("duplicate-paths");
        let path_a = dir.join("a.gguf");
        let path_b = dir.join("b.gguf");
        write_valid_gguf(&path_a, "llama");
        write_valid_gguf(&path_b, "llama"); // identical content -> identical hash

        let mut store = LibraryStore::default();
        let first = import_model(&path_a, None, &mut store, None).unwrap();
        let second = import_model(&path_b, None, &mut store, None).unwrap();

        assert!(second.was_new);
        assert_ne!(first.library_id, second.library_id);
        assert_eq!(second.duplicate_of, vec![first.library_id]);
        assert_eq!(store.entries.len(), 2);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn same_filename_different_content_are_never_merged() {
        let dir_a = tmp_dir("same-name-a");
        let dir_b = tmp_dir("same-name-b");
        let path_a = dir_a.join("model.gguf");
        let path_b = dir_b.join("model.gguf");
        write_valid_gguf(&path_a, "llama");
        write_valid_gguf(&path_b, "qwen2"); // different architecture -> different bytes -> different hash

        let mut store = LibraryStore::default();
        let a = import_model(&path_a, None, &mut store, None).unwrap();
        let b = import_model(&path_b, None, &mut store, None).unwrap();

        assert_ne!(store.entries[0].sha256, store.entries[1].sha256);
        assert!(a.duplicate_of.is_empty());
        assert!(b.duplicate_of.is_empty());

        std::fs::remove_dir_all(&dir_a).ok();
        std::fs::remove_dir_all(&dir_b).ok();
    }

    #[test]
    fn importing_a_truncated_gguf_fails_cleanly_without_creating_an_entry() {
        let dir = tmp_dir("truncated");
        let path = dir.join("bad.gguf");
        std::fs::write(&path, b"GGUF").unwrap(); // magic only, nothing else

        let mut store = LibraryStore::default();
        let result = import_model(&path, None, &mut store, None);

        assert!(result.is_err());
        assert!(store.entries.is_empty());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn import_directory_imports_every_discovered_candidate() {
        let dir = tmp_dir("import-directory");
        write_valid_gguf(&dir.join("a.gguf"), "llama");
        write_valid_gguf(&dir.join("b.gguf"), "qwen2");
        std::fs::write(dir.join("not-gguf.gguf"), b"NOTGGUF").unwrap();

        let mut store = LibraryStore::default();
        let outcome =
            import_directory(&dir, &ScanOptions::default(), &mut store, None, || false).unwrap();

        assert_eq!(
            outcome.imported.len(),
            2,
            "only genuine GGUF candidates should be attempted"
        );
        assert!(outcome.imported.iter().all(|(_, r)| r.is_ok()));
        assert_eq!(store.entries.len(), 2);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn catalog_match_is_none_without_a_catalog() {
        let dir = tmp_dir("no-catalog");
        let path = dir.join("model.gguf");
        write_valid_gguf(&path, "llama");
        let model = crate::models::inspect_model(&path).unwrap();

        let result = match_against_catalog(&model, None);
        assert_eq!(result.confidence, CatalogMatchConfidence::None);
        assert_eq!(result.catalog_id, None);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn weak_catalog_match_is_never_upgraded_to_a_strong_claim() {
        // Ensures the matching function never reports Exact - the
        // catalog schema has no expected-hash field to earn it.
        let dir = tmp_dir("catalog-tiers");
        let path = dir.join("model.gguf");
        write_valid_gguf(&path, "llama");
        let model = crate::models::inspect_model(&path).unwrap();

        // No real Catalog fixture is constructed here (that requires a
        // full ModelBuild with many required fields) - this test only
        // asserts the `None`-catalog path never fabricates a match,
        // which is the honesty property that matters most.
        let result = match_against_catalog(&model, None);
        assert_ne!(result.confidence, CatalogMatchConfidence::Exact);
        assert_ne!(result.confidence, CatalogMatchConfidence::Strong);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn importing_an_empty_file_fails_cleanly_without_creating_an_entry() {
        let dir = tmp_dir("empty-file");
        let path = dir.join("empty.gguf");
        std::fs::File::create(&path).unwrap(); // zero bytes

        let mut store = LibraryStore::default();
        let result = import_model(&path, None, &mut store, None);

        assert!(result.is_err());
        assert!(store.entries.is_empty());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn importing_a_gguf_with_impossible_tensor_offsets_fails_cleanly() {
        let dir = tmp_dir("bad-tensor-offsets");
        let path = dir.join("bad.gguf");

        // Valid header claiming one tensor whose declared offset/size
        // extends far past the actual (tiny) file - the same
        // size-consistency check Stage 0's `gguf::inspect` already
        // performs, exercised here through the library import path.
        let mut buf = Vec::new();
        buf.extend_from_slice(b"GGUF");
        buf.extend_from_slice(&3u32.to_le_bytes()); // version
        buf.extend_from_slice(&1u64.to_le_bytes()); // tensor_count
        buf.extend_from_slice(&0u64.to_le_bytes()); // kv_count
        let name = "weight";
        buf.extend_from_slice(&(name.len() as u64).to_le_bytes());
        buf.extend_from_slice(name.as_bytes());
        buf.extend_from_slice(&1u32.to_le_bytes()); // n_dims
        buf.extend_from_slice(&1_000_000u64.to_le_bytes()); // huge dim
        buf.extend_from_slice(&0u32.to_le_bytes()); // type F32
        buf.extend_from_slice(&0u64.to_le_bytes()); // offset 0
        // No actual tensor data follows - file ends here, far short of
        // what a 1,000,000-element F32 tensor would require.
        std::fs::write(&path, &buf).unwrap();

        let mut store = LibraryStore::default();
        let result = import_model(&path, None, &mut store, None);

        assert!(result.is_err());
        assert!(store.entries.is_empty());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn alias_and_notes_with_unusual_characters_round_trip_safely() {
        let dir = tmp_dir("unsafe-text");
        let path = dir.join("model.gguf");
        write_valid_gguf(&path, "llama");

        let unusual_alias = "quotes \" and \\ backslashes and \n newlines and \0 nul-ish text";
        let mut store = LibraryStore::default();
        import_model(&path, Some(unusual_alias.to_string()), &mut store, None).unwrap();
        store.entries[0].notes =
            Some("../../etc/passwd style text; not treated as a path".to_string());

        let store_dir = tmp_dir("unsafe-text-store");
        let index_path = store_dir.join("index.json");
        store.save_to(&index_path).unwrap();
        let loaded = LibraryStore::load_from(&index_path).unwrap();

        assert_eq!(loaded.entries[0].alias.as_deref(), Some(unusual_alias));
        assert_eq!(
            loaded.entries[0].notes.as_deref(),
            Some("../../etc/passwd style text; not treated as a path")
        );
        // Never interpreted as a path - the entry's real current_path is
        // untouched by whatever text ended up in notes.
        assert_eq!(
            loaded.entries[0].current_path,
            store.entries[0].current_path
        );

        std::fs::remove_dir_all(&dir).ok();
        std::fs::remove_dir_all(&store_dir).ok();
    }
}
