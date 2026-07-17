//! Controlled integration with llama.cpp's official prebuilt binaries.
//!
//! We do not build or vendor llama.cpp. `brute` launches two of its
//! command-line tools, found inside a directory the user points us at
//! (typically populated by `scripts/fetch-llama-cpp.ps1`, which pins and
//! verifies the exact release - see `scripts/llama-cpp-manifest.json`):
//!
//! - `llama-bench.exe`: structured (`-o json`) throughput benchmarking with
//!   built-in repetition and variance, used for prompt-processing and
//!   generation token/sec measurements.
//! - `llama-cli.exe`: used for a one-shot run to capture model load time
//!   from its own self-reported perf log, and as a no-model `--version`
//!   smoke test of process launch + binary verification.
//!
//! Every argv is built as a `Vec<String>` and passed straight to
//! `std::process::Command` (via `runtime::process::run`) - never through a
//! shell - so a model path or prompt can never be interpreted as shell
//! syntax.

use super::{Backend, RuntimeConfig};
use crate::errors::ProcessError;
use crate::security::{self, VerificationStatus};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, Serialize)]
pub struct BinaryCheck {
    pub path: PathBuf,
    pub sha256: String,
    pub verified: bool,
    pub detail: String,
}

/// Verifies `binary_path` against its local pin (see
/// `security::verify_binary`). A hash **mismatch** is always a hard error -
/// that indicates the file changed after a trusted fetch. A **missing pin**
/// is only allowed through when `allow_unverified` is set, since Stage 0 may
/// be pointed at a binary the user built or obtained themselves.
pub fn verify_llama_binary(
    binary_path: &Path,
    allow_unverified: bool,
) -> Result<BinaryCheck, ProcessError> {
    if !binary_path.is_file() {
        return Err(ProcessError::BinaryNotFound(binary_path.to_path_buf()));
    }

    let (sha256, status) =
        security::verify_binary(binary_path).map_err(|source| ProcessError::SpawnFailed {
            program: binary_path.to_path_buf(),
            source,
        })?;

    match status {
        VerificationStatus::Matches => Ok(BinaryCheck {
            path: binary_path.to_path_buf(),
            sha256,
            verified: true,
            detail: "sha256 matches local pin written after verified fetch".to_string(),
        }),
        VerificationStatus::Mismatch { expected } => Err(ProcessError::HashMismatch {
            path: binary_path.to_path_buf(),
            expected,
            actual: sha256,
        }),
        VerificationStatus::NoPin if allow_unverified => Ok(BinaryCheck {
            path: binary_path.to_path_buf(),
            sha256,
            verified: false,
            detail:
                "no local pin found; running unverified because --allow-unverified-binary was set"
                    .to_string(),
        }),
        VerificationStatus::NoPin => Err(ProcessError::SpawnFailed {
            program: binary_path.to_path_buf(),
            source: std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "no local .sha256 pin for this binary; run scripts/fetch-llama-cpp.ps1 or pass --allow-unverified-binary",
            ),
        }),
    }
}

pub fn llama_bench_path(binary_dir: &Path) -> PathBuf {
    binary_dir.join("llama-bench.exe")
}

pub fn llama_cli_path(binary_dir: &Path) -> PathBuf {
    binary_dir.join("llama-cli.exe")
}

fn backend_gpu_layers(backend: Backend, requested: u32) -> u32 {
    match backend {
        Backend::Cpu => 0,
        Backend::Cuda | Backend::Vulkan => requested,
    }
}

fn build_bench_args(config: &RuntimeConfig, model: &Path) -> Vec<String> {
    vec![
        "-m".to_string(),
        model.display().to_string(),
        "-p".to_string(),
        config.prompt_tokens.to_string(),
        "-n".to_string(),
        config.gen_tokens.to_string(),
        "-t".to_string(),
        config.threads.to_string(),
        "-b".to_string(),
        config.batch_size.to_string(),
        "-ngl".to_string(),
        backend_gpu_layers(config.backend, config.gpu_layers).to_string(),
        "-r".to_string(),
        config.repetitions.to_string(),
        "-o".to_string(),
        "json".to_string(),
    ]
}

