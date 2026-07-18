//! Stability classification for a *tuning candidate*, derived from
//! repeated independent benchmark attempts (full process launches, not
//! `llama-bench`'s own internal `-r` repetition average - see
//! `benchmark::scoring` for that narrower, single-run classification).
//!
//! Per spec: a single successful run is never treated as proof of
//! stability. `Stable`/`Marginal`/`Unstable` require at least
//! [`MIN_SUCCESSFUL_REPETITIONS_FOR_VARIANCE`] successful repetitions so a
//! coefficient of variation can actually be computed; fewer than that
//! (but at least one success) is `Unknown`, not `Stable`. Zero successes
//! is `Failed`. Zero repetitions attempted at all is `Unknown`.

use serde::Serialize;

/// Bumped whenever the thresholds or the classification logic below
/// change, so a stored runtime profile can record exactly which formula
/// produced its stability verdict.
pub const STABILITY_FORMULA_VERSION: &str = "stage2-stability-v1";

/// Coefficient of variation at or below this is `Stable`.
pub const CV_STABLE_MAX: f64 = 0.05;
/// Coefficient of variation at or below this (and above `CV_STABLE_MAX`)
/// is `Marginal`; above it is `Unstable`.
pub const CV_MARGINAL_MAX: f64 = 0.15;
/// Fewer successful repetitions than this cannot produce a `Stable`,
/// `Marginal`, or `Unstable` verdict - only `Unknown` or `Failed`.
pub const MIN_SUCCESSFUL_REPETITIONS_FOR_VARIANCE: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TuningStabilityStatus {
    Stable,
    Marginal,
    Unstable,
    Failed,
    Unknown,
}

