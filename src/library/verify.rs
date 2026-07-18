//! GGUF verification pipeline (spec section 5) and file-change detection
//! (spec section 8). See `docs/model-import-and-verification.md` and
//! `docs/model-file-change-detection.md`.
//!
//! Two distinct, deliberately separate checks live here:
//! - [`verify_artifact`]: a full structural + content-hash pass against
//!   the file currently on disk - the expensive, authoritative check.
//! - [`quick_file_status`]: a cheap size/mtime comparison against what
//!   was last recorded, used by `library refresh` to decide whether a
//!   full re-verify is even warranted, without reading the file's
//!   contents at all.
//!
//! [`verify_entry`] and [`refresh_entry`] apply those two checks to a
//! stored [`LibraryEntry`] in place; [`locate`] is the move/rename
//! recovery workflow (spec section 9) - it only ever rebinds a stored
//! entry's path when the candidate file's hash genuinely matches the
//! original artifact, and rejects (never overwrites) a mismatch.

use super::{
    CatalogMatchConfidence, CheckState, FileStatus, GgufVerification, LibraryEntry, LibraryStore,
    OverallIntegrity, TrustStatus,
};
use crate::errors::{BruteError, GgufError, LibraryError};
use crate::models::gguf;
use crate::security::hashing::sha256_file;
use std::path::Path;

/// Runs the full verification pipeline against `path`. `expected_sha256`
/// is the hash previously recorded for this artifact (`None` for a
/// brand-new import, where there's nothing yet to compare against -
/// every fresh import's hash is trusted as the initial baseline, not
/// "verified against" anything). `catalog_match` is computed separately
/// (see `import::match_against_catalog`) and threaded through here only
/// to be recorded on the result.
pub fn verify_artifact(
    path: &Path,
    expected_sha256: Option<&str>,
    catalog_match: CatalogMatchConfidence,
) -> GgufVerification {
    if !path.is_file() {
        return GgufVerification {
            header: CheckState::Unavailable,
            structure: CheckState::Unavailable,
            sha256: CheckState::Unavailable,
            runtime_load: CheckState::NotTested,
            benchmark: CheckState::NotTested,
            catalog_match,
            overall_integrity: OverallIntegrity::Unknown,
            trust: TrustStatus::Missing,
        };
    }

    match gguf::inspect(path) {
        Err(GgufError::BadMagic { .. }) => GgufVerification {
            header: CheckState::Failed,
            structure: CheckState::Failed,
            sha256: CheckState::NotTested,
            runtime_load: CheckState::NotTested,
            benchmark: CheckState::NotTested,
            catalog_match,
            overall_integrity: OverallIntegrity::Corrupt,
            trust: TrustStatus::Corrupt,
        },
        // Magic and version are read before anything that could produce
        // a different error variant, so header is genuinely verified
        // here even though we can't parse further.
        Err(GgufError::UnsupportedVersion(_)) => GgufVerification {
            header: CheckState::Verified,
            structure: CheckState::Unavailable,
            sha256: CheckState::NotTested,
            runtime_load: CheckState::NotTested,
            benchmark: CheckState::NotTested,
            catalog_match,
            overall_integrity: OverallIntegrity::Unknown,
            trust: TrustStatus::Unsupported,
        },
        Err(_other) => GgufVerification {
            header: CheckState::Verified,
            structure: CheckState::Failed,
            sha256: CheckState::NotTested,
            runtime_load: CheckState::NotTested,
            benchmark: CheckState::NotTested,
            catalog_match,
            overall_integrity: OverallIntegrity::Corrupt,
            trust: TrustStatus::Corrupt,
        },
        Ok(_summary) => {
            let sha256_state = match sha256_file(path) {
                Ok(actual) => match expected_sha256 {
                    Some(expected) if expected != actual => CheckState::Failed,
                    _ => CheckState::Verified,
                },
                Err(_) => CheckState::Unavailable,
            };

            let (overall_integrity, trust) = match sha256_state {
                CheckState::Failed => (
                    OverallIntegrity::Suspect,
                    TrustStatus::ModifiedSinceVerification,
                ),
                CheckState::Unavailable => (OverallIntegrity::Unknown, TrustStatus::Unknown),
                _ => {
                    let trust = match catalog_match {
                        CatalogMatchConfidence::Exact | CatalogMatchConfidence::Strong => {
                            TrustStatus::CatalogMetadataMatched
                        }
                        _ => TrustStatus::LocalUnverifiedSource,
                    };
                    (OverallIntegrity::Verified, trust)
                }
            };

            GgufVerification {
                header: CheckState::Verified,
                structure: CheckState::Verified,
                sha256: sha256_state,
                runtime_load: CheckState::NotTested,
                benchmark: CheckState::NotTested,
                catalog_match,
                overall_integrity,
                trust,
            }
        }
    }
}

