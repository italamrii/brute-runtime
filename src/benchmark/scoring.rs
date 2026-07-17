//! Stability classification derived purely from the spread of repeated
//! benchmark samples plus run outcome - no external heuristics.

use super::metrics::{RunOutcome, StabilityStatus, ThroughputMetric};

const CV_STABLE_MAX: f64 = 0.05;
const CV_MARGINAL_MAX: f64 = 0.15;

pub fn coefficient_of_variation(avg: f64, stddev: f64) -> f64 {
    if avg.abs() < f64::EPSILON {
        0.0
    } else {
        stddev / avg
    }
}

pub fn classify_stability(
    prompt_processing: &ThroughputMetric,
    generation: &ThroughputMetric,
    cli_outcome: &RunOutcome,
    bench_outcome: &RunOutcome,
) -> StabilityStatus {
    if !cli_outcome.succeeded || !bench_outcome.succeeded {
        return StabilityStatus::Unstable;
    }

    let worst_cv = prompt_processing
        .coefficient_of_variation
        .max(generation.coefficient_of_variation);

    if worst_cv <= CV_STABLE_MAX {
        StabilityStatus::Stable
    } else if worst_cv <= CV_MARGINAL_MAX {
        StabilityStatus::Marginal
    } else {
        StabilityStatus::Unstable
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn throughput(avg: f64, stddev: f64) -> ThroughputMetric {
        ThroughputMetric {
            avg_tokens_per_second: avg,
            stddev_tokens_per_second: stddev,
            samples_tokens_per_second: vec![avg],
            coefficient_of_variation: coefficient_of_variation(avg, stddev),
        }
    }

    fn ok_outcome() -> RunOutcome {
        RunOutcome {
            exit_code: Some(0),
            timed_out: false,
            succeeded: true,
        }
    }

    #[test]
    fn low_variance_is_stable() {
        let pp = throughput(100.0, 1.0);
        let tg = throughput(30.0, 0.5);
        let status = classify_stability(&pp, &tg, &ok_outcome(), &ok_outcome());
        assert_eq!(status, StabilityStatus::Stable);
    }

    #[test]
    fn high_variance_is_unstable() {
        let pp = throughput(100.0, 40.0);
        let tg = throughput(30.0, 0.5);
        let status = classify_stability(&pp, &tg, &ok_outcome(), &ok_outcome());
        assert_eq!(status, StabilityStatus::Unstable);
    }

    #[test]
    fn failed_run_is_always_unstable_regardless_of_variance() {
        let pp = throughput(100.0, 0.0);
        let tg = throughput(30.0, 0.0);
        let mut failed = ok_outcome();
        failed.succeeded = false;
        let status = classify_stability(&pp, &tg, &failed, &ok_outcome());
        assert_eq!(status, StabilityStatus::Unstable);
    }

    #[test]
    fn zero_average_does_not_divide_by_zero() {
        assert_eq!(coefficient_of_variation(0.0, 5.0), 0.0);
    }
}
