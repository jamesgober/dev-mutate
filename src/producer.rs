//! [`Producer`] integration: wrap a [`MutateRun`] + [`MutateThreshold`]
//! and emit a [`Report`] every time the producer runs.
//!
//! [`Producer`]: dev_report::Producer
//! [`Report`]: dev_report::Report

use dev_report::{CheckResult, Producer, Report, Severity};

use crate::{MutateRun, MutateThreshold};

/// `Producer` adapter that drives a [`MutateRun`] and converts the
/// result into a `Report` against the configured [`MutateThreshold`].
///
/// Subprocess failures map to a single failing `CheckResult` named
/// `mutate::<subject>` with `Severity::Critical`. No panics.
///
/// # Example
///
/// ```no_run
/// use dev_mutate::{MutateProducer, MutateRun, MutateThreshold};
/// use dev_report::Producer;
///
/// let producer = MutateProducer::new(
///     MutateRun::new("my-crate", "0.1.0"),
///     MutateThreshold::min_kill_pct(70.0),
/// );
/// let report = producer.produce();
/// println!("{}", report.to_json().unwrap());
/// ```
pub struct MutateProducer {
    run: MutateRun,
    threshold: MutateThreshold,
}

impl MutateProducer {
    /// Build a producer with the given run + threshold.
    pub fn new(run: MutateRun, threshold: MutateThreshold) -> Self {
        Self { run, threshold }
    }
}

impl Producer for MutateProducer {
    fn produce(&self) -> Report {
        let subject = self.run.subject().to_string();
        let version = self.run.subject_version().to_string();
        let mut report = Report::new(&subject, &version).with_producer("dev-mutate");
        match self.run.execute() {
            Ok(result) => {
                report.push(result.into_check_result(self.threshold));
            }
            Err(e) => {
                let check = CheckResult::fail(format!("mutate::{subject}"), Severity::Critical)
                    .with_detail(e.to_string())
                    .with_tag("mutate")
                    .with_tag("subprocess");
                report.push(check);
            }
        }
        report.finish();
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "spawns inner `cargo mutants` which deadlocks on the workspace target-dir lock when run from `cargo test`; run with CARGO_TARGET_DIR outside the workspace via `cargo test -- --ignored`"]
    fn produce_returns_report_regardless_of_subprocess() {
        // Contract: returns a Report, never panics.
        let producer = MutateProducer::new(
            MutateRun::new("self", "0.0.0"),
            MutateThreshold::min_kill_pct(70.0),
        );
        let report = producer.produce();
        assert_eq!(report.subject, "self");
        assert!(!report.checks.is_empty());
    }
}