fn build_cli_args(config: &RuntimeConfig, model: &Path, prompt: &str) -> Vec<String> {
    vec![
        "-m".to_string(),
        model.display().to_string(),
        "-p".to_string(),
        prompt.to_string(),
        "-n".to_string(),
        config.gen_tokens.to_string(),
        "-t".to_string(),
        config.threads.to_string(),
        "-c".to_string(),
        config.context_size.to_string(),
        "-b".to_string(),
        config.batch_size.to_string(),
        "-ngl".to_string(),
        backend_gpu_layers(config.backend, config.gpu_layers).to_string(),
        "-no-cnv".to_string(),
        "--no-display-prompt".to_string(),
        "--simple-io".to_string(),
    ]
}

#[derive(Debug, Clone, Serialize)]
pub struct BenchRow {
    pub test: String, // "pp" or "tg", derived from which of n_prompt/n_gen is nonzero
    pub n_prompt: u64,
    pub n_gen: u64,
    pub avg_ts: f64,
    pub stddev_ts: f64,
    pub samples_ts: Vec<f64>,
    pub model_n_params: Option<u64>,
    pub model_size_bytes: Option<u64>,
    pub cpu_info: Option<String>,
    pub gpu_info: Option<String>,
    pub n_threads: Option<u64>,
    pub n_gpu_layers: Option<i64>,
}

/// Runs `llama-bench` once with both `-p` and `-n` set, which produces two
/// rows in its JSON output: a prompt-processing-only test and a
/// generation-only test, each already averaged over `-r` repetitions.
pub fn run_bench(
    binary_dir: &Path,
    model: &Path,
    config: &RuntimeConfig,
    allow_unverified: bool,
    on_tick: impl FnMut(&std::process::Child),
) -> Result<(Vec<BenchRow>, super::process::ProcessRun), ProcessError> {
    let bin = llama_bench_path(binary_dir);
    verify_llama_binary(&bin, allow_unverified)?;

    let args = build_bench_args(config, model);
    let run = super::process::run(&bin, &args, config.timeout(), on_tick)?;

    if run.timed_out {
        return Err(ProcessError::TimedOut {
            timeout_secs: config.timeout_secs,
        });
    }
    if !run.succeeded() {
        return Err(ProcessError::NonZeroExit {
            code: run.exit_code,
            stderr_tail: tail(&run.stderr, 2000),
        });
    }

    let rows = parse_bench_json(&run.stdout).map_err(|detail| ProcessError::NonZeroExit {
        code: run.exit_code,
        stderr_tail: format!("failed to parse llama-bench JSON output: {detail}"),
    })?;

    Ok((rows, run))
}

fn parse_bench_json(stdout: &str) -> Result<Vec<BenchRow>, String> {
    let value: serde_json::Value =
        serde_json::from_str(stdout.trim()).map_err(|e| e.to_string())?;
    let array = value.as_array().ok_or("expected a JSON array")?;

    let mut rows = Vec::new();
    for entry in array {
        let n_prompt = entry.get("n_prompt").and_then(|v| v.as_u64()).unwrap_or(0);
        let n_gen = entry.get("n_gen").and_then(|v| v.as_u64()).unwrap_or(0);
        let test = if n_gen == 0 { "pp" } else { "tg" }.to_string();

        rows.push(BenchRow {
            test,
            n_prompt,
            n_gen,
            avg_ts: entry.get("avg_ts").and_then(|v| v.as_f64()).unwrap_or(0.0),
            stddev_ts: entry
                .get("stddev_ts")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0),
            samples_ts: entry
                .get("samples_ts")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_f64()).collect())
                .unwrap_or_default(),
            model_n_params: entry.get("model_n_params").and_then(|v| v.as_u64()),
            model_size_bytes: entry.get("model_size").and_then(|v| v.as_u64()),
            cpu_info: entry
                .get("cpu_info")
                .and_then(|v| v.as_str())
                .map(String::from),
            gpu_info: entry
                .get("gpu_info")
                .and_then(|v| v.as_str())
                .map(String::from),
            n_threads: entry.get("n_threads").and_then(|v| v.as_u64()),
            n_gpu_layers: entry.get("n_gpu_layers").and_then(|v| v.as_i64()),
        });
    }
    Ok(rows)
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct CliPerfMetrics {
    pub load_time_ms: Option<f64>,
    pub prompt_eval_ms: Option<f64>,
    pub prompt_eval_tokens: Option<u64>,
    pub prompt_eval_tokens_per_second: Option<f64>,
    pub eval_ms: Option<f64>,
    pub eval_tokens: Option<u64>,
    pub eval_tokens_per_second: Option<f64>,
}

