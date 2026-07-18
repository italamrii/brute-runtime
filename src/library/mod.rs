//! Trusted Local Model Library (Stage 3): metadata about the user's GGUF
//! model files, stored locally and never copied/moved/executed/uploaded
//! by default. See `docs/stage-3-trusted-local-library.md` for the
//! overview and `docs/local-library-schema.md` for the full field
//! reference.
//!
//! **Storage format decision: one bounded JSON file, not SQLite.** Every
//! other local store in this project (catalog, calibration, runtime
//! profiles) is a flat, versioned JSON file loaded fully into memory -
//! appropriate because entry counts are expected in the hundreds to low
//! thousands (this module is measured up to 10,000 synthetic entries;
//! see `docs/stage-3-verification.md`), and every real lookup here is
//! either "by ID", "by hash", or "scan every entry" - none of which need
//! a query planner or joins. Introducing `rusqlite`/`sqlx` would add a
//! new dependency, a migration framework, and a background-service-
//! adjacent complexity surface for a problem a plain `Vec<LibraryEntry>`
//! with a linear scan (or a `BTreeMap` grouping, where one's already
//! useful - see `duplicates::find_duplicate_groups`) already solves. See
//! `docs/local-library-schema.md` for the full writeup.

pub mod associations;
pub mod audit;
pub mod duplicates;
pub mod import;
pub mod scan;
pub mod storage;
pub mod verify;

use crate::errors::LibraryError;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const LIBRARY_SCHEMA_VERSION: &str = "stage3-library-v1";

/// `%LOCALAPPDATA%\BruteRuntime\library\index.json` - local-only, never
/// git-tracked, never uploaded. See `docs/privacy-model.md`.
pub fn default_library_dir() -> PathBuf {
    crate::identity::default_local_state_dir().join("library")
}

