# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.9.0] - 2026-05-11

### Added

- Initial crate skeleton.
- `MutateRun` builder.
- `SurvivingMutant` struct (file, line, description).
- `MutateResult` with `mutants_total`, `mutants_killed`,
  `mutants_survived`, `mutants_timeout`, `survivors` list.
- `MutateResult::kill_pct` excluding timeouts from the denominator.
- `MutateThreshold::MinKillPct` and `min_kill_pct` constructor.
- `MutateResult::into_check_result(threshold)` produces a
  `dev-report::CheckResult` with kill_pct attached as
  `Evidence::Numeric`.
- `MutateError` for tool-missing / subprocess / parse failures.
- Smoke tests covering kill-rate math, threshold pass/fail, zero-mutants
  edge case.

### Note

This is the name-claim release. The actual `cargo-mutants` subprocess
integration lands in `0.9.1`.

[Unreleased]: https://github.com/jamesgober/dev-mutate/compare/v0.9.0...HEAD
[0.9.0]: https://github.com/jamesgober/dev-mutate/releases/tag/v0.9.0