/// Runs `llama-cli` once to capture its self-reported `llama_perf_context_print`
/// timing block from stderr - this is the source for model load time, which
/// `llama-bench` does not report.
pub fn run_cli_once(
    binary_dir: &Path,
    model: &Path,
    config: &RuntimeConfig,
    prompt: &str,
    allow_unverified: bool,
    on_tick: impl FnMut(&std::process::Child),
) -> Result<(CliPerfMetrics, super::process::ProcessRun), ProcessError> {
    let bin = llama_cli_path(binary_dir);
    verify_llama_binary(&bin, allow_unverified)?;

    let args = build_cli_args(config, model, prompt);
    let run = super::process::run(&bin, &args, config.timeout(), on_tick)?;

    if run.timed_out {
        return Err(ProcessError::TimedOut {
            timeout_secs: config.timeout_secs,
        });
    }
    if !run.succeeded() {
        return Err(ProcessError::NonZeroExit {
            code: run.exit_code,
            stderr_tail: tail(&run.stderr, 2000),
        });
    }

    let metrics = parse_cli_perf(&run.stderr);
    Ok((metrics, run))
}

/// Runs `llama-cli --version` with no model - a pure process-launch and
/// binary-verification smoke test.
///
/// Timeout is generous (60s) because the *first ever* execution of a
/// freshly downloaded, unrecognized .exe on Windows can be held up for
/// several seconds to tens of seconds by Windows Defender/SmartScreen
/// scanning it before it's allowed to run - a one-time cost per binary, not
/// a llama.cpp or `brute` performance issue. Observed directly during
/// Stage 0 verification: the first `doctor` run against a newly fetched
/// binary took >15s, subsequent runs completed in well under a second.
pub fn run_cli_version(
    binary_dir: &Path,
    allow_unverified: bool,
) -> Result<super::process::ProcessRun, ProcessError> {
    let bin = llama_cli_path(binary_dir);
    verify_llama_binary(&bin, allow_unverified)?;
    super::process::run(
        &bin,
        &["--version".to_string()],
        Duration::from_secs(60),
        |_| {},
    )
}

/// Parses lines like:
/// `llama_perf_context_print:        load time =    123.45 ms`
/// `llama_perf_context_print: prompt eval time =    12.34 ms /    8 tokens (    1.54 ms per token,   648.30 tokens per second)`
/// `llama_perf_context_print:        eval time =    56.78 ms /   32 runs   (    1.77 ms per token,   563.58 tokens per second)`
fn parse_cli_perf(stderr: &str) -> CliPerfMetrics {
    let mut m = CliPerfMetrics::default();

    for line in stderr.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("llama_perf_context_print:") {
            let rest = rest.trim();
            if let Some(v) = rest.strip_prefix("load time =") {
                m.load_time_ms = first_number(v);
            } else if let Some(v) = rest.strip_prefix("prompt eval time =") {
                m.prompt_eval_ms = first_number(v);
                m.prompt_eval_tokens = nth_integer_before(v, "tokens");
                m.prompt_eval_tokens_per_second = number_before(v, "tokens per second");
            } else if let Some(v) = rest.strip_prefix("eval time =") {
                m.eval_ms = first_number(v);
                m.eval_tokens = nth_integer_before(v, "runs");
                m.eval_tokens_per_second = number_before(v, "tokens per second");
            }
        }
    }

    m
}

