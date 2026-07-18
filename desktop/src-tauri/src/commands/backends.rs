//! Backend verification commands - real end-to-end confirmation that a
//! backend (CPU/CUDA/Vulkan) actually works with the pinned llama.cpp
//! binaries, never mere driver/library presence. See
//! `docs/tauri-security-boundary.md` for why `model`/`llama_bin` are
//! re-validated by the underlying engine call regardless of where the
//! path string came from.

use brute::backends::{self, BackendVerification};
use brute::runtime::Backend;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Verifies one backend (or, when `backend` is `None`, CPU + CUDA +
/// Vulkan) against a real model file and the given llama.cpp binary
/// directory. Never reports a backend verified on driver/library
/// presence alone - only a real short benchmark that actually reports
/// GPU use counts.
#[tauri::command(rename_all = "snake_case")]
pub fn backends_verify(
    model: String,
    llama_bin: String,
    backend: Option<Backend>,
    allow_unverified_binary: bool,
    timeout_secs: u64,
) -> Result<Vec<BackendVerification>, String> {
    let model_path = PathBuf::from(&model);
    let llama_bin_path = PathBuf::from(&llama_bin);
    let hw = brute::hardware::inspect(model_path.parent());
    let profile = brute::profile::build_profile(&hw, now_rfc3339(), 0);

    let to_check: Vec<Backend> = backend
        .map(|b| vec![b])
        .unwrap_or_else(|| vec![Backend::Cpu, Backend::Cuda, Backend::Vulkan]);
    let timeout = Duration::from_secs(timeout_secs);

    Ok(to_check
        .into_iter()
        .map(|b| {
            backends::verify_backend(
                b,
                Some(Path::new(&llama_bin_path)),
                &model_path,
                &profile,
                allow_unverified_binary,
                timeout,
            )
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A missing llama.cpp binary directory and a missing model must
    /// degrade to an honest failed/unavailable status for every backend -
    /// never panic, and never spawn an arbitrary process from an
    /// unvalidated path.
    #[test]
    fn backends_verify_degrades_gracefully_with_no_real_binary_or_model() {
        let result = backends_verify(
            "C:\\this\\model\\does\\not\\exist.gguf".to_string(),
            "C:\\this\\binary\\dir\\does\\not\\exist".to_string(),
            None,
            true,
            5,
        );

        let verifications = result.expect("verification always returns a report, never an error");
        assert_eq!(
            verifications.len(),
            3,
            "CPU, CUDA, and Vulkan should all be checked"
        );
        for v in &verifications {
            assert_ne!(v.status, brute::backends::BackendStatus::Verified);
        }
    }

    #[test]
    fn backends_verify_checks_only_the_requested_backend_when_one_is_pinned() {
        let result = backends_verify(
            "C:\\this\\model\\does\\not\\exist.gguf".to_string(),
            "C:\\this\\binary\\dir\\does\\not\\exist".to_string(),
            Some(Backend::Cpu),
            true,
            5,
        );

        let verifications = result.unwrap();
        assert_eq!(verifications.len(), 1);
        assert_eq!(verifications[0].backend, Backend::Cpu);
    }
}
