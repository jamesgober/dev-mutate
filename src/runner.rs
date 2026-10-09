//! `cargo mutants` subprocess invocation + `outcomes.json` parser.
//!
//! `cargo mutants` prints human-readable progress on stdout; its `--json`
//! flag only applies to `--list`. The machine-readable result of a run is
//! the `outcomes.json` file it writes into `<output>/mutants.out/`. We pass
//! `--output` explicitly so we always know where that file is.
//!
//! Shape of `outcomes.json` (cargo-mutants 27), trimmed to the fields we
//! read:
//!
//! ```text
//! { "outcomes": [
//!     { "scenario": "Baseline", "summary": "Success", ... },
//!     { "scenario": { "Mutant": {
//!         "name": "src/lib.rs:2:35: replace > with == in is_pos",
//!         "file": "src/lib.rs",
//!         "function": { "function_name": "is_pos", ... },
//!         "span": { "start": { "line": 2, "column": 35 }, ... },
//!         ... } },
//!       "summary": "MissedMutant", ... } ],
//!   "total_mutants": 14, "caught": 6, "missed": 8, ... }
//! ```
//!
//! `summary` values:
//!
//! - `CaughtMutant` → killed.
//! - `MissedMutant` → survived.
//! - `Timeout` → counted separately (excluded from kill rate per REPS § 3).
//! - `Unviable` / `Failure` → the mutant did not build; not a test-quality signal.
//! - `Success` (baseline) → ignored.
//!
//! Exit codes: 0 = all caught, 2 = some missed, 3 = some timed out,
//! 4 = the unmutated baseline failed, 1 = usage error.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::{aggregate_breakdown, MutateError, MutateResult, MutateRun, SurvivingMutant};

pub(crate) fn run(cfg: &MutateRun) -> Result<MutateResult, MutateError> {
    if let Some(dir) = cfg.workdir_path() {
        if !dir.is_dir() {
            return Err(MutateError::SubprocessFailed(format!(
                "working directory does not exist: {}",
                dir.display()
            )));
        }
    }
    detect_tool(cfg.workdir_path())?;

    let out = OutputDir::prepare(cfg.output_dir_path())?;
    let started = SystemTime::now();
    let mut cmd = build_command(cfg, out.path());
    let output = cmd
        .output()
        .map_err(|e| MutateError::SubprocessFailed(format!("could not run cargo mutants: {e}")))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    check_exit(output.status, &stdout, &stderr)?;

    let outcomes = out.path().join("mutants.out").join("outcomes.json");
    let bytes = read_fresh(&outcomes, started, out.is_caller_owned()).map_err(|e| {
        MutateError::SubprocessFailed(format!(
            "cargo mutants exited with {} but {} could not be read: {e}\n{}",
            output.status,
            outcomes.display(),
            stderr.trim_end()
        ))
    })?;
    let text = String::from_utf8_lossy(&bytes);
    parse_outcomes(
        &text,
        cfg.subject(),
        cfg.subject_version(),
        cfg.allow_list_view(),
    )
}

/// Map cargo-mutants' exit status to "go parse the outcomes" or an error.
fn check_exit(status: ExitStatus, stdout: &str, stderr: &str) -> Result<(), MutateError> {
    match status.code() {
        // 0: every mutant caught; 2: some missed; 3: some timed out.
        Some(0) | Some(2) | Some(3) => Ok(()),
        Some(4) => Err(MutateError::SubprocessFailed(format!(
            "the test suite fails on the unmutated tree, so no mutants were tested \
             (exit 4)\n{}\n{}",
            stderr.trim_end(),
            tail(stdout, 40)
        ))),
        _ => Err(MutateError::SubprocessFailed(format!(
            "exited with {status}\n{}",
            stderr.trim_end()
        ))),
    }
}

/// Last `n` lines of `s`.
fn tail(s: &str, n: usize) -> String {
    let lines: Vec<&str> = s.lines().collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].join("\n")
}

/// Read `path`, refusing a file left over from an earlier run when the
/// directory is one the caller keeps between runs.
fn read_fresh(path: &Path, started: SystemTime, check_age: bool) -> io::Result<Vec<u8>> {
    if check_age {
        let modified = fs::metadata(path)?.modified()?;
        // Allow for coarse filesystem timestamps.
        let floor = started
            .checked_sub(Duration::from_secs(2))
            .unwrap_or(UNIX_EPOCH);
        if modified < floor {
            return Err(io::Error::other(
                "file is older than this run (left over from a previous run)",
            ));
        }
    }
    fs::read(path)
}

