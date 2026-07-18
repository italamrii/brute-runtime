mod benchmark;
mod calibration;
mod catalog;
mod cli;
mod errors;
mod estimator;
mod fit;
mod hardware;
mod models;
mod profile;
mod provenance;
mod recommend;
mod report;
mod runtime;
mod security;
#[cfg(test)]
mod stage1_fixtures_test;

use benchmark::BenchmarkReport;
use catalog::TaskCategory;
use chrono::Utc;
use clap::Parser;
use cli::{
    BenchmarkArgs, CalibrationsCommands, CatalogArgs, CatalogCommands, Cli, Commands,
    ModelCommands, ProfileCommands,
};
use errors::BruteError;
use recommend::Priority;
use report::CapabilityReport;
use runtime::{Backend, RuntimeConfig};
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
        Commands::Profile {
            action:
                ProfileCommands::Create {
                    output,
                    calibration,
                    storage_path,
                    json,
                },
        } => cmd_profile_create(
            output.as_deref(),
            &calibration,
            storage_path.as_deref(),
            json,
        ),
        Commands::Catalog {
            action: CatalogCommands::List { catalog, json },
        } => cmd_catalog_list(&catalog, json),
        Commands::Catalog {
            action:
                CatalogCommands::Show {
                    catalog,
                    catalog_id,
                    json,
                },
        } => cmd_catalog_show(&catalog, &catalog_id, json),
        Commands::Fit {
            catalog,
            model,
            all,
            context,
            backend,
            json,
        } => cmd_fit(&catalog, model.as_deref(), all, context, backend, json),
        Commands::RecommendModel {
            catalog,
            task,
            priority,
            json,
        } => cmd_recommend_model(&catalog, task, priority, json),
        Commands::ExplainFit {
            catalog,
            model,
            task,
            priority,
            json,
        } => cmd_explain_fit(&catalog, &model, task, priority, json),
        Commands::Calibrations {
            action: CalibrationsCommands::List { calibration, json },
        } => cmd_calibrations_list(&calibration, json),
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
    maybe_save_calibration(args.save_calibration.as_deref(), &model_report, &bench)?;
    Ok((model_report, bench))
}

