//! # dev-mutate
//!
//! Mutation testing for Rust. Part of the `dev-*` verification suite.
//!
//! Wraps `cargo-mutants` and emits results as `dev-report::Report`.
//! Mutation testing makes small deliberate changes to your code and
//! checks whether your tests catch them. A test suite that doesn't
//! catch the mutations isn't actually testing what it claims to.
//!
//! ## What is mutation testing?
//!
//! A tool deliberately modifies your code — flipping `<` to `>`,
//! changing `+` to `-`, removing a `return`. Then it runs your tests
//! against each mutation. If your tests still pass with broken code,
//! they aren't really testing the behavior. The "kill rate" measures
//! how many mutations your suite caught.
//!
//! ## Quick example
//!
//! ```no_run
//! use dev_mutate::{MutateRun, MutateThreshold};
//!
//! let run = MutateRun::new("my-crate", "0.1.0");
//! let result = run.execute().unwrap();
//!
//! let threshold = MutateThreshold::min_kill_pct(70.0);
//! let check = result.into_check_result(threshold);
//! ```
//!
//! ## Status
//!
//! Pre-1.0. API shape defined; subprocess integration lands in `0.9.1`.

#![cfg_attr(docsrs, feature(doc_cfg))]
#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

use dev_report::{CheckResult, Evidence, Severity};

/// Configuration for a mutation testing run.
#[derive(Debug, Clone)]
pub struct MutateRun {
    name: String,
    version: String,
}

impl MutateRun {
    /// Begin a new mutation testing run.
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
        }
    }

    /// Execute the run.
    ///
    /// In `0.9.0` this is a stub; subprocess integration lands in `0.9.1`.
    pub fn execute(&self) -> Result<MutateResult, MutateError> {
        Ok(MutateResult {
            name: self.name.clone(),
            version: self.version.clone(),
            mutants_total: 0,
            mutants_killed: 0,
            mutants_survived: 0,
            mutants_timeout: 0,
            survivors: Vec::new(),
        })
    }
}

/// A surviving mutant — one that the test suite did NOT catch.
#[derive(Debug, Clone)]
pub struct SurvivingMutant {
    /// Source file the mutation was applied to.
    pub file: String,
    /// Line number (1-indexed).
    pub line: u32,
    /// Description of the mutation (e.g. "replace `+` with `-`").
    pub description: String,
}

/// Result of a mutation testing run.
#[derive(Debug, Clone)]
pub struct MutateResult {
    /// Crate name.
    pub name: String,
    /// Crate version.
    pub version: String,
    /// Total mutants generated.
    pub mutants_total: u64,
    /// Mutants caught by the test suite.
    pub mutants_killed: u64,
    /// Mutants the test suite missed.
    pub mutants_survived: u64,
    /// Mutants that caused timeouts (counted separately).
    pub mutants_timeout: u64,
    /// Details of surviving mutants.
    pub survivors: Vec<SurvivingMutant>,
}

impl MutateResult {
    /// Kill rate as a percent in `[0.0, 100.0]`.
    ///
    /// Mutants killed divided by total mutants. Timeouts count as
    /// neither killed nor survived (excluded from denominator).
    pub fn kill_pct(&self) -> f64 {
        let counted = self.mutants_killed + self.mutants_survived;
        if counted == 0 {
            return 0.0;
        }
        (self.mutants_killed as f64 / counted as f64) * 100.0
    }
}

/// Threshold defining the minimum acceptable kill rate.
#[derive(Debug, Clone, Copy)]
pub enum MutateThreshold {
    /// Fail if `kill_pct < pct`.
    MinKillPct(f64),
}

impl MutateThreshold {
    /// Build a kill-rate threshold.
    pub fn min_kill_pct(pct: f64) -> Self {
        Self::MinKillPct(pct)
    }
}

impl MutateResult {
    /// Convert this result into a `CheckResult` against the given threshold.
    pub fn into_check_result(self, threshold: MutateThreshold) -> CheckResult {
        let name = format!("mutate::{}", self.name);
        let kill_pct = self.kill_pct();
        let detail = format!(
            "kill rate {:.2}% ({}/{}; {} timeouts; {} survivors)",
            kill_pct,
            self.mutants_killed,
            self.mutants_killed + self.mutants_survived,
            self.mutants_timeout,
            self.mutants_survived
        );
        let check = match threshold {
            MutateThreshold::MinKillPct(target) => {
                if kill_pct < target {
                    CheckResult::fail(name, Severity::Warning).with_detail(detail)
                } else {
                    CheckResult::pass(name).with_detail(detail)
                }
            }
        };
        check.with_evidence(Evidence::numeric("kill_pct", kill_pct))
    }
}

/// Errors that can arise during a mutation testing run.
#[derive(Debug)]
pub enum MutateError {
    /// `cargo-mutants` is not installed.
    ToolNotInstalled,
    /// Subprocess failure.
    SubprocessFailed(String),
    /// Output parsing failure.
    ParseError(String),
}

impl std::fmt::Display for MutateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ToolNotInstalled => write!(f, "cargo-mutants is not installed"),
            Self::SubprocessFailed(s) => write!(f, "subprocess failed: {s}"),
            Self::ParseError(s) => write!(f, "parse error: {s}"),
        }
    }
}

impl std::error::Error for MutateError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_builds() {
        let _ = MutateRun::new("x", "0.1.0");
    }

    #[test]
    fn kill_pct_excludes_timeouts() {
        let r = MutateResult {
            name: "x".into(),
            version: "0.1.0".into(),
            mutants_total: 110,
            mutants_killed: 80,
            mutants_survived: 20,
            mutants_timeout: 10,
            survivors: Vec::new(),
        };
        // 80 / (80 + 20) = 80%, timeouts not counted.
        assert!((r.kill_pct() - 80.0).abs() < 0.0001);
    }

    #[test]
    fn threshold_pass() {
        let r = MutateResult {
            name: "x".into(),
            version: "0.1.0".into(),
            mutants_total: 100,
            mutants_killed: 85,
            mutants_survived: 15,
            mutants_timeout: 0,
            survivors: Vec::new(),
        };
        let c = r.into_check_result(MutateThreshold::min_kill_pct(80.0));
        assert!(matches!(c.verdict, dev_report::Verdict::Pass));
    }

    #[test]
    fn threshold_fail() {
        let r = MutateResult {
            name: "x".into(),
            version: "0.1.0".into(),
            mutants_total: 100,
            mutants_killed: 50,
            mutants_survived: 50,
            mutants_timeout: 0,
            survivors: Vec::new(),
        };
        let c = r.into_check_result(MutateThreshold::min_kill_pct(80.0));
        assert!(matches!(c.verdict, dev_report::Verdict::Fail));
    }

    #[test]
    fn zero_counted_means_zero_pct() {
        let r = MutateResult {
            name: "x".into(),
            version: "0.1.0".into(),
            mutants_total: 0,
            mutants_killed: 0,
            mutants_survived: 0,
            mutants_timeout: 0,
            survivors: Vec::new(),
        };
        assert_eq!(r.kill_pct(), 0.0);
    }
}
