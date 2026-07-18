//! Path validation shared by model import and llama.cpp binary launch.
//!
//! Windows paths need care beyond "does it exist": UNC/device paths, reparse
//! points (symlinks/junctions) that can redirect outside an intended
//! directory, and non-ASCII (e.g. Arabic) components that naive byte-slicing
//! can mangle. We canonicalize and stat via `std::fs`, which resolves
//! reparse points to their real target, then check the *resolved* path.

use crate::errors::PathError;
use std::fs;
use std::path::{Path, PathBuf};

/// Canonicalizes `path` and verifies it points at a regular, non-empty file.
///
/// Returns the canonicalized path on success. This is deliberately strict:
/// Stage 0 never operates on directories, device files, or zero-byte paths,
/// since a GGUF file and a llama.cpp binary can never legitimately be either.
pub fn validate_regular_file(path: &Path) -> Result<PathBuf, PathError> {
    if !path.exists() {
        return Err(PathError::NotFound(path.to_path_buf()));
    }

    let canonical = fs::canonicalize(path).map_err(|source| PathError::CanonicalizeFailed {
        path: path.to_path_buf(),
        source,
    })?;

    let metadata = fs::metadata(&canonical).map_err(|source| PathError::CanonicalizeFailed {
        path: canonical.clone(),
        source,
    })?;

    if !metadata.is_file() {
        return Err(PathError::NotAFile(canonical));
    }

    if metadata.len() == 0 {
        return Err(PathError::Empty(canonical));
    }

    Ok(canonical)
}

/// Renders a path for a *shareable* report (JSON exports, anything meant
/// to leave this machine) with the Windows user profile directory name
/// replaced - `C:\Users\Alice\Downloads\model.gguf` becomes
/// `C:\Users\<redacted>\Downloads\model.gguf`. Every other path component
/// (drive, folder structure, filename) is preserved, since those are
/// useful for debugging and not personally identifying on their own.
///
/// This is a display-time transform only - never applied to paths used
/// for actual file operations, which always use the real, unredacted path.
pub fn redact_username_for_report(path: &Path) -> String {
    let text = path.display().to_string();
    let mut parts: Vec<String> = text.split('\\').map(str::to_string).collect();

    for i in 0..parts.len() {
        if parts[i].eq_ignore_ascii_case("Users") && i + 1 < parts.len() && !parts[i + 1].is_empty()
        {
            parts[i + 1] = "<redacted>".to_string();
        }
    }

    parts.join("\\")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn accepts_a_normal_file() {
        let dir = tempfile_dir();
        let path = dir.join("model.gguf");
        fs::File::create(&path).unwrap().write_all(b"data").unwrap();

        let result = validate_regular_file(&path);
        assert!(result.is_ok());

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn accepts_paths_with_spaces_and_arabic() {
        let dir = tempfile_dir();
        let path = dir.join("نموذج اختبار model.gguf");
        fs::File::create(&path).unwrap().write_all(b"data").unwrap();

        let result = validate_regular_file(&path);
        assert!(result.is_ok(), "expected ok, got {result:?}");

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_missing_file() {
        let dir = tempfile_dir();
        let path = dir.join("does-not-exist.gguf");
        let result = validate_regular_file(&path);
        assert!(matches!(result, Err(PathError::NotFound(_))));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_directory() {
        let dir = tempfile_dir();
        let result = validate_regular_file(&dir);
        assert!(matches!(result, Err(PathError::NotAFile(_))));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_empty_file() {
        let dir = tempfile_dir();
        let path = dir.join("empty.gguf");
        fs::File::create(&path).unwrap();
        let result = validate_regular_file(&path);
        assert!(matches!(result, Err(PathError::Empty(_))));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn redact_username_replaces_only_the_profile_directory_name() {
        let redacted =
            redact_username_for_report(Path::new(r"C:\Users\Alice\Downloads\model.gguf"));
        assert_eq!(redacted, r"C:\Users\<redacted>\Downloads\model.gguf");
    }

    #[test]
    fn redact_username_handles_arabic_usernames_too() {
        let redacted = redact_username_for_report(Path::new(r"C:\Users\أحمد\Downloads\model.gguf"));
        assert_eq!(redacted, r"C:\Users\<redacted>\Downloads\model.gguf");
    }

    #[test]
    fn redact_username_leaves_non_profile_paths_unchanged() {
        let redacted =
            redact_username_for_report(Path::new(r"C:\Models\qwen2.5-0.5b-instruct-q4_k_m.gguf"));
        assert_eq!(redacted, r"C:\Models\qwen2.5-0.5b-instruct-q4_k_m.gguf");
    }

    #[test]
    fn redact_username_handles_extended_length_prefix() {
        // std::fs::canonicalize on Windows returns \\?\C:\Users\... paths.
        let redacted = redact_username_for_report(Path::new(r"\\?\C:\Users\Bob\model.gguf"));
        assert_eq!(redacted, r"\\?\C:\Users\<redacted>\model.gguf");
    }

    fn tempfile_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "brute-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }
}
