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
