//! Stage 3 performance measurements (spec section 21). Real timings
//! only - printed via `eprintln!` (`cargo test stage3_performance -- --nocapture`),
//! mirroring `stage1_fixtures_test::measure_stage1_performance`'s
//! pattern. Synthetic libraries use in-memory `LibraryEntry` fixtures
//! (never real multi-gigabyte files) at 10/100/1,000/10,000 entries;
//! streaming SHA-256 throughput is measured separately against whatever
//! real GGUF file this machine has, when available, without requiring
//! one to exist.

#[cfg(test)]
mod tests {
    use crate::library::{
        CatalogMatchResult, FileStatus, LIBRARY_SCHEMA_VERSION, LibraryEntry, LibraryStore,
        TrustStatus,
    };
    use std::path::PathBuf;
    use std::time::Instant;

    fn synthetic_entry(i: usize) -> LibraryEntry {
        LibraryEntry {
            library_id: format!("model-perf-{i:06}"),
            schema_version: LIBRARY_SCHEMA_VERSION.to_string(),
            sha256: format!("{i:064x}"),
            file_size_bytes: 400_000_000 + (i as u64 * 1000),
            gguf_version: 3,
            tensor_count: 100,
            kv_count: 20,
            architecture: Some(["llama", "qwen2", "mistral"][i % 3].to_string()),
            quantization: Some(["Q4_K_M", "Q8_0", "Q5_K_M"][i % 3].to_string()),
            parameter_count: Some(500_000_000 + (i as u64 * 10_000_000)),
            current_path: PathBuf::from(format!(r"C:\Models\synthetic-{i:06}.gguf")),
            original_import_path: None,
            imported_at_rfc3339: format!("2026-01-01T00:{:02}:{:02}Z", (i / 60) % 60, i % 60),
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

    fn synthetic_store(count: usize) -> LibraryStore {
        LibraryStore {
            schema_version: LIBRARY_SCHEMA_VERSION.to_string(),
            entries: (0..count).map(synthetic_entry).collect(),
        }
    }

    fn tmp_dir(name: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "brute-stage3-perf-{name}-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn measure_stage3_library_operations_at_scale() {
        for &count in &[10usize, 100, 1_000, 10_000] {
            let store = synthetic_store(count);
            let dir = tmp_dir(&format!("scale-{count}"));
            let index_path = dir.join("index.json");

            let t0 = Instant::now();
            store.save_to(&index_path).unwrap();
            let save_time = t0.elapsed();

            let t1 = Instant::now();
            let loaded = LibraryStore::load_from(&index_path).unwrap();
            let load_time = t1.elapsed();
            assert_eq!(loaded.entries.len(), count);

            let t2 = Instant::now();
            let duplicate_groups = crate::library::duplicates::find_duplicate_groups(&loaded);
            let duplicate_grouping_time = t2.elapsed();
            assert!(
                duplicate_groups.is_empty(),
                "synthetic fixtures use unique hashes"
            );

            let t3 = Instant::now();
            let storage_summary = crate::library::storage::summarize(&loaded);
            let storage_summary_time = t3.elapsed();
            assert_eq!(storage_summary.total_entries, count);

            let t4 = Instant::now();
            let audit_report = crate::library::audit::audit(&loaded, &dir.join("profiles"), None);
            let audit_time = t4.elapsed();
            // These synthetic fixtures have no real backing file on disk
            // (this test never writes multi-gigabyte or even real GGUF
            // files at scale), so every entry is correctly `missing`,
            // not `healthy` - this assertion is itself a check that
            // audit's file-presence detection stays correct at scale.
            assert_eq!(audit_report.missing.len(), count);

            let t5 = Instant::now();
            let sanitized: Vec<LibraryEntry> = loaded
                .entries
                .iter()
                .map(crate::library::sanitize_entry_for_export)
                .collect();
            let export_path = dir.join("export.json");
            std::fs::write(
                &export_path,
                serde_json::to_string_pretty(&sanitized).unwrap(),
            )
            .unwrap();
            let export_time = t5.elapsed();

            eprintln!(
                "PERF entries={count} save={save_time:?} load={load_time:?} \
                 duplicate_grouping={duplicate_grouping_time:?} storage_summary={storage_summary_time:?} \
                 audit={audit_time:?} export={export_time:?}"
            );

            std::fs::remove_dir_all(&dir).ok();
        }
    }

    /// Real streaming SHA-256 throughput, measured against this
    /// machine's real GGUF model when one is present at the documented
    /// Stage 0+ path - never a synthetic multi-gigabyte file. Skips
    /// (does not fail) when the file isn't present, since this test
    /// suite must not depend on external fixtures being available.
    #[test]
    fn measure_real_sha256_throughput_if_the_real_model_is_present() {
        let path = PathBuf::from(r"C:\Models\qwen2.5-0.5b-instruct-q4_k_m.gguf");
        if !path.is_file() {
            eprintln!(
                "PERF sha256_throughput=skipped (real model not present at {})",
                path.display()
            );
            return;
        }

        let size_bytes = std::fs::metadata(&path).unwrap().len();
        let t0 = Instant::now();
        let _hash = crate::security::hashing::sha256_file(&path).unwrap();
        let elapsed = t0.elapsed();
        let mb_per_sec = (size_bytes as f64 / 1_000_000.0) / elapsed.as_secs_f64();

        eprintln!(
            "PERF sha256_throughput: {size_bytes} bytes in {elapsed:?} ({mb_per_sec:.1} MB/s)"
        );
    }

    #[test]
    fn measure_scan_planning_time_on_a_synthetic_directory_tree() {
        let dir = tmp_dir("scan-planning");
        for i in 0..200 {
            let sub = dir.join(format!("group-{}", i % 10));
            std::fs::create_dir_all(&sub).unwrap();
            std::fs::write(
                sub.join(format!("model-{i}.gguf")),
                b"GGUF-synthetic-fixture-bytes",
            )
            .unwrap();
        }

        let options = crate::library::scan::ScanOptions {
            recursive: true,
            ..crate::library::scan::ScanOptions::default()
        };
        let t0 = Instant::now();
        let result = crate::library::scan::scan(&dir, &options, || false).unwrap();
        let scan_time = t0.elapsed();
        assert_eq!(result.discovered.len(), 200);

        eprintln!("PERF scan_planning: 200 files across 10 directories in {scan_time:?}");

        std::fs::remove_dir_all(&dir).ok();
    }
}
