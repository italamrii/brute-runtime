mod benchmark;
mod cli;
mod errors;
mod hardware;
mod models;
mod report;
mod runtime;
mod security;

use benchmark::BenchmarkReport;
use chrono::Utc;
use clap::Parser;
use cli::{BenchmarkArgs, Cli, Commands, ModelCommands};
use errors::BruteError;
use report::CapabilityReport;
use runtime::RuntimeConfig;
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Inspect { json, storage_path } => cmd_inspect(json, storage_path.as_deref()),
        Commands::Model {
            action: ModelCommands::Inspect { path, json },
        } => cmd_model_inspect(&path, json),
        Commands::Benchmark {
            model,
            llama_bin,
            args,
            json,
        } => cmd_benchmark(&model, &llama_bin, &args, json),
        Commands::Recommend {
            model,
            llama_bin,
            args,
            json,
        } => cmd_recommend(&model, &llama_bin, &args, json),
        Commands::Doctor {
            llama_bin,
            allow_unverified_binary,
        } => cmd_doctor(&llama_bin, allow_unverified_binary),
        Commands::Report {
            output,
            model,
            llama_bin,
            args,
        } => cmd_report(&output, model.as_deref(), llama_bin.as_deref(), &args),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

fn resolve_threads(requested: u32) -> u32 {
    if requested > 0 {
        requested
    } else {
        std::thread::available_parallelism()
            .map(|n| n.get() as u32)
            .unwrap_or(1)
    }
}

fn runtime_config_from_args(llama_bin: &Path, args: &BenchmarkArgs) -> RuntimeConfig {
    RuntimeConfig {
        backend: args.backend,
        binary_dir: llama_bin.to_path_buf(),
        threads: resolve_threads(args.threads),
        gpu_layers: args.gpu_layers,
        context_size: args.context_size,
        batch_size: args.batch_size,
        prompt_tokens: args.prompt_tokens,
        gen_tokens: args.gen_tokens,
        repetitions: args.repetitions,
        timeout_secs: args.timeout_secs,
    }
}

fn cmd_inspect(json: bool, storage_path: Option<&Path>) -> Result<(), BruteError> {
    let hw = hardware::inspect(storage_path);
    let report = CapabilityReport {
        generated_at_rfc3339: now_rfc3339(),
        hardware: hw,
        model: None,
        benchmark: None,
        recommendation: None,
    };

    print_report(&report, json)
}

fn cmd_model_inspect(path: &Path, json: bool) -> Result<(), BruteError> {
    let model = models::inspect_model(path)?;
    let report = CapabilityReport {
        generated_at_rfc3339: now_rfc3339(),
        hardware: hardware::inspect(path.parent()),
        model: Some(model),
        benchmark: None,
        recommendation: None,
    };
    print_report(&report, json)
}

fn run_benchmark(
    model: &Path,
    llama_bin: &Path,
    args: &BenchmarkArgs,
) -> Result<(models::ModelReport, BenchmarkReport), BruteError> {
    let model_report = models::inspect_model(model)?;
    let config = runtime_config_from_args(llama_bin, args);
    let bench = benchmark::run(
        &model_report.path,
        llama_bin,
        &config,
        args.allow_unverified_binary,
    )?;
    Ok((model_report, bench))
}

fn cmd_benchmark(
    model: &Path,
    llama_bin: &Path,
    args: &BenchmarkArgs,
    json: bool,
) -> Result<(), BruteError> {
    let (model_report, bench) = run_benchmark(model, llama_bin, args)?;
    let report = CapabilityReport {
        generated_at_rfc3339: now_rfc3339(),
        hardware: hardware::inspect(model.parent()),
        model: Some(model_report),
        benchmark: Some(bench),
        recommendation: None,
    };
    print_report(&report, json)
}

fn cmd_recommend(
    model: &Path,
    llama_bin: &Path,
    args: &BenchmarkArgs,
    json: bool,
) -> Result<(), BruteError> {
    let (model_report, bench) = run_benchmark(model, llama_bin, args)?;
    let recommendation = report::build_recommendation(&bench);
    let report = CapabilityReport {
        generated_at_rfc3339: now_rfc3339(),
        hardware: hardware::inspect(model.parent()),
        model: Some(model_report),
        benchmark: Some(bench),
        recommendation: Some(recommendation),
    };
    print_report(&report, json)
}

fn cmd_doctor(llama_bin: &Path, allow_unverified_binary: bool) -> Result<(), BruteError> {
    let run = runtime::llama_cpp::run_cli_version(llama_bin, allow_unverified_binary)?;
    println!("llama-cli --version exit code: {:?}", run.exit_code);
    println!("timed out: {}", run.timed_out);
    println!("stdout:\n{}", run.stdout.trim());
    if !run.stderr.trim().is_empty() {
        println!("stderr:\n{}", run.stderr.trim());
    }
    if run.succeeded() {
        println!(
            "\nllama.cpp binary at {} is verified and launchable.",
            llama_bin.display()
        );
        Ok(())
    } else {
        Err(errors::ProcessError::NonZeroExit {
            code: run.exit_code,
            stderr_tail: run.stderr,
        }
        .into())
    }
}

fn cmd_report(
    output: &Path,
    model: Option<&Path>,
    llama_bin: Option<&Path>,
    args: &BenchmarkArgs,
) -> Result<(), BruteError> {
    let storage_path = model.and_then(Path::parent);
    let hw = hardware::inspect(storage_path);

    let (model_report, benchmark_report, recommendation) = match (model, llama_bin) {
        (Some(model_path), Some(bin_dir)) => {
            let (m, b) = run_benchmark(model_path, bin_dir, args)?;
            let rec = report::build_recommendation(&b);
            (Some(m), Some(b), Some(rec))
        }
        (Some(model_path), None) => {
            let m = models::inspect_model(model_path)?;
            (Some(m), None, None)
        }
        _ => (None, None, None),
    };

    let report = CapabilityReport {
        generated_at_rfc3339: now_rfc3339(),
        hardware: hw,
        model: model_report,
        benchmark: benchmark_report,
        recommendation,
    };

    report::json::write_to_file(&report, output)?;
    println!("Report written to {}", output.display());
    Ok(())
}

fn print_report(report: &CapabilityReport, json: bool) -> Result<(), BruteError> {
    if json {
        println!("{}", report::json::to_pretty_string(report)?);
    } else {
        println!("{}", report::terminal::render(report));
    }
    Ok(())
}