/// Cheap size/mtime-only comparison against `entry`'s last-recorded
/// values - never reads file contents, so this is safe to run on every
/// `library list`/`audit` invocation without the cost of a full hash.
/// A `Modified` result here is a *heuristic*, not a confirmed content
/// change - only [`verify_artifact`]'s hash recompute confirms that.
pub fn quick_file_status(entry: &LibraryEntry) -> FileStatus {
    let metadata = match std::fs::metadata(&entry.current_path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return FileStatus::Missing,
        Err(_) => return FileStatus::Inaccessible,
    };

    if !metadata.is_file() {
        return FileStatus::Inaccessible;
    }

    let size_changed = metadata.len() != entry.file_size_bytes;
    let mtime_changed = match (metadata.modified().ok(), &entry.file_modified_at_rfc3339) {
        (Some(actual), Some(recorded)) => &system_time_to_rfc3339(actual) != recorded,
        _ => false,
    };

    if size_changed || mtime_changed {
        FileStatus::Modified
    } else {
        FileStatus::Unchanged
    }
}

/// Cheap in-place refresh (`brute library refresh`): updates only
/// `entry.file_status` from a quick size/mtime check. Never recomputes
/// the hash and never touches `trust` - a `Modified` result here is a
/// heuristic prompting a full [`verify_entry`] pass, not a confirmed
/// content change on its own.
pub fn refresh_entry(entry: &mut LibraryEntry) -> FileStatus {
    let status = quick_file_status(entry);
    entry.file_status = status;
    status
}

/// Full verification pass (`brute library verify`) against a stored
/// entry: recomputes the hash, compares it to what's on record, and
/// updates `file_status`/`trust`/`last_verification`/
/// `last_verified_at_rfc3339` in place. A hash mismatch here is what
/// actually confirms [`FileStatus::Replaced`] - as opposed to
/// [`refresh_entry`]'s merely-suspicious [`FileStatus::Modified`]. When
/// content is confirmed unchanged, the recorded size/mtime baseline is
/// refreshed too (e.g. a `touch` with no real content change should not
/// keep tripping the cheap heuristic forever).
pub fn verify_entry(
    entry: &mut LibraryEntry,
    catalog_match_confidence: CatalogMatchConfidence,
) -> GgufVerification {
    let result = verify_artifact(
        &entry.current_path,
        Some(&entry.sha256),
        catalog_match_confidence,
    );

    entry.trust = result.trust;
    entry.last_verification = Some(result.clone());
    entry.last_verified_at_rfc3339 = Some(chrono::Utc::now().to_rfc3339());
    entry.file_status = match result.sha256 {
        CheckState::Failed => FileStatus::Replaced,
        _ if !entry.current_path.is_file() => FileStatus::Missing,
        _ => FileStatus::Unchanged,
    };

    if entry.file_status == FileStatus::Unchanged
        && let Ok(metadata) = std::fs::metadata(&entry.current_path)
    {
        entry.file_size_bytes = metadata.len();
        entry.file_modified_at_rfc3339 = metadata.modified().ok().map(system_time_to_rfc3339);
    }

    result
}

/// What `brute library locate` reports - `reported_status` is always
/// `FileStatus::Moved` for a successful call (the user-facing framing of
/// "this file was found again at a new path"), even though the entry's
/// *persisted* `file_status` normalizes to `Unchanged` once the move is
/// confirmed, since from that point on it's simply the current, valid
/// location.
#[derive(serde::Serialize)]
pub struct LocateOutcome {
    pub verification: GgufVerification,
    pub reported_status: FileStatus,
}