/// Probe for `cargo mutants`. Runs in the same directory as the real run
/// so a `rust-toolchain.toml` there selects the same toolchain.
fn detect_tool(workdir: Option<&Path>) -> Result<(), MutateError> {
    let mut cmd = Command::new("cargo");
    cmd.args(["mutants", "--version"]);
    if let Some(dir) = workdir {
        cmd.current_dir(dir);
    }
    match cmd.output() {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            if is_missing_subcommand(&stderr) {
                Err(MutateError::ToolNotInstalled)
            } else {
                Err(MutateError::SubprocessFailed(format!(
                    "`cargo mutants --version` exited with {}\n{}",
                    o.status,
                    stderr.trim_end()
                )))
            }
        }
        // `cargo` itself could not be started (not on PATH).
        Err(_) => Err(MutateError::ToolNotInstalled),
    }
}

/// `true` when cargo's stderr says the subcommand does not exist.
/// Current cargo prints "no such command", older releases printed
/// "no such subcommand".
fn is_missing_subcommand(stderr: &str) -> bool {
    stderr.contains("no such command") || stderr.contains("no such subcommand")
}

fn build_command(cfg: &MutateRun, output_dir: &Path) -> Command {
    let mut cmd = Command::new("cargo");
    cmd.arg("mutants");
    cmd.arg("--output").arg(output_dir);
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
        cmd.current_dir(dir);
    }
    cmd
}

// ---------------------------------------------------------------------------
// Output directory
// ---------------------------------------------------------------------------

/// Where `cargo mutants --output` writes `mutants.out/`. Either the
/// caller's directory (kept) or a fresh temporary one (removed on drop).
struct OutputDir {
    path: PathBuf,
    temporary: bool,
}

impl OutputDir {
    fn prepare(caller: Option<&Path>) -> Result<Self, MutateError> {
        if let Some(dir) = caller {
            fs::create_dir_all(dir).map_err(|e| {
                MutateError::SubprocessFailed(format!(
                    "could not create output directory {}: {e}",
                    dir.display()
                ))
            })?;
            return Ok(Self {
                path: dir.to_path_buf(),
                temporary: false,
            });
        }
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let mut last_err = None;
        for _ in 0..8 {
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("dev-mutate-{}-{nanos}-{n}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => {
                    return Ok(Self {
                        path,
                        temporary: true,
                    })
                }
                Err(e) => last_err = Some(e),
            }
        }
        Err(MutateError::SubprocessFailed(format!(
            "could not create a temporary output directory: {}",
            last_err.map_or_else(|| "unknown error".to_string(), |e| e.to_string())
        )))
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn is_caller_owned(&self) -> bool {
        !self.temporary
    }
}

