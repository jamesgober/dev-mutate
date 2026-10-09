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
    use dev_report::Verdict;

    fn assert_single_critical_fail(report: &Report) {
        assert_eq!(report.subject, "self");
        assert_eq!(report.checks.len(), 1);
        let check = &report.checks[0];
        assert_eq!(check.name, "mutate::self");
        assert_eq!(check.verdict, Verdict::Fail);
        assert_eq!(check.severity, Some(Severity::Critical));
        assert!(check.has_tag("subprocess"));
    }

    #[test]
    fn produce_maps_run_failure_to_critical_fail() {
        // An empty directory has no Cargo.toml, so the run fails fast
        // whether or not cargo-mutants is installed.
        let dir = tempfile::tempdir().unwrap();
        let producer = MutateProducer::new(
            MutateRun::new("self", "0.0.0").in_dir(dir.path()),
            MutateThreshold::min_kill_pct(70.0),
        );
        assert_single_critical_fail(&producer.produce());
    }

    #[test]
    fn produce_maps_missing_workdir_to_critical_fail() {
        let producer = MutateProducer::new(
            MutateRun::new("self", "0.0.0").in_dir("definitely/not/a/dir/7f3a"),
            MutateThreshold::min_kill_pct(70.0),
        );
        let report = producer.produce();
        assert_single_critical_fail(&report);
        assert!(report.checks[0]
            .detail
            .as_deref()
            .unwrap_or("")
            .contains("does not exist"));
    }
}