/// Recovery workflow for a moved/renamed model (spec section 9).
/// Validates `new_path`, computes its hash, and only rebinds the stored
/// entry's `current_path` if that hash matches the original artifact's
/// recorded hash exactly - a mismatched candidate is rejected with
/// [`LibraryError::RelocationHashMismatch`] and the entry is left
/// completely untouched. Runtime profiles and calibration records stay
/// valid across a successful locate because they're associated by
/// content hash, never by path - see `docs/runtime-profile-association.md`.
pub fn locate(
    store: &mut LibraryStore,
    library_id: &str,
    new_path: &Path,
) -> Result<LocateOutcome, BruteError> {
    let canonical = crate::security::validate_regular_file(new_path)?;
    let actual_hash = sha256_file(&canonical).map_err(|source| BruteError::Io {
        context: format!("hashing {}", canonical.display()),
        source,
    })?;

    let entry = store
        .find_by_id_mut(library_id)
        .ok_or_else(|| LibraryError::NotFound(library_id.to_string()))?;

    if actual_hash != entry.sha256 {
        return Err(LibraryError::RelocationHashMismatch {
            expected: entry.sha256.clone(),
            actual: actual_hash,
        }
        .into());
    }

    entry.current_path = canonical;
    let catalog_confidence = entry.catalog_match.confidence;
    let verification = verify_entry(entry, catalog_confidence);

    Ok(LocateOutcome {
        verification,
        reported_status: FileStatus::Moved,
    })
}