/// Appends a real calibration record from a just-completed benchmark run
/// when `--save-calibration <path>` was given. Skips (with a warning, not
/// an error) if the model's architecture/quantization/parameter_count
/// aren't known - a calibration record without them isn't matchable by
/// `calibration::find_nearest` later, so recording it would be dead data.
fn maybe_save_calibration(
    save_path: Option<&Path>,
    model_report: &models::ModelReport,
    bench: &BenchmarkReport,
) -> Result<(), BruteError> {
    let Some(path) = save_path else {
        return Ok(());
    };
    let Some(architecture) = model_report.architecture.clone() else {
        eprintln!("warning: not saving calibration record - model architecture unknown");
        return Ok(());
    };
    let Some(quantization) = model_report.quantization.clone() else {
        eprintln!("warning: not saving calibration record - model quantization unknown");
        return Ok(());
    };
    let Some(parameter_count) = model_report.parameter_count else {
        eprintln!("warning: not saving calibration record - model parameter_count unknown");
        return Ok(());
    };

    let hw = hardware::inspect(None);
    let machine_id = profile::build_profile(&hw, now_rfc3339(), 0).machine_id;

    let record = calibration::CalibrationRecord {
        recorded_at_rfc3339: now_rfc3339(),
        machine_id,
        machine_profile_schema_version: profile::SCHEMA_VERSION.to_string(),
        catalog_id: None,
        architecture,
        quantization,
        parameter_count,
        backend: bench.config.backend,
        threads: bench.config.threads,
        gpu_layers: bench.config.gpu_layers,
        context_size: bench.config.context_size,
        batch_size: bench.config.batch_size,
        prompt_processing_tokens_per_second: bench.prompt_processing.avg_tokens_per_second,
        generation_tokens_per_second: bench.generation.avg_tokens_per_second,
        peak_ram_bytes: bench.memory.process_peak_working_set_bytes.value,
        peak_vram_bytes: None,
        stability: bench.stability,
    };

    let mut store = calibration::CalibrationStore::load(path)?;
    store.add(record);
    store.save(path).map_err(|source| BruteError::Io {
        context: format!("saving calibration record to {}", path.display()),
        source,
    })?;
    println!("Calibration record saved to {}", path.display());
    Ok(())
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

fn to_json<T: serde::Serialize>(value: &T) -> Result<String, BruteError> {
    serde_json::to_string_pretty(value)
        .map_err(errors::ReportError::Serialize)
        .map_err(BruteError::from)
}

fn build_profile_for_cli(
    calibration_path: &Path,
    storage_path: Option<&Path>,
) -> Result<profile::HardwareCapabilityProfile, BruteError> {
    let hw = hardware::inspect(storage_path);
    let calib_store = calibration::CalibrationStore::load(calibration_path)?;
    // machine_id is derived from `hw` alone, so it's safe to build a
    // throwaway profile first just to learn it, then filter the
    // calibration count down to records that actually apply to *this*
    // machine (not just however many happen to be in the store file).
    let preliminary = profile::build_profile(&hw, now_rfc3339(), 0);
    let this_machine_count = calib_store
        .records
        .iter()
        .filter(|r| r.machine_id == preliminary.machine_id)
        .count();
    Ok(profile::HardwareCapabilityProfile {
        calibration_record_count: this_machine_count,
        ..preliminary
    })
}

fn cmd_profile_create(
    output: Option<&Path>,
    calibration: &Path,
    storage_path: Option<&Path>,
    json: bool,
) -> Result<(), BruteError> {
    let mut profile = build_profile_for_cli(calibration, storage_path)?;
    if let Some(storage) = &mut profile.storage {
        storage.path_queried =
            security::redact_username_for_report(Path::new(&storage.path_queried));
    }
    let text = to_json(&profile)?;

    if let Some(out) = output {
        std::fs::write(out, &text).map_err(|source| BruteError::Io {
            context: format!("writing profile to {}", out.display()),
            source,
        })?;
        println!("Profile written to {}", out.display());
    } else if json {
        println!("{text}");
    } else {
        println!("Machine ID: {}", profile.machine_id);
        println!("Schema version: {}", profile.schema_version);
        println!("Captured: {}", profile.captured_at_rfc3339);
        println!(
            "CPU: {} ({} physical / {} logical cores)",
            profile.cpu.brand.value.as_deref().unwrap_or("unknown"),
            profile
                .cpu
                .physical_cores
                .value
                .map(|v| v.to_string())
                .unwrap_or_else(|| "unknown".into()),
            profile
                .cpu
                .logical_cores
                .value
                .map(|v| v.to_string())
                .unwrap_or_else(|| "unknown".into()),
        );
        println!(
            "RAM: {} total, {} available",
            profile
                .memory
                .total_bytes
                .value
                .map(|v| format!("{v} bytes"))
                .unwrap_or_else(|| "unknown".into()),
            profile
                .memory
                .available_bytes
                .value
                .map(|v| format!("{v} bytes"))
                .unwrap_or_else(|| "unknown".into()),
        );
        for gpu in &profile.gpus {
            println!("GPU: {} ({:?})", gpu.name, gpu.vendor);
        }
        println!(
            "Backends: cpu=true cuda={:?} vulkan={:?}",
            profile.backends.cuda.value, profile.backends.vulkan.value
        );
        println!(
            "Calibration records for this machine: {}",
            profile.calibration_record_count
        );
        println!(
            "Confidence summary: measured={} detected={} inferred={} unavailable={} (of {} fields)",
            profile.confidence_summary.measured_count,
            profile.confidence_summary.detected_count,
            profile.confidence_summary.inferred_count,
            profile.confidence_summary.unavailable_count,
            profile.confidence_summary.total(),
        );
    }
    Ok(())
}

fn cmd_catalog_list(catalog_path: &Path, json: bool) -> Result<(), BruteError> {
    let catalog = catalog::load_catalog(catalog_path)?;
    if json {
        println!("{}", to_json(&catalog.builds)?);
    } else {
        if let Some(notice) = &catalog.notice {
            println!("{notice}\n");
        }
        if let Some(version) = &catalog.schema_version {
            println!("Catalog schema version: {version}");
        }
        println!("{} build(s):", catalog.builds.len());
        for b in &catalog.builds {
            println!(
                "  {:<40} {:<28} {:>4.1}B params  {:<8} {:?}",
                b.catalog_id,
                b.display_name,
                b.parameter_count as f64 / 1_000_000_000.0,
                b.quantization,
                b.license,
            );
        }
    }
    Ok(())
}

/// Never phrases `Unknown` as if it meant `Allowed` - the catalog schema's
/// entire point is that these two must never be conflated.
fn describe_commercial_use(status: catalog::CommercialUse) -> &'static str {
    match status {
        catalog::CommercialUse::Allowed => "allowed (per catalog metadata)",
        catalog::CommercialUse::Restricted => {
            "restricted - check the license before commercial use"
        }
        catalog::CommercialUse::Unknown => {
            "unknown - not established by catalog metadata, do not assume allowed"
        }
    }
}

