//! Configuration Ranking: turns a `tuning::runner::TuningRunSummary` into
//! an ordered list of viable configurations for a chosen priority, plus a
//! winner, runner-up, and a separate "safer fallback".
//!
//! Ranking is **lexicographic**, not a single weighted sum: each priority
//! defines an ordered sequence of comparison tiers, and a candidate can
//! only win on a later tier once every earlier tier is tied. This is a
//! deliberate choice over a weighted linear score (the approach Stage 1's
//! `recommend` module uses for *model* selection) - a weighted sum cannot
//! guarantee an ordering property for arbitrary magnitudes, but a tiered
//! comparator does by construction.
//!
//! Only `Balanced` (the default) puts stability ahead of raw throughput,
//! matching the spec requirement that "a marginally faster unstable
//! configuration must NOT outrank a stable safe one *by default*". The
//! other priorities put their named criterion first and stability as a
//! tiebreaker instead - if the user explicitly asks for "fastest
//! generation" they get the fastest completed candidate, but
//! `safer_fallback` always separately surfaces the best genuinely
//! `Stable` alternative so that choice is never silent. Every priority
//! still refuses to let an incomplete (zero successful repetitions)
//! candidate win at all - that gate is first in every tier list.

use super::candidates::{Candidate, TuningPlan};
use super::runner::{CandidateResult, TuningRunSummary};
use super::stability::TuningStabilityStatus;
use serde::Serialize;
use std::cmp::Ordering;
use std::collections::HashMap;

pub const TUNING_RANKING_FORMULA_VERSION: &str = "stage2-ranking-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum RankingPriority {
    Balanced,
    FastestGeneration,
    FastestPromptProcessing,
    LowestMemory,
    LongestContext,
    MaximumStability,
    LaptopFriendly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RankingConfidence {
    /// The winner completed successfully and was classified `Stable`.
    High,
    /// The winner completed but was only `Marginal` (or repetitions were
    /// too few to classify variance at all).
    Medium,
    /// No candidate completed, or the winner is `Unstable`/`Failed`.
    Low,
}