pub fn system_time_to_rfc3339(time: std::time::SystemTime) -> String {
    let datetime: chrono::DateTime<chrono::Utc> = time.into();
    datetime.to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::super::{CatalogMatchResult, LIBRARY_SCHEMA_VERSION};
    use super::*;
    use std::io::Write;

    /// A `LibraryEntry` with only the fields `quick_file_status` actually
    /// reads populated meaningfully - everything else is a placeholder,
    /// since these tests exercise file-change detection only.
    fn minimal_entry_for_path(path: &Path) -> LibraryEntry {
        LibraryEntry {
            library_id: "lib-test".to_string(),
            schema_version: LIBRARY_SCHEMA_VERSION.to_string(),
            sha256: "test-hash".to_string(),
            file_size_bytes: 0,
            gguf_version: 3,
            tensor_count: 0,
            kv_count: 0,
            architecture: None,
            quantization: None,
            parameter_count: None,
            current_path: path.to_path_buf(),
            original_import_path: None,
            imported_at_rfc3339: "2026-07-18T00:00:00Z".to_string(),
            last_verified_at_rfc3339: None,
            file_modified_at_rfc3339: None,
            file_status: FileStatus::Unchanged,
            trust: TrustStatus::LocalUnverifiedSource,
            last_verification: None,
            catalog_match: CatalogMatchResult::none(),
            alias: None,
            notes: None,
            quarantine: None,
            managed_copy: false,
        }
    }

    fn tmp_dir(name: &str) -> std::path::PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "brute-library-verify-test-{name}-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Minimal valid GGUF byte fixture - mirrors
    /// `models::gguf::tests::valid_minimal`, duplicated here (rather than
    /// exported from that module) since each module builds its own small
    /// test fixtures throughout this codebase.
    fn write_valid_gguf(path: &Path) {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"GGUF");
        buf.extend_from_slice(&3u32.to_le_bytes()); // version
        buf.extend_from_slice(&0u64.to_le_bytes()); // tensor_count
        buf.extend_from_slice(&1u64.to_le_bytes()); // kv_count
        let key = "general.architecture";
        buf.extend_from_slice(&(key.len() as u64).to_le_bytes());
        buf.extend_from_slice(key.as_bytes());
        buf.extend_from_slice(&8u32.to_le_bytes()); // GGUF_TYPE_STRING
        let value = "llama";
        buf.extend_from_slice(&(value.len() as u64).to_le_bytes());
        buf.extend_from_slice(value.as_bytes());
        std::fs::File::create(path)
            .unwrap()
            .write_all(&buf)
            .unwrap();
    }

    #[test]
    fn missing_file_reports_unavailable_checks_and_missing_trust() {
        let result = verify_artifact(
            Path::new(r"C:\nonexistent\model.gguf"),
            None,
            CatalogMatchConfidence::None,
        );
        assert_eq!(result.header, CheckState::Unavailable);
        assert_eq!(result.trust, TrustStatus::Missing);
        assert_eq!(result.overall_integrity, OverallIntegrity::Unknown);
    }

    #[test]
    fn bad_magic_is_reported_as_corrupt_not_missing() {
        let dir = tmp_dir("bad-magic");
        let path = dir.join("model.gguf");
        std::fs::write(&path, b"NOTAGGUF-not-real-content").unwrap();

        let result = verify_artifact(&path, None, CatalogMatchConfidence::None);
        assert_eq!(result.header, CheckState::Failed);
        assert_eq!(result.overall_integrity, OverallIntegrity::Corrupt);
        assert_eq!(result.trust, TrustStatus::Corrupt);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn valid_gguf_with_no_expected_hash_is_verified_and_locally_unverified_source() {
        let dir = tmp_dir("valid-fresh");
        let path = dir.join("model.gguf");
        write_valid_gguf(&path);

        let result = verify_artifact(&path, None, CatalogMatchConfidence::None);
        assert_eq!(result.header, CheckState::Verified);
        assert_eq!(result.structure, CheckState::Verified);
        assert_eq!(result.sha256, CheckState::Verified);
        assert_eq!(result.overall_integrity, OverallIntegrity::Verified);
        assert_eq!(result.trust, TrustStatus::LocalUnverifiedSource);
        assert_eq!(result.runtime_load, CheckState::NotTested);
        assert_eq!(result.benchmark, CheckState::NotTested);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn matching_expected_hash_with_strong_catalog_match_is_catalog_metadata_matched() {
        let dir = tmp_dir("matched");
        let path = dir.join("model.gguf");
        write_valid_gguf(&path);
        let actual = sha256_file(&path).unwrap();

        let result = verify_artifact(&path, Some(&actual), CatalogMatchConfidence::Exact);
        assert_eq!(result.sha256, CheckState::Verified);
        assert_eq!(result.trust, TrustStatus::CatalogMetadataMatched);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn mismatched_expected_hash_means_modified_since_verification() {
        let dir = tmp_dir("mismatch");
        let path = dir.join("model.gguf");
        write_valid_gguf(&path);

        let result = verify_artifact(&path, Some(&"0".repeat(64)), CatalogMatchConfidence::None);
        assert_eq!(result.sha256, CheckState::Failed);
        assert_eq!(result.trust, TrustStatus::ModifiedSinceVerification);
        assert_eq!(result.overall_integrity, OverallIntegrity::Suspect);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn unsupported_version_is_distinguished_from_corrupt() {
        let dir = tmp_dir("unsupported");
        let path = dir.join("model.gguf");
        let mut buf = Vec::new();
        buf.extend_from_slice(b"GGUF");
        buf.extend_from_slice(&99u32.to_le_bytes()); // unsupported version
        std::fs::write(&path, &buf).unwrap();

        let result = verify_artifact(&path, None, CatalogMatchConfidence::None);
        assert_eq!(result.header, CheckState::Verified);
        assert_eq!(result.trust, TrustStatus::Unsupported);
        assert_ne!(result.trust, TrustStatus::Corrupt);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn quick_status_reports_missing_without_reading_contents() {
        let entry = minimal_entry_for_path(Path::new(r"C:\nonexistent\model.gguf"));
        assert_eq!(quick_file_status(&entry), FileStatus::Missing);
    }

    #[test]
    fn quick_status_detects_a_size_change() {
        let dir = tmp_dir("size-change");
        let path = dir.join("model.gguf");
        std::fs::write(&path, b"short").unwrap();

        let mut entry = minimal_entry_for_path(&path);
        entry.file_size_bytes = 99999; // deliberately wrong

        assert_eq!(quick_file_status(&entry), FileStatus::Modified);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn quick_status_is_unchanged_when_size_and_mtime_match() {
        let dir = tmp_dir("unchanged");
        let path = dir.join("model.gguf");
        std::fs::write(&path, b"exact content").unwrap();
        let metadata = std::fs::metadata(&path).unwrap();

        let mut entry = minimal_entry_for_path(&path);
        entry.file_size_bytes = metadata.len();
        entry.file_modified_at_rfc3339 = metadata.modified().ok().map(system_time_to_rfc3339);

        assert_eq!(quick_file_status(&entry), FileStatus::Unchanged);

        std::fs::remove_dir_all(&dir).ok();
    }

    fn entry_for(path: &Path, sha256: &str) -> LibraryEntry {
        let mut entry = minimal_entry_for_path(path);
        entry.sha256 = sha256.to_string();
        entry
    }

    #[test]
    fn refresh_entry_updates_file_status_without_recomputing_hash() {
        let dir = tmp_dir("refresh");
        let path = dir.join("model.gguf");
        write_valid_gguf(&path);

        let mut entry = entry_for(&path, "stale-hash-never-recomputed-by-refresh");
        let status = refresh_entry(&mut entry);

        // Size differs from the placeholder 0 the fixture starts with,
        // so a cheap refresh correctly flags it as (heuristically) Modified.
        assert_eq!(status, FileStatus::Modified);
        assert_eq!(entry.sha256, "stale-hash-never-recomputed-by-refresh");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn verify_entry_confirms_unchanged_content_and_refreshes_the_baseline() {
        let dir = tmp_dir("verify-entry-unchanged");
        let path = dir.join("model.gguf");
        write_valid_gguf(&path);
        let real_hash = sha256_file(&path).unwrap();

        let mut entry = entry_for(&path, &real_hash);
        let result = verify_entry(&mut entry, CatalogMatchConfidence::None);

        assert_eq!(result.sha256, CheckState::Verified);
        assert_eq!(entry.file_status, FileStatus::Unchanged);
        assert_eq!(entry.trust, TrustStatus::LocalUnverifiedSource);
        assert!(entry.last_verified_at_rfc3339.is_some());
        assert_eq!(
            entry.file_size_bytes,
            std::fs::metadata(&path).unwrap().len()
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn verify_entry_detects_replaced_content() {
        let dir = tmp_dir("verify-entry-replaced");
        let path = dir.join("model.gguf");
        write_valid_gguf(&path);

        let mut entry = entry_for(&path, &"f".repeat(64)); // deliberately wrong recorded hash
        let result = verify_entry(&mut entry, CatalogMatchConfidence::None);

        assert_eq!(result.sha256, CheckState::Failed);
        assert_eq!(entry.file_status, FileStatus::Replaced);
        assert_eq!(entry.trust, TrustStatus::ModifiedSinceVerification);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn verify_entry_detects_a_missing_file() {
        let mut entry = entry_for(Path::new(r"C:\nonexistent\gone.gguf"), "some-hash");
        let result = verify_entry(&mut entry, CatalogMatchConfidence::None);

        assert_eq!(result.trust, TrustStatus::Missing);
        assert_eq!(entry.file_status, FileStatus::Missing);
        assert_eq!(entry.trust, TrustStatus::Missing);
    }

    #[test]
    fn locate_rebinds_the_path_when_the_hash_matches() {
        let dir = tmp_dir("locate-match");
        let old_path = dir.join("old.gguf");
        let new_path = dir.join("new.gguf");
        write_valid_gguf(&old_path);
        std::fs::copy(&old_path, &new_path).unwrap();
        let real_hash = sha256_file(&old_path).unwrap();

        let mut store = LibraryStore::default();
        let mut entry = entry_for(&old_path, &real_hash);
        entry.library_id = "lib-1".to_string();
        store.entries.push(entry);

        let outcome = locate(&mut store, "lib-1", &new_path).unwrap();

        assert_eq!(outcome.reported_status, FileStatus::Moved);
        let updated = store.find_by_id("lib-1").unwrap();
        assert_eq!(
            updated.current_path,
            std::fs::canonicalize(&new_path).unwrap()
        );
        assert_eq!(updated.file_status, FileStatus::Unchanged);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn locate_rejects_a_mismatched_replacement_and_leaves_the_entry_untouched() {
        let dir = tmp_dir("locate-mismatch");
        let old_path = dir.join("old.gguf");
        let wrong_path = dir.join("wrong.gguf");
        write_valid_gguf(&old_path);
        std::fs::write(&wrong_path, b"GGUF-totally-different-content-here").unwrap();
        let real_hash = sha256_file(&old_path).unwrap();

        let mut store = LibraryStore::default();
        let mut entry = entry_for(&old_path, &real_hash);
        entry.library_id = "lib-1".to_string();
        store.entries.push(entry);

        let result = locate(&mut store, "lib-1", &wrong_path);

        assert!(matches!(
            result,
            Err(BruteError::Library(
                LibraryError::RelocationHashMismatch { .. }
            ))
        ));
        let unchanged = store.find_by_id("lib-1").unwrap();
        assert_eq!(unchanged.current_path, old_path);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn locate_reports_not_found_for_an_unknown_library_id() {
        let dir = tmp_dir("locate-unknown");
        let path = dir.join("model.gguf");
        write_valid_gguf(&path);

        let mut store = LibraryStore::default();
        let result = locate(&mut store, "nonexistent", &path);
        assert!(matches!(
            result,
            Err(BruteError::Library(LibraryError::NotFound(_)))
        ));

        std::fs::remove_dir_all(&dir).ok();
    }
}
