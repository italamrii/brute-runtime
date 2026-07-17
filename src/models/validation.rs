//! Model path validation. Delegates the actual filesystem safety checks to
//! `security::paths` (shared with the llama.cpp binary launch path) so
//! there is exactly one place that decides what counts as a safe path.

use crate::errors::PathError;
use crate::security;
use std::path::{Path, PathBuf};

pub fn validate_model_path(path: &Path) -> Result<PathBuf, PathError> {
    security::validate_regular_file(path)
}
