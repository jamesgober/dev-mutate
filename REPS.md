# dev-mutate — Project Specification (REPS)

> Rust Engineering Project Specification.
> Normative language follows RFC 2119.

## 1. Purpose

`dev-mutate` MUST run mutation testing and emit results as
`dev-report::Report`. Output MUST be machine-readable so AI agents
and CI gates can act on test-suite-quality findings.

## 2. Scope

This crate MUST provide:

- A `MutateRun` builder.
- A `MutateResult` with counts and a `kill_pct` computation.
- A `MutateThreshold::MinKillPct` for verdict mapping.
- A `SurvivingMutant` struct for actionable detail on what wasn't caught.
- A `CheckResult` integration via `into_check_result`.

This crate SHOULD provide (later versions):

- `cargo-mutants` subprocess integration (`0.9.1`).
- Per-file kill-rate breakdown.
- Survivor-list attachment via `Evidence::FileRef`.
- Mutation operator selection (which mutations to apply).

This crate MUST NOT:

- Implement a mutation engine. We wrap `cargo-mutants`.
- Auto-edit source code (the underlying tool handles all mutation;
  we never write to user source).
- Replace `mutagen` or other alternatives. Pick one engine.

## 3. Kill-rate definition

```text
kill_pct = killed / (killed + survived) * 100
```

Timeouts MUST NOT count toward either numerator or denominator. A
timeout doesn't indicate test quality; it indicates the test suite
itself is too slow to grade.

## 4. Severity mapping

A kill rate below the threshold produces a `Fail` verdict with
`Severity::Warning`. Mutation testing is advisory by default; rate
escalation to `Error` is a future configurable option.

## 5. Stability

Through `0.9.x` the public API MAY shift. The `1.0` release pins the
API and the kill-rate computation.
