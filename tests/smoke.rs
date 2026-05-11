use dev_mutate::{MutateResult, MutateRun, MutateThreshold};

#[test]
fn smoke_run_builds() {
    let _ = MutateRun::new("x", "0.1.0");
}

#[test]
fn smoke_kill_pct_basic() {
    let r = MutateResult {
        name: "x".into(),
        version: "0.1.0".into(),
        mutants_total: 100,
        mutants_killed: 80,
        mutants_survived: 20,
        mutants_timeout: 0,
        survivors: Vec::new(),
    };
    assert!((r.kill_pct() - 80.0).abs() < 0.0001);
}

#[test]
fn smoke_kill_pct_excludes_timeouts() {
    let r = MutateResult {
        name: "x".into(),
        version: "0.1.0".into(),
        mutants_total: 120,
        mutants_killed: 80,
        mutants_survived: 20,
        mutants_timeout: 20,
        survivors: Vec::new(),
    };
    // 80 / (80 + 20) = 80%. Timeouts excluded.
    assert!((r.kill_pct() - 80.0).abs() < 0.0001);
}

#[test]
fn smoke_threshold_pass() {
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
fn smoke_threshold_fail() {
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
fn smoke_kill_pct_attached_as_evidence() {
    let r = MutateResult {
        name: "x".into(),
        version: "0.1.0".into(),
        mutants_total: 100,
        mutants_killed: 75,
        mutants_survived: 25,
        mutants_timeout: 0,
        survivors: Vec::new(),
    };
    let c = r.into_check_result(MutateThreshold::min_kill_pct(50.0));
    assert!(!c.evidence.is_empty());
}
