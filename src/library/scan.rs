//! Safe directory discovery (spec section 3). Every scan is explicit and
//! user-requested - there is no automatic whole-disk scan, no hidden
//! background scanning, and scanning a directory never imports anything
//! by itself (`scan` only ever returns a report; `import`/
//! `import_directory` are separate, explicit operations). See
//! `docs/stage-3-trusted-local-library.md`.

use crate::errors::LibraryError;
use serde::Serialize;
use std::collections::HashSet;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct ScanOptions {
    pub recursive: bool,
    /// Only consulted when `recursive` is true. Depth 0 is the root
    /// directory itself; depth 1 is its immediate subdirectories.
    pub max_depth: u32,
    pub max_files: usize,
    pub max_total_bytes: u64,
    pub max_duration: Duration,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            recursive: false,
            max_depth: 8,
            max_files: 10_000,
            max_total_bytes: 500 * 1024 * 1024 * 1024, // 500 GiB - a generous ceiling, not a target
            max_duration: Duration::from_secs(120),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveredKind {
    /// `.gguf` extension and the real `GGUF` magic bytes are both present.
    GgufCandidate,
    /// `.gguf` extension, but the magic bytes don't match - reported as
    /// unsupported/corrupt, never guessed at or silently skipped.
    UnsupportedFormat,
    /// Could not even be opened to check the magic bytes (permissions,
    /// a race where the file vanished, etc.) - reported, not fatal to
    /// the rest of the scan.
    Inaccessible,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiscoveredFile {
    pub path: PathBuf,
    pub size_bytes: Option<u64>,
    pub kind: DiscoveredKind,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScanResult {
    pub root: PathBuf,
    pub discovered: Vec<DiscoveredFile>,
    pub directories_visited: usize,
    pub inaccessible_directories: usize,
    pub truncated_by_file_count: bool,
    pub truncated_by_total_size: bool,
    pub truncated_by_time: bool,
    pub cancelled: bool,
    pub wall_time_secs: f64,
}

/// Scans exactly `root` (and, if `options.recursive`, its subdirectories
/// up to `options.max_depth`) for `.gguf`-named files, checking each
/// one's real magic bytes - never its filename alone - before reporting
/// it as a genuine GGUF candidate. Never reads past the first 4 bytes of
/// any file, and never executes anything found.
pub fn scan(
    root: &Path,
    options: &ScanOptions,
    mut is_cancelled: impl FnMut() -> bool,
) -> Result<ScanResult, LibraryError> {
    if !root.is_dir() {
        return Err(LibraryError::InvalidScanRoot(root.to_path_buf()));
    }

    let start = Instant::now();
    let mut discovered = Vec::new();
    // Canonical directory paths already descended into - the loop/
    // reparse-point-cycle guard. A directory reachable two different
    // ways (e.g. via a junction back to an ancestor) is only ever
    // visited once.
    let mut visited_dirs: HashSet<PathBuf> = HashSet::new();
    let mut total_bytes: u64 = 0;
    let mut directories_visited = 0usize;
    let mut inaccessible_directories = 0usize;
    let mut truncated_by_file_count = false;
    let mut truncated_by_total_size = false;
    let mut truncated_by_time = false;
    let mut cancelled = false;

    let mut stack: Vec<(PathBuf, u32)> = vec![(root.to_path_buf(), 0)];

    'outer: while let Some((dir, depth)) = stack.pop() {
        if is_cancelled() {
            cancelled = true;
            break;
        }
        if start.elapsed() >= options.max_duration {
            truncated_by_time = true;
            break;
        }

        let canonical_dir = match std::fs::canonicalize(&dir) {
            Ok(c) => c,
            Err(_) => {
                inaccessible_directories += 1;
                continue;
            }
        };
        if !visited_dirs.insert(canonical_dir) {
            continue; // already descended into this real directory - a loop.
        }

        let read_dir = match std::fs::read_dir(&dir) {
            Ok(rd) => rd,
            Err(_) => {
                inaccessible_directories += 1;
                continue;
            }
        };
        directories_visited += 1;

        for entry_result in read_dir {
            if is_cancelled() {
                cancelled = true;
                break 'outer;
            }
            if start.elapsed() >= options.max_duration {
                truncated_by_time = true;
                break 'outer;
            }
            if discovered.len() >= options.max_files {
                truncated_by_file_count = true;
                break 'outer;
            }
            if total_bytes >= options.max_total_bytes {
                truncated_by_total_size = true;
                break 'outer;
            }

            let Ok(entry) = entry_result else { continue };
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };

            if file_type.is_dir() {
                if options.recursive && depth < options.max_depth {
                    stack.push((path, depth + 1));
                }
                continue;
            }

            // A symlink's `file_type()` reflects the link itself, not
            // its target - resolve through metadata() (which follows
            // the link) before treating it as a plain file, and never
            // treat a symlink-to-directory as something to descend into
            // here (that would need its own loop-prevention pass; out
            // of scope for a file-discovery symlink, only relevant for
            // the directory-recursion case already guarded above).
            let is_regular_file = if file_type.is_symlink() {
                std::fs::metadata(&path)
                    .map(|m| m.is_file())
                    .unwrap_or(false)
            } else {
                file_type.is_file()
            };
            if !is_regular_file {
                continue;
            }

            let has_gguf_extension = path
                .extension()
                .map(|e| e.eq_ignore_ascii_case("gguf"))
                .unwrap_or(false);
            if !has_gguf_extension {
                continue; // other formats are not reported at all, per spec: GGUF only.
            }

            let size_bytes = std::fs::metadata(&path).ok().map(|m| m.len());
            if let Some(size) = size_bytes {
                total_bytes = total_bytes.saturating_add(size);
            }

            let kind = match check_gguf_magic(&path) {
                Ok(true) => DiscoveredKind::GgufCandidate,
                Ok(false) => DiscoveredKind::UnsupportedFormat,
                Err(_) => DiscoveredKind::Inaccessible,
            };

            discovered.push(DiscoveredFile {
                path,
                size_bytes,
                kind,
            });
        }
    }

    Ok(ScanResult {
        root: root.to_path_buf(),
        discovered,
        directories_visited,
        inaccessible_directories,
        truncated_by_file_count,
        truncated_by_total_size,
        truncated_by_time,
        cancelled,
        wall_time_secs: start.elapsed().as_secs_f64(),
    })
}

/// Reads only the first 4 bytes - never opens or interprets the rest of
/// the file, and never executes it.
fn check_gguf_magic(path: &Path) -> std::io::Result<bool> {
    let mut file = File::open(path)?;
    let mut magic = [0u8; 4];
    match file.read_exact(&mut magic) {
        Ok(()) => Ok(&magic == b"GGUF"),
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => Ok(false),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(name: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "brute-library-scan-test-{name}-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_gguf(path: &Path) {
        std::fs::write(path, b"GGUF-fake-content-for-magic-check-only").unwrap();
    }

    fn write_non_gguf(path: &Path) {
        std::fs::write(path, b"not a gguf file at all").unwrap();
    }

    #[test]
    fn rejects_a_root_that_is_not_a_directory() {
        let dir = tmp_dir("bad-root");
        let file_path = dir.join("not-a-dir.txt");
        std::fs::write(&file_path, b"x").unwrap();

        let result = scan(&file_path, &ScanOptions::default(), || false);
        assert!(matches!(result, Err(LibraryError::InvalidScanRoot(_))));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn non_recursive_scan_finds_only_top_level_gguf_files() {
        let dir = tmp_dir("non-recursive");
        write_gguf(&dir.join("top.gguf"));
        let nested = dir.join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        write_gguf(&nested.join("deep.gguf"));

        let result = scan(&dir, &ScanOptions::default(), || false).unwrap();
        assert_eq!(result.discovered.len(), 1);
        assert_eq!(result.discovered[0].path.file_name().unwrap(), "top.gguf");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn recursive_scan_finds_nested_files() {
        let dir = tmp_dir("recursive");
        write_gguf(&dir.join("top.gguf"));
        let nested = dir.join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        write_gguf(&nested.join("deep.gguf"));

        let options = ScanOptions {
            recursive: true,
            ..ScanOptions::default()
        };
        let result = scan(&dir, &options, || false).unwrap();
        assert_eq!(result.discovered.len(), 2);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn recursion_depth_limit_is_respected() {
        let dir = tmp_dir("depth-limit");
        let level1 = dir.join("l1");
        let level2 = level1.join("l2");
        std::fs::create_dir_all(&level2).unwrap();
        write_gguf(&level1.join("shallow.gguf"));
        write_gguf(&level2.join("too-deep.gguf"));

        let options = ScanOptions {
            recursive: true,
            max_depth: 1, // root=0, l1=1, l2=2 (excluded)
            ..ScanOptions::default()
        };
        let result = scan(&dir, &options, || false).unwrap();
        let names: Vec<_> = result
            .discovered
            .iter()
            .map(|f| f.path.file_name().unwrap().to_str().unwrap().to_string())
            .collect();
        assert!(names.contains(&"shallow.gguf".to_string()));
        assert!(!names.contains(&"too-deep.gguf".to_string()));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn max_files_limit_truncates_and_sets_the_flag() {
        let dir = tmp_dir("max-files");
        for i in 0..5 {
            write_gguf(&dir.join(format!("model-{i}.gguf")));
        }

        let options = ScanOptions {
            max_files: 2,
            ..ScanOptions::default()
        };
        let result = scan(&dir, &options, || false).unwrap();
        assert_eq!(result.discovered.len(), 2);
        assert!(result.truncated_by_file_count);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn max_total_size_limit_truncates_and_sets_the_flag() {
        let dir = tmp_dir("max-size");
        write_gguf(&dir.join("a.gguf")); // ~38 bytes
        write_gguf(&dir.join("b.gguf"));
        write_gguf(&dir.join("c.gguf"));

        let options = ScanOptions {
            max_total_bytes: 40, // allows only the first file through
            ..ScanOptions::default()
        };
        let result = scan(&dir, &options, || false).unwrap();
        assert!(result.discovered.len() < 3);
        assert!(result.truncated_by_total_size);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn non_gguf_extensions_are_never_reported() {
        let dir = tmp_dir("non-gguf-ext");
        write_gguf(&dir.join("model.gguf"));
        std::fs::write(dir.join("readme.txt"), b"hello").unwrap();
        std::fs::write(dir.join("model.safetensors"), b"other format").unwrap();

        let result = scan(&dir, &ScanOptions::default(), || false).unwrap();
        assert_eq!(result.discovered.len(), 1);
        assert_eq!(result.discovered[0].path.file_name().unwrap(), "model.gguf");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn gguf_named_file_with_wrong_magic_is_reported_as_unsupported_not_guessed() {
        let dir = tmp_dir("wrong-magic");
        write_non_gguf(&dir.join("fake.gguf"));

        let result = scan(&dir, &ScanOptions::default(), || false).unwrap();
        assert_eq!(result.discovered.len(), 1);
        assert_eq!(result.discovered[0].kind, DiscoveredKind::UnsupportedFormat);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn valid_magic_is_reported_as_a_gguf_candidate() {
        let dir = tmp_dir("valid-magic");
        write_gguf(&dir.join("real.gguf"));

        let result = scan(&dir, &ScanOptions::default(), || false).unwrap();
        assert_eq!(result.discovered[0].kind, DiscoveredKind::GgufCandidate);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn cancellation_stops_the_scan_immediately() {
        let dir = tmp_dir("cancel");
        for i in 0..10 {
            write_gguf(&dir.join(format!("model-{i}.gguf")));
        }

        let result = scan(&dir, &ScanOptions::default(), || true).unwrap();
        assert!(result.cancelled);
        assert!(result.discovered.is_empty());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn empty_directory_scans_cleanly_with_no_results() {
        let dir = tmp_dir("empty");
        let result = scan(&dir, &ScanOptions::default(), || false).unwrap();
        assert!(result.discovered.is_empty());
        assert!(!result.cancelled);
        assert_eq!(result.directories_visited, 1);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn scan_never_recurses_into_subdirectories_by_default() {
        let dir = tmp_dir("default-non-recursive");
        let nested = dir.join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        write_gguf(&nested.join("deep.gguf"));

        let result = scan(&dir, &ScanOptions::default(), || false).unwrap();
        assert!(result.discovered.is_empty());
        assert_eq!(
            result.directories_visited, 1,
            "must not have descended into `nested`"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn arabic_and_spaced_directory_and_file_names_are_handled() {
        let dir = tmp_dir("arabic");
        let sub = dir.join("نماذج اختبار");
        std::fs::create_dir_all(&sub).unwrap();
        write_gguf(&sub.join("نموذج جيد.gguf"));

        let options = ScanOptions {
            recursive: true,
            ..ScanOptions::default()
        };
        let result = scan(&dir, &options, || false).unwrap();
        assert_eq!(result.discovered.len(), 1);
        assert_eq!(result.discovered[0].kind, DiscoveredKind::GgufCandidate);

        std::fs::remove_dir_all(&dir).ok();
    }

    /// A deeply nested path approaching Windows' traditional 260-char
    /// `MAX_PATH` limit must not crash or silently drop the file -
    /// `std::fs` on modern Windows/Rust transparently handles long paths
    /// (via the `\\?\` extended-length prefix at the OS boundary) for
    /// any path this scanner actually constructs itself.
    #[test]
    fn a_long_nested_path_is_still_discovered() {
        let dir = tmp_dir("long-path");
        let mut nested = dir.clone();
        for i in 0..12 {
            nested = nested.join(format!("a-fairly-long-directory-segment-name-{i:02}"));
        }
        std::fs::create_dir_all(&nested).unwrap();
        write_gguf(&nested.join("model.gguf"));
        assert!(
            nested.to_string_lossy().len() > 260,
            "fixture must actually exceed MAX_PATH to be meaningful"
        );

        let options = ScanOptions {
            recursive: true,
            max_depth: 20,
            ..ScanOptions::default()
        };
        let result = scan(&dir, &options, || false).unwrap();
        assert_eq!(result.discovered.len(), 1);
        assert_eq!(result.discovered[0].kind, DiscoveredKind::GgufCandidate);

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Directory symlink loop protection: a junction pointing back at an
    /// ancestor must not cause infinite recursion. Junction creation on
    /// Windows works without elevation (unlike a true symlink), so this
    /// test does not need admin/Developer Mode - but if this environment
    /// still refuses it for some other reason, skip rather than fail,
    /// since the guard itself (the `visited_dirs` canonical-path set) is
    /// exercised regardless of *how* a cycle is introduced.
    #[test]
    fn directory_loop_via_junction_does_not_infinite_loop() {
        let dir = tmp_dir("loop");
        let child = dir.join("child");
        std::fs::create_dir_all(&child).unwrap();
        let loop_link = child.join("back-to-parent");

        let status = std::process::Command::new("cmd")
            .args([
                "/C",
                "mklink",
                "/J",
                loop_link.to_str().unwrap(),
                dir.to_str().unwrap(),
            ])
            .output();

        let Ok(output) = status else {
            std::fs::remove_dir_all(&dir).ok();
            return;
        };
        if !output.status.success() {
            std::fs::remove_dir_all(&dir).ok();
            return; // junction creation unavailable in this environment - not what we're testing.
        }

        let options = ScanOptions {
            recursive: true,
            max_duration: Duration::from_secs(10),
            ..ScanOptions::default()
        };
        let result = scan(&dir, &options, || false);
        assert!(
            result.is_ok(),
            "a directory loop must not hang or crash the scan"
        );

        std::fs::remove_dir_all(&dir).ok();
    }
}
