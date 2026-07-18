//! Duplicate detection (spec section 7). Grouping is by SHA-256 only -
//! filename similarity is never treated as proof of identity. Nothing
//! here deletes or moves anything; every group only ever suggests a
//! safe next action for the user to take themselves.

use super::verify::quick_file_status;
use super::{FileStatus, LibraryStore};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize)]
pub struct DuplicateMember {
    pub library_id: String,
    pub path: PathBuf,
    pub size_bytes: u64,
    pub file_status: FileStatus,
    pub available: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DuplicateGroup {
    pub sha256: String,
    /// The member suggested as the one to keep - preferring an
    /// available (on-disk, unchanged) copy over a missing one, and
    /// otherwise the earliest imported. Never auto-selected for
    /// deletion; purely a suggestion.
    pub canonical_library_id: String,
    pub members: Vec<DuplicateMember>,
    pub suggested_action: String,
}

/// Groups every set of library entries sharing an identical content
/// hash. Deterministic: entries within a group are ordered by import
/// time then library ID, and groups themselves are ordered by hash.
pub fn find_duplicate_groups(store: &LibraryStore) -> Vec<DuplicateGroup> {
    let mut by_hash: BTreeMap<&str, Vec<&super::LibraryEntry>> = BTreeMap::new();
    for entry in &store.entries {
        by_hash
            .entry(entry.sha256.as_str())
            .or_default()
            .push(entry);
    }

    let mut groups: Vec<DuplicateGroup> = by_hash
        .into_iter()
        .filter(|(_, members)| members.len() > 1)
        .map(|(sha256, mut members)| {
            members.sort_by(|a, b| {
                a.imported_at_rfc3339
                    .cmp(&b.imported_at_rfc3339)
                    .then_with(|| a.library_id.cmp(&b.library_id))
            });

            let member_infos: Vec<DuplicateMember> = members
                .iter()
                .map(|entry| {
                    let status = quick_file_status(entry);
                    DuplicateMember {
                        library_id: entry.library_id.clone(),
                        path: entry.current_path.clone(),
                        size_bytes: entry.file_size_bytes,
                        file_status: status,
                        available: status == FileStatus::Unchanged,
                    }
                })
                .collect();

            let canonical_library_id = member_infos
                .iter()
                .find(|m| m.available)
                .or_else(|| member_infos.first())
                .map(|m| m.library_id.clone())
                .unwrap_or_default();

            let hash_prefix = &sha256[..sha256.len().min(12)];
            let suggested_action = format!(
                "{} identical copies found (sha256 {hash_prefix}...). Consider keeping \
                 {canonical_library_id} and running `brute library forget <id>` on the others \
                 once you've confirmed no external workflow depends on the copy - BRUTE never \
                 deletes or removes duplicates automatically.",
                member_infos.len()
            );

            DuplicateGroup {
                sha256: sha256.to_string(),
                canonical_library_id,
                members: member_infos,
                suggested_action,
            }
        })
        .collect();

    groups.sort_by(|a, b| a.sha256.cmp(&b.sha256));
    groups
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::{CatalogMatchResult, LIBRARY_SCHEMA_VERSION, LibraryEntry, TrustStatus};

    fn entry(id: &str, sha256: &str, path: &str, imported_at: &str) -> LibraryEntry {
        LibraryEntry {
            library_id: id.to_string(),
            schema_version: LIBRARY_SCHEMA_VERSION.to_string(),
            sha256: sha256.to_string(),
            file_size_bytes: 1000,
            gguf_version: 3,
            tensor_count: 10,
            kv_count: 5,
            architecture: Some("llama".to_string()),
            quantization: Some("Q4_K_M".to_string()),
            parameter_count: Some(500_000_000),
            current_path: PathBuf::from(path),
            original_import_path: Some(PathBuf::from(path)),
            imported_at_rfc3339: imported_at.to_string(),
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

    #[test]
    fn no_duplicates_when_every_hash_is_unique() {
        let mut store = LibraryStore::default();
        store
            .entries
            .push(entry("a", "hash-1", r"C:\a.gguf", "2026-01-01T00:00:00Z"));
        store
            .entries
            .push(entry("b", "hash-2", r"C:\b.gguf", "2026-01-01T00:00:00Z"));

        let groups = find_duplicate_groups(&store);
        assert!(groups.is_empty());
    }

    #[test]
    fn identical_hash_different_paths_forms_a_group() {
        let mut store = LibraryStore::default();
        store
            .entries
            .push(entry("a", "shared", r"C:\a.gguf", "2026-01-01T00:00:00Z"));
        store
            .entries
            .push(entry("b", "shared", r"C:\b.gguf", "2026-01-02T00:00:00Z"));

        let groups = find_duplicate_groups(&store);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].members.len(), 2);
        assert_eq!(groups[0].sha256, "shared");
    }

    #[test]
    fn canonical_pick_prefers_the_earliest_imported() {
        let mut store = LibraryStore::default();
        store.entries.push(entry(
            "later",
            "shared",
            r"C:\b.gguf",
            "2026-06-01T00:00:00Z",
        ));
        store.entries.push(entry(
            "earlier",
            "shared",
            r"C:\a.gguf",
            "2026-01-01T00:00:00Z",
        ));

        let groups = find_duplicate_groups(&store);
        assert_eq!(groups[0].canonical_library_id, "earlier");
    }

    #[test]
    fn grouping_is_deterministic_across_repeated_calls() {
        let mut store = LibraryStore::default();
        store
            .entries
            .push(entry("c", "hash-b", r"C:\c.gguf", "2026-01-01T00:00:00Z"));
        store
            .entries
            .push(entry("a", "hash-a", r"C:\a.gguf", "2026-01-01T00:00:00Z"));
        store
            .entries
            .push(entry("d", "hash-b", r"C:\d.gguf", "2026-01-02T00:00:00Z"));
        store
            .entries
            .push(entry("b", "hash-a", r"C:\b.gguf", "2026-01-02T00:00:00Z"));

        let first = find_duplicate_groups(&store);
        let second = find_duplicate_groups(&store);

        let first_ids: Vec<Vec<String>> = first
            .iter()
            .map(|g| g.members.iter().map(|m| m.library_id.clone()).collect())
            .collect();
        let second_ids: Vec<Vec<String>> = second
            .iter()
            .map(|g| g.members.iter().map(|m| m.library_id.clone()).collect())
            .collect();
        assert_eq!(first_ids, second_ids);
        assert_eq!(first.len(), 2);
    }

    #[test]
    fn suggested_action_never_claims_automatic_deletion() {
        let mut store = LibraryStore::default();
        store
            .entries
            .push(entry("a", "shared", r"C:\a.gguf", "2026-01-01T00:00:00Z"));
        store
            .entries
            .push(entry("b", "shared", r"C:\b.gguf", "2026-01-02T00:00:00Z"));

        let groups = find_duplicate_groups(&store);
        assert!(groups[0].suggested_action.contains("never deletes"));
        assert!(groups[0].suggested_action.contains("forget"));
    }
}