fn first_number(s: &str) -> Option<f64> {
    s.split_whitespace()
        .find_map(|tok| tok.trim_end_matches("ms").parse::<f64>().ok())
}

fn nth_integer_before(s: &str, marker: &str) -> Option<u64> {
    let tokens: Vec<&str> = s.split_whitespace().collect();
    let idx = tokens.iter().position(|t| *t == marker)?;
    tokens.get(idx.wrapping_sub(1))?.parse().ok()
}

fn number_before(s: &str, marker_phrase: &str) -> Option<f64> {
    let idx = s.find(marker_phrase)?;
    let prefix = &s[..idx];
    prefix
        .split(['(', ','])
        .next_back()
        .and_then(|tok| tok.trim().parse::<f64>().ok())
}

fn tail(s: &str, max_chars: usize) -> String {
    if s.len() <= max_chars {
        s.to_string()
    } else {
        let start = s.len() - max_chars;
        format!("...{}", &s[start..])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config(backend: Backend) -> RuntimeConfig {
        RuntimeConfig {
            backend,
            binary_dir: PathBuf::from(r"C:\tools\llama.cpp"),
            threads: 8,
            gpu_layers: 20,
            context_size: 2048,
            batch_size: 512,
            prompt_tokens: 512,
            gen_tokens: 128,
            repetitions: 3,
            timeout_secs: 120,
        }
    }

    #[test]
    fn build_bench_args_puts_model_path_as_a_single_argv_element() {
        // A path containing shell metacharacters must survive as one
        // literal argv element, never be split or reinterpreted - this is
        // what actually prevents command injection (argv-based Command,
        // never a shell string).
        let model = Path::new(r"C:\models\evil; rm -rf / && calc.exe.gguf");
        let args = build_bench_args(&test_config(Backend::Cpu), model);

        let m_idx = args.iter().position(|a| a == "-m").unwrap();
        assert_eq!(args[m_idx + 1], model.display().to_string());
        // The hostile fragment must appear as exactly one element, not be
        // split into multiple argv entries.
        assert!(args.iter().any(|a| a.contains("rm -rf / && calc.exe")));
        assert_eq!(
            args.iter().filter(|a| a.contains("rm -rf")).count(),
            1,
            "hostile fragment must not be split across multiple argv entries"
        );
    }

    #[test]
    fn build_bench_args_preserves_spaces_and_arabic_paths_verbatim() {
        let model = Path::new(r"C:\models\نموذج اختبار model.gguf");
        let args = build_bench_args(&test_config(Backend::Cpu), model);
        let m_idx = args.iter().position(|a| a == "-m").unwrap();
        assert_eq!(args[m_idx + 1], model.display().to_string());
    }

    #[test]
    fn build_bench_args_forces_zero_gpu_layers_on_cpu_backend() {
        let model = Path::new(r"C:\models\test.gguf");
        let args = build_bench_args(&test_config(Backend::Cpu), model);
        let ngl_idx = args.iter().position(|a| a == "-ngl").unwrap();
        assert_eq!(args[ngl_idx + 1], "0");
    }

    #[test]
    fn build_bench_args_passes_through_requested_gpu_layers_on_cuda_backend() {
        let model = Path::new(r"C:\models\test.gguf");
        let args = build_bench_args(&test_config(Backend::Cuda), model);
        let ngl_idx = args.iter().position(|a| a == "-ngl").unwrap();
        assert_eq!(args[ngl_idx + 1], "20");
    }

    #[test]
    fn build_bench_args_includes_json_output_and_repetitions() {
        let model = Path::new(r"C:\models\test.gguf");
        let args = build_bench_args(&test_config(Backend::Cpu), model);
        assert!(args.windows(2).any(|w| w == ["-o", "json"]));
        assert!(args.windows(2).any(|w| w == ["-r", "3"]));
    }

    #[test]
    fn build_cli_args_never_invokes_conversation_mode() {
        let model = Path::new(r"C:\models\test.gguf");
        let args = build_cli_args(&test_config(Backend::Cpu), model, "hello");
        assert!(args.iter().any(|a| a == "-no-cnv"));
    }

    #[test]
    fn parses_llama_bench_json_pp_and_tg_rows() {
        let json = r#"[
            {"n_prompt": 512, "n_gen": 0, "avg_ts": 120.5, "stddev_ts": 1.2, "samples_ts": [119.0, 121.0, 121.5], "model_n_params": 7000000000, "model_size": 4000000000, "cpu_info": "test-cpu", "gpu_info": "", "n_threads": 8, "n_gpu_layers": 0},
            {"n_prompt": 0, "n_gen": 128, "avg_ts": 30.2, "stddev_ts": 0.5, "samples_ts": [30.0, 30.4], "model_n_params": 7000000000, "model_size": 4000000000, "cpu_info": "test-cpu", "gpu_info": "", "n_threads": 8, "n_gpu_layers": 0}
        ]"#;

        let rows = parse_bench_json(json).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].test, "pp");
        assert_eq!(rows[0].n_prompt, 512);
        assert_eq!(rows[1].test, "tg");
        assert_eq!(rows[1].n_gen, 128);
        assert_eq!(rows[1].samples_ts.len(), 2);
    }

    #[test]
    fn rejects_non_json_output_clearly() {
        let result = parse_bench_json("not json at all");
        assert!(result.is_err());
    }

    #[test]
    fn parses_cli_perf_lines() {
        let stderr = "\
some other log line
llama_perf_context_print:        load time =    987.65 ms
llama_perf_context_print: prompt eval time =    12.34 ms /     8 tokens (    1.54 ms per token,   648.30 tokens per second)
llama_perf_context_print:        eval time =    56.78 ms /    32 runs   (    1.77 ms per token,   563.58 tokens per second)
llama_perf_context_print:       total time =  1056.77 ms /    40 tokens
";
        let m = parse_cli_perf(stderr);
        assert_eq!(m.load_time_ms, Some(987.65));
        assert_eq!(m.prompt_eval_ms, Some(12.34));
        assert_eq!(m.prompt_eval_tokens, Some(8));
        assert_eq!(m.prompt_eval_tokens_per_second, Some(648.30));
        assert_eq!(m.eval_ms, Some(56.78));
        assert_eq!(m.eval_tokens, Some(32));
        assert_eq!(m.eval_tokens_per_second, Some(563.58));
    }

    #[test]
    fn parses_cli_perf_gracefully_when_absent() {
        let m = parse_cli_perf("no perf lines here at all\njust some other output\n");
        assert_eq!(m.load_time_ms, None);
        assert_eq!(m.eval_tokens_per_second, None);
    }

    #[test]
    fn verify_llama_binary_missing_file_is_clear_error() {
        let result = verify_llama_binary(Path::new(r"C:\nonexistent\llama-bench.exe"), true);
        assert!(matches!(result, Err(ProcessError::BinaryNotFound(_))));
    }

    #[test]
    fn verify_llama_binary_without_pin_requires_explicit_allow() {
        let dir = std::env::temp_dir().join(format!("brute-llamabin-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let bin = dir.join("llama-bench.exe");
        std::fs::write(&bin, b"not a real binary, just test bytes").unwrap();

        let denied = verify_llama_binary(&bin, false);
        assert!(denied.is_err());

        let allowed = verify_llama_binary(&bin, true).unwrap();
        assert!(!allowed.verified);

        std::fs::remove_dir_all(&dir).ok();
    }
}
