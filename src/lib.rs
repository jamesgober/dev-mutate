//! # dev-mutate
//!
//! Mutation testing for Rust. Wraps [`cargo-mutants`][cargo-mutants]
//! and emits results as [`dev_report::Report`].
//!
//! Mutation testing makes small deliberate changes to your code —
//! flipping `<` to `>`, changing `+` to `-`, removing a `return`. It
//! then runs your tests against each mutation. A test suite that
//! catches the mutations *kills* them; surviving mutations are
//! evidence the suite isn't actually testing that behavior.
//!
//! ## Kill rate
//!
//! ```text
//! kill_pct = killed / (killed + survived) * 100
//! ```
//!
//! Timeouts MUST NOT count toward either numerator or denominator —
//! they don't reflect test quality (REPS § 3).
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
//! ## Requirements
//!
//! ```text
//! cargo install cargo-mutants
//! ```
//!
//! The crate detects absence and surfaces
//! [`MutateError::ToolNotInstalled`] without panicking.
//!
//! [cargo-mutants]: https://crates.io/crates/cargo-mutants

#![cfg_attr(docsrs, feature(doc_cfg))]
#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use dev_report::{CheckResult, Evidence, Severity};
use serde::{Deserialize, Serialize};

mod producer;
mod runner;

pub use producer::MutateProducer;

// ---------------------------------------------------------------------------
// MutateRun
// ---------------------------------------------------------------------------

/// Configuration for a mutation testing run.
///
/// # Example
///
/// ```no_run
/// use dev_mutate::MutateRun;
/// use std::time::Duration;
///
/// let run = MutateRun::new("my-crate", "0.1.0")
///     .workspace()
///     .jobs(4)
///     .timeout(Duration::from_secs(120))
///     .exclude_re(r"^src/generated/");
///
/// let _result = run.execute().unwrap();
/// ```
#[derive(Debug, Clone)]
pub struct MutateRun {
    name: String,
    version: String,
    workdir: Option<PathBuf>,
    workspace: bool,
    jobs: Option<u32>,
    timeout: Option<Duration>,
    exclude_re: Vec<String>,
    file_filters: Vec<String>,
    allow_list: Vec<String>,
}

impl MutateRun {
    /// Begin a new mutation testing run.
    ///
    /// `name` and `version` are descriptive — they identify the
    /// subject in the produced `Report`.
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            workdir: None,
            workspace: false,
            jobs: None,
            timeout: None,
            exclude_re: Vec::new(),
            file_filters: Vec::new(),
            allow_list: Vec::new(),
        }
    }

    /// Run `cargo mutants` from `dir` instead of the current directory.
    pub fn in_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.workdir = Some(dir.into());
        self
    }

    /// Pass `--workspace` so every workspace member is mutated.
    pub fn workspace(mut self) -> Self {
        self.workspace = true;
        self
    }

    /// Pass `--jobs <N>` (parallel mutation runs). Maps to cargo-mutants'
    /// own job limit.
    pub fn jobs(mut self, n: u32) -> Self {
        self.jobs = Some(n);
        self
    }

    /// Per-mutant timeout (passed through as `--timeout <secs>`).
    pub fn timeout(mut self, d: Duration) -> Self {
        self.timeout = Some(d);
        self
    }

    /// Skip files matching the given regex. Repeatable; maps to
    /// `--exclude-re <pattern>` per invocation.
    pub fn exclude_re(mut self, pattern: impl Into<String>) -> Self {
        self.exclude_re.push(pattern.into());
        self
    }

    /// Restrict to files matching the given glob / path. Repeatable;
    /// maps to `--file <pattern>` per invocation.
    pub fn file(mut self, pattern: impl Into<String>) -> Self {
        self.file_filters.push(pattern.into());
        self
    }

    /// Suppress a known-survivor by descriptor. The match is on the
    /// `description` field of `SurvivingMutant` (e.g. `"replace + with -"`).
    pub fn allow(mut self, description: impl Into<String>) -> Self {
        self.allow_list.push(description.into());
        self
    }

    /// Bulk version of [`allow`](Self::allow).
    pub fn allow_all<I, S>(mut self, descriptions: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.allow_list
            .extend(descriptions.into_iter().map(Into::into));
        self
    }

    /// Subject name passed in via [`new`](Self::new).
    pub fn subject(&self) -> &str {
        &self.name
    }

    /// Subject version passed in via [`new`](Self::new).
    pub fn subject_version(&self) -> &str {
        &self.version
    }

    /// Execute the run.
    ///
    /// Spawns `cargo mutants --json` with the configured flags. Tool
    /// absence, subprocess failure, and parse failure surface as
    /// typed [`MutateError`] variants. No panics.
    pub fn execute(&self) -> Result<MutateResult, MutateError> {
        runner::run(self)
    }

    pub(crate) fn workdir_path(&self) -> Option<&std::path::Path> {
        self.workdir.as_deref()
    }

    pub(crate) fn workspace_flag(&self) -> bool {
        self.workspace
    }

    pub(crate) fn jobs_value(&self) -> Option<u32> {
        self.jobs
    }

    pub(crate) fn timeout_value(&self) -> Option<Duration> {
        self.timeout
    }

    pub(crate) fn exclude_re_view(&self) -> &[String] {
        &self.exclude_re
    }

    pub(crate) fn file_filters_view(&self) -> &[String] {
        &self.file_filters
    }

    pub(crate) fn allow_list_view(&self) -> &[String] {
        &self.allow_list
    }
}