fn cmd_catalog_show(catalog_path: &Path, catalog_id: &str, json: bool) -> Result<(), BruteError> {
    let catalog = catalog::load_catalog(catalog_path)?;
    let build = catalog.require(catalog_id)?;
    if json {
        println!("{}", to_json(build)?);
    } else {
        println!("{} ({})", build.display_name, build.catalog_id);
        println!("  Publisher: {}", build.publisher);
        println!("  Source: {}", build.official_source_url);
        println!(
            "  Architecture: {}  Quantization: {}",
            build.architecture, build.quantization
        );
        println!(
            "  Parameters: {}  File size: {} bytes",
            build.parameter_count, build.file_size_bytes
        );
        println!("  Context sizes: {:?}", build.context_sizes);
        println!("  Task categories: {:?}", build.task_categories);
        println!("  Supported backends: {:?}", build.supported_backends);
        println!(
            "  License: {:?}  Commercial use: {}  Gated: {:?}",
            build.license,
            describe_commercial_use(build.commercial_use),
            build.gated_access
        );
        println!("  Strength: {}", build.strength);
        println!("  Limitation: {}", build.limitation);
        println!("  Provenance: {}", build.metadata_provenance);
    }
    Ok(())
}

fn cmd_fit(
    catalog_args: &CatalogArgs,
    model: Option<&str>,
    all: bool,
    context: Option<u32>,
    backend: Option<Backend>,
    json: bool,
) -> Result<(), BruteError> {
    if model.is_none() && !all {
        return Err(BruteError::Usage(
            "brute fit requires either --model <catalog-id> or --all".to_string(),
        ));
    }

    let catalog = catalog::load_catalog(&catalog_args.catalog)?;
    let calib = calibration::CalibrationStore::load(&catalog_args.calibration)?;
    let profile = build_profile_for_cli(
        &catalog_args.calibration,
        catalog_args.storage_path.as_deref(),
    )?;

    let targets: Vec<&catalog::ModelBuild> = if all {
        catalog.builds.iter().collect()
    } else {
        vec![catalog.require(model.unwrap())?]
    };

    let evaluations: Vec<_> = targets
        .into_iter()
        .map(|b| {
            recommend::evaluate_build(
                b,
                &profile,
                &calib,
                None,
                Priority::Balanced,
                context,
                backend,
            )
        })
        .collect();

    if json {
        println!("{}", to_json(&evaluations)?);
    } else {
        for e in &evaluations {
            println!(
                "{:<40} {:<15} headroom_ratio={}",
                e.build.catalog_id,
                e.fit.state.as_str(),
                e.fit
                    .headroom_ratio
                    .map(|r| format!("{r:.2}"))
                    .unwrap_or_else(|| "n/a".to_string()),
            );
            for reason in &e.fit.reasons {
                println!("    - {reason}");
            }
        }
    }
    Ok(())
}