/// One repetition's outcome, as reported by whatever actually launched
/// the benchmark (`tuning::runner` in production, a fixture in tests).
#[derive(Debug, Clone)]
pub struct RepetitionOutcome {
    pub succeeded: bool,
    pub timed_out: bool,
    pub cancelled: bool,
    pub generation_tokens_per_second: Option<f64>,
    pub prompt_tokens_per_second: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StabilityAssessment {
    pub status: TuningStabilityStatus,
    pub formula_version: String,
    pub repetitions_requested: u32,
    pub repetitions_succeeded: u32,
    pub generation_coefficient_of_variation: Option<f64>,
    pub prompt_coefficient_of_variation: Option<f64>,
    pub reason: String,
}

pub fn classify(repetitions: &[RepetitionOutcome]) -> StabilityAssessment {
    let requested = repetitions.len() as u32;

    if repetitions.is_empty() {
        return unknown(0, 0, "no repetitions were run".to_string());
    }

    let succeeded: Vec<&RepetitionOutcome> = repetitions.iter().filter(|r| r.succeeded).collect();
    let succeeded_count = succeeded.len() as u32;

    if succeeded_count == 0 {
        return StabilityAssessment {
            status: TuningStabilityStatus::Failed,
            formula_version: STABILITY_FORMULA_VERSION.to_string(),
            repetitions_requested: requested,
            repetitions_succeeded: 0,
            generation_coefficient_of_variation: None,
            prompt_coefficient_of_variation: None,
            reason: format!(
                "all {requested} repetition(s) failed - {}",
                failure_breakdown(repetitions)
            ),
        };
    }

    if (succeeded_count as usize) < MIN_SUCCESSFUL_REPETITIONS_FOR_VARIANCE {
        return unknown(
            requested,
            succeeded_count,
            format!(
                "only {succeeded_count} successful repetition(s) out of {requested} - at least \
                 {MIN_SUCCESSFUL_REPETITIONS_FOR_VARIANCE} are required to measure run-to-run \
                 variance; a single success is not proof of stability"
            ),
        );
    }

    let generation_values: Vec<f64> = succeeded
        .iter()
        .filter_map(|r| r.generation_tokens_per_second)
        .collect();
    let prompt_values: Vec<f64> = succeeded
        .iter()
        .filter_map(|r| r.prompt_tokens_per_second)
        .collect();

    let generation_cv = coefficient_of_variation(&generation_values);
    let prompt_cv = coefficient_of_variation(&prompt_values);
    let worst_cv = [generation_cv, prompt_cv]
        .into_iter()
        .flatten()
        .fold(0.0_f64, f64::max);

    let any_failures = succeeded_count < requested;

    let status = if any_failures {
        TuningStabilityStatus::Unstable
    } else if worst_cv <= CV_STABLE_MAX {
        TuningStabilityStatus::Stable
    } else if worst_cv <= CV_MARGINAL_MAX {
        TuningStabilityStatus::Marginal
    } else {
        TuningStabilityStatus::Unstable
    };

    let reason = if any_failures {
        format!(
            "{succeeded_count}/{requested} repetitions succeeded ({}) - a partial failure across \
             repeated runs is never classified as stable, regardless of the throughput variance \
             among the runs that did succeed",
            failure_breakdown(repetitions)
        )
    } else {
        format!(
            "{succeeded_count}/{requested} repetitions succeeded; worst-case coefficient of \
             variation {worst_cv:.4} (stable <= {CV_STABLE_MAX}, marginal <= {CV_MARGINAL_MAX})"
        )
    };

    StabilityAssessment {
        status,
        formula_version: STABILITY_FORMULA_VERSION.to_string(),
        repetitions_requested: requested,
        repetitions_succeeded: succeeded_count,
        generation_coefficient_of_variation: generation_cv,
        prompt_coefficient_of_variation: prompt_cv,
        reason,
    }
}

/// Breaks the non-succeeded repetitions down by cause - distinguishing a
/// timeout from a cancellation from an outright crash/non-zero-exit
/// matters for a human reading the reason text (per spec section 7,
/// "timeouts, crashes" are meant to be visible in the classification,
/// not folded into one generic "failed").
fn failure_breakdown(repetitions: &[RepetitionOutcome]) -> String {
    let failed: Vec<&RepetitionOutcome> = repetitions.iter().filter(|r| !r.succeeded).collect();
    let timed_out = failed.iter().filter(|r| r.timed_out).count();
    let cancelled = failed.iter().filter(|r| r.cancelled).count();
    let crashed = failed.len() - timed_out - cancelled;

    let mut parts = Vec::new();
    if timed_out > 0 {
        parts.push(format!("{timed_out} timed out"));
    }
    if cancelled > 0 {
        parts.push(format!("{cancelled} cancelled"));
    }
    if crashed > 0 {
        parts.push(format!("{crashed} crashed/exited non-zero"));
    }
    if parts.is_empty() {
        "no failures".to_string()
    } else {
        parts.join(", ")
    }
}

fn unknown(requested: u32, succeeded: u32, reason: String) -> StabilityAssessment {
    StabilityAssessment {
        status: TuningStabilityStatus::Unknown,
        formula_version: STABILITY_FORMULA_VERSION.to_string(),
        repetitions_requested: requested,
        repetitions_succeeded: succeeded,
        generation_coefficient_of_variation: None,
        prompt_coefficient_of_variation: None,
        reason,
    }
}

fn coefficient_of_variation(values: &[f64]) -> Option<f64> {
    if values.len() < MIN_SUCCESSFUL_REPETITIONS_FOR_VARIANCE {
        return None;
    }
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    if mean.abs() < f64::EPSILON {
        return Some(0.0);
    }
    let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / values.len() as f64;
    Some(variance.sqrt() / mean)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(gen_tps: f64, prompt_tps: f64) -> RepetitionOutcome {
        RepetitionOutcome {
            succeeded: true,
            timed_out: false,
            cancelled: false,
            generation_tokens_per_second: Some(gen_tps),
            prompt_tokens_per_second: Some(prompt_tps),
        }
    }

    fn failed() -> RepetitionOutcome {
        RepetitionOutcome {
            succeeded: false,
            timed_out: false,
            cancelled: false,
            generation_tokens_per_second: None,
            prompt_tokens_per_second: None,
        }
    }

    #[test]
    fn no_repetitions_is_unknown() {
        let result = classify(&[]);
        assert_eq!(result.status, TuningStabilityStatus::Unknown);
        assert_eq!(result.repetitions_requested, 0);
    }

    #[test]
    fn single_success_is_unknown_not_stable() {
        let result = classify(&[ok(30.0, 100.0)]);
        assert_eq!(result.status, TuningStabilityStatus::Unknown);
        assert_eq!(result.repetitions_succeeded, 1);
    }

    #[test]
    fn all_failed_is_failed() {
        let result = classify(&[failed(), failed(), failed()]);
        assert_eq!(result.status, TuningStabilityStatus::Failed);
        assert_eq!(result.repetitions_succeeded, 0);
    }

    #[test]
    fn low_variance_repeated_successes_is_stable() {
        let result = classify(&[ok(30.0, 100.0), ok(30.2, 99.8), ok(29.9, 100.1)]);
        assert_eq!(result.status, TuningStabilityStatus::Stable);
        assert_eq!(result.repetitions_succeeded, 3);
    }

    #[test]
    fn moderate_variance_is_marginal() {
        // ~10% spread in generation throughput.
        let result = classify(&[ok(30.0, 100.0), ok(33.0, 100.0), ok(27.0, 100.0)]);
        assert_eq!(result.status, TuningStabilityStatus::Marginal);
    }

    #[test]
    fn high_variance_is_unstable() {
        let result = classify(&[ok(30.0, 100.0), ok(60.0, 100.0), ok(10.0, 100.0)]);
        assert_eq!(result.status, TuningStabilityStatus::Unstable);
    }

    #[test]
    fn a_partial_failure_among_repetitions_is_unstable_even_with_tight_variance_on_successes() {
        let result = classify(&[ok(30.0, 100.0), ok(30.1, 100.0), failed()]);
        assert_eq!(result.status, TuningStabilityStatus::Unstable);
        assert_eq!(result.repetitions_succeeded, 2);
        assert_eq!(result.repetitions_requested, 3);
    }

    #[test]
    fn cancelled_repetitions_count_as_not_succeeded() {
        let cancelled = RepetitionOutcome {
            succeeded: false,
            timed_out: false,
            cancelled: true,
            generation_tokens_per_second: None,
            prompt_tokens_per_second: None,
        };
        let result = classify(&[cancelled]);
        assert_eq!(result.status, TuningStabilityStatus::Failed);
    }

    #[test]
    fn zero_average_throughput_does_not_divide_by_zero() {
        assert_eq!(coefficient_of_variation(&[0.0, 0.0]), Some(0.0));
    }

    #[test]
    fn formula_version_is_recorded_on_every_assessment() {
        for outcomes in [
            vec![],
            vec![ok(30.0, 100.0)],
            vec![ok(30.0, 100.0), ok(30.0, 100.0)],
            vec![failed()],
        ] {
            let result = classify(&outcomes);
            assert_eq!(result.formula_version, STABILITY_FORMULA_VERSION);
        }
    }
}
