//! Public-API smoke tests.
//!
//! The real-subprocess path is exercised only when `cargo-mutants` is
//! installed and `CARGO_TARGET_DIR` points outside the workspace (see
//! the `#[ignore]`d test at the bottom for the workaround).

use dev_mutate::{FileBreakdown, MutateResult, MutateRun, MutateThreshold, SurvivingMutant};

fn make_result(killed: u64, survived: u64, timeout: u64) -> MutateResult {
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
fn run_builds() {
    let _ = MutateRun::new("x", "0.1.0");
}

#[test]
fn kill_pct_basic() {
    let r = make_result(80, 20, 0);
    assert!((r.kill_pct() - 80.0).abs() < 0.0001);
}

#[test]
fn kill_pct_excludes_timeouts() {
    let r = make_result(80, 20, 20);
    assert!((r.kill_pct() - 80.0).abs() < 0.0001);
}

#[test]
fn threshold_pass() {
    let r = make_result(85, 15, 0);
    let c = r.into_check_result(MutateThreshold::min_kill_pct(80.0));
    assert!(matches!(c.verdict, dev_report::Verdict::Pass));
}

#[test]
fn threshold_fail() {
    let r = make_result(50, 50, 0);
    let c = r.into_check_result(MutateThreshold::min_kill_pct(80.0));
    assert!(matches!(c.verdict, dev_report::Verdict::Fail));
}

#[test]
fn kill_pct_attached_as_evidence() {
    let r = make_result(75, 25, 0);
    let c = r.into_check_result(MutateThreshold::min_kill_pct(50.0));
    assert!(c.evidence.iter().any(|e| e.label == "kill_pct"));
}

#[test]
fn surviving_mutant_round_trips() {
    let s = SurvivingMutant {
        file: "src/lib.rs".into(),
        line: 42,
        description: "replace + with -".into(),
        function: Some("add".into()),
    };
    let json = serde_json::to_string(&s).unwrap();
    let back: SurvivingMutant = serde_json::from_str(&json).unwrap();
    assert_eq!(back.file, "src/lib.rs");
    assert_eq!(back.line, 42);
}

#[test]
fn file_breakdown_kill_pct_excludes_timeouts() {
    let f = FileBreakdown {
        file: "x.rs".into(),
        killed: 7,
        survived: 3,
        timeout: 5,
    };
    assert!((f.kill_pct() - 70.0).abs() < 1e-9);
}

/// Real subprocess test. Skipped by default.
///
/// Run with `cargo-mutants` installed and `CARGO_TARGET_DIR` pointing
/// outside the workspace so the inner `cargo` invocations don't fight
/// the outer `cargo test` for the workspace target-dir lock:
///
/// ```text
/// CARGO_TARGET_DIR=/tmp/mutate-target cargo test -- --ignored
/// ```
#[test]
#[ignore = "requires cargo-mutants + CARGO_TARGET_DIR outside the workspace"]
fn execute_against_real_cargo_mutants() {
    let run = MutateRun::new("dev-mutate", "0.9.0");
    let res = run.execute().expect("cargo-mutants is installed");
    let _ = res.into_check_result(MutateThreshold::min_kill_pct(50.0));
}
