//! `cargo mutants` subprocess invocation + NDJSON parser.
//!
//! `cargo mutants --json` emits one JSON object per line. Each record
//! describes one mutant outcome. The fields we care about:
//!
//! ```text
//! { "scenario": "Mutant",
//!   "mutant": { "package": "...", "file": "src/lib.rs", "line": 42,
//!               "function": "foo", "description": "replace + with -",
//!               "genre": "BinaryOperator" },
//!   "outcome": "Caught" | "Missed" | "Timeout" | "Failure" | "Unviable" | "Success" }
//! ```
//!
//! - `Caught` → killed.
//! - `Missed` → survived.
//! - `Timeout` → counted separately (excluded from kill rate per REPS § 3).
//! - `Failure` / `Unviable` → mutation generation failed; not a test-quality signal.
//! - `Success` / baseline → ignored.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use serde::Deserialize;

use crate::{aggregate_breakdown, MutateError, MutateResult, MutateRun, SurvivingMutant};

pub(crate) fn run(cfg: &MutateRun) -> Result<MutateResult, MutateError> {
    detect_tool()?;
    let output = run_cargo_mutants(cfg)?;
    // cargo-mutants exits non-zero when it finds surviving mutants —
    // that's the success path. Only treat empty stdout + non-zero exit
    // as a hard failure.
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    if stdout.trim().is_empty() && !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        return Err(MutateError::SubprocessFailed(stderr));
    }
    let mut result = parse_outcomes(&stdout, cfg.subject(), cfg.subject_version())?;

    let allow = cfg.allow_list_view();
    if !allow.is_empty() {
        let before = result.survivors.len();
        result
            .survivors
            .retain(|s| !allow.iter().any(|d| d == &s.description));
        let removed = (before - result.survivors.len()) as u64;
        // Allow-listed survivors are reclassified as killed for kill-rate purposes —
        // the user explicitly declared them acceptable.
        result.mutants_survived = result.mutants_survived.saturating_sub(removed);
        result.mutants_killed = result.mutants_killed.saturating_add(removed);
    }

    Ok(result)
}

fn detect_tool() -> Result<(), MutateError> {
    let probe = Command::new("cargo")
        .args(["mutants", "--version"])
        .output();
    match probe {
        Ok(o) if o.status.success() => Ok(()),
        _ => Err(MutateError::ToolNotInstalled),
    }
}

fn run_cargo_mutants(cfg: &MutateRun) -> Result<std::process::Output, MutateError> {
    let mut cmd = Command::new("cargo");
    cmd.args(["mutants", "--json", "--no-shuffle"]);
    if cfg.workspace_flag() {
        cmd.arg("--workspace");
    }
    if let Some(j) = cfg.jobs_value() {
        cmd.args(["--jobs", &j.to_string()]);
    }
    if let Some(t) = cfg.timeout_value() {
        cmd.args(["--timeout", &t.as_secs().max(1).to_string()]);
    }
    for pat in cfg.exclude_re_view() {
        cmd.args(["--exclude-re", pat]);
    }
    for pat in cfg.file_filters_view() {
        cmd.args(["--file", pat]);
    }
    if let Some(dir) = cfg.workdir_path() {
        cmd.current_dir(dir as &Path);
    }
    cmd.output()
        .map_err(|e| MutateError::SubprocessFailed(e.to_string()))
}

// ---------------------------------------------------------------------------
// NDJSON shape
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct MutantRecord {
    #[serde(default)]
    scenario: String,
    #[serde(default)]
    mutant: MutantInfo,
    #[serde(default)]
    outcome: serde_json::Value,
}

#[derive(Deserialize, Default)]
struct MutantInfo {
    #[serde(default)]
    file: String,
    #[serde(default)]
    line: u32,
    #[serde(default)]
    description: String,
    #[serde(default)]
    function: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Caught,
    Missed,
    Timeout,
    Other,
}

fn outcome_from_value(value: &serde_json::Value) -> Outcome {
    let s = match value {
        // cargo-mutants newer JSON: `"outcome": "Caught"`.
        serde_json::Value::String(s) => s.as_str(),
        // Older shapes may wrap the outcome as a tagged enum;
        // `{ "Caught": ... }` → take the key.
        serde_json::Value::Object(map) if !map.is_empty() => {
            map.keys().next().map(|s| s.as_str()).unwrap_or("")
        }
        _ => "",
    };
    let s = s.to_ascii_lowercase();
    if s.contains("caught") {
        Outcome::Caught
    } else if s.contains("missed") {
        Outcome::Missed
    } else if s.contains("timeout") {
        Outcome::Timeout
    } else {
        Outcome::Other
    }
}