// ---------------------------------------------------------------------------
// SurvivingMutant
// ---------------------------------------------------------------------------

/// A surviving mutant — one the test suite did NOT catch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurvivingMutant {
    /// Source file the mutation was applied to.
    pub file: String,
    /// Line number (1-indexed).
    pub line: u32,
    /// Description of the mutation (e.g. "replace `+` with `-`").
    pub description: String,
    /// Function the mutation lived in, when the underlying tool exposed it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function: Option<String>,
}

// ---------------------------------------------------------------------------
// FileBreakdown
// ---------------------------------------------------------------------------

/// Per-file mutation outcome counts and derived kill rate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileBreakdown {
    /// Source file path.
    pub file: String,
    /// Mutants caught by the test suite (for this file).
    pub killed: u64,
    /// Mutants the test suite missed (for this file).
    pub survived: u64,
    /// Mutants that timed out (excluded from `kill_pct`).
    pub timeout: u64,
}

impl FileBreakdown {
    /// Kill rate for this file in `[0.0, 100.0]`. Timeouts excluded.
    pub fn kill_pct(&self) -> f64 {
        let counted = self.killed + self.survived;
        if counted == 0 {
            return 0.0;
        }
        (self.killed as f64 / counted as f64) * 100.0
    }
}

// ---------------------------------------------------------------------------
// MutateResult
// ---------------------------------------------------------------------------

/// Result of a mutation testing run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutateResult {
    /// Subject name.
    pub name: String,
    /// Subject version.
    pub version: String,
    /// Total mutants generated.
    pub mutants_total: u64,
    /// Mutants caught by the test suite.
    pub mutants_killed: u64,
    /// Mutants the test suite missed.
    pub mutants_survived: u64,
    /// Mutants that timed out (excluded from `kill_pct`).
    pub mutants_timeout: u64,
    /// Per-survivor detail (sorted by `(file, line)` for determinism).
    pub survivors: Vec<SurvivingMutant>,
    /// Per-file breakdown (sorted by `file`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<FileBreakdown>,
}

impl MutateResult {
    /// Kill rate as a percent in `[0.0, 100.0]`.
    ///
    /// `killed / (killed + survived) * 100`. Timeouts excluded.
    pub fn kill_pct(&self) -> f64 {
        let counted = self.mutants_killed + self.mutants_survived;
        if counted == 0 {
            return 0.0;
        }
        (self.mutants_killed as f64 / counted as f64) * 100.0
    }

    /// `true` when the kill rate meets or exceeds the given threshold.
    ///
    /// Exists so callers can avoid recomputing `kill_pct` and the
    /// comparison logic locally.
    pub fn meets(&self, threshold: MutateThreshold) -> bool {
        match threshold {
            MutateThreshold::MinKillPct(target) => self.kill_pct() >= target,
        }
    }

