//! Crate-wide error types. Every fallible operation returns one of these -
//! nothing is swallowed, and every variant carries enough context to explain
//! itself to a user without re-deriving the failure from a stack trace.

use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BruteError {
    #[error(transparent)]
    Path(#[from] PathError),

    #[error(transparent)]
    Gguf(#[from] GgufError),

    #[error(transparent)]
    Process(#[from] ProcessError),

    #[error(transparent)]
    Report(#[from] ReportError),

    #[error(transparent)]
    Catalog(#[from] CatalogError),

    #[error(transparent)]
    Library(#[from] LibraryError),

    #[error("io error at {context}: {source}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },

    #[error("{0}")]
    Usage(String),
}

#[derive(Debug, Error)]
pub enum PathError {
    #[error("path does not exist: {0}")]
    NotFound(PathBuf),

    #[error("path is not a regular file: {0}")]
    NotAFile(PathBuf),

    #[error("path could not be canonicalized: {path}: {source}")]
    CanonicalizeFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("file is empty: {0}")]
    Empty(PathBuf),
}

#[derive(Debug, Error)]
pub enum GgufError {
    #[error("not a GGUF file: expected magic 'GGUF', found {found:?}")]
    BadMagic { found: [u8; 4] },

    #[error("unsupported GGUF version: {0} (supported: 2, 3)")]
    UnsupportedVersion(u32),

    #[error("file is truncated: expected to read {expected} bytes for {context}, got {actual}")]
    Truncated {
        context: String,
        expected: usize,
        actual: usize,
    },

    #[error("malformed GGUF: {0}")]
    Malformed(String),

    #[error("hostile or corrupt count field for {field}: {value} exceeds sanity limit {limit}")]
    CountOutOfRange {
        field: String,
        value: u64,
        limit: u64,
    },

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Error)]
pub enum ProcessError {
    #[error("binary not found at {0}")]
    BinaryNotFound(PathBuf),

    #[error("binary failed verification: {path} - expected sha256 {expected}, got {actual}")]
    HashMismatch {
        path: PathBuf,
        expected: String,
        actual: String,
    },

    #[error("failed to spawn process {program}: {source}")]
    SpawnFailed {
        program: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("process timed out after {timeout_secs}s and was killed")]
    TimedOut { timeout_secs: u64 },

    #[error("process exited with non-zero status {code:?}: {stderr_tail}")]
    NonZeroExit {
        code: Option<i32>,
        stderr_tail: String,
    },

    #[error("failed to kill timed-out process: {0}")]
    KillFailed(std::io::Error),
}

#[derive(Debug, Error)]
pub enum ReportError {
    #[error("failed to serialize report: {0}")]
    Serialize(#[from] serde_json::Error),

    #[error("failed to write report to {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Catalog files are untrusted input (curated by hand today, potentially
/// imported from elsewhere later) - every variant here corresponds to a
/// concrete safety/sanity check, not just "serde failed."
#[derive(Debug, Error)]
pub enum CatalogError {
    #[error(
        "catalog file {path} is {actual_bytes} bytes, exceeding the {limit_bytes}-byte safety limit"
    )]
    FileTooLarge {
        path: PathBuf,
        actual_bytes: u64,
        limit_bytes: u64,
    },

    #[error(
        "catalog at {path} declares {actual} entries, exceeding the {limit}-entry safety limit"
    )]
    TooManyEntries {
        path: PathBuf,
        actual: usize,
        limit: usize,
    },

    #[error("failed to read catalog file {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error(
        "catalog file {path} is not valid JSON or does not match the expected schema: {source}"
    )]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    #[error("catalog entry {catalog_id:?} is invalid: {reason}")]
    InvalidEntry { catalog_id: String, reason: String },

    #[error("catalog has duplicate catalog_id {0:?}")]
    DuplicateId(String),

    #[error("no catalog entry with id {0:?}")]
    NotFound(String),
}

/// The local model library's index file is untrusted-ish input too (it's
/// local, but a crash mid-write or a hand edit could still corrupt it) -
/// every variant corresponds to a concrete failure mode, never a generic
/// "serde failed."
#[derive(Debug, Error)]
pub enum LibraryError {
    #[error("failed to read library index {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to write library index {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error(
        "library index {path} is not valid JSON or does not match the expected schema: {source}"
    )]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    #[error("no library entry with id {0:?}")]
    NotFound(String),

    #[error("library entry {0:?} is quarantined: {1}")]
    Quarantined(String, String),

    #[error(
        "relocation target does not match the original artifact's hash: expected {expected}, found {actual}"
    )]
    RelocationHashMismatch { expected: String, actual: String },

    #[error("path escapes the managed library root: {0}")]
    PathEscapesManagedRoot(PathBuf),

    #[error("{0:?} is not a managed (BRUTE-copied) library entry - nothing to remove")]
    NotManaged(String),

    #[error("scan directory does not exist or is not a directory: {0}")]
    InvalidScanRoot(PathBuf),
}

/// Errors from `conversations::` (local Chat history storage) - mirrors
/// `LibraryError`'s shape (every variant a concrete failure mode, never
/// a generic "serde failed") with messages that correctly describe a
/// conversation file rather than reusing `LibraryError`'s "library
/// index" wording.
#[derive(Debug, Error)]
pub enum ConversationError {
    #[error("failed to read conversation {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to write conversation {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error(
        "conversation {path} is not valid JSON or does not match the expected schema: {source}"
    )]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    #[error("no conversation with id {0:?}")]
    NotFound(String),
}