pub(crate) fn parse_outcomes(
    ndjson: &str,
    subject: &str,
    version: &str,
) -> Result<MutateResult, MutateError> {
    let mut killed = 0u64;
    let mut survived = 0u64;
    let mut timeout = 0u64;
    let mut total_mutant_records = 0u64;
    let mut survivors: Vec<SurvivingMutant> = Vec::new();
    let mut by_file_killed: BTreeMap<String, u64> = BTreeMap::new();
    let mut by_file_survived: BTreeMap<String, u64> = BTreeMap::new();
    let mut by_file_timeout: BTreeMap<String, u64> = BTreeMap::new();

    for raw in ndjson.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let record: MutantRecord = match serde_json::from_str(line) {
            Ok(r) => r,
            Err(_) => continue, // cargo-mutants occasionally emits non-JSON noise even under --json
        };
        // Only count Mutant-scenario records; baseline / unmutated / other
        // metadata records don't reflect test quality.
        if !record.scenario.eq_ignore_ascii_case("mutant") {
            continue;
        }
        total_mutant_records += 1;
        let outcome = outcome_from_value(&record.outcome);
        let file = if record.mutant.file.is_empty() {
            "<unknown>".to_string()
        } else {
            record.mutant.file.clone()
        };
        match outcome {
            Outcome::Caught => {
                killed += 1;
                *by_file_killed.entry(file).or_insert(0) += 1;
            }
            Outcome::Missed => {
                survived += 1;
                *by_file_survived.entry(file.clone()).or_insert(0) += 1;
                survivors.push(SurvivingMutant {
                    file,
                    line: record.mutant.line,
                    description: record.mutant.description,
                    function: record.mutant.function,
                });
            }
            Outcome::Timeout => {
                timeout += 1;
                *by_file_timeout.entry(file).or_insert(0) += 1;
            }
            Outcome::Other => {
                // Mutation generation failure, unviable mutation, etc.
                // Counted in mutants_total but not in killed / survived /
                // timeout.
            }
        }
    }

    // Deterministic ordering for diff-friendly output.
    survivors.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));

    let mut files = aggregate_breakdown(
        &survivors,
        &by_file_killed,
        &by_file_survived,
        &by_file_timeout,
    );
    files.sort_by(|a, b| a.file.cmp(&b.file));

    Ok(MutateResult {
        name: subject.to_string(),
        version: version.to_string(),
        mutants_total: total_mutant_records,
        mutants_killed: killed,
        mutants_survived: survived,
        mutants_timeout: timeout,
        survivors,
        files,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_yields_zero_result() {
        let r = parse_outcomes("", "x", "0.1.0").unwrap();
        assert_eq!(r.mutants_total, 0);
        assert_eq!(r.mutants_killed, 0);
        assert_eq!(r.mutants_survived, 0);
        assert_eq!(r.kill_pct(), 0.0);
    }

    #[test]
    fn parses_caught_missed_timeout() {
        let ndjson = concat!(
            r#"{"scenario":"Mutant","mutant":{"file":"src/lib.rs","line":10,"description":"replace + with -"},"outcome":"Caught"}"#,
            "\n",
            r#"{"scenario":"Mutant","mutant":{"file":"src/lib.rs","line":20,"description":"replace - with +"},"outcome":"Missed"}"#,
            "\n",
            r#"{"scenario":"Mutant","mutant":{"file":"src/lib.rs","line":30,"description":"slow"},"outcome":"Timeout"}"#,
        );
        let r = parse_outcomes(ndjson, "x", "0.1.0").unwrap();
        assert_eq!(r.mutants_killed, 1);
        assert_eq!(r.mutants_survived, 1);
        assert_eq!(r.mutants_timeout, 1);
        assert_eq!(r.survivors.len(), 1);
        assert_eq!(r.survivors[0].line, 20);
    }

    #[test]
    fn tagged_enum_outcome_shape_is_recognized() {
        // Older / future cargo-mutants might wrap the outcome as a
        // tagged enum: `{"Caught": ...}`. Make sure that shape works.
        let ndjson = concat!(
            r#"{"scenario":"Mutant","mutant":{"file":"a","line":1,"description":"x"},"outcome":{"Caught":{}}}"#,
            "\n",
            r#"{"scenario":"Mutant","mutant":{"file":"a","line":2,"description":"y"},"outcome":{"Missed":{}}}"#,
        );
        let r = parse_outcomes(ndjson, "x", "0.1.0").unwrap();
        assert_eq!(r.mutants_killed, 1);
        assert_eq!(r.mutants_survived, 1);
    }

    #[test]
    fn baseline_and_other_scenarios_ignored() {
        let ndjson = concat!(
            r#"{"scenario":"Baseline","mutant":{"file":"a","line":1,"description":"x"},"outcome":"Success"}"#,
            "\n",
            r#"{"scenario":"Mutant","mutant":{"file":"a","line":1,"description":"x"},"outcome":"Caught"}"#,
        );
        let r = parse_outcomes(ndjson, "x", "0.1.0").unwrap();
        assert_eq!(r.mutants_total, 1);
        assert_eq!(r.mutants_killed, 1);
    }

    #[test]
    fn non_json_lines_are_skipped() {
        let ndjson = concat!(
            "    Building cargo-mutants\n",
            r#"{"scenario":"Mutant","mutant":{"file":"a","line":1,"description":"x"},"outcome":"Missed"}"#,
            "\n",
            "garbage\n",
        );
        let r = parse_outcomes(ndjson, "x", "0.1.0").unwrap();
        assert_eq!(r.mutants_survived, 1);
    }

    #[test]
    fn unrecognized_outcome_strings_stay_uncounted() {
        let ndjson = concat!(
            r#"{"scenario":"Mutant","mutant":{"file":"a","line":1,"description":"x"},"outcome":"Failure"}"#,
            "\n",
            r#"{"scenario":"Mutant","mutant":{"file":"a","line":2,"description":"y"},"outcome":"Unviable"}"#,
        );
        let r = parse_outcomes(ndjson, "x", "0.1.0").unwrap();
        // total counts the mutant records seen
        assert_eq!(r.mutants_total, 2);
        // but neither contributes to killed/survived/timeout
        assert_eq!(r.mutants_killed, 0);
        assert_eq!(r.mutants_survived, 0);
        assert_eq!(r.mutants_timeout, 0);
    }

    #[test]
    fn per_file_breakdown_computed() {
        let ndjson = concat!(
            r#"{"scenario":"Mutant","mutant":{"file":"a.rs","line":1,"description":"x"},"outcome":"Caught"}"#,
            "\n",
            r#"{"scenario":"Mutant","mutant":{"file":"a.rs","line":2,"description":"y"},"outcome":"Missed"}"#,
            "\n",
            r#"{"scenario":"Mutant","mutant":{"file":"b.rs","line":1,"description":"z"},"outcome":"Caught"}"#,
            "\n",
            r#"{"scenario":"Mutant","mutant":{"file":"b.rs","line":2,"description":"w"},"outcome":"Caught"}"#,
        );
        let r = parse_outcomes(ndjson, "x", "0.1.0").unwrap();
        assert_eq!(r.files.len(), 2);
        let a = r.files.iter().find(|f| f.file == "a.rs").unwrap();
        let b = r.files.iter().find(|f| f.file == "b.rs").unwrap();
        assert!((a.kill_pct() - 50.0).abs() < 1e-9);
        assert!((b.kill_pct() - 100.0).abs() < 1e-9);
    }

    #[test]
    fn survivors_sorted_by_file_then_line() {
        let ndjson = concat!(
            r#"{"scenario":"Mutant","mutant":{"file":"b.rs","line":5,"description":"x"},"outcome":"Missed"}"#,
            "\n",
            r#"{"scenario":"Mutant","mutant":{"file":"a.rs","line":10,"description":"y"},"outcome":"Missed"}"#,
            "\n",
            r#"{"scenario":"Mutant","mutant":{"file":"a.rs","line":3,"description":"z"},"outcome":"Missed"}"#,
        );
        let r = parse_outcomes(ndjson, "x", "0.1.0").unwrap();
        assert_eq!(r.survivors[0].file, "a.rs");
        assert_eq!(r.survivors[0].line, 3);
        assert_eq!(r.survivors[1].file, "a.rs");
        assert_eq!(r.survivors[1].line, 10);
        assert_eq!(r.survivors[2].file, "b.rs");
    }

    #[test]
    fn missing_file_field_falls_back_to_placeholder() {
        let ndjson =
            r#"{"scenario":"Mutant","mutant":{"line":1,"description":"x"},"outcome":"Missed"}"#;
        let r = parse_outcomes(ndjson, "x", "0.1.0").unwrap();
        assert_eq!(r.mutants_survived, 1);
        assert_eq!(r.survivors[0].file, "<unknown>");
    }
}