fn cmd_recommend_model(
    catalog_args: &CatalogArgs,
    task: Option<TaskCategory>,
    priority: Priority,
    json: bool,
) -> Result<(), BruteError> {
    let catalog = catalog::load_catalog(&catalog_args.catalog)?;
    let calib = calibration::CalibrationStore::load(&catalog_args.calibration)?;
    let profile = build_profile_for_cli(
        &catalog_args.calibration,
        catalog_args.storage_path.as_deref(),
    )?;

    let ranked = recommend::rank(&catalog, &profile, &calib, task, priority);
    let rec = recommend::recommend(&ranked, &profile);

    if json {
        println!("{}", to_json(&rec)?);
    } else {
        match &rec {
            Some(r) => {
                println!(
                    "Recommended: {} ({})",
                    r.recommended.build.display_name, r.recommended.build.catalog_id
                );
                println!("  Fit: {}", r.recommended.fit.state.as_str());
                println!(
                    "  Estimated RAM: {}-{} bytes",
                    r.recommended.estimate.estimated_total_ram_bytes_low,
                    r.recommended.estimate.estimated_total_ram_bytes_high
                );
                if let Some(m) = &r.recommended.calibration_match {
                    println!(
                        "  {}",
                        recommend::explain::describe_calibration_performance(m)
                    );
                } else {
                    println!(
                        "  No calibration data available for this build - performance not estimated."
                    );
                }
                println!(
                    "  Score: {:.3} (ranking formula {})",
                    r.recommended.score, ranked.ranking_formula_version
                );
                if let Some(safer) = &r.safer_fallback {
                    println!(
                        "Safer fallback: {} ({})",
                        safer.build.display_name, safer.build.catalog_id
                    );
                }
                if let Some(stronger) = &r.stronger_optional {
                    println!(
                        "Stronger optional: {} ({})",
                        stronger.build.display_name, stronger.build.catalog_id
                    );
                }
                println!("\n{}", r.explanation.simple);
            }
            None => println!("No catalog entries matched this task/priority."),
        }
    }
    Ok(())
}

fn cmd_explain_fit(
    catalog_args: &CatalogArgs,
    model: &str,
    task: Option<TaskCategory>,
    priority: Priority,
    json: bool,
) -> Result<(), BruteError> {
    let catalog = catalog::load_catalog(&catalog_args.catalog)?;
    let calib = calibration::CalibrationStore::load(&catalog_args.calibration)?;
    let profile = build_profile_for_cli(
        &catalog_args.calibration,
        catalog_args.storage_path.as_deref(),
    )?;
    let build = catalog.require(model)?;

    let evaluation = recommend::evaluate_build(build, &profile, &calib, task, priority, None, None);
    let explanation = recommend::explain::build_explanation(&evaluation, &None, &None, &profile);

    if json {
        #[derive(serde::Serialize)]
        struct Output<'a> {
            evaluation: &'a recommend::BuildEvaluation,
            explanation: &'a recommend::explain::Explanation,
        }
        println!(
            "{}",
            to_json(&Output {
                evaluation: &evaluation,
                explanation: &explanation
            })?
        );
    } else {
        println!("{}\n", explanation.simple);
        println!("Technical detail:");
        println!(
            "  Detected resources: {}",
            explanation.technical.detected_resources
        );
        println!(
            "  Memory calculation: {}",
            explanation.technical.memory_calculation
        );
        if let Some(calib) = &explanation.technical.calibration_used {
            println!("  Calibration used: {calib}");
        } else {
            println!("  Calibration used: none - no matching real benchmark on record");
        }
        println!(
            "  Backend assumptions: {}",
            explanation.technical.backend_assumptions
        );
        println!(
            "  Context/batch assumptions: {}",
            explanation.technical.context_and_batch_assumptions
        );
        println!("  Fit thresholds: {}", explanation.technical.fit_thresholds);
        println!(
            "  Confidence calculation: {}",
            explanation.technical.confidence_calculation
        );
    }
    Ok(())
}

fn cmd_calibrations_list(calibration_path: &Path, json: bool) -> Result<(), BruteError> {
    let store = calibration::CalibrationStore::load(calibration_path)?;
    if json {
        println!("{}", to_json(&store)?);
    } else {
        println!("{} calibration record(s):", store.records.len());
        for r in &store.records {
            println!(
                "  {} {} {} backend={:?} {:.1}/{:.1} tok/s (prompt/gen) {:?} recorded={}",
                r.architecture,
                r.quantization,
                r.parameter_count,
                r.backend,
                r.prompt_processing_tokens_per_second,
                r.generation_tokens_per_second,
                r.stability,
                r.recorded_at_rfc3339,
            );
        }
    }
    Ok(())
}
