//! Storage planning (spec section 11). Purely a read-only analysis over
//! the library index - nothing here deletes, moves, or reclaims
//! anything. Duplicate space is reported as *potentially* recoverable,
//! never presented as safely reclaimable - the user decides, via
//! `brute library forget`, once they've confirmed no external workflow
//! depends on a given copy. See `docs/storage-management.md`.

use super::duplicates::find_duplicate_groups;
use super::verify::quick_file_status;
use super::{FileStatus, LibraryStore, TrustStatus};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize)]
pub struct LargestEntry {
    pub library_id: String,
    pub path: PathBuf,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct StorageSummary {
    pub total_entries: usize,
    pub total_bytes: u64,
    /// Sum of sizes for entries whose file is currently present and
    /// matching its last-recorded size/mtime (a live, cheap check - see
    /// `verify::quick_file_status`).
    pub available_bytes: u64,
    /// Sum, across every duplicate group, of every member's size except
    /// the one suggested as canonical - a *potential* reclaim estimate,
    /// never a claim that reclaiming it is safe.
    pub duplicate_bytes: u64,
    pub missing_count: usize,
    pub corrupt_count: usize,
    pub largest: Vec<LargestEntry>,
    pub bytes_by_architecture: BTreeMap<String, u64>,
    pub bytes_by_quantization: BTreeMap<String, u64>,
    pub managed_bytes: u64,
    pub external_bytes: u64,
}

const MAX_LARGEST_ENTRIES: usize = 10;

pub fn summarize(store: &LibraryStore) -> StorageSummary {
    let total_entries = store.entries.len();
    let total_bytes: u64 = store.entries.iter().map(|e| e.file_size_bytes).sum();

    let available_bytes: u64 = store
        .entries
        .iter()
        .filter(|e| quick_file_status(e) == FileStatus::Unchanged)
        .map(|e| e.file_size_bytes)
        .sum();

    let missing_count = store
        .entries
        .iter()
        .filter(|e| quick_file_status(e) == FileStatus::Missing)
        .count();

    let corrupt_count = store
        .entries
        .iter()
        .filter(|e| e.trust == TrustStatus::Corrupt)
        .count();

    let duplicate_groups = find_duplicate_groups(store);
    let duplicate_bytes: u64 = duplicate_groups
        .iter()
        .map(|group| {
            group
                .members
                .iter()
                .filter(|m| m.library_id != group.canonical_library_id)
                .map(|m| m.size_bytes)
                .sum::<u64>()
        })
        .sum();

    let mut largest: Vec<LargestEntry> = store
        .entries
        .iter()
        .map(|e| LargestEntry {
            library_id: e.library_id.clone(),
            path: e.current_path.clone(),
            size_bytes: e.file_size_bytes,
        })
        .collect();
    largest.sort_by(|a, b| {
        b.size_bytes
            .cmp(&a.size_bytes)
            .then_with(|| a.library_id.cmp(&b.library_id))
    });
    largest.truncate(MAX_LARGEST_ENTRIES);

    let mut bytes_by_architecture: BTreeMap<String, u64> = BTreeMap::new();
    let mut bytes_by_quantization: BTreeMap<String, u64> = BTreeMap::new();
    let mut managed_bytes: u64 = 0;
    for entry in &store.entries {
        let arch = entry
            .architecture
            .clone()
            .unwrap_or_else(|| "unknown".to_string());
        let quant = entry
            .quantization
            .clone()
            .unwrap_or_else(|| "unknown".to_string());
        *bytes_by_architecture.entry(arch).or_insert(0) += entry.file_size_bytes;
        *bytes_by_quantization.entry(quant).or_insert(0) += entry.file_size_bytes;
        if entry.managed_copy {
            managed_bytes += entry.file_size_bytes;
        }
    }

    StorageSummary {
        total_entries,
        total_bytes,
        available_bytes,
        duplicate_bytes,
        missing_count,
        corrupt_count,
        largest,
        bytes_by_architecture,
        bytes_by_quantization,
        managed_bytes,
        external_bytes: total_bytes - managed_bytes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::{CatalogMatchResult, LIBRARY_SCHEMA_VERSION, LibraryEntry};

    fn entry(
        id: &str,
        sha256: &str,
        size: u64,
        architecture: &str,
        quantization: &str,
    ) -> LibraryEntry {
        LibraryEntry {
            library_id: id.to_string(),
            schema_version: LIBRARY_SCHEMA_VERSION.to_string(),
            sha256: sha256.to_string(),
            file_size_bytes: size,
            gguf_version: 3,
            tensor_count: 10,
            kv_count: 5,
            architecture: Some(architecture.to_string()),
            quantization: Some(quantization.to_string()),
            parameter_count: Some(500_000_000),
            current_path: PathBuf::from(format!(r"C:\Models\{id}.gguf")),
            original_import_path: None,
            imported_at_rfc3339: "2026-01-01T00:00:00Z".to_string(),
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
    fn empty_library_summarizes_to_all_zeros() {
        let store = LibraryStore::default();
        let summary = summarize(&store);
        assert_eq!(summary.total_entries, 0);
        assert_eq!(summary.total_bytes, 0);
        assert!(summary.largest.is_empty());
    }

    #[test]
    fn totals_sum_across_every_entry() {
        let mut store = LibraryStore::default();
        store
            .entries
            .push(entry("a", "hash-a", 1000, "llama", "Q4_K_M"));
        store
            .entries
            .push(entry("b", "hash-b", 2000, "qwen2", "Q8_0"));

        let summary = summarize(&store);
        assert_eq!(summary.total_entries, 2);
        assert_eq!(summary.total_bytes, 3000);
    }

    #[test]
    fn largest_is_sorted_descending_and_bounded() {
        let mut store = LibraryStore::default();
        for i in 0..15 {
            store.entries.push(entry(
                &format!("e{i}"),
                &format!("hash-{i}"),
                i * 100,
                "llama",
                "Q4_K_M",
            ));
        }

        let summary = summarize(&store);
        assert_eq!(summary.largest.len(), MAX_LARGEST_ENTRIES);
        assert_eq!(summary.largest[0].size_bytes, 1400); // i=14 -> 1400 is the largest
        for pair in summary.largest.windows(2) {
            assert!(pair[0].size_bytes >= pair[1].size_bytes);
        }
    }

    #[test]
    fn breaks_down_bytes_by_architecture_and_quantization() {
        let mut store = LibraryStore::default();
        store
            .entries
            .push(entry("a", "hash-a", 1000, "llama", "Q4_K_M"));
        store
            .entries
            .push(entry("b", "hash-b", 500, "llama", "Q8_0"));
        store
            .entries
            .push(entry("c", "hash-c", 2000, "qwen2", "Q4_K_M"));

        let summary = summarize(&store);
        assert_eq!(summary.bytes_by_architecture.get("llama"), Some(&1500));
        assert_eq!(summary.bytes_by_architecture.get("qwen2"), Some(&2000));
        assert_eq!(summary.bytes_by_quantization.get("Q4_K_M"), Some(&3000));
        assert_eq!(summary.bytes_by_quantization.get("Q8_0"), Some(&500));
    }

    #[test]
    fn duplicate_bytes_excludes_the_canonical_copy() {
        let mut store = LibraryStore::default();
        let mut a = entry("a", "shared", 1000, "llama", "Q4_K_M");
        a.imported_at_rfc3339 = "2026-01-01T00:00:00Z".to_string();
        let mut b = entry("b", "shared", 1000, "llama", "Q4_K_M");
        b.imported_at_rfc3339 = "2026-02-01T00:00:00Z".to_string();
        store.entries.push(a);
        store.entries.push(b);

        let summary = summarize(&store);
        // One copy (the non-canonical one) counted as duplicate weight.
        assert_eq!(summary.duplicate_bytes, 1000);
    }

    #[test]
    fn missing_entries_are_counted_but_not_double_counted_as_corrupt() {
        let mut store = LibraryStore::default();
        let mut missing = entry("a", "hash-a", 1000, "llama", "Q4_K_M");
        missing.current_path = PathBuf::from(r"C:\nonexistent\gone.gguf");
        store.entries.push(missing);

        let summary = summarize(&store);
        assert_eq!(summary.missing_count, 1);
        assert_eq!(summary.corrupt_count, 0);
    }

    #[test]
    fn managed_and_external_bytes_partition_the_total() {
        let mut store = LibraryStore::default();
        let mut managed = entry("a", "hash-a", 1000, "llama", "Q4_K_M");
        managed.managed_copy = true;
        store.entries.push(managed);
        store
            .entries
            .push(entry("b", "hash-b", 500, "llama", "Q4_K_M"));

        let summary = summarize(&store);
        assert_eq!(summary.managed_bytes, 1000);
        assert_eq!(summary.external_bytes, 500);
        assert_eq!(
            summary.managed_bytes + summary.external_bytes,
            summary.total_bytes
        );
    }
}