impl Drop for OutputDir {
    fn drop(&mut self) {
        if self.temporary {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

// ---------------------------------------------------------------------------
// outcomes.json parser
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Caught,
    Missed,
    Timeout,
    Other,
}

fn outcome_from_value(value: &Value) -> Outcome {
    let s = match value {
        // cargo-mutants writes `"summary": "CaughtMutant"`.
        Value::String(s) => s.as_str(),
        // Tolerate a tagged-enum shape: `{ "CaughtMutant": ... }` → take the key.
        Value::Object(map) => map.keys().next().map(|s| s.as_str()).unwrap_or(""),
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

/// The mutant object of an outcome record, or `None` for the baseline
/// and any other non-mutant scenario.
fn mutant_of(record: &Value) -> Option<&Value> {
    match record.get("scenario")? {
        Value::Object(map) => map.get("Mutant"),
        Value::String(s) if s.eq_ignore_ascii_case("mutant") => record.get("mutant"),
        _ => None,
    }
}

fn str_field<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

fn mutant_line(m: &Value) -> u32 {
    let raw = m
        .get("span")
        .and_then(|s| s.get("start"))
        .and_then(|s| s.get("line"))
        .and_then(Value::as_u64)
        .or_else(|| m.get("line").and_then(Value::as_u64))
        .unwrap_or(0);
    u32::try_from(raw).unwrap_or(u32::MAX)
}

fn mutant_function(m: &Value) -> Option<String> {
    match m.get("function")? {
        Value::Object(f) => f
            .get("function_name")
            .and_then(Value::as_str)
            .map(str::to_string),
        Value::String(s) => Some(s.clone()),
        _ => None,
    }
}

/// The mutation text without its `file:line:col: ` location prefix,
/// e.g. `replace > with == in is_pos`.
fn mutant_description(m: &Value, file: &str) -> String {
    if let Some(name) = str_field(m, "name") {
        return strip_location(name, file).to_string();
    }
    if let Some(d) = str_field(m, "description") {
        return d.to_string();
    }
    match (mutant_function(m), str_field(m, "replacement")) {
        (Some(f), Some(r)) => format!("replace {f} with {r}"),
        _ => String::new(),
    }
}

/// Strip a leading `file`, `file:line` or `file:line:col` and the
/// following `": "` from a cargo-mutants mutant name.
fn strip_location<'a>(name: &'a str, file: &str) -> &'a str {
    if file.is_empty() {
        return name;
    }
    let Some(mut rest) = name.strip_prefix(file) else {
        return name;
    };
    while let Some(after_colon) = rest.strip_prefix(':') {
        if let Some(desc) = after_colon.strip_prefix(' ') {
            return desc;
        }
        let digits = after_colon
            .bytes()
            .take_while(|b| b.is_ascii_digit())
            .count();
        if digits == 0 {
            break;
        }
        rest = &after_colon[digits..];
    }
    name
}

/// Parse a cargo-mutants `outcomes.json` document.
///
/// Survivors whose description is in `allow` are counted as killed, both
/// in the totals and in the per-file breakdown.
pub(crate) fn parse_outcomes(
    json: &str,
    subject: &str,
    version: &str,
    allow: &[String],
) -> Result<MutateResult, MutateError> {
    let doc: Value = serde_json::from_str(json)
        .map_err(|e| MutateError::ParseError(format!("outcomes.json: {e}")))?;
    let records = doc
        .get("outcomes")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            MutateError::ParseError("outcomes.json has no `outcomes` array".to_string())
        })?;

    let mut killed = 0u64;
    let mut survived = 0u64;
    let mut timeout = 0u64;
    let mut total_mutant_records = 0u64;
    let mut survivors: Vec<SurvivingMutant> = Vec::new();
    let mut by_file_killed: BTreeMap<String, u64> = BTreeMap::new();
    let mut by_file_survived: BTreeMap<String, u64> = BTreeMap::new();
    let mut by_file_timeout: BTreeMap<String, u64> = BTreeMap::new();

    for record in records {
        // Only mutant scenarios reflect test quality; skip the baseline.
        let Some(m) = mutant_of(record) else {
            continue;
        };
        total_mutant_records += 1;
        let outcome = record
            .get("summary")
            .or_else(|| record.get("outcome"))
            .map_or(Outcome::Other, outcome_from_value);
        let file = match str_field(m, "file") {
            Some(f) if !f.is_empty() => f.to_string(),
            _ => "<unknown>".to_string(),
        };
        match outcome {
            Outcome::Caught => {
                killed += 1;
                *by_file_killed.entry(file).or_insert(0) += 1;
            }
            Outcome::Missed => {
                let description = mutant_description(m, &file);
                if allow.iter().any(|a| a == &description) {
                    // The caller declared this survivor acceptable.
                    killed += 1;
                    *by_file_killed.entry(file).or_insert(0) += 1;
                } else {
                    survived += 1;
                    *by_file_survived.entry(file.clone()).or_insert(0) += 1;
                    survivors.push(SurvivingMutant {
                        file,
                        line: mutant_line(m),
                        description,
                        function: mutant_function(m),
                    });
                }
            }
            Outcome::Timeout => {
                timeout += 1;
                *by_file_timeout.entry(file).or_insert(0) += 1;
            }
            Outcome::Other => {
                // Unviable (did not build) or a tool failure. Counted in
                // mutants_total but not in killed / survived / timeout.
            }
        }
    }