#[derive(Debug, Clone, Serialize)]
pub struct CandidateMeasurements {
    pub candidate_id: String,
    pub completed: bool,
    pub stability: TuningStabilityStatus,
    pub backend: crate::runtime::Backend,
    pub mean_generation_tokens_per_second: Option<f64>,
    pub mean_prompt_tokens_per_second: Option<f64>,
    pub context_size: u32,
    pub batch_size: u32,
    pub gpu_layers: u32,
    pub threads: u32,
    pub predicted_ram_bytes: Option<u64>,
    pub predicted_vram_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RankedCandidate {
    pub candidate_id: String,
    pub rank: usize,
    pub measurements: CandidateMeasurements,
}

#[derive(Debug, Clone, Serialize)]
pub struct RejectedCandidate {
    pub candidate_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RankingResult {
    pub formula_version: String,
    pub priority: RankingPriority,
    pub winner: Option<RankedCandidate>,
    pub runner_up: Option<RankedCandidate>,
    /// The best `Stable` completed candidate, independent of the winner.
    /// Equals the winner whenever the winner is itself `Stable`; is `None`
    /// only when no candidate both completed and was classified `Stable`.
    pub safer_fallback: Option<RankedCandidate>,
    /// Candidates with strictly higher measured generation throughput
    /// than the winner that were nonetheless ranked below it, with the
    /// reason - this is what makes a "faster but riskier" rejection
    /// explicit rather than silent.
    pub rejected_faster_candidates: Vec<RejectedCandidate>,
    pub confidence: RankingConfidence,
    /// Human-readable notes on what the winner's measurements do *not*
    /// cover (e.g. VRAM was never measured) - never silently omitted.
    pub unknown_values: Vec<String>,
    pub ordered: Vec<RankedCandidate>,
    pub model_sha256: String,
    pub machine_profile_schema_version: String,
}

/// Ranks every non-skipped candidate result in `summary` against its
/// generating `plan` for `priority`. Skipped candidates (safety-guard
/// skips - insufficient RAM/disk, cancelled, timed out at the plan level)
/// are excluded entirely rather than ranked as "incomplete", since they
/// were never actually attempted.
pub fn rank(
    plan: &TuningPlan,
    summary: &TuningRunSummary,
    priority: RankingPriority,
) -> RankingResult {
    let candidates_by_id: HashMap<&str, &Candidate> =
        plan.candidates.iter().map(|c| (c.id.as_str(), c)).collect();

    let mut measured: Vec<CandidateMeasurements> = summary
        .candidate_results
        .iter()
        .filter(|r| !r.skipped)
        .filter_map(|r| {
            candidates_by_id
                .get(r.candidate_id.as_str())
                .map(|c| measurements(c, r))
        })
        .collect();

    measured.sort_by(|a, b| compare_keys(&rank_key(priority, a), &rank_key(priority, b)).reverse());

    let ordered: Vec<RankedCandidate> = measured
        .into_iter()
        .enumerate()
        .map(|(i, m)| RankedCandidate {
            candidate_id: m.candidate_id.clone(),
            rank: i + 1,
            measurements: m,
        })
        .collect();

    // `completed` is always the first (dominant) tier for every priority,
    // so `ordered` already groups every completed candidate ahead of
    // every incomplete one - but filtering explicitly here means a
    // winner/runner-up is never handed out when nothing actually
    // completed, even if `ordered` itself is non-empty.
    let winner = ordered.iter().find(|c| c.measurements.completed).cloned();
    let runner_up = ordered
        .iter()
        .filter(|c| c.measurements.completed)
        .nth(1)
        .cloned();

    let safer_fallback = match &winner {
        Some(w) if w.measurements.stability == TuningStabilityStatus::Stable => Some(w.clone()),
        _ => ordered
            .iter()
            .find(|c| c.measurements.stability == TuningStabilityStatus::Stable)
            .cloned(),
    };

    let rejected_faster_candidates = winner
        .as_ref()
        .map(|w| rejected_faster_than(w, &ordered))
        .unwrap_or_default();

    let confidence = match &winner {
        None => RankingConfidence::Low,
        Some(w) => match w.measurements.stability {
            TuningStabilityStatus::Stable => RankingConfidence::High,
            TuningStabilityStatus::Marginal => RankingConfidence::Medium,
            TuningStabilityStatus::Unstable
            | TuningStabilityStatus::Failed
            | TuningStabilityStatus::Unknown => RankingConfidence::Low,
        },
    };

    let unknown_values = winner
        .as_ref()
        .map(|w| unknown_fields(&w.measurements))
        .unwrap_or_default();

    RankingResult {
        formula_version: TUNING_RANKING_FORMULA_VERSION.to_string(),
        priority,
        winner,
        runner_up,
        safer_fallback,
        rejected_faster_candidates,
        confidence,
        unknown_values,
        ordered,
        model_sha256: plan.model_sha256.clone(),
        machine_profile_schema_version: plan.machine_profile_schema_version.clone(),
    }
}

fn measurements(candidate: &Candidate, result: &CandidateResult) -> CandidateMeasurements {
    let successful: Vec<&super::runner::RepetitionSample> = result
        .attempts
        .iter()
        .filter_map(|a| a.sample.as_ref())
        .filter(|s| s.succeeded)
        .collect();

    CandidateMeasurements {
        candidate_id: candidate.id.clone(),
        completed: !successful.is_empty(),
        stability: result.stability.status,
        backend: candidate.backend,
        mean_generation_tokens_per_second: mean_of(
            successful
                .iter()
                .filter_map(|s| s.generation_tokens_per_second),
        ),
        mean_prompt_tokens_per_second: mean_of(
            successful.iter().filter_map(|s| s.prompt_tokens_per_second),
        ),
        context_size: candidate.context_size,
        batch_size: candidate.batch_size,
        gpu_layers: candidate.gpu_layers,
        threads: candidate.threads,
        predicted_ram_bytes: candidate.predicted_ram_bytes,
        predicted_vram_bytes: candidate.predicted_vram_bytes,
    }
}

fn mean_of(values: impl Iterator<Item = f64>) -> Option<f64> {
    let values: Vec<f64> = values.collect();
    if values.is_empty() {
        None
    } else {
        Some(values.iter().sum::<f64>() / values.len() as f64)
    }
}

fn stability_rank(status: TuningStabilityStatus) -> f64 {
    match status {
        TuningStabilityStatus::Stable => 4.0,
        TuningStabilityStatus::Marginal => 3.0,
        TuningStabilityStatus::Unknown => 2.0,
        TuningStabilityStatus::Unstable => 1.0,
        TuningStabilityStatus::Failed => 0.0,
    }
}

fn stability_label(status: TuningStabilityStatus) -> &'static str {
    match status {
        TuningStabilityStatus::Stable => "stable",
        TuningStabilityStatus::Marginal => "marginal",
        TuningStabilityStatus::Unstable => "unstable",
        TuningStabilityStatus::Failed => "failed",
        TuningStabilityStatus::Unknown => "unknown",
    }
}

/// Every value in the returned tier list is oriented so **higher is
/// always better** - callers compare tier lists lexicographically
/// left-to-right. Unmeasured/unsafe values fall back to `f64::MIN` so an
/// unmeasured candidate never outranks a measured one on that tier.
fn rank_key(priority: RankingPriority, m: &CandidateMeasurements) -> Vec<f64> {
    let completed = if m.completed { 1.0 } else { 0.0 };
    let stability = stability_rank(m.stability);
    // Lower predicted RAM usage is safer - negate so "higher is better".
    let ram_safety = m
        .predicted_ram_bytes
        .map(|b| -(b as f64))
        .unwrap_or(f64::MIN);
    let generation = m.mean_generation_tokens_per_second.unwrap_or(f64::MIN);
    let prompt = m.mean_prompt_tokens_per_second.unwrap_or(f64::MIN);
    let context = m.context_size as f64;
    // Fewer threads used is more laptop/battery-friendly - negate.
    let fewer_threads = -(m.threads as f64);

    match priority {
        RankingPriority::Balanced => vec![
            completed,
            stability,
            ram_safety,
            generation,
            prompt,
            context,
            fewer_threads,
        ],
        RankingPriority::FastestGeneration => {
            vec![completed, generation, stability, prompt, ram_safety]
        }
        RankingPriority::FastestPromptProcessing => {
            vec![completed, prompt, stability, generation, ram_safety]
        }
        RankingPriority::LowestMemory => vec![completed, ram_safety, stability, generation],
        RankingPriority::LongestContext => vec![completed, context, stability, generation],
        RankingPriority::MaximumStability => vec![completed, stability, generation, prompt],
        RankingPriority::LaptopFriendly => {
            vec![completed, ram_safety, fewer_threads, stability, generation]
        }
    }
}

fn compare_keys(a: &[f64], b: &[f64]) -> Ordering {
    for (x, y) in a.iter().zip(b.iter()) {
        let ord = x.total_cmp(y);
        if ord != Ordering::Equal {
            return ord;
        }
    }
    Ordering::Equal
}

fn rejected_faster_than(
    winner: &RankedCandidate,
    ordered: &[RankedCandidate],
) -> Vec<RejectedCandidate> {
    let Some(winner_generation) = winner.measurements.mean_generation_tokens_per_second else {
        return vec![];
    };

    ordered
        .iter()
        .filter(|c| c.candidate_id != winner.candidate_id)
        .filter_map(|c| {
            let candidate_generation = c.measurements.mean_generation_tokens_per_second?;
            if candidate_generation <= winner_generation {
                return None;
            }
            Some(RejectedCandidate {
                candidate_id: c.candidate_id.clone(),
                reason: format!(
                    "measured {:.2} tok/s generation (faster than the winner's {:.2} tok/s) but \
                     ranked below it because its stability was classified {} versus the \
                     winner's {} (formula {TUNING_RANKING_FORMULA_VERSION})",
                    candidate_generation,
                    winner_generation,
                    stability_label(c.measurements.stability),
                    stability_label(winner.measurements.stability),
                ),
            })
        })
        .collect()
}

fn unknown_fields(m: &CandidateMeasurements) -> Vec<String> {
    let mut unknown = Vec::new();
    if m.mean_generation_tokens_per_second.is_none() {
        unknown.push("generation throughput was not measured".to_string());
    }
    if m.mean_prompt_tokens_per_second.is_none() {
        unknown.push("prompt-processing throughput was not measured".to_string());
    }
    if m.predicted_ram_bytes.is_none() {
        unknown.push("predicted RAM usage is unknown".to_string());
    }
    if m.gpu_layers > 0 && m.predicted_vram_bytes.is_none() {
        unknown.push("predicted VRAM usage is unknown".to_string());
    }
    unknown
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::Backend;
    use crate::tuning::candidates::{CandidateDimension, TuningDefaults};
    use crate::tuning::runner::{RepetitionAttempt, RepetitionSample};
    use crate::tuning::stability::StabilityAssessment;
    use std::time::Duration;

    fn candidate(
        id: &str,
        gpu_layers: u32,
        context_size: u32,
        threads: u32,
        predicted_ram: Option<u64>,
    ) -> Candidate {
        Candidate {
            id: id.to_string(),
            dimension: CandidateDimension::Threads,
            backend: if gpu_layers > 0 {
                Backend::Cuda
            } else {
                Backend::Cpu
            },
            threads,
            gpu_layers,
            context_size,
            batch_size: 512,
            predicted_vram_bytes: None,
            predicted_ram_bytes: predicted_ram,
        }
    }

    fn plan_of(candidates: Vec<Candidate>) -> TuningPlan {
        TuningPlan {
            formula_version: "test".to_string(),
            model_sha256: "test-sha".to_string(),
            machine_profile_schema_version: "test-schema".to_string(),
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

    fn stable_result(id: &str, generation_tps: f64) -> CandidateResult {
        completed_result(id, generation_tps, TuningStabilityStatus::Stable, 3, 3)
    }

    fn marginal_result(id: &str, generation_tps: f64) -> CandidateResult {
        completed_result(id, generation_tps, TuningStabilityStatus::Marginal, 3, 3)
    }

    fn completed_result(
        id: &str,
        generation_tps: f64,
        status: TuningStabilityStatus,
        succeeded: u32,
        requested: u32,
    ) -> CandidateResult {
        CandidateResult {
            candidate_id: id.to_string(),
            attempts: vec![RepetitionAttempt {
                attempt_number: 1,
                is_retry: false,
                sample: Some(RepetitionSample {
                    succeeded: true,
                    timed_out: false,
                    cancelled: false,
                    crashed: false,
                    generation_tokens_per_second: Some(generation_tps),
                    prompt_tokens_per_second: Some(generation_tps * 3.0),
                }),
                skip_reason: None,
            }],
            stability: StabilityAssessment {
                status,
                formula_version: "test".to_string(),
                repetitions_requested: requested,
                repetitions_succeeded: succeeded,
                generation_coefficient_of_variation: None,
                prompt_coefficient_of_variation: None,
                reason: "test".to_string(),
            },
            skipped: false,
        }
    }

    fn failed_result(id: &str) -> CandidateResult {
        CandidateResult {
            candidate_id: id.to_string(),
            attempts: vec![RepetitionAttempt {
                attempt_number: 1,
                is_retry: false,
                sample: Some(RepetitionSample {
                    succeeded: false,
                    timed_out: false,
                    cancelled: false,
                    crashed: true,
                    generation_tokens_per_second: None,
                    prompt_tokens_per_second: None,
                }),
                skip_reason: None,
            }],
            stability: StabilityAssessment {
                status: TuningStabilityStatus::Failed,
                formula_version: "test".to_string(),
                repetitions_requested: 1,
                repetitions_succeeded: 0,
                generation_coefficient_of_variation: None,
                prompt_coefficient_of_variation: None,
                reason: "test".to_string(),
            },
            skipped: false,
        }
    }

    fn summary_of(results: Vec<CandidateResult>) -> TuningRunSummary {
        TuningRunSummary {
            candidate_results: results,
            cancelled: false,
            wall_time: Duration::from_secs(0),
        }
    }

    #[test]
    fn balanced_priority_never_lets_a_faster_unstable_candidate_outrank_a_stable_one() {
        let plan = plan_of(vec![
            candidate("slow_stable", 0, 2048, 8, Some(1_000_000)),
            candidate("fast_unstable", 0, 2048, 8, Some(1_000_000)),
        ]);
        let summary = summary_of(vec![
            stable_result("slow_stable", 20.0),
            completed_result("fast_unstable", 50.0, TuningStabilityStatus::Unstable, 2, 3),
        ]);

        let result = rank(&plan, &summary, RankingPriority::Balanced);

        assert_eq!(result.winner.unwrap().candidate_id, "slow_stable");
        assert_eq!(result.rejected_faster_candidates.len(), 1);
        assert_eq!(
            result.rejected_faster_candidates[0].candidate_id,
            "fast_unstable"
        );
    }

    #[test]
    fn fastest_generation_priority_picks_raw_speed_even_when_only_marginal() {
        let plan = plan_of(vec![
            candidate("slow_stable", 0, 2048, 8, None),
            candidate("fast_marginal", 0, 2048, 8, None),
        ]);
        let summary = summary_of(vec![
            stable_result("slow_stable", 20.0),
            marginal_result("fast_marginal", 50.0),
        ]);

        let result = rank(&plan, &summary, RankingPriority::FastestGeneration);

        let winner = result.winner.clone().unwrap();
        assert_eq!(winner.candidate_id, "fast_marginal");
        assert_eq!(result.confidence, RankingConfidence::Medium);
        // The safer fallback must differ from the winner here - it should
        // point back at the proven-stable, slower candidate.
        let fallback = result.safer_fallback.unwrap();
        assert_eq!(fallback.candidate_id, "slow_stable");
        assert_ne!(fallback.candidate_id, winner.candidate_id);
    }

    #[test]
    fn incomplete_candidates_never_win_regardless_of_priority() {
        let plan = plan_of(vec![
            candidate("never_completed", 0, 2048, 8, None),
            candidate("completed_ok", 0, 2048, 8, None),
        ]);
        let summary = summary_of(vec![
            failed_result("never_completed"),
            stable_result("completed_ok", 5.0),
        ]);

        for priority in [
            RankingPriority::Balanced,
            RankingPriority::FastestGeneration,
            RankingPriority::LowestMemory,
            RankingPriority::LongestContext,
            RankingPriority::MaximumStability,
            RankingPriority::LaptopFriendly,
        ] {
            let result = rank(&plan, &summary, priority);
            assert_eq!(
                result.winner.unwrap().candidate_id,
                "completed_ok",
                "priority {priority:?} let an incomplete candidate win"
            );
        }
    }

    #[test]
    fn winner_and_safer_fallback_are_the_same_when_the_winner_is_already_stable() {
        let plan = plan_of(vec![candidate("only", 0, 2048, 8, None)]);
        let summary = summary_of(vec![stable_result("only", 10.0)]);

        let result = rank(&plan, &summary, RankingPriority::Balanced);
        let winner = result.winner.clone().unwrap();
        let fallback = result.safer_fallback.unwrap();
        assert_eq!(winner.candidate_id, fallback.candidate_id);
        assert_eq!(result.confidence, RankingConfidence::High);
    }

    #[test]
    fn no_completed_candidates_means_no_winner_and_low_confidence() {
        let plan = plan_of(vec![candidate("dead", 0, 2048, 8, None)]);
        let summary = summary_of(vec![failed_result("dead")]);

        let result = rank(&plan, &summary, RankingPriority::Balanced);
        assert!(result.winner.is_none());
        assert!(result.safer_fallback.is_none());
        assert_eq!(result.confidence, RankingConfidence::Low);
        assert!(result.rejected_faster_candidates.is_empty());
    }

    #[test]
    fn ranks_are_sequential_starting_at_one() {
        let plan = plan_of(vec![
            candidate("a", 0, 2048, 8, None),
            candidate("b", 0, 2048, 8, None),
            candidate("c", 0, 2048, 8, None),
        ]);
        let summary = summary_of(vec![
            stable_result("a", 10.0),
            stable_result("b", 30.0),
            stable_result("c", 20.0),
        ]);

        let result = rank(&plan, &summary, RankingPriority::FastestGeneration);
        let ranks: Vec<usize> = result.ordered.iter().map(|c| c.rank).collect();
        assert_eq!(ranks, vec![1, 2, 3]);
        assert_eq!(result.ordered[0].candidate_id, "b");
        assert_eq!(result.ordered[1].candidate_id, "c");
        assert_eq!(result.ordered[2].candidate_id, "a");
    }

    #[test]
    fn unknown_values_reports_missing_measurements_honestly() {
        let plan = plan_of(vec![candidate("gpu_no_vram", 10, 2048, 8, None)]);
        let summary = summary_of(vec![stable_result("gpu_no_vram", 10.0)]);

        let result = rank(&plan, &summary, RankingPriority::Balanced);
        let unknown = result.unknown_values;
        assert!(unknown.iter().any(|u| u.contains("RAM")));
        assert!(unknown.iter().any(|u| u.contains("VRAM")));
    }

    #[test]
    fn skipped_candidates_are_excluded_from_ranking_entirely() {
        use crate::tuning::runner::SkipReason;

        let plan = plan_of(vec![
            candidate("skipped", 0, 2048, 8, None),
            candidate("ran", 0, 2048, 8, None),
        ]);
        let summary = summary_of(vec![
            CandidateResult {
                candidate_id: "skipped".to_string(),
                attempts: vec![RepetitionAttempt {
                    attempt_number: 0,
                    is_retry: false,
                    sample: None,
                    skip_reason: Some(SkipReason::Cancelled.description()),
                }],
                stability: crate::tuning::stability::classify(&[]),
                skipped: true,
            },
            stable_result("ran", 10.0),
        ]);

        let result = rank(&plan, &summary, RankingPriority::Balanced);
        assert_eq!(result.ordered.len(), 1);
        assert_eq!(result.ordered[0].candidate_id, "ran");
    }

    #[test]
    fn formula_version_is_stamped_on_every_result() {
        let plan = plan_of(vec![candidate("a", 0, 2048, 8, None)]);
        let summary = summary_of(vec![stable_result("a", 10.0)]);
        let result = rank(&plan, &summary, RankingPriority::Balanced);
        assert_eq!(result.formula_version, TUNING_RANKING_FORMULA_VERSION);
    }
}