pub fn default_index_path() -> PathBuf {
    default_library_dir().join("index.json")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckState {
    Verified,
    Failed,
    NotTested,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverallIntegrity {
    Verified,
    Suspect,
    Corrupt,
    Unknown,
}

/// Provenance/trust states - deliberately its own vocabulary, matching
/// Stage 2's precedent (`backends::BackendStatus` doesn't reuse
/// `hardware::Confidence`/`provenance::Provenance` either): trust
/// describes something categorically different from either of those.
/// See `docs/trust-and-provenance.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustStatus {
    LocalUnverifiedSource,
    UserConfirmedSource,
    CatalogMetadataMatched,
    ExpectedHashMatched,
    RuntimeVerified,
    ModifiedSinceVerification,
    Missing,
    Corrupt,
    Unsupported,
    Unknown,
}

/// File-change-detection state (spec section 8) - distinct from `trust`:
/// this asks "does the file on disk still match what we last saw",
/// `trust` asks "how much do we trust this artifact's origin". A file
/// can be `Unchanged` and still only `LocalUnverifiedSource`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileStatus {
    Unchanged,
    Modified,
    Moved,
    Missing,
    Inaccessible,
    Replaced,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogMatchConfidence {
    Exact,
    Strong,
    Probable,
    Weak,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogMatchResult {
    pub catalog_id: Option<String>,
    pub confidence: CatalogMatchConfidence,
    pub notes: Vec<String>,
}

impl CatalogMatchResult {
    pub fn none() -> Self {
        Self {
            catalog_id: None,
            confidence: CatalogMatchConfidence::None,
            notes: vec![],
        }
    }
}

/// One point-in-time verification result (spec section 5) - never
/// collapsed into a single boolean. `catalog_match` here reuses the
/// same confidence tiers as `CatalogMatchResult` rather than the spec's
/// placeholder `"matched"` string, since the tiers already carry
/// strictly more information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GgufVerification {
    pub header: CheckState,
    pub structure: CheckState,
    pub sha256: CheckState,
    pub runtime_load: CheckState,
    pub benchmark: CheckState,
    pub catalog_match: CatalogMatchConfidence,
    pub overall_integrity: OverallIntegrity,
    pub trust: TrustStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuarantineInfo {
    pub reason: String,
    pub quarantined_at_rfc3339: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryEntry {
    pub library_id: String,
    pub schema_version: String,

    pub sha256: String,
    pub file_size_bytes: u64,
    pub gguf_version: u32,
    pub tensor_count: u64,
    pub kv_count: u64,
    pub architecture: Option<String>,
    pub quantization: Option<String>,
    pub parameter_count: Option<u64>,

    /// Real, unredacted local path - private state only, never written
    /// to a shareable export. See `export::sanitize`.
    pub current_path: PathBuf,
    pub original_import_path: Option<PathBuf>,

    pub imported_at_rfc3339: String,
    pub last_verified_at_rfc3339: Option<String>,
    pub file_modified_at_rfc3339: Option<String>,

    pub file_status: FileStatus,
    pub trust: TrustStatus,
    pub last_verification: Option<GgufVerification>,
    pub catalog_match: CatalogMatchResult,

    pub alias: Option<String>,
    pub notes: Option<String>,

    pub quarantine: Option<QuarantineInfo>,

    /// `true` only for a file BRUTE itself copied into the managed
    /// library root - Stage 3 does not implement managed copying (see
    /// `docs/model-import-and-verification.md`), so this is always
    /// `false` today; the field exists so `remove-managed`'s "not a
    /// managed copy" rejection has something real to check rather than
    /// being permanently a hard-coded error path.
    pub managed_copy: bool,
}

impl LibraryEntry {
    pub fn is_quarantined(&self) -> bool {
        self.quarantine.is_some()
    }

    /// A short, human-readable one-line summary - used in `library list`
    /// and audit output.
    pub fn metadata_summary(&self) -> String {
        let arch = self
            .architecture
            .as_deref()
            .unwrap_or("unknown architecture");
        let quant = self
            .quantization
            .as_deref()
            .unwrap_or("unknown quantization");
        match self.parameter_count {
            Some(p) => format!("{arch}, {:.1}B params, {quant}", p as f64 / 1_000_000_000.0),
            None => format!("{arch}, unknown param count, {quant}"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryStore {
    pub schema_version: String,
    pub entries: Vec<LibraryEntry>,
}

impl Default for LibraryStore {
    fn default() -> Self {
        Self {
            schema_version: LIBRARY_SCHEMA_VERSION.to_string(),
            entries: Vec::new(),
        }
    }
}

impl LibraryStore {
    /// Loads the index at `path`, or an empty store if it doesn't exist
    /// yet - a missing library is not an error, it just means nothing
    /// has been imported yet.
    pub fn load_from(path: &Path) -> Result<Self, LibraryError> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let contents = std::fs::read_to_string(path).map_err(|source| LibraryError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        if contents.trim().is_empty() {
            return Ok(Self::default());
        }
        let store: LibraryStore =
            serde_json::from_str(&contents).map_err(|source| LibraryError::Parse {
                path: path.to_path_buf(),
                source,
            })?;
        Ok(migrate(store))
    }

    /// Writes the index via write-to-temp-then-rename, so a crash mid-
    /// write can never leave a half-written index in place - the rename
    /// either lands the complete new file or doesn't happen at all.
    pub fn save_to(&self, path: &Path) -> Result<(), LibraryError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| LibraryError::Write {
                path: path.to_path_buf(),
                source,
            })?;
        }
        let json = serde_json::to_string_pretty(self).expect("LibraryStore always serializes");
        let tmp_path = {
            let mut p = path.as_os_str().to_owned();
            p.push(".tmp");
            PathBuf::from(p)
        };
        std::fs::write(&tmp_path, &json).map_err(|source| LibraryError::Write {
            path: tmp_path.clone(),
            source,
        })?;
        std::fs::rename(&tmp_path, path).map_err(|source| LibraryError::Write {
            path: path.to_path_buf(),
            source,
        })
    }

    pub fn find_by_id(&self, id: &str) -> Option<&LibraryEntry> {
        self.entries.iter().find(|e| e.library_id == id)
    }

    pub fn find_by_id_mut(&mut self, id: &str) -> Option<&mut LibraryEntry> {
        self.entries.iter_mut().find(|e| e.library_id == id)
    }

    pub fn require(&self, id: &str) -> Result<&LibraryEntry, LibraryError> {
        self.find_by_id(id)
            .ok_or_else(|| LibraryError::NotFound(id.to_string()))
    }

    pub fn find_by_sha256(&self, sha256: &str) -> Vec<&LibraryEntry> {
        self.entries.iter().filter(|e| e.sha256 == sha256).collect()
    }

    /// Removes the metadata entry only - never touches the underlying
    /// file. See `docs/quarantine-and-recovery.md` for why "forget" and
    /// "delete" must never be the same operation.
    pub fn forget(&mut self, id: &str) -> Result<LibraryEntry, LibraryError> {
        let index = self
            .entries
            .iter()
            .position(|e| e.library_id == id)
            .ok_or_else(|| LibraryError::NotFound(id.to_string()))?;
        Ok(self.entries.remove(index))
    }

    /// Marks an entry quarantined (spec section 13) - a metadata-only
    /// hold. Never moves the file; a quarantined entry is simply refused
    /// for benchmarking/launch by anything that checks
    /// `LibraryEntry::is_quarantined` first.
    pub fn quarantine(&mut self, id: &str, reason: String) -> Result<(), LibraryError> {
        let entry = self
            .find_by_id_mut(id)
            .ok_or_else(|| LibraryError::NotFound(id.to_string()))?;
        entry.quarantine = Some(QuarantineInfo {
            reason,
            quarantined_at_rfc3339: chrono::Utc::now().to_rfc3339(),
        });
        Ok(())
    }

    /// Lifts quarantine only after a fresh full verification pass
    /// actually passes - manual unquarantine must never bypass
    /// validation (spec section 13). Returns the verification result
    /// either way; the quarantine flag itself is only cleared when
    /// `overall_integrity` comes back `Verified`.
    pub fn unquarantine(&mut self, id: &str) -> Result<GgufVerification, LibraryError> {
        let entry = self
            .find_by_id_mut(id)
            .ok_or_else(|| LibraryError::NotFound(id.to_string()))?;
        let catalog_confidence = entry.catalog_match.confidence;
        let result = verify::verify_entry(entry, catalog_confidence);

        if result.overall_integrity != OverallIntegrity::Verified {
            return Err(LibraryError::Quarantined(
                id.to_string(),
                format!(
                    "re-verification did not pass (overall_integrity={:?}) - quarantine not lifted",
                    result.overall_integrity
                ),
            ));
        }

        entry.quarantine = None;
        Ok(result)
    }

    /// Removes a *BRUTE-managed* copy from disk and its library entry
    /// together (spec section 12). Stage 3 does not implement managed
    /// copying (see `docs/model-import-and-verification.md`), so
    /// `managed_copy` is `false` on every entry this codebase creates
    /// and this always returns [`LibraryError::NotManaged`] today -
    /// honestly reporting that, rather than a permanently-dead success
    /// path. The path-traversal guard below is still real and tested
    /// (via a synthetic `managed_copy: true` fixture), ready for the day
    /// managed copying is added.
    pub fn remove_managed(&mut self, id: &str) -> Result<(), LibraryError> {
        let entry = self.require(id)?;
        if !entry.managed_copy {
            return Err(LibraryError::NotManaged(id.to_string()));
        }

        let managed_root = default_library_dir();
        let canonical_path =
            std::fs::canonicalize(&entry.current_path).map_err(|source| LibraryError::Write {
                path: entry.current_path.clone(),
                source,
            })?;
        if !canonical_path.starts_with(&managed_root) {
            return Err(LibraryError::PathEscapesManagedRoot(canonical_path));
        }

        std::fs::remove_file(&canonical_path).map_err(|source| LibraryError::Write {
            path: canonical_path,
            source,
        })?;
        self.forget(id)?;
        Ok(())
    }
}

/// Schema migration entry point. `stage3-library-v1` is the only schema
/// version that exists today, so this is currently a no-op identity
/// migration - but it's structured so a real future migration only has
/// to add a match arm here, and `tests::migrating_an_unknown_older_schema_version_is_a_no_op_today`
/// exercises the scaffold with a synthetic older-version fixture.
fn migrate(store: LibraryStore) -> LibraryStore {
    if store.schema_version == LIBRARY_SCHEMA_VERSION {
        return store;
    }
    // No prior schema version has ever shipped, so there is nothing to
    // transform yet - just stamp the current version. A real migration
    // would back up the original file before transforming field shapes;
    // see `docs/local-library-schema.md`.
    LibraryStore {
        schema_version: LIBRARY_SCHEMA_VERSION.to_string(),
        ..store
    }
}

/// Returns a copy of `entry` safe to write to a shareable export:
/// real filesystem paths are replaced with `<LOCAL_MODEL_PATH>`, and
/// nothing else about the entry is a personally-identifying value (the
/// rest is content hashes, GGUF metadata, and user-supplied alias/notes
/// text, which the user explicitly chose to attach). See
/// `docs/library-privacy.md`.
pub const REDACTED_PATH_PLACEHOLDER: &str = "<LOCAL_MODEL_PATH>";

pub fn sanitize_entry_for_export(entry: &LibraryEntry) -> LibraryEntry {
    let mut sanitized = entry.clone();
    sanitized.current_path = PathBuf::from(REDACTED_PATH_PLACEHOLDER);
    sanitized.original_import_path = sanitized
        .original_import_path
        .as_ref()
        .map(|_| PathBuf::from(REDACTED_PATH_PLACEHOLDER));
    sanitized
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_entry(id: &str, sha256: &str) -> LibraryEntry {
        LibraryEntry {
            library_id: id.to_string(),
            schema_version: LIBRARY_SCHEMA_VERSION.to_string(),
            sha256: sha256.to_string(),
            file_size_bytes: 491_400_032,
            gguf_version: 3,
            tensor_count: 100,
            kv_count: 10,
            architecture: Some("qwen2".to_string()),
            quantization: Some("Q4_K_M".to_string()),
            parameter_count: Some(630_167_424),
            current_path: PathBuf::from(r"C:\Models\test.gguf"),
            original_import_path: Some(PathBuf::from(r"C:\Users\alice\Downloads\test.gguf")),
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

    fn tmp_dir(name: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "brute-library-test-{name}-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn loading_a_missing_index_returns_an_empty_store_not_an_error() {
        let store = LibraryStore::load_from(Path::new(r"C:\nonexistent\index.json")).unwrap();
        assert!(store.entries.is_empty());
        assert_eq!(store.schema_version, LIBRARY_SCHEMA_VERSION);
    }

    #[test]
    fn save_and_load_roundtrip_preserves_entries() {
        let dir = tmp_dir("roundtrip");
        let path = dir.join("index.json");
        let mut store = LibraryStore::default();
        store.entries.push(sample_entry("lib-1", "hash-a"));

        store.save_to(&path).unwrap();
        let loaded = LibraryStore::load_from(&path).unwrap();

        assert_eq!(loaded.entries.len(), 1);
        assert_eq!(loaded.entries[0].library_id, "lib-1");
        assert_eq!(loaded.entries[0].sha256, "hash-a");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn atomic_write_leaves_no_tmp_file_behind_on_success() {
        let dir = tmp_dir("atomic");
        let path = dir.join("index.json");
        let store = LibraryStore::default();
        store.save_to(&path).unwrap();

        assert!(path.is_file());
        assert!(!dir.join("index.json.tmp").exists());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn forget_removes_the_entry_but_never_touches_the_file() {
        let dir = tmp_dir("forget");
        let model_path = dir.join("model.gguf");
        std::fs::write(&model_path, b"not a real gguf, just needs to exist").unwrap();

        let mut store = LibraryStore::default();
        let mut entry = sample_entry("lib-1", "hash-a");
        entry.current_path = model_path.clone();
        store.entries.push(entry);

        store.forget("lib-1").unwrap();

        assert!(store.find_by_id("lib-1").is_none());
        assert!(
            model_path.is_file(),
            "forget must never delete the underlying file"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn forgetting_an_unknown_id_is_a_clear_error() {
        let mut store = LibraryStore::default();
        let result = store.forget("nonexistent");
        assert!(matches!(result, Err(LibraryError::NotFound(id)) if id == "nonexistent"));
    }

    #[test]
    fn find_by_sha256_returns_every_entry_sharing_that_hash() {
        let mut store = LibraryStore::default();
        store.entries.push(sample_entry("lib-1", "shared-hash"));
        store.entries.push(sample_entry("lib-2", "shared-hash"));
        store.entries.push(sample_entry("lib-3", "different-hash"));

        let matches = store.find_by_sha256("shared-hash");
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn sanitize_for_export_replaces_paths_and_nothing_else() {
        let entry = sample_entry("lib-1", "hash-a");
        let sanitized = sanitize_entry_for_export(&entry);

        assert_eq!(
            sanitized.current_path,
            PathBuf::from(REDACTED_PATH_PLACEHOLDER)
        );
        assert_eq!(
            sanitized.original_import_path,
            Some(PathBuf::from(REDACTED_PATH_PLACEHOLDER))
        );
        assert_eq!(sanitized.sha256, entry.sha256);
        assert_eq!(sanitized.library_id, entry.library_id);
        assert_eq!(sanitized.architecture, entry.architecture);
    }

    #[test]
    fn migrating_an_unknown_older_schema_version_is_a_no_op_today() {
        let mut store = LibraryStore {
            schema_version: "stage3-library-v0-hypothetical".to_string(),
            ..LibraryStore::default()
        };
        store.entries.push(sample_entry("lib-1", "hash-a"));

        let migrated = migrate(store);
        assert_eq!(migrated.schema_version, LIBRARY_SCHEMA_VERSION);
        assert_eq!(migrated.entries.len(), 1);
    }

    #[test]
    fn loading_an_older_schema_version_from_disk_migrates_on_load() {
        let dir = tmp_dir("migrate");
        let path = dir.join("index.json");
        let mut store = LibraryStore {
            schema_version: "stage3-library-v0-hypothetical".to_string(),
            ..LibraryStore::default()
        };
        store.entries.push(sample_entry("lib-1", "hash-a"));
        // Bypass save_to's use of the *current* constant by writing raw JSON.
        std::fs::write(&path, serde_json::to_string_pretty(&store).unwrap()).unwrap();

        let loaded = LibraryStore::load_from(&path).unwrap();
        assert_eq!(loaded.schema_version, LIBRARY_SCHEMA_VERSION);
        assert_eq!(loaded.entries.len(), 1);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn corrupted_index_json_is_a_clear_parse_error_not_a_panic() {
        let dir = tmp_dir("corrupt");
        let path = dir.join("index.json");
        std::fs::write(&path, "{ this is not valid json at all").unwrap();

        let result = LibraryStore::load_from(&path);
        assert!(matches!(result, Err(LibraryError::Parse { .. })));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn metadata_summary_reports_unknowns_honestly() {
        let mut entry = sample_entry("lib-1", "hash-a");
        entry.architecture = None;
        entry.quantization = None;
        entry.parameter_count = None;
        let summary = entry.metadata_summary();
        assert!(summary.contains("unknown architecture"));
        assert!(summary.contains("unknown quantization"));
        assert!(summary.contains("unknown param count"));
    }

    fn write_valid_gguf(path: &Path) {
        use std::io::Write;
        let mut buf = Vec::new();
        buf.extend_from_slice(b"GGUF");
        buf.extend_from_slice(&3u32.to_le_bytes());
        buf.extend_from_slice(&0u64.to_le_bytes());
        buf.extend_from_slice(&1u64.to_le_bytes());
        let key = "general.architecture";
        buf.extend_from_slice(&(key.len() as u64).to_le_bytes());
        buf.extend_from_slice(key.as_bytes());
        buf.extend_from_slice(&8u32.to_le_bytes());
        let value = "llama";
        buf.extend_from_slice(&(value.len() as u64).to_le_bytes());
        buf.extend_from_slice(value.as_bytes());
        std::fs::File::create(path)
            .unwrap()
            .write_all(&buf)
            .unwrap();
    }

    #[test]
    fn quarantine_sets_the_reason_and_can_be_looked_up() {
        let mut store = LibraryStore::default();
        store.entries.push(sample_entry("lib-1", "hash-a"));

        store
            .quarantine("lib-1", "malformed tensor offsets".to_string())
            .unwrap();

        let entry = store.find_by_id("lib-1").unwrap();
        assert!(entry.is_quarantined());
        assert_eq!(
            entry.quarantine.as_ref().unwrap().reason,
            "malformed tensor offsets"
        );
    }

    #[test]
    fn quarantining_an_unknown_id_is_a_clear_error() {
        let mut store = LibraryStore::default();
        let result = store.quarantine("nonexistent", "test".to_string());
        assert!(matches!(result, Err(LibraryError::NotFound(_))));
    }

    #[test]
    fn unquarantine_succeeds_only_after_a_passing_reverification() {
        let dir = tmp_dir("unquarantine-ok");
        let path = dir.join("model.gguf");
        write_valid_gguf(&path);
        let real_hash = crate::security::hashing::sha256_file(&path).unwrap();

        let mut store = LibraryStore::default();
        let mut entry = sample_entry("lib-1", &real_hash);
        entry.current_path = path;
        entry.quarantine = Some(QuarantineInfo {
            reason: "test".to_string(),
            quarantined_at_rfc3339: "2026-01-01T00:00:00Z".to_string(),
        });
        store.entries.push(entry);

        store.unquarantine("lib-1").unwrap();
        assert!(!store.find_by_id("lib-1").unwrap().is_quarantined());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn unquarantine_refuses_to_clear_when_reverification_fails() {
        let mut store = LibraryStore::default();
        let mut entry = sample_entry("lib-1", "hash-a");
        entry.current_path = PathBuf::from(r"C:\nonexistent\gone.gguf"); // will fail verification: Missing
        entry.quarantine = Some(QuarantineInfo {
            reason: "test".to_string(),
            quarantined_at_rfc3339: "2026-01-01T00:00:00Z".to_string(),
        });
        store.entries.push(entry);

        let result = store.unquarantine("lib-1");
        assert!(matches!(result, Err(LibraryError::Quarantined(_, _))));
        assert!(store.find_by_id("lib-1").unwrap().is_quarantined());
    }

    #[test]
    fn remove_managed_refuses_a_non_managed_entry() {
        let mut store = LibraryStore::default();
        store.entries.push(sample_entry("lib-1", "hash-a")); // managed_copy: false

        let result = store.remove_managed("lib-1");
        assert!(matches!(result, Err(LibraryError::NotManaged(_))));
        assert!(
            store.find_by_id("lib-1").is_some(),
            "a rejected removal must not remove the entry"
        );
    }

    #[test]
    fn remove_managed_rejects_a_path_outside_the_managed_root_even_if_flagged_managed() {
        let dir = tmp_dir("escape-managed-root");
        let path = dir.join("model.gguf");
        write_valid_gguf(&path);

        let mut store = LibraryStore::default();
        let mut entry = sample_entry("lib-1", "hash-a");
        entry.current_path = path.clone();
        entry.managed_copy = true; // synthetic - no real code path sets this true today
        store.entries.push(entry);

        let result = store.remove_managed("lib-1");
        assert!(matches!(
            result,
            Err(LibraryError::PathEscapesManagedRoot(_))
        ));
        assert!(
            path.is_file(),
            "a rejected removal must never delete the file"
        );
        assert!(store.find_by_id("lib-1").is_some());

        std::fs::remove_dir_all(&dir).ok();
    }
}
