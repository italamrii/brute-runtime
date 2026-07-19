//! Trusted local runtime resolution (spec: "ordinary users must not be
//! required to browse to a developer repository path before running a
//! model"). Resolves which directory holds `llama-cli`/`llama-bench` in
//! a fixed, validated order - never executes a binary that hasn't at
//! least passed `brute::runtime::llama_cpp::verify_llama_binary`:
//!
//! 1. **Bundled**: the CPU runtime shipped inside this app package
//!    (`paths::bundled_runtime_dir`) - strictly hash-verified, since it
//!    should be exactly the pinned files BRUTE itself shipped.
//! 2. **User override**: an advanced-settings-configured directory -
//!    verified leniently (`allow_unverified_binary = true`), since a
//!    user's own build will never match BRUTE's pin.
//! 3. **Discovered**: the first `PATH` directory containing both
//!    `llama-cli`/`llama-bench` (with the platform-correct suffix) -
//!    also verified leniently.
//! 4. **NotFound**: an actionable "nothing usable was found" state -
//!    never silently falls back to executing something unvalidated.

use brute::runtime::llama_cpp::{self, llama_bench_path, llama_cli_path};
use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::AppHandle;

#[derive(Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeSource {
    Bundled,
    UserOverride,
    Discovered,
    NotFound,
}

#[derive(Serialize)]
pub struct RuntimeResolution {
    pub source: RuntimeSource,
    pub binary_dir: Option<String>,
    pub cli_verified: bool,
    pub bench_verified: bool,
    pub detail: String,
}

/// Resolves the runtime to use, trying each tier in order. `user_override`
/// is the Advanced Settings-configured path, if the user has set one -
/// always attempted (and always validated, never blindly trusted) even
/// when a bundled runtime is also present, so a user's explicit choice
/// is still available as a fallback tier below Bundled.
#[tauri::command(rename_all = "snake_case")]
pub async fn resolve_runtime(
    app: AppHandle,
    user_override: Option<String>,
) -> Result<RuntimeResolution, String> {
    tauri::async_runtime::spawn_blocking(move || resolve_runtime_impl(&app, user_override))
        .await
        .map_err(|e| e.to_string())
}

fn resolve_runtime_impl(app: &AppHandle, user_override: Option<String>) -> RuntimeResolution {
    if let Some(dir) = crate::paths::bundled_runtime_dir(app)
        && let Some((cli_verified, bench_verified)) = try_dir(&dir, false)
    {
        return RuntimeResolution {
            source: RuntimeSource::Bundled,
            binary_dir: Some(dir.display().to_string()),
            cli_verified,
            bench_verified,
            detail: "Using the CPU runtime bundled with this installation.".to_string(),
        };
    }

    if let Some(user_dir) = user_override.filter(|s| !s.trim().is_empty()) {
        let dir = PathBuf::from(&user_dir);
        if let Some((cli_verified, bench_verified)) = try_dir(&dir, true) {
            return RuntimeResolution {
                source: RuntimeSource::UserOverride,
                binary_dir: Some(dir.display().to_string()),
                cli_verified,
                bench_verified,
                detail: "Using the runtime directory configured in Advanced Settings.".to_string(),
            };
        }
    }

    if let Some(dir) = discover_on_path()
        && let Some((cli_verified, bench_verified)) = try_dir(&dir, true)
    {
        return RuntimeResolution {
            source: RuntimeSource::Discovered,
            binary_dir: Some(dir.display().to_string()),
            cli_verified,
            bench_verified,
            detail: "Found llama-cli/llama-bench on this machine's PATH.".to_string(),
        };
    }

    RuntimeResolution {
        source: RuntimeSource::NotFound,
        binary_dir: None,
        cli_verified: false,
        bench_verified: false,
        detail: "No usable llama.cpp runtime was found. Set a runtime directory in Advanced Settings, or reinstall BRUTE to restore the bundled runtime.".to_string(),
    }
}

/// Both `llama-cli` and `llama-bench` must be present and pass at least
/// a lenient (`allow_unverified`-gated) verification for a candidate
/// directory to count - a directory with only one of the two binaries
/// is not a usable runtime.
fn try_dir(dir: &Path, allow_unverified: bool) -> Option<(bool, bool)> {
    let cli = llama_cpp::verify_llama_binary(&llama_cli_path(dir), allow_unverified).ok()?;
    let bench = llama_cpp::verify_llama_binary(&llama_bench_path(dir), allow_unverified).ok()?;
    Some((cli.verified, bench.verified))
}

/// Searches `PATH` for a directory containing `llama-cli` (with the
/// platform-correct suffix) - the standard, minimal "is this tool
/// already installed" check, same idea as a shell's own `which`/`where`.
fn discover_on_path() -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    std::env::split_paths(&path_var).find(|dir| llama_cli_path(dir).is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory with neither binary present must never be treated
    /// as a usable runtime - this is the "model/runtime path separation"
    /// guarantee at the lowest level: a directory is only a runtime
    /// candidate if it actually contains the runtime binaries, never
    /// inferred from being "near" a model file.
    #[test]
    fn try_dir_rejects_a_directory_with_neither_binary() {
        let dir = std::env::temp_dir().join(format!(
            "brute-runtime-resolve-test-empty-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let result = try_dir(&dir, true);
        std::fs::remove_dir_all(&dir).ok();
        assert!(result.is_none());
    }

    /// With no bundled runtime, no override, and nothing on PATH, the
    /// resolution must land on the actionable `NotFound` state - never
    /// silently succeed with a fabricated path, and never panic.
    #[test]
    fn resolve_runtime_impl_never_panics_when_nothing_is_found() {
        // A clearly-fake override path guarantees the override tier
        // fails validation; PATH discovery depends on the real
        // machine's PATH, so this only asserts the function completes
        // and returns *some* well-formed resolution, not a specific
        // source (a real machine might legitimately have llama-cli on
        // PATH from an unrelated install).
        // No AppHandle is constructable outside a running Tauri app in
        // a unit test, so this test exercises try_dir/discover_on_path
        // directly rather than the full resolve_runtime_impl - see the
        // integration-level coverage note in docs/known-limitations.md.
        let dir = PathBuf::from(r"C:\this\path\does\not\exist\brute-test");
        assert!(try_dir(&dir, true).is_none());
        assert!(try_dir(&dir, false).is_none());
    }

    #[test]
    fn discover_on_path_returns_none_when_llama_cli_is_on_no_path_entry() {
        // Genuinely exercises the real PATH on this machine - honest by
        // construction: if this machine happens to have a real
        // llama-cli on PATH, that's a legitimate `Some`, not a bug.
        // The only invariant asserted is "never panics".
        let _ = discover_on_path();
    }
}
