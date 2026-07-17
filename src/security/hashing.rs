//! SHA-256 hashing used for GGUF file identity and llama.cpp binary
//! verification. Streams the file in fixed-size chunks - never reads a
//! whole model or binary into memory at once.

use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::Path;

const CHUNK_SIZE: usize = 1024 * 1024; // 1 MiB

/// Computes the lowercase hex SHA-256 digest of the file at `path`.
pub fn sha256_file(path: &Path) -> io::Result<String> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; CHUNK_SIZE];

    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }

    Ok(hex::encode(hasher.finalize()))
}

/// Result of checking a binary against its locally pinned hash (a sibling
/// `<file>.sha256` written by `scripts/fetch-llama-cpp.ps1` after it
/// verified the download against the manifest pinned in this repo). This
/// does not re-verify against the upstream source on every launch - it
/// verifies the binary has not changed since the trusted fetch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerificationStatus {
    Matches,
    Mismatch { expected: String },
    NoPin,
}

/// Computes `path`'s SHA-256 and compares it against the hash recorded in
/// the sibling `<path>.sha256` file, if any. Returns the computed hash
/// alongside the comparison outcome so callers can report both.
pub fn verify_binary(path: &Path) -> io::Result<(String, VerificationStatus)> {
    let actual = sha256_file(path)?;

    let pin_path = {
        let mut p = path.as_os_str().to_owned();
        p.push(".sha256");
        std::path::PathBuf::from(p)
    };

    if !pin_path.exists() {
        return Ok((actual, VerificationStatus::NoPin));
    }

    let expected = std::fs::read_to_string(&pin_path)?
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .trim()
        .to_lowercase();

    if expected == actual {
        Ok((actual, VerificationStatus::Matches))
    } else {
        Ok((actual, VerificationStatus::Mismatch { expected }))
    }
}

/// Minimal hex encoder to avoid pulling in the `hex` crate for one function.
mod hex {
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        let bytes = bytes.as_ref();
        let mut s = String::with_capacity(bytes.len() * 2);
        for b in bytes {
            s.push_str(&format!("{b:02x}"));
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn matches_known_vector() {
        // SHA-256("abc") is a standard published test vector.
        let dir = std::env::temp_dir().join(format!("brute-hash-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("abc.txt");
        File::create(&path).unwrap().write_all(b"abc").unwrap();

        let digest = sha256_file(&path).unwrap();
        assert_eq!(
            digest,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn empty_file_matches_known_vector() {
        let dir =
            std::env::temp_dir().join(format!("brute-hash-test-empty-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("empty.txt");
        File::create(&path).unwrap();

        let digest = sha256_file(&path).unwrap();
        assert_eq!(
            digest,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn verify_binary_with_no_pin_file_reports_no_pin() {
        let dir = std::env::temp_dir().join(format!("brute-verify-nopin-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("tool.exe");
        File::create(&path)
            .unwrap()
            .write_all(b"binary-content")
            .unwrap();

        let (_, status) = verify_binary(&path).unwrap();
        assert_eq!(status, VerificationStatus::NoPin);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn verify_binary_with_matching_pin_reports_matches() {
        let dir = std::env::temp_dir().join(format!("brute-verify-match-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("tool.exe");
        File::create(&path)
            .unwrap()
            .write_all(b"binary-content")
            .unwrap();
        let expected = sha256_file(&path).unwrap();
        std::fs::write(dir.join("tool.exe.sha256"), &expected).unwrap();

        let (_, status) = verify_binary(&path).unwrap();
        assert_eq!(status, VerificationStatus::Matches);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn verify_binary_with_wrong_pin_reports_mismatch() {
        let dir =
            std::env::temp_dir().join(format!("brute-verify-mismatch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("tool.exe");
        File::create(&path)
            .unwrap()
            .write_all(b"binary-content")
            .unwrap();
        std::fs::write(dir.join("tool.exe.sha256"), "0".repeat(64)).unwrap();

        let (_, status) = verify_binary(&path).unwrap();
        assert!(matches!(status, VerificationStatus::Mismatch { .. }));

        std::fs::remove_dir_all(&dir).ok();
    }
}