    // Deterministic ordering for diff-friendly output.
    survivors.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then(a.line.cmp(&b.line))
            .then_with(|| a.description.cmp(&b.description))
    });

    let mut files = aggregate_breakdown(&by_file_killed, &by_file_survived, &by_file_timeout);
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

    /// Trimmed from a real `mutants.out/outcomes.json` written by
    /// cargo-mutants 27.0.0 on Windows (two runs merged: one with missed
    /// mutants, one with a timeout and an unviable mutant). Extra fields
    /// such as `phase_results`, `log_path` and `genre` are kept to prove
    /// they are ignored.
    const REAL_OUTCOMES_27: &str = r#"{
  "outcomes": [
    {"scenario": "Baseline", "summary": "Success", "log_path": "log/baseline.log", "diff_path": null,
     "phase_results": [{"phase": "Build", "duration": 0.35, "process_status": "Success",
       "argv": ["C:\\Users\\me\\.rustup\\toolchains\\stable\\bin\\cargo.exe", "test", "--no-run"]}]},
    {"scenario": {"Mutant": {"name": "src/lib.rs:1:39: replace + with - in add", "package": "tinymut",
       "file": "src/lib.rs",
       "function": {"function_name": "add", "return_type": "-> i32",
         "span": {"start": {"line": 1, "column": 1}, "end": {"line": 1, "column": 44}}},
       "span": {"start": {"line": 1, "column": 39}, "end": {"line": 1, "column": 40}},
       "replacement": "-", "genre": "BinaryOperator"}},
     "summary": "CaughtMutant", "log_path": "log/src__lib.rs_line_1_col_39.log",
     "phase_results": [{"phase": "Test", "duration": 0.1, "process_status": {"Failure": 101}, "argv": []}]},
    {"scenario": {"Mutant": {"name": "src/lib.rs:2:35: replace > with == in is_pos", "package": "tinymut",
       "file": "src/lib.rs",
       "function": {"function_name": "is_pos", "return_type": "-> bool",
         "span": {"start": {"line": 2, "column": 1}, "end": {"line": 2, "column": 40}}},
       "span": {"start": {"line": 2, "column": 35}, "end": {"line": 2, "column": 36}},
       "replacement": "==", "genre": "BinaryOperator"}},
     "summary": "MissedMutant"},
    {"scenario": {"Mutant": {"name": "src/lib.rs:2:33: replace is_pos -> bool with true", "package": "tinymut",
       "file": "src/lib.rs",
       "function": {"function_name": "is_pos", "return_type": "-> bool",
         "span": {"start": {"line": 2, "column": 1}, "end": {"line": 2, "column": 40}}},
       "span": {"start": {"line": 2, "column": 33}, "end": {"line": 2, "column": 40}},
       "replacement": "true", "genre": "FnValue"}},
     "summary": "MissedMutant"},
    {"scenario": {"Mutant": {"name": "src/lib.rs:3:67: replace += with *= in loop_until", "package": "tm2",
       "file": "src/lib.rs",
       "function": {"function_name": "loop_until", "return_type": "-> u32",
         "span": {"start": {"line": 3, "column": 1}, "end": {"line": 3, "column": 80}}},
       "span": {"start": {"line": 3, "column": 67}, "end": {"line": 3, "column": 69}},
       "replacement": "*=", "genre": "BinaryOperator"}},
     "summary": "Timeout"},
    {"scenario": {"Mutant": {"name": "src/util.rs:2:27: replace mk -> Foo with Default::default()", "package": "tm2",
       "file": "src/util.rs",
       "function": {"function_name": "mk", "return_type": "-> Foo",
         "span": {"start": {"line": 2, "column": 1}, "end": {"line": 2, "column": 35}}},
       "span": {"start": {"line": 2, "column": 27}, "end": {"line": 2, "column": 33}},
       "replacement": "Default::default()", "genre": "FnValue"}},
     "summary": "Unviable"}
  ],
  "total_mutants": 5, "missed": 2, "caught": 1, "timeout": 1, "unviable": 1, "success": 0,
  "start_time": "2026-10-09T09:15:06.836214Z", "end_time": "2026-10-09T09:15:14.8377143Z",
  "cargo_mutants_version": "27.0.0"
}"#;

    #[test]
    fn parses_real_cargo_mutants_27_outcomes() {
        let r = parse_outcomes(REAL_OUTCOMES_27, "x", "0.1.0", &[]).unwrap();
        assert_eq!(r.mutants_total, 5);
        assert_eq!(r.mutants_killed, 1);
        assert_eq!(r.mutants_survived, 2);
        assert_eq!(r.mutants_timeout, 1);
        assert!((r.kill_pct() - 100.0 / 3.0).abs() < 1e-9);
        // Sorted by (file, line, description).
        assert_eq!(r.survivors.len(), 2);
        assert_eq!(r.survivors[0].file, "src/lib.rs");
        assert_eq!(r.survivors[0].line, 2);
        assert_eq!(r.survivors[0].description, "replace > with == in is_pos");
        assert_eq!(r.survivors[0].function.as_deref(), Some("is_pos"));
        assert_eq!(
            r.survivors[1].description,
            "replace is_pos -> bool with true"
        );
        // The unviable mutant's file still has no counted outcomes.
        let lib = r.files.iter().find(|f| f.file == "src/lib.rs").unwrap();
        assert_eq!((lib.killed, lib.survived, lib.timeout), (1, 2, 1));
        assert!(r.files.iter().all(|f| f.file != "src/util.rs"));
    }

    #[test]
    fn crlf_document_parses() {
        let crlf = REAL_OUTCOMES_27.replace('\n', "\r\n");
        let r = parse_outcomes(&crlf, "x", "0.1.0", &[]).unwrap();
        assert_eq!(r.mutants_total, 5);
    }

    #[test]
    fn allow_list_moves_survivor_to_killed_in_totals_and_per_file() {
        let allow = vec!["replace > with == in is_pos".to_string()];
        let r = parse_outcomes(REAL_OUTCOMES_27, "x", "0.1.0", &allow).unwrap();
        assert_eq!(r.mutants_killed, 2);
        assert_eq!(r.mutants_survived, 1);
        assert_eq!(r.survivors.len(), 1);
        let lib = r.files.iter().find(|f| f.file == "src/lib.rs").unwrap();
        assert_eq!((lib.killed, lib.survived), (2, 1));
    }

    #[test]
    fn baseline_failure_document_yields_no_mutants() {
        let doc = r#"{"outcomes":[{"scenario":"Baseline","summary":"Failure"}],
            "total_mutants":0,"missed":0,"caught":0,"timeout":0,"unviable":0,"success":0}"#;
        let r = parse_outcomes(doc, "x", "0.1.0", &[]).unwrap();
        assert_eq!(r.mutants_total, 0);
    }

    #[test]
    fn empty_outcomes_yield_zero_result() {
        let r = parse_outcomes(r#"{"outcomes":[]}"#, "x", "0.1.0", &[]).unwrap();
        assert_eq!(r.mutants_total, 0);
        assert_eq!(r.kill_pct(), 0.0);
    }

    #[test]
    fn garbage_and_wrong_shape_are_parse_errors() {
        assert!(matches!(
            parse_outcomes("Found 14 mutants to test", "x", "0.1.0", &[]),
            Err(MutateError::ParseError(_))
        ));
        assert!(matches!(
            parse_outcomes(r#"{"total_mutants": 3}"#, "x", "0.1.0", &[]),
            Err(MutateError::ParseError(_))
        ));
    }

    #[test]
    fn older_flat_mutant_shape_is_tolerated() {
        // Older releases used a flat `line` and a plain-string `function`
        // and no `name`.
        let doc = r#"{"outcomes":[
            {"scenario":{"Mutant":{"file":"src/a.rs","line":7,"function":"go",
              "replacement":"()"}},"summary":"MissedMutant"},
            {"scenario":{"Mutant":{"file":"src/a.rs","line":9}},"summary":{"CaughtMutant":null}}
        ]}"#;
        let r = parse_outcomes(doc, "x", "0.1.0", &[]).unwrap();
        assert_eq!(r.mutants_killed, 1);
        assert_eq!(r.survivors[0].line, 7);
        assert_eq!(r.survivors[0].function.as_deref(), Some("go"));
        assert_eq!(r.survivors[0].description, "replace go with ()");
    }

    #[test]
    fn missing_file_and_huge_line_do_not_panic() {
        let doc = r#"{"outcomes":[{"scenario":{"Mutant":{"name":"x",
            "span":{"start":{"line":99999999999}}}},"summary":"MissedMutant"}]}"#;
        let r = parse_outcomes(doc, "x", "0.1.0", &[]).unwrap();
        assert_eq!(r.survivors[0].file, "<unknown>");
        assert_eq!(r.survivors[0].line, u32::MAX);
        assert_eq!(r.survivors[0].description, "x");
    }

    #[test]
    fn strip_location_handles_each_prefix_form() {
        let f = "src/lib.rs";
        assert_eq!(
            strip_location("src/lib.rs:2:35: replace > with == in is_pos", f),
            "replace > with == in is_pos"
        );
        assert_eq!(strip_location("src/lib.rs:2: replace x", f), "replace x");
        assert_eq!(strip_location("src/lib.rs: replace x", f), "replace x");
        // Not prefixed by the file: unchanged.
        assert_eq!(strip_location("other.rs:1:1: y", f), "other.rs:1:1: y");
        // Malformed location: unchanged rather than mangled.
        assert_eq!(strip_location("src/lib.rs:abc", f), "src/lib.rs:abc");
        assert_eq!(strip_location("src/lib.rs:12", f), "src/lib.rs:12");
    }

    #[test]
    fn outcome_strings_map_to_categories() {
        let s = |v: &str| outcome_from_value(&Value::String(v.into()));
        assert_eq!(s("CaughtMutant"), Outcome::Caught);
        assert_eq!(s("MissedMutant"), Outcome::Missed);
        assert_eq!(s("Timeout"), Outcome::Timeout);
        assert_eq!(s("Unviable"), Outcome::Other);
        assert_eq!(s("Failure"), Outcome::Other);
        assert_eq!(s("Success"), Outcome::Other);
    }

    #[cfg(any(unix, windows))]
    fn status(code: i32) -> ExitStatus {
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            ExitStatus::from_raw(code << 8)
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::ExitStatusExt;
            ExitStatus::from_raw(code as u32)
        }
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn exit_codes_map_to_parse_or_error() {
        for ok in [0, 2, 3] {
            assert!(check_exit(status(ok), "", "").is_ok(), "exit {ok}");
        }
        let baseline = check_exit(
            status(4),
            "test tests::t ... FAILED",
            "ERROR cargo test failed in an unmutated tree, so no mutants were tested",
        )
        .unwrap_err();
        let msg = baseline.to_string();
        assert!(msg.contains("unmutated tree"), "{msg}");
        assert!(msg.contains("tests::t ... FAILED"), "{msg}");
        let usage = check_exit(status(1), "", "error: unexpected argument").unwrap_err();
        assert!(usage.to_string().contains("unexpected argument"));
    }

    #[test]
    fn missing_subcommand_detection_matches_cargo_wording() {
        assert!(is_missing_subcommand(
            "error: no such command: `mutants`\n\nhelp: view all installed commands with `cargo --list`"
        ));
        assert!(is_missing_subcommand(
            "error: no such subcommand: `mutants`\r\n"
        ));
        assert!(!is_missing_subcommand(
            "error: toolchain 'x' is not installed"
        ));
    }

    #[test]
    fn stale_outcomes_in_caller_dir_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("outcomes.json");
        fs::write(&p, "{}").unwrap();
        // A run that "started" a minute from now makes the file stale.
        let later = SystemTime::now() + Duration::from_secs(60);
        assert!(read_fresh(&p, later, true).is_err());
        // Temporary directories skip the check.
        assert!(read_fresh(&p, later, false).is_ok());
        assert!(read_fresh(&p, SystemTime::now(), true).is_ok());
    }

    #[test]
    fn temporary_output_dir_is_removed_on_drop() {
        let out = OutputDir::prepare(None).unwrap();
        let p = out.path().to_path_buf();
        assert!(p.is_dir());
        fs::write(p.join("f"), "x").unwrap();
        drop(out);
        assert!(!p.exists());
    }

    #[test]
    fn caller_output_dir_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("nested").join("out");
        let out = OutputDir::prepare(Some(&target)).unwrap();
        assert!(out.is_caller_owned());
        drop(out);
        assert!(target.is_dir());
    }

    #[test]
    fn command_line_includes_output_and_filters() {
        let run = MutateRun::new("x", "0.1.0")
            .workspace()
            .jobs(2)
            .timeout(Duration::from_millis(1500))
            .exclude_re("slow")
            .file("src/lib.rs");
        let cmd = build_command(&run, Path::new("C:/out dir"));
        let args: Vec<String> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            args,
            [
                "mutants",
                "--output",
                "C:/out dir",
                "--workspace",
                "--jobs",
                "2",
                "--timeout",
                "1",
                "--exclude-re",
                "slow",
                "--file",
                "src/lib.rs"
            ]
        );
        // `--json` only applies to `--list`; it must not be passed.
        assert!(!args.iter().any(|a| a == "--json"));
    }
}
