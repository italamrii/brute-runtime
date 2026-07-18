mod backends;
mod benchmark;
mod calibration;
mod catalog;
mod cli;
mod errors;
mod estimator;
mod fit;
mod hardware;
mod identity;
mod models;
mod profile;
mod provenance;
mod recommend;
mod report;
mod runtime;
mod security;
#[cfg(test)]
mod stage1_fixtures_test;
mod tuning;

use benchmark::BenchmarkReport;
use catalog::TaskCategory;
use chrono::Utc;
use clap::Parser;
use cli::{
    BackendsCommands, BenchmarkArgs, CalibrationsCommands, CatalogArgs, CatalogCommands, Cli,
    Commands, ModelCommands, ProfileCommands, ProfilesCommands, TuneCommands,
};
use errors::BruteError;
use recommend::Priority;
use report::CapabilityReport;
use runtime::{Backend, RuntimeConfig};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

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
        Commands::Privacy {
            action: cli::PrivacyCommands::ShowId,
        } => cmd_privacy_show_id(),
        Commands::Privacy {
            action: cli::PrivacyCommands::ResetId,
        } => cmd_privacy_reset_id(),
        Commands::Backends {
            action:
                BackendsCommands::Verify {
                    model,
                    llama_bin,
                    backend,
                    allow_unverified_binary,
                    timeout_secs,
                    json,
                },
        } => cmd_backends_verify(
            &model,
            &llama_bin,
            backend,
            allow_unverified_binary,
            timeout_secs,
            json,
        ),
        Commands::Tune {
            action:
                TuneCommands::Run {
                    model,
                    llama_bin,
                    priority,
                    backend,
                    max_duration_secs,
                    dry_run,
                    allow_unverified_binary,
                    save_profile,
                    json,
                },
        } => cmd_tune_run(
            &model,
            &llama_bin,
            priority,
            backend,
            max_duration_secs,
            dry_run,
            allow_unverified_binary,
            save_profile,
            json,
        ),
        Commands::Tune {
            action: TuneCommands::Status { json },
        } => cmd_tune_status(json),
        Commands::Tune {
            action: TuneCommands::Cancel,
        } => cmd_tune_cancel(),
        Commands::Profiles {
            action: ProfilesCommands::List { json },
        } => cmd_profiles_list(json),
        Commands::Profiles {
            action: ProfilesCommands::Show { profile_id, json },
        } => cmd_profiles_show(&profile_id, json),
        Commands::Profiles {
            action:
                ProfilesCommands::Verify {
                    profile_id,
                    model,
                    llama_bin,
                    allow_unverified_binary,
                    timeout_secs,
                    json,
                },
        } => cmd_profiles_verify(
            &profile_id,
            &model,
            &llama_bin,
            allow_unverified_binary,
            timeout_secs,
            json,
        ),
        Commands::Profiles {
            action: ProfilesCommands::Export { profile_id, output },
        } => cmd_profiles_export(&profile_id, &output),
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

/// Shareable shape for `brute profile create`'s JSON/file output:
/// `local_instance_id` (random, resettable, safe to export) sits alongside
/// the profile fields, and `machine_id` inside the flattened profile is
/// always the redacted placeholder - never the real coarse hardware hash.
/// See `docs/privacy-model.md`.
#[derive(serde::Serialize)]
struct ShareableProfile {
    local_instance_id: String,
    #[serde(flatten)]
    profile: profile::HardwareCapabilityProfile,
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

    let local_instance_id = identity::load_or_create_local_instance_id(
        &identity::default_instance_id_path(),
    )
    .map_err(|source| BruteError::Io {
        context: "reading/creating local instance id".to_string(),
        source,
    })?;

    let shareable = ShareableProfile {
        local_instance_id: local_instance_id.clone(),
        profile: profile::redact_machine_id_for_export(&profile),
    };
    let text = to_json(&shareable)?;

