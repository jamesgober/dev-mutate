# dev-mutate — Project Specification (REPS)

> Rust Engineering Project Specification.
> Normative language follows RFC 2119.

## 1. Purpose

`dev-mutate` MUST run mutation testing and emit results as
`dev-report::Report`. Output MUST be machine-readable so AI agents
and CI gates can act on test-suite-quality findings without parsing
free-form logs.

## 2. Scope

This crate MUST provide:

- A `MutateRun` builder with `in_dir`, `workspace`, `jobs`,
  `timeout`, `exclude_re`, `file`, `allow`, `allow_all`, `subject`,
  `subject_version`, and `execute`.
- A `MutateResult` with `mutants_total`, `mutants_killed`,
  `mutants_survived`, `mutants_timeout`, `survivors`, `files`, and
  `kill_pct`, `meets`, `weakest_files`, `into_check_result`.
- A `SurvivingMutant` struct with `file`, `line`, `description`,
  `function`.
- A `FileBreakdown` struct with per-file counts and `kill_pct`.
- A `MutateThreshold::MinKillPct` variant and `min_kill_pct`
  constructor.
- `cargo-mutants` subprocess integration with NDJSON parsing for
  both the plain-string and tagged-enum outcome shapes.
- A `MutateProducer` adapter implementing `dev_report::Producer`.

This crate MAY provide later:

- Mutation operator selection (which mutations to apply).
- Diff against a stored baseline (this run vs. a previous run's
  kill rate).
- Test-name attribution: link surviving mutants to the tests that
  *should* have caught them.

This crate MUST NOT:

- Implement a mutation engine. We wrap `cargo-mutants`.
- Auto-edit source code. The underlying tool handles all mutation;
  we never write to user source.
- Replace `mutagen` or other alternatives. Pick one engine — the
  choice is `cargo-mutants`.

## 3. Kill-rate definition

```text
kill_pct = killed / (killed + survived) * 100
```

Timeouts MUST NOT count toward either numerator or denominator. A
timeout doesn't indicate test quality; it indicates the test suite
itself is too slow to grade.

`MutateResult::kill_pct()` MUST return `0.0` when `killed +
survived == 0` (i.e. only timeouts or no mutants at all). This
defines the "no data" case as failing — there is no meaningful kill
rate to report.

## 4. Severity mapping

A kill rate below the threshold produces a `Fail` verdict with
`Severity::Warning`. Mutation testing is advisory by default;
escalation to `Error` is a future configurable option.

## 5. Determinism

The same project + same `cargo-mutants` snapshot SHOULD produce the
same `MutateResult` (mutation generation is deterministic when
`--no-shuffle` is passed, which `dev-mutate` does by default).

`MutateResult::into_check_result` MUST be byte-deterministic given a
fixed `MutateResult` and threshold (modulo the auto-stamped
timestamps on the produced `Report`). Survivor ordering MUST be
ascending by `(file, line)`. File breakdown ordering MUST be
ascending by `file`.

## 6. Wire format

`MutateResult`, `SurvivingMutant`, and `FileBreakdown` MUST be
serializable via `serde_json`. Field names MUST use `snake_case`.
Optional fields with default values (e.g. `survivors = vec![]`,
`files = vec![]`, `function = None`) MUST be omitted from
serialization.

## 7. Tool dependency

`cargo-mutants` MUST be installed externally. Detection of missing
tool produces `MutateError::ToolNotInstalled`.

Subprocess failures (non-zero exit *and* empty stdout) MUST surface
as `MutateError::SubprocessFailed(stderr)`. Parse failures MUST
surface as `MutateError::ParseError(detail)`. Neither MUST cause a
panic.

`cargo-mutants` returns non-zero exit when it finds surviving
mutants — this is the success path for `dev-mutate`. The crate MUST
parse stdout regardless of exit code as long as stdout is non-empty.

## 8. Producer contract

`MutateProducer::produce()` MUST always return a `Report`. It MUST
NOT panic on subprocess failure; instead, it MUST emit a single
`CheckResult::fail("mutate::<subject>", Severity::Critical)`
carrying the error message in `detail` and the tags `mutate` +
`subprocess`.

## 9. Stability

Through `0.9.x` the public API MAY shift. The `1.0` release pins the
API and the kill-rate computation. The wire format of
`MutateResult` and the JSON shape emitted through `dev-report` MUST
stay stable from `1.0` onward.