    /// Files with the lowest kill rate, ascending. Useful for emitting
    /// evidence about test-quality hotspots.
    pub fn weakest_files(&self, n: usize) -> Vec<&FileBreakdown> {
        let mut refs: Vec<&FileBreakdown> = self.files.iter().collect();
        refs.sort_by(|a, b| {
            a.kill_pct()
                .partial_cmp(&b.kill_pct())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        refs.into_iter().take(n).collect()
    }

    /// Convert this result into a [`CheckResult`] against `threshold`.
    ///
    /// Pass when `kill_pct >= threshold`, otherwise fail with
    /// `Severity::Warning` (per REPS § 4 — mutation testing is
    /// advisory by default). The verdict carries numeric evidence for
    /// both the measured and target percentages plus the raw counts.
    /// When survivors are present, the first one is attached as
    /// `Evidence::FileRef` pointing at the affected `(file, line)`.
    pub fn into_check_result(self, threshold: MutateThreshold) -> CheckResult {
        let name = format!("mutate::{}", self.name);
        let kill_pct = self.kill_pct();
        let MutateThreshold::MinKillPct(target) = threshold;
        let detail = format!(
            "kill rate {:.2}% ({}/{}; {} timeouts; {} survivors)",
            kill_pct,
            self.mutants_killed,
            self.mutants_killed + self.mutants_survived,
            self.mutants_timeout,
            self.mutants_survived
        );
        let mut check = if kill_pct < target {
            CheckResult::fail(name, Severity::Warning).with_detail(detail)
        } else {
            CheckResult::pass(name).with_detail(detail)
        };
        check = check
            .with_tag("mutate")
            .with_evidence(Evidence::numeric("kill_pct", kill_pct))
            .with_evidence(Evidence::numeric("kill_pct_threshold", target))
            .with_evidence(Evidence::numeric_int(
                "mutants_killed",
                self.mutants_killed as i64,
            ))
            .with_evidence(Evidence::numeric_int(
                "mutants_survived",
                self.mutants_survived as i64,
            ))
            .with_evidence(Evidence::numeric_int(
                "mutants_timeout",
                self.mutants_timeout as i64,
            ));
        if let Some(first) = self.survivors.first() {
            check = check.with_evidence(Evidence::file_ref_lines(
                "first_survivor",
                first.file.clone(),
                first.line.max(1),
                first.line.max(1),
            ));
        }
        check
    }
}

// ---------------------------------------------------------------------------
// MutateThreshold
// ---------------------------------------------------------------------------

/// Threshold defining the minimum acceptable kill rate.
#[derive(Debug, Clone, Copy)]
pub enum MutateThreshold {
    /// Fail when `kill_pct < pct`.
    MinKillPct(f64),
}

impl MutateThreshold {
    /// Build a kill-rate threshold.
    pub fn min_kill_pct(pct: f64) -> Self {
        Self::MinKillPct(pct)
    }
}

// ---------------------------------------------------------------------------
// MutateError
// ---------------------------------------------------------------------------

/// Errors that can arise during a mutation testing run.
#[derive(Debug)]
pub enum MutateError {
    /// `cargo-mutants` is not installed.
    ToolNotInstalled,
    /// Subprocess returned a fatal error and no parseable JSON.
    SubprocessFailed(String),
    /// JSON output could not be parsed.
    ParseError(String),
}

impl std::fmt::Display for MutateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ToolNotInstalled => write!(
                f,
                "cargo-mutants is not installed; run `cargo install cargo-mutants`"
            ),
            Self::SubprocessFailed(s) => write!(f, "cargo mutants failed: {s}"),
            Self::ParseError(s) => write!(f, "could not parse cargo mutants output: {s}"),
        }
    }
}

impl std::error::Error for MutateError {}

// ---------------------------------------------------------------------------
// Helpers for the runner
// ---------------------------------------------------------------------------

