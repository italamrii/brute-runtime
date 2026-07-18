//! Executes a `TuningPlan`'s candidates under the resource and process
//! safety guarantees required by Stage 2 (spec section 6): a bounded
//! total time budget, a fresh RAM/disk check immediately before *every*
//! single repetition (not just once at candidate-generation time), a
//! small bounded retry on a spurious single-repetition failure, a
//! cooldown around "heavy" runs (any GPU offload or a large context -
//! the cases most likely to leave a GPU/driver in a state that skews an
//! immediately following measurement), and cooperative cancellation that
//! is checked between repetitions/candidates.
//!
//! This module never launches `llama-bench`/`llama-cli` itself, and so
//! never spawns a child process directly - it calls a `run_repetition`
//! closure that performs one benchmark attempt and reports back a
//! [`RepetitionSample`]. In production that closure wraps
//! `benchmark::runner::run` (itself built on `runtime::process::run`,
//! which already guarantees a killed/reaped child on timeout or
//! `TickAction::Cancel` - see that module's doc comment). Keeping the
//! guard logic here independent of the real closure is what makes it
//! fully unit-testable without a real llama.cpp binary or physical GPU.
//! Mid-process cancellation (Ctrl+C, a cross-process `brute tune cancel`
//! flag file) is wired at the CLI layer in a later task and surfaces here
//! only through the `is_cancelled` closure.

use super::candidates::{Candidate, TuningPlan};
use super::stability::{self, RepetitionOutcome, StabilityAssessment};
use serde::Serialize;
use std::time::{Duration, Instant};

pub const DEFAULT_REPETITIONS_PER_CANDIDATE: u32 = 3;
pub const DEFAULT_MAX_RETRIES_PER_REPETITION: u32 = 1;
pub const DEFAULT_TOTAL_TIMEOUT: Duration = Duration::from_secs(30 * 60);
pub const DEFAULT_MIN_AVAILABLE_RAM_BYTES: u64 = 256_000_000;
pub const DEFAULT_MIN_FREE_DISK_BYTES: u64 = 500_000_000;
pub const DEFAULT_COOLDOWN_BETWEEN_HEAVY_RUNS: Duration = Duration::from_millis(750);

/// A candidate counts as "heavy" - and triggers a cooldown before it (and
/// before whatever follows it) - when it offloads any layers to a GPU or
/// requests a large context.
const HEAVY_CONTEXT_THRESHOLD: u32 = 4096;

#[derive(Debug, Clone)]
pub struct RunnerConfig {
    pub repetitions_per_candidate: u32,
    pub max_retries_per_repetition: u32,
    pub total_timeout: Duration,
    pub min_available_ram_bytes: u64,
    pub min_free_disk_bytes: u64,
    pub cooldown_between_heavy_runs: Duration,
}

impl Default for RunnerConfig {
    fn default() -> Self {
        Self {
            repetitions_per_candidate: DEFAULT_REPETITIONS_PER_CANDIDATE,
            max_retries_per_repetition: DEFAULT_MAX_RETRIES_PER_REPETITION,
            total_timeout: DEFAULT_TOTAL_TIMEOUT,
            min_available_ram_bytes: DEFAULT_MIN_AVAILABLE_RAM_BYTES,
            min_free_disk_bytes: DEFAULT_MIN_FREE_DISK_BYTES,
            cooldown_between_heavy_runs: DEFAULT_COOLDOWN_BETWEEN_HEAVY_RUNS,
        }
    }
}

/// What one benchmark attempt for one candidate produced. Built from a
/// real `benchmark::BenchmarkReport` in production; constructed directly
/// in tests.
#[derive(Debug, Clone, Serialize)]
pub struct RepetitionSample {
    pub succeeded: bool,
    pub timed_out: bool,
    pub cancelled: bool,
    pub crashed: bool,
    pub generation_tokens_per_second: Option<f64>,
    pub prompt_tokens_per_second: Option<f64>,
}

impl RepetitionSample {
    fn to_outcome(&self) -> RepetitionOutcome {
        RepetitionOutcome {
            succeeded: self.succeeded,
            timed_out: self.timed_out,
            cancelled: self.cancelled,
            generation_tokens_per_second: self.generation_tokens_per_second,
            prompt_tokens_per_second: self.prompt_tokens_per_second,
        }
    }
}

#[derive(Debug, Clone)]
pub enum SkipReason {
    InsufficientRam {
        available_bytes: u64,
        required_bytes: u64,
    },
    InsufficientDisk {
        available_bytes: u64,
        required_bytes: u64,
    },
    TotalTimeoutExpired,
    Cancelled,
}