    if let Some(out) = output {
        std::fs::write(out, &text).map_err(|source| BruteError::Io {
            context: format!("writing profile to {}", out.display()),
            source,
        })?;
        println!("Profile written to {}", out.display());
    } else if json {
        println!("{text}");
    } else {
        println!("Local instance ID (safe to share): {local_instance_id}");
        println!(
            "Machine ID (coarse, local-eyes-only, omitted from exports): {}",
            profile.machine_id
        );
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

fn cmd_privacy_show_id() -> Result<(), BruteError> {
    let id = identity::load_or_create_local_instance_id(&identity::default_instance_id_path())
        .map_err(|source| BruteError::Io {
            context: "reading/creating local instance id".to_string(),
            source,
        })?;
    println!("{id}");
    println!("This ID is random, local-only, and safe to include in shared reports.");
    println!("It does not identify your hardware and carries no meaning across machines.");
    println!("Run `brute privacy reset-id` to generate a new, unrelated one at any time.");
    Ok(())
}

fn cmd_privacy_reset_id() -> Result<(), BruteError> {
    let path = identity::default_instance_id_path();
    identity::reset_local_instance_id(&path).map_err(|source| BruteError::Io {
        context: format!("resetting local instance id at {}", path.display()),
        source,
    })?;
    let new_id =
        identity::load_or_create_local_instance_id(&path).map_err(|source| BruteError::Io {
            context: "creating new local instance id".to_string(),
            source,
        })?;
    println!("Local instance ID reset. New ID: {new_id}");
    Ok(())
}

// ---------------------------------------------------------------------
// Stage 2: backend verification, runtime auto-tuning, local profiles.
// ---------------------------------------------------------------------

fn binary_hash(path: &Path, allow_unverified_binary: bool) -> Option<String> {
    runtime::llama_cpp::verify_llama_binary(path, allow_unverified_binary)
        .ok()
        .map(|c| c.sha256)
}

fn cmd_backends_verify(
    model: &Path,
    llama_bin: &Path,
    backend: Option<Backend>,
    allow_unverified_binary: bool,
    timeout_secs: u64,
    json: bool,
) -> Result<(), BruteError> {
    let profile = profile::build_profile(&hardware::inspect(model.parent()), now_rfc3339(), 0);
    let to_check = backend
        .map(|b| vec![b])
        .unwrap_or_else(|| vec![Backend::Cpu, Backend::Cuda, Backend::Vulkan]);
    let timeout = Duration::from_secs(timeout_secs);

    let results: Vec<backends::BackendVerification> = to_check
        .into_iter()
        .map(|b| {
            backends::verify_backend(
                b,
                Some(llama_bin),
                model,
                &profile,
                allow_unverified_binary,
                timeout,
            )
        })
        .collect();

    if json {
        println!("{}", to_json(&results)?);
    } else {
        for r in &results {
            println!("{:?}: {}", r.backend, r.status.as_str());
            if let Some(reason) = &r.failure_reason {
                println!("  reason: {reason}");
            }
            if let Some(tps) = r.verification_tokens_per_second {
                println!("  verification generation throughput: {tps:.2} tok/s");
            }
            if let Some(gpu_info) = &r.reported_gpu_info {
                println!("  reported gpu_info: {gpu_info:?}");
            }
        }
    }
    Ok(())
}

/// Decides which single GPU backend (if any) tuning is allowed to
/// generate GPU-offload candidates for - `None` unless a real
/// end-to-end verification (not mere driver detection) passed. When the
/// user pins `--backend cpu`, no GPU backend is even attempted; when they
/// pin a specific GPU backend, only that one is checked; otherwise both
/// CUDA and Vulkan are opportunistically verified and the first verified
/// one wins.
fn determine_verified_gpu_backend(
    model: &Path,
    llama_bin: &Path,
    profile: &profile::HardwareCapabilityProfile,
    requested_backend: Option<Backend>,
    allow_unverified_binary: bool,
    timeout: Duration,
) -> (Option<Backend>, Vec<backends::BackendVerification>) {
    let to_check: Vec<Backend> = match requested_backend {
        Some(Backend::Cpu) => vec![],
        Some(b) => vec![b],
        None => vec![Backend::Cuda, Backend::Vulkan],
    };

    let mut verifications = Vec::new();
    let mut verified_gpu = None;
    for b in to_check {
        let v = backends::verify_backend(
            b,
            Some(llama_bin),
            model,
            profile,
            allow_unverified_binary,
            timeout,
        );
        if v.status == backends::BackendStatus::Verified && verified_gpu.is_none() {
            verified_gpu = Some(b);
        }
        verifications.push(v);
    }
    (verified_gpu, verifications)
}

const TUNE_PROMPT_TOKENS: u32 = 64;
const TUNE_GEN_TOKENS: u32 = 32;
const TUNE_PER_RUN_TIMEOUT_SECS: u64 = 60;
const TUNE_BACKEND_VERIFY_TIMEOUT_SECS: u64 = 60;

fn run_one_tuning_repetition(
    llama_bin: &Path,
    model_path: &Path,
    candidate: &tuning::candidates::Candidate,
    allow_unverified_binary: bool,
) -> tuning::runner::RepetitionSample {
    let config = RuntimeConfig {
        backend: candidate.backend,
        binary_dir: llama_bin.to_path_buf(),
        threads: candidate.threads,
        gpu_layers: candidate.gpu_layers,
        context_size: candidate.context_size,
        batch_size: candidate.batch_size,
        prompt_tokens: TUNE_PROMPT_TOKENS,
        gen_tokens: TUNE_GEN_TOKENS,
        repetitions: 1,
        timeout_secs: TUNE_PER_RUN_TIMEOUT_SECS,
    };

    match runtime::llama_cpp::run_bench(
        llama_bin,
        model_path,
        &config,
        allow_unverified_binary,
        |_| runtime::process::TickAction::Continue,
    ) {
        Ok((rows, run)) => tuning::runner::RepetitionSample {
            succeeded: run.succeeded(),
            timed_out: run.timed_out,
            cancelled: run.cancelled,
            crashed: !run.succeeded() && !run.timed_out && !run.cancelled,
            generation_tokens_per_second: rows.iter().find(|r| r.test == "tg").map(|r| r.avg_ts),
            prompt_tokens_per_second: rows.iter().find(|r| r.test == "pp").map(|r| r.avg_ts),
        },
        Err(errors::ProcessError::TimedOut { .. }) => tuning::runner::RepetitionSample {
            succeeded: false,
            timed_out: true,
            cancelled: false,
            crashed: false,
            generation_tokens_per_second: None,
            prompt_tokens_per_second: None,
        },
        Err(_) => tuning::runner::RepetitionSample {
            succeeded: false,
            timed_out: false,
            cancelled: false,
            crashed: true,
            generation_tokens_per_second: None,
            prompt_tokens_per_second: None,
        },
    }
}

/// Cross-process progress reporting for `brute tune status` - a
/// best-effort convenience, not a safety mechanism. Writing it is never
/// allowed to fail the actual tuning run (see `write_tune_status`).
#[derive(serde::Serialize, serde::Deserialize)]
struct TuneStatus {
    pid: u32,
    started_at: String,
    model_sha256: String,
    total_candidates: usize,
    completed_candidates: usize,
    current_candidate_id: Option<String>,
    finished: bool,
    cancelled: bool,
    finished_at: Option<String>,
}

fn tune_status_path() -> PathBuf {
    identity::default_local_state_dir().join("tune-status.json")
}

fn tune_cancel_flag_path() -> PathBuf {
    identity::default_local_state_dir().join("tune-cancel-flag")
}

fn write_tune_status(status: &TuneStatus) {
    let path = tune_status_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(status) {
        let _ = std::fs::write(path, json);
    }
}

#[allow(clippy::too_many_arguments)]
fn cmd_tune_run(
    model: &Path,
    llama_bin: &Path,
    priority: tuning::ranking::RankingPriority,
    backend: Option<Backend>,
    max_duration_secs: Option<u64>,
    dry_run: bool,
    allow_unverified_binary: bool,
    save_profile: bool,
    json: bool,
) -> Result<(), BruteError> {
    let model_report = models::inspect_model(model)?;
    let machine_profile =
        profile::build_profile(&hardware::inspect(model.parent()), now_rfc3339(), 0);

    let (verified_gpu_backend, backend_verifications) = determine_verified_gpu_backend(
        model,
        llama_bin,
        &machine_profile,
        backend,
        allow_unverified_binary,
        Duration::from_secs(TUNE_BACKEND_VERIFY_TIMEOUT_SECS),
    );

    let plan =
        tuning::candidates::generate_plan(&model_report, &machine_profile, verified_gpu_backend);

    if dry_run {
        return print_tune_plan(&plan, &backend_verifications, json);
    }

    let _ = std::fs::remove_file(tune_cancel_flag_path());

    let runner_config = tuning::runner::RunnerConfig {
        total_timeout: max_duration_secs
            .map(Duration::from_secs)
            .unwrap_or(tuning::runner::DEFAULT_TOTAL_TIMEOUT),
        ..tuning::runner::RunnerConfig::default()
    };

    let started_at = now_rfc3339();
    let total = plan.candidates.len();
    write_tune_status(&TuneStatus {
        pid: std::process::id(),
        started_at: started_at.clone(),
        model_sha256: model_report.sha256.clone(),
        total_candidates: total,
        completed_candidates: 0,
        current_candidate_id: None,
        finished: false,
        cancelled: false,
        finished_at: None,
    });

    let cancel_flag = tune_cancel_flag_path();
    let model_path = model_report.path.clone();

    let summary = tuning::runner::execute_plan(
        &plan,
        &runner_config,
        || hardware::memory::sample_bytes().map(|(_, avail)| avail),
        || {
            hardware::windows::inspect_storage(&model_path)
                .free_bytes
                .value
        },
        || cancel_flag.is_file(),
        |candidate| {
            // `run_repetition` is called once per repetition attempt (up
            // to `repetitions_per_candidate` times per candidate), not
            // once per candidate - deriving progress from the
            // candidate's position in the plan (rather than counting
            // calls) is what keeps `completed_candidates` an honest
            // count of *candidates*, not repetition attempts.
            let candidate_index = plan
                .candidates
                .iter()
                .position(|c| c.id == candidate.id)
                .unwrap_or(0);
            write_tune_status(&TuneStatus {
                pid: std::process::id(),
                started_at: started_at.clone(),
                model_sha256: model_report.sha256.clone(),
                total_candidates: total,
                completed_candidates: candidate_index,
                current_candidate_id: Some(candidate.id.clone()),
                finished: false,
                cancelled: false,
                finished_at: None,
            });
            run_one_tuning_repetition(llama_bin, &model_path, candidate, allow_unverified_binary)
        },
    );

    write_tune_status(&TuneStatus {
        pid: std::process::id(),
        started_at,
        model_sha256: model_report.sha256.clone(),
        total_candidates: total,
        completed_candidates: total,
        current_candidate_id: None,
        finished: true,
        cancelled: summary.cancelled,
        finished_at: Some(now_rfc3339()),
    });

    let ranking_result = tuning::ranking::rank(&plan, &summary, priority);

    if save_profile {
        maybe_save_runtime_profile(
            &plan,
            &summary,
            &ranking_result,
            &machine_profile,
            &model_report,
            llama_bin,
            allow_unverified_binary,
        )?;
    }

    print_tune_result(&backend_verifications, &summary, &ranking_result, json)
}

fn print_tune_plan(
    plan: &tuning::candidates::TuningPlan,
    verifications: &[backends::BackendVerification],
    json: bool,
) -> Result<(), BruteError> {
    if json {
        #[derive(serde::Serialize)]
        struct Output<'a> {
            plan: &'a tuning::candidates::TuningPlan,
            backend_verifications: &'a [backends::BackendVerification],
        }
        println!(
            "{}",
            to_json(&Output {
                plan,
                backend_verifications: verifications
            })?
        );
    } else {
        println!("Dry run - no benchmarks will be launched.");
        println!("Formula version: {}", plan.formula_version);
        println!(
            "Defaults: threads={} gpu_layers={} context={} batch={}",
            plan.defaults.threads,
            plan.defaults.gpu_layers,
            plan.defaults.context_size,
            plan.defaults.batch_size
        );
        println!("\n{} candidate(s):", plan.candidates.len());
        for c in &plan.candidates {
            println!(
                "  {:<20} backend={:?} threads={} gpu_layers={} context={} batch={}",
                c.id, c.backend, c.threads, c.gpu_layers, c.context_size, c.batch_size
            );
        }
        if !plan.pruned.is_empty() {
            println!("\n{} candidate(s) pruned:", plan.pruned.len());
            for p in &plan.pruned {
                println!("  {:<20} {}", p.id, p.reason);
            }
        }
        if plan.truncated {
            println!(
                "\nWarning: candidate list was truncated to the {} safety maximum.",
                tuning::candidates::MAX_CANDIDATES
            );
        }
    }
    Ok(())
}

fn print_tune_result(
    verifications: &[backends::BackendVerification],
    summary: &tuning::runner::TuningRunSummary,
    ranking: &tuning::ranking::RankingResult,
    json: bool,
) -> Result<(), BruteError> {
    if json {
        #[derive(serde::Serialize)]
        struct Output<'a> {
            backend_verifications: &'a [backends::BackendVerification],
            cancelled: bool,
            wall_time_secs: f64,
            candidate_results: &'a [tuning::runner::CandidateResult],
            ranking: &'a tuning::ranking::RankingResult,
        }
        println!(
            "{}",
            to_json(&Output {
                backend_verifications: verifications,
                cancelled: summary.cancelled,
                wall_time_secs: summary.wall_time.as_secs_f64(),
                candidate_results: &summary.candidate_results,
                ranking,
            })?
        );
    } else {
        println!(
            "Tuning complete in {:.1}s ({} candidate(s) benchmarked).",
            summary.wall_time.as_secs_f64(),
            summary.candidate_results.len()
        );
        if summary.cancelled {
            println!("Run was cancelled before completion.");
        }
        match &ranking.winner {
            Some(w) => {
                println!(
                    "\nWinner: {} ({:?} confidence)",
                    w.candidate_id, ranking.confidence
                );
                println!(
                    "  backend={:?} threads={} gpu_layers={} context={} batch={}",
                    w.measurements.backend,
                    w.measurements.threads,
                    w.measurements.gpu_layers,
                    w.measurements.context_size,
                    w.measurements.batch_size
                );
                println!(
                    "  stability={:?} generation={:?} tok/s prompt={:?} tok/s",
                    w.measurements.stability,
                    w.measurements.mean_generation_tokens_per_second,
                    w.measurements.mean_prompt_tokens_per_second
                );
            }
            None => println!("\nNo candidate completed successfully."),
        }
        if let Some(r) = &ranking.runner_up {
            println!("Runner-up: {}", r.candidate_id);
        }
        if let Some(f) = &ranking.safer_fallback
            && ranking
                .winner
                .as_ref()
                .is_none_or(|w| w.candidate_id != f.candidate_id)
        {
            println!("Safer fallback (differs from winner): {}", f.candidate_id);
        }
        for rejected in &ranking.rejected_faster_candidates {
            println!(
                "Rejected faster candidate {}: {}",
                rejected.candidate_id, rejected.reason
            );
        }
        for u in &ranking.unknown_values {
            println!("Unknown: {u}");
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn maybe_save_runtime_profile(
    plan: &tuning::candidates::TuningPlan,
    summary: &tuning::runner::TuningRunSummary,
    ranking: &tuning::ranking::RankingResult,
    machine_profile: &profile::HardwareCapabilityProfile,
    model_report: &models::ModelReport,
    llama_bin: &Path,
    allow_unverified_binary: bool,
) -> Result<(), BruteError> {
    let Some(winner) = &ranking.winner else {
        println!("No candidate completed successfully - no profile saved.");
        return Ok(());
    };
    let Some(stability) = summary
        .candidate_results
        .iter()
        .find(|r| r.candidate_id == winner.candidate_id)
        .map(|r| &r.stability)
    else {
        return Ok(());
    };

    let profile_id = tuning::runtime_profile::generate_profile_id();
    let runtime_profile = tuning::runtime_profile::build_profile(
        tuning::runtime_profile::ProfileInputs {
            plan,
            winner,
            stability,
            confidence: ranking.confidence,
            machine_profile,
            model: model_report,
            llama_cli_sha256: binary_hash(
                &runtime::llama_cpp::llama_cli_path(llama_bin),
                allow_unverified_binary,
            ),
            llama_bench_sha256: binary_hash(
                &runtime::llama_cpp::llama_bench_path(llama_bin),
                allow_unverified_binary,
            ),
            tuning_date: now_rfc3339(),
        },
        profile_id.clone(),
    );

    let dir = tuning::runtime_profile::default_profiles_dir();
    tuning::runtime_profile::save_profile_to(&dir, &runtime_profile).map_err(|source| {
        BruteError::Io {
            context: format!("saving runtime profile to {}", dir.display()),
            source,
        }
    })?;
    println!("Runtime profile saved: {profile_id}");
    Ok(())
}

fn cmd_tune_status(json: bool) -> Result<(), BruteError> {
    let path = tune_status_path();
    if !path.is_file() {
        if json {
            println!(
                "{}",
                to_json(&serde_json::json!({"status": "no_tune_has_run"}))?
            );
        } else {
            println!("No tuning run has been started yet (or its status file was cleared).");
        }
        return Ok(());
    }

    let contents = std::fs::read_to_string(&path).map_err(|source| BruteError::Io {
        context: format!("reading {}", path.display()),
        source,
    })?;

    if json {
        println!("{contents}");
    } else {
        let status: TuneStatus = serde_json::from_str(&contents)
            .map_err(errors::ReportError::Serialize)
            .map_err(BruteError::from)?;
        println!("Model sha256: {}", status.model_sha256);
        println!("Started: {}", status.started_at);
        println!(
            "Progress: {}/{} candidates",
            status.completed_candidates, status.total_candidates
        );
        if let Some(id) = &status.current_candidate_id {
            println!("Current candidate: {id}");
        }
        println!("Finished: {}", status.finished);
        println!("Cancelled: {}", status.cancelled);
        if let Some(at) = &status.finished_at {
            println!("Finished at: {at}");
        }
    }
    Ok(())
}

fn cmd_tune_cancel() -> Result<(), BruteError> {
    let path = tune_cancel_flag_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| BruteError::Io {
            context: format!("creating {}", parent.display()),
            source,
        })?;
    }
    std::fs::write(&path, b"cancel").map_err(|source| BruteError::Io {
        context: format!("writing cancel flag to {}", path.display()),
        source,
    })?;
    println!(
        "Cancellation requested. A running `brute tune run` will stop at its next safe \
         checkpoint (between repetitions/candidates), never mid-process."
    );
    Ok(())
}

fn cmd_profiles_list(json: bool) -> Result<(), BruteError> {
    let dir = tuning::runtime_profile::default_profiles_dir();
    let ids =
        tuning::runtime_profile::list_profile_ids_in(&dir).map_err(|source| BruteError::Io {
            context: format!("listing profiles in {}", dir.display()),
            source,
        })?;

    if json {
        println!("{}", to_json(&ids)?);
    } else if ids.is_empty() {
        println!("No saved runtime profiles.");
    } else {
        println!("{} saved profile(s):", ids.len());
        for id in &ids {
            println!("  {id}");
        }
    }
    Ok(())
}

fn load_profile_or_error(
    profile_id: &str,
) -> Result<tuning::runtime_profile::RuntimeProfile, BruteError> {
    let dir = tuning::runtime_profile::default_profiles_dir();
    tuning::runtime_profile::load_profile_from(&dir, profile_id).map_err(|source| BruteError::Io {
        context: format!("loading profile {profile_id} from {}", dir.display()),
        source,
    })
}

fn cmd_profiles_show(profile_id: &str, json: bool) -> Result<(), BruteError> {
    let profile = load_profile_or_error(profile_id)?;
    if json {
        println!("{}", to_json(&profile)?);
    } else {
        println!("Profile: {}", profile.profile_id);
        println!("  Tuned: {}", profile.tuning_date);
        println!("  Model sha256: {}", profile.model_sha256);
        println!(
            "  Backend: {:?}  Threads: {}  GPU layers: {}",
            profile.backend, profile.threads, profile.gpu_layers
        );
        println!(
            "  Context: {}  Batch: {}",
            profile.context_size, profile.batch_size
        );
        println!(
            "  Generation: {:?} tok/s  Prompt: {:?} tok/s",
            profile.mean_generation_tokens_per_second, profile.mean_prompt_tokens_per_second
        );
        println!(
            "  Stability: {:?}  Confidence: {:?}",
            profile.stability, profile.confidence
        );
    }
    Ok(())
}

fn cmd_profiles_verify(
    profile_id: &str,
    model: &Path,
    llama_bin: &Path,
    allow_unverified_binary: bool,
    timeout_secs: u64,
    json: bool,
) -> Result<(), BruteError> {
    let profile = load_profile_or_error(profile_id)?;
    let model_report = models::inspect_model(model)?;
    let machine_profile =
        profile::build_profile(&hardware::inspect(model.parent()), now_rfc3339(), 0);

    let cli_hash = binary_hash(
        &runtime::llama_cpp::llama_cli_path(llama_bin),
        allow_unverified_binary,
    );
    let bench_hash = binary_hash(
        &runtime::llama_cpp::llama_bench_path(llama_bin),
        allow_unverified_binary,
    );

    let result = tuning::apply::apply_and_verify(tuning::apply::ApplyRequest {
        profile: &profile,
        binary_dir: llama_bin,
        model: &model_report,
        machine_profile: &machine_profile,
        llama_cli_sha256: cli_hash.as_deref(),
        llama_bench_sha256: bench_hash.as_deref(),
        allow_unverified_binary,
        timeout: Duration::from_secs(timeout_secs),
    });

    if json {
        println!("{}", to_json(&result)?);
    } else {
        println!("Status: {:?}", result.status);
        println!("Rollback recommended: {}", result.rollback_recommended);
        println!("{}", result.detail);
        for issue in &result.compatibility_issues {
            println!("  - {issue}");
        }
    }

    if result.status == tuning::apply::ApplyStatus::Verified {
        Ok(())
    } else {
        Err(BruteError::Usage(format!(
            "profile verification did not succeed: {}",
            result.detail
        )))
    }
}

fn cmd_profiles_export(profile_id: &str, output: &Path) -> Result<(), BruteError> {
    let profile = load_profile_or_error(profile_id)?;
    let sanitized = tuning::runtime_profile::sanitize_for_export(&profile);
    let text = to_json(&sanitized)?;
    std::fs::write(output, &text).map_err(|source| BruteError::Io {
        context: format!("writing exported profile to {}", output.display()),
        source,
    })?;
    println!(
        "Profile exported to {} (machine ID redacted).",
        output.display()
    );
    Ok(())
}