pub(crate) fn aggregate_breakdown(
    survivors: &[SurvivingMutant],
    by_file_killed: &BTreeMap<String, u64>,
    by_file_survived: &BTreeMap<String, u64>,
    by_file_timeout: &BTreeMap<String, u64>,
) -> Vec<FileBreakdown> {
    let _ = survivors;
    let mut all_files: BTreeMap<String, FileBreakdown> = BTreeMap::new();
    for (file, count) in by_file_killed {
        all_files
            .entry(file.clone())
            .or_insert(FileBreakdown {
                file: file.clone(),
                killed: 0,
                survived: 0,
                timeout: 0,
            })
            .killed = *count;
    }
    for (file, count) in by_file_survived {
        all_files
            .entry(file.clone())
            .or_insert(FileBreakdown {
                file: file.clone(),
                killed: 0,
                survived: 0,
                timeout: 0,
            })
            .survived = *count;
    }
    for (file, count) in by_file_timeout {
        all_files
            .entry(file.clone())
            .or_insert(FileBreakdown {
                file: file.clone(),
                killed: 0,
                survived: 0,
                timeout: 0,
            })
            .timeout = *count;
    }
    all_files.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use dev_report::Verdict;

    fn sample(killed: u64, survived: u64, timeout: u64) -> MutateResult {
        MutateResult {
            name: "x".into(),
            version: "0.1.0".into(),
            mutants_total: killed + survived + timeout,
            mutants_killed: killed,
            mutants_survived: survived,
            mutants_timeout: timeout,
            survivors: Vec::new(),
            files: Vec::new(),
        }
    }

    #[test]
    fn kill_pct_excludes_timeouts() {
        // 80 / (80 + 20) = 80%, with 10 timeouts not counted.
        let r = sample(80, 20, 10);
        assert!((r.kill_pct() - 80.0).abs() < 1e-9);
    }

    #[test]
    fn kill_pct_zero_when_nothing_counted() {
        let r = sample(0, 0, 5);
        assert_eq!(r.kill_pct(), 0.0);
    }

    #[test]
    fn threshold_pass_when_above() {
        let r = sample(85, 15, 0);
        let c = r.into_check_result(MutateThreshold::min_kill_pct(80.0));
        assert_eq!(c.verdict, Verdict::Pass);
        assert!(c.has_tag("mutate"));
        assert!(c.evidence.iter().any(|e| e.label == "kill_pct"));
        assert!(c.evidence.iter().any(|e| e.label == "kill_pct_threshold"));
    }

    #[test]
    fn threshold_fail_uses_warning_severity() {
        let r = sample(50, 50, 0);
        let c = r.into_check_result(MutateThreshold::min_kill_pct(80.0));
        assert_eq!(c.verdict, Verdict::Fail);
        assert_eq!(c.severity, Some(Severity::Warning));
    }

    #[test]
    fn meets_helper_matches_into_check_result() {
        let r = sample(85, 15, 0);
        assert!(r.meets(MutateThreshold::min_kill_pct(80.0)));
        assert!(!r.meets(MutateThreshold::min_kill_pct(95.0)));
    }

    #[test]
    fn first_survivor_attached_as_evidence() {
        let mut r = sample(80, 20, 0);
        r.survivors = vec![SurvivingMutant {
            file: "src/lib.rs".into(),
            line: 42,
            description: "replace + with -".into(),
            function: Some("add".into()),
        }];
        let c = r.into_check_result(MutateThreshold::min_kill_pct(85.0));
        // 80% < 85%, fails. Verify first_survivor evidence present.
        assert!(c.evidence.iter().any(|e| e.label == "first_survivor"));
    }

    #[test]
    fn weakest_files_sorts_ascending_by_kill_pct() {
        let r = MutateResult {
            name: "x".into(),
            version: "0.1.0".into(),
            mutants_total: 0,
            mutants_killed: 0,
            mutants_survived: 0,
            mutants_timeout: 0,
            survivors: Vec::new(),
            files: vec![
                FileBreakdown {
                    file: "a.rs".into(),
                    killed: 9,
                    survived: 1,
                    timeout: 0,
                },
                FileBreakdown {
                    file: "b.rs".into(),
                    killed: 5,
                    survived: 5,
                    timeout: 0,
                },
                FileBreakdown {
                    file: "c.rs".into(),
                    killed: 7,
                    survived: 3,
                    timeout: 0,
                },
            ],
        };
        let weakest = r.weakest_files(2);
        assert_eq!(weakest.len(), 2);
        assert_eq!(weakest[0].file, "b.rs"); // 50%
        assert_eq!(weakest[1].file, "c.rs"); // 70%
    }

    #[test]
    fn file_breakdown_kill_pct() {
        let f = FileBreakdown {
            file: "x".into(),
            killed: 7,
            survived: 3,
            timeout: 0,
        };
        assert!((f.kill_pct() - 70.0).abs() < 1e-9);
    }

    #[test]
    fn result_round_trips_through_json() {
        let mut r = sample(80, 20, 0);
        r.survivors.push(SurvivingMutant {
            file: "src/lib.rs".into(),
            line: 1,
            description: "x".into(),
            function: None,
        });
        let s = serde_json::to_string(&r).unwrap();
        let back: MutateResult = serde_json::from_str(&s).unwrap();
        assert_eq!(back.survivors.len(), 1);
    }

    #[test]
    fn builder_chain_compiles() {
        let r = MutateRun::new("x", "0.1.0")
            .workspace()
            .jobs(4)
            .timeout(Duration::from_secs(120))
            .exclude_re(r"^src/generated/")
            .file("src/lib.rs")
            .allow("replace + with -")
            .allow_all(["a", "b"]);
        assert_eq!(r.subject(), "x");
        assert_eq!(r.subject_version(), "0.1.0");
        assert!(r.workspace_flag());
        assert_eq!(r.jobs_value(), Some(4));
        assert_eq!(r.exclude_re_view().len(), 1);
        assert_eq!(r.file_filters_view().len(), 1);
        assert_eq!(r.allow_list_view().len(), 3);
    }
}