impl SkipReason {
    pub fn description(&self) -> String {
        match self {
            SkipReason::InsufficientRam {
                available_bytes,
                required_bytes,
            } => format!(
                "skipped: only {available_bytes} bytes RAM available, {required_bytes} bytes \
                 required as a safety floor before launching"
            ),
            SkipReason::InsufficientDisk {
                available_bytes,
                required_bytes,
            } => format!(
                "skipped: only {available_bytes} bytes free disk space, {required_bytes} bytes \
                 required as a safety floor before launching"
            ),
            SkipReason::TotalTimeoutExpired => {
                "skipped: total tuning time budget was exhausted before this candidate could run"
                    .to_string()
            }
            SkipReason::Cancelled => "skipped: tuning run was cancelled".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RepetitionAttempt {
    pub attempt_number: u32,
    pub is_retry: bool,
    pub sample: Option<RepetitionSample>,
    pub skip_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CandidateResult {
    pub candidate_id: String,
    pub attempts: Vec<RepetitionAttempt>,
    pub stability: StabilityAssessment,
    pub skipped: bool,
}

#[derive(Debug, Clone)]
pub struct TuningRunSummary {
    pub candidate_results: Vec<CandidateResult>,
    pub cancelled: bool,
    pub wall_time: Duration,
}

/// Runs every candidate in `plan` under the guards in `config`.
/// `available_ram_bytes`/`free_disk_bytes` are called fresh before every
/// single repetition attempt, since they can change while a long tuning
/// run is in progress. `is_cancelled` is polled between repetitions and
/// between candidates.
pub fn execute_plan(
    plan: &TuningPlan,
    config: &RunnerConfig,
    mut available_ram_bytes: impl FnMut() -> Option<u64>,
    mut free_disk_bytes: impl FnMut() -> Option<u64>,
    mut is_cancelled: impl FnMut() -> bool,
    mut run_repetition: impl FnMut(&Candidate) -> RepetitionSample,
) -> TuningRunSummary {
    let start = Instant::now();
    let mut candidate_results = Vec::with_capacity(plan.candidates.len());
    let mut cancelled = false;
    let mut previous_was_heavy = false;

    for candidate in &plan.candidates {
        if cancelled {
            candidate_results.push(skipped_result(candidate, SkipReason::Cancelled));
            continue;
        }
        if is_cancelled() {
            cancelled = true;
            candidate_results.push(skipped_result(candidate, SkipReason::Cancelled));
            continue;
        }
        if start.elapsed() >= config.total_timeout {
            candidate_results.push(skipped_result(candidate, SkipReason::TotalTimeoutExpired));
            continue;
        }

        let heavy = is_heavy(candidate);
        if heavy || previous_was_heavy {
            std::thread::sleep(config.cooldown_between_heavy_runs);
        }
        previous_was_heavy = heavy;

        let mut attempts = Vec::new();
        let mut attempt_number = 0u32;

        for _rep in 0..config.repetitions_per_candidate {
            if is_cancelled() {
                cancelled = true;
                break;
            }
            if start.elapsed() >= config.total_timeout {
                break;
            }

            let mut retries_used = 0u32;
            let mut blocked = false;
            loop {
                attempt_number += 1;

                if let Some(reason) =
                    precondition_failure(config, available_ram_bytes(), free_disk_bytes())
                {
                    attempts.push(RepetitionAttempt {
                        attempt_number,
                        is_retry: retries_used > 0,
                        sample: None,
                        skip_reason: Some(reason.description()),
                    });
                    blocked = true;
                    break;
                }

                let sample = run_repetition(candidate);
                let sample_failed = !sample.succeeded;
                let sample_cancelled = sample.cancelled;
                attempts.push(RepetitionAttempt {
                    attempt_number,
                    is_retry: retries_used > 0,
                    sample: Some(sample),
                    skip_reason: None,
                });

                if sample_cancelled {
                    cancelled = true;
                    break;
                }
                if sample_failed && retries_used < config.max_retries_per_repetition {
                    retries_used += 1;
                    continue;
                }
                break;
            }

            if blocked || cancelled {
                break;
            }
        }

        let outcomes: Vec<RepetitionOutcome> = attempts
            .iter()
            .filter_map(|a| a.sample.as_ref())
            .map(RepetitionSample::to_outcome)
            .collect();
        let stability = stability::classify(&outcomes);

        candidate_results.push(CandidateResult {
            candidate_id: candidate.id.clone(),
            attempts,
            stability,
            skipped: false,
        });
    }

    TuningRunSummary {
        candidate_results,
        cancelled,
        wall_time: start.elapsed(),
    }
}

fn is_heavy(candidate: &Candidate) -> bool {
    candidate.gpu_layers > 0 || candidate.context_size >= HEAVY_CONTEXT_THRESHOLD
}

fn precondition_failure(
    config: &RunnerConfig,
    available_ram_bytes: Option<u64>,
    free_disk_bytes: Option<u64>,
) -> Option<SkipReason> {
    if let Some(available) = available_ram_bytes
        && available < config.min_available_ram_bytes
    {
        return Some(SkipReason::InsufficientRam {
            available_bytes: available,
            required_bytes: config.min_available_ram_bytes,
        });
    }
    if let Some(free) = free_disk_bytes
        && free < config.min_free_disk_bytes
    {
        return Some(SkipReason::InsufficientDisk {
            available_bytes: free,
            required_bytes: config.min_free_disk_bytes,
        });
    }
    None
}

fn skipped_result(candidate: &Candidate, reason: SkipReason) -> CandidateResult {
    CandidateResult {
        candidate_id: candidate.id.clone(),
        attempts: vec![RepetitionAttempt {
            attempt_number: 0,
            is_retry: false,
            sample: None,
            skip_reason: Some(reason.description()),
        }],
        stability: stability::classify(&[]),
        skipped: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::Backend;
    use crate::tuning::candidates::{CandidateDimension, TuningDefaults, TuningPlan};

    fn candidate(id: &str, gpu_layers: u32, context_size: u32) -> Candidate {
        Candidate {
            id: id.to_string(),
            dimension: CandidateDimension::Threads,
            backend: if gpu_layers > 0 {
                Backend::Cuda
            } else {
                Backend::Cpu
            },
            threads: 8,
            gpu_layers,
            context_size,
            batch_size: 512,
            predicted_vram_bytes: None,
            predicted_ram_bytes: None,
        }
    }

    fn plan_of(candidates: Vec<Candidate>) -> TuningPlan {
        TuningPlan {
            formula_version: "test".to_string(),
            model_sha256: "test".to_string(),
            machine_profile_schema_version: "test".to_string(),
            defaults: TuningDefaults {
                threads: 8,
                gpu_layers: 0,
                context_size: 2048,
                batch_size: 512,
            },
            candidates,
            pruned: vec![],
            truncated: false,
        }
    }

    fn good_sample() -> RepetitionSample {
        RepetitionSample {
            succeeded: true,
            timed_out: false,
            cancelled: false,
            crashed: false,
            generation_tokens_per_second: Some(30.0),
            prompt_tokens_per_second: Some(100.0),
        }
    }

    fn no_limits() -> RunnerConfig {
        RunnerConfig {
            repetitions_per_candidate: 3,
            max_retries_per_repetition: 1,
            total_timeout: Duration::from_secs(60),
            min_available_ram_bytes: 0,
            min_free_disk_bytes: 0,
            cooldown_between_heavy_runs: Duration::from_millis(0),
        }
    }

    #[test]
    fn all_successful_repetitions_produce_a_stable_result_for_every_candidate() {
        let plan = plan_of(vec![candidate("a", 0, 2048), candidate("b", 0, 2048)]);
        let summary = execute_plan(
            &plan,
            &no_limits(),
            || Some(u64::MAX),
            || Some(u64::MAX),
            || false,
            |_| good_sample(),
        );

        assert!(!summary.cancelled);
        assert_eq!(summary.candidate_results.len(), 2);
        for result in &summary.candidate_results {
            assert!(!result.skipped);
            assert_eq!(
                result.stability.status,
                stability::TuningStabilityStatus::Stable
            );
            assert_eq!(result.attempts.len(), 3);
        }
    }

    #[test]
    fn cancellation_stops_remaining_candidates_and_marks_the_summary_cancelled() {
        let plan = plan_of(vec![
            candidate("a", 0, 2048),
            candidate("b", 0, 2048),
            candidate("c", 0, 2048),
        ]);
        let mut calls = 0;
        let summary = execute_plan(
            &plan,
            &no_limits(),
            || Some(u64::MAX),
            || Some(u64::MAX),
            move || {
                calls += 1;
                calls > 1
            },
            |_| good_sample(),
        );

        assert!(summary.cancelled);
        assert!(!summary.candidate_results[0].skipped);
        assert!(summary.candidate_results[1].skipped || summary.candidate_results[2].skipped);
    }

    #[test]
    fn insufficient_ram_skips_without_ever_calling_run_repetition() {
        let plan = plan_of(vec![candidate("a", 0, 2048)]);
        let mut run_calls = 0;
        let mut config = no_limits();
        config.min_available_ram_bytes = 1_000_000_000;

        let summary = execute_plan(
            &plan,
            &config,
            || Some(100), // far below the floor
            || Some(u64::MAX),
            || false,
            |_| {
                run_calls += 1;
                good_sample()
            },
        );

        assert_eq!(run_calls, 0);
        let result = &summary.candidate_results[0];
        assert!(
            result
                .attempts
                .iter()
                .any(|a| a.skip_reason.as_deref().is_some_and(|r| r.contains("RAM")))
        );
    }

    #[test]
    fn insufficient_disk_skips_without_ever_calling_run_repetition() {
        let plan = plan_of(vec![candidate("a", 0, 2048)]);
        let mut run_calls = 0;
        let mut config = no_limits();
        config.min_free_disk_bytes = 1_000_000_000;

        let summary = execute_plan(
            &plan,
            &config,
            || Some(u64::MAX),
            || Some(100),
            || false,
            |_| {
                run_calls += 1;
                good_sample()
            },
        );

        assert_eq!(run_calls, 0);
        let result = &summary.candidate_results[0];
        assert!(
            result
                .attempts
                .iter()
                .any(|a| a.skip_reason.as_deref().is_some_and(|r| r.contains("disk")))
        );
    }

    #[test]
    fn a_single_spurious_failure_is_retried_and_can_still_recover() {
        let plan = plan_of(vec![candidate("a", 0, 2048)]);
        let mut call_count = 0;
        let summary = execute_plan(
            &plan,
            &no_limits(),
            || Some(u64::MAX),
            || Some(u64::MAX),
            || false,
            move |_| {
                call_count += 1;
                if call_count == 1 {
                    RepetitionSample {
                        succeeded: false,
                        timed_out: false,
                        cancelled: false,
                        crashed: true,
                        generation_tokens_per_second: None,
                        prompt_tokens_per_second: None,
                    }
                } else {
                    good_sample()
                }
            },
        );

        let result = &summary.candidate_results[0];
        assert!(result.attempts.iter().any(|a| a.is_retry));
        assert_eq!(result.stability.repetitions_succeeded, 3);
    }

    #[test]
    fn a_cancelled_sample_mid_run_stops_the_whole_plan() {
        let plan = plan_of(vec![candidate("a", 0, 2048), candidate("b", 0, 2048)]);
        let summary = execute_plan(
            &plan,
            &no_limits(),
            || Some(u64::MAX),
            || Some(u64::MAX),
            || false,
            |_| RepetitionSample {
                succeeded: false,
                timed_out: false,
                cancelled: true,
                crashed: false,
                generation_tokens_per_second: None,
                prompt_tokens_per_second: None,
            },
        );

        assert!(summary.cancelled);
        assert!(summary.candidate_results[1].skipped);
    }

    #[test]
    fn total_timeout_expiry_skips_remaining_candidates() {
        let plan = plan_of(vec![
            candidate("a", 0, 2048),
            candidate("b", 0, 2048),
            candidate("c", 0, 2048),
        ]);
        let mut config = no_limits();
        config.total_timeout = Duration::from_millis(20);
        config.repetitions_per_candidate = 1;

        let summary = execute_plan(
            &plan,
            &config,
            || Some(u64::MAX),
            || Some(u64::MAX),
            || false,
            |_| {
                std::thread::sleep(Duration::from_millis(30));
                good_sample()
            },
        );

        assert!(
            summary.candidate_results.iter().any(|r| r.skipped),
            "at least one candidate should have been skipped once the total timeout elapsed"
        );
    }

    #[test]
    fn heavy_candidates_incur_a_cooldown_sleep() {
        let plan = plan_of(vec![candidate("a", 10, 2048), candidate("b", 10, 2048)]);
        let mut config = no_limits();
        config.repetitions_per_candidate = 1;
        config.cooldown_between_heavy_runs = Duration::from_millis(50);

        let summary = execute_plan(
            &plan,
            &config,
            || Some(u64::MAX),
            || Some(u64::MAX),
            || false,
            |_| good_sample(),
        );

        assert!(summary.wall_time >= Duration::from_millis(50));
    }

    #[test]
    fn empty_plan_produces_an_empty_summary() {
        let plan = plan_of(vec![]);
        let summary = execute_plan(
            &plan,
            &no_limits(),
            || Some(u64::MAX),
            || Some(u64::MAX),
            || false,
            |_| good_sample(),
        );
        assert!(summary.candidate_results.is_empty());
        assert!(!summary.cancelled);
    }
}
