# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.9.3] - 2026-10-09

cargo-mutants results are read from `outcomes.json`, plus fixes from a
review pass and the MSRV rollback to Rust 1.75. The rollback follows
`dev-fixtures` 0.9.5 swapping `tempfile` for `mod-tempdir` 1.0, which
removed the `getrandom 0.4.2 -> edition2024` chain that held the dev-*
collection at 1.85.

### Added

- `MutateRun::output_dir(dir)` keeps cargo-mutants' `mutants.out/` in a
  directory you choose. By default each run uses a fresh temporary
  directory that is removed afterwards, so nothing is left in the
  source tree. Outcomes older than the run in a kept directory are
  rejected as stale.
- `VERSION` constant with the crate version as compiled, so tools that
  bundle this crate can report what is actually linked.

### Fixed

- **Every run reported 0 mutants and a 0% kill rate.** Results were read
  from stdout as NDJSON via `--json`, but cargo-mutants only honours
  `--json` with `--list`; a real run prints text. Results now come from
  `mutants.out/outcomes.json`, checked against real cargo-mutants 27
  runs with caught, missed, timeout, unviable and baseline-failure
  outcomes. The deprecated `--no-shuffle` flag is no longer passed.
- A test suite that already fails on the unmutated tree (exit code 4)
  was reported as a 0% kill rate. It is now `SubprocessFailed` with
  stderr and the tail of the output. Exit codes 0, 2 and 3 are parsed;
  anything else is an error.
- Survivor descriptions carried a `file:line:col:` prefix, so `allow()`
  entries written from cargo-mutants' own output never matched. The
  allow-list also updated the totals but not the per-file breakdown;
  both agree now.
- Any failing `cargo mutants --version` was reported as
  `ToolNotInstalled`. Only cargo's "no such command" error means not
  installed now; the probe runs in the work dir, and a missing work dir
  gives a clear error.
- A NaN threshold passed every run, and huge deserialized counts could
  overflow `kill_pct`. Both fixed.
- The producer test was `#[ignore]`d; it now runs against an empty temp
  dir.

### Changed

- `rust-version` lowered from `1.85` to `1.75`. CI's MSRV job now
  builds on 1.75 against an MSRV-compatible lockfile; it was still
  pinned to 1.85.

### Documentation

- `exclude_re` matches mutant names, not files.
- Allow-list examples use the real description format.
- New README section "How results are collected"; MSRV section says
  1.75.
- `docs/API.md` fields and examples fixed (missing `files` and
  `function`).

[0.9.3]: https://github.com/jamesgober/dev-mutate/releases/tag/v0.9.3

## [0.9.2] - 2026-05-12

Cleanup release surfaced by the post-polish audit pass. No behavior change.

### Changed

- `aggregate_breakdown()` (pub(crate) helper) no longer takes a `&[SurvivingMutant]` parameter — it was unused and silenced via `let _ = survivors;`. Removed both the parameter and the dead-binding, and updated the single caller in `runner.rs` to match. The function signature now mirrors exactly what it consumes (the three `by_file_*` BTreeMaps).

[0.9.2]: https://github.com/jamesgober/dev-mutate/releases/tag/v0.9.2

## [0.9.1] - 2026-05-12

Documentation and SEO pass. No code changes.

### Changed

- README header standardized: Rust logo image, MSRV badge between CI and docs.rs (was at the end, lowercase label), copyright block at bottom.
- Subtitle now reads `MUTATION TESTING WITH KILL-RATE GATES` (was `MUTATION TESTING FOR RUST`). Surfaces the kill-rate gate as the defining capability.
- Tagline rewritten to lead with the wrapped tool, the headline metric (kill rate), and the actual developer outcome (catch tests that don't assert).
- `## The dev-* suite` retitled to `The dev-* collection` and expanded with the full 14-crate map.
- `Cargo.toml` description rewritten: lists the metrics (kill rate, surviving-mutant evidence) and the gating mechanism (threshold).
- `Cargo.toml` keywords retuned: dropped `verification` and `ai-tools`, added `cargo-mutants` and `ci` for crates.io search.

### Added

- "Part of the `dev-*` verification collection" block on the README, under the intro, linking the umbrella `dev-tools` crate.

[0.9.1]: https://github.com/jamesgober/dev-mutate/releases/tag/v0.9.1

## [0.9.0] - 2026-05-12

Foundation release. Replaces the `0.1.0` name-claim with full
`cargo-mutants` integration.

### Added

- Real `cargo mutants --json --no-shuffle` subprocess integration. Detects missing `cargo-mutants` as `MutateError::ToolNotInstalled`; subprocess errors with empty stdout become `MutateError::SubprocessFailed(stderr)`.
- NDJSON parser in `src/runner.rs` recognizes cargo-mutants' per-mutant records. Outcome classification: `Caught` → killed; `Missed` → survived; `Timeout` → timeout (excluded from kill rate per REPS § 3); `Failure` / `Unviable` → tracked in `mutants_total` but not in killed / survived / timeout. Supports both the plain-string `"outcome": "Caught"` shape and the tagged-enum `"outcome": {"Caught": ...}` shape.
- `MutateRun` builder gains the full surface: `in_dir(path)`, `workspace()`, `jobs(n)`, `timeout(Duration)`, `exclude_re(pattern)`, `file(pattern)`, `allow(description)`, `allow_all(iter)`, `subject()`, `subject_version()`.
- New `FileBreakdown` type with per-file `killed`, `survived`, `timeout` counts and a `kill_pct()` helper. Populated automatically from the NDJSON records.
- `SurvivingMutant` gains an optional `function: Option<String>` field. The struct is now `serde`-derived for wire-format round-tripping.
- `MutateResult` gains: `files: Vec<FileBreakdown>` (sorted by file path), `meets(threshold)` helper, `weakest_files(n)` returning the lowest-kill-rate files in ascending order.
- `MutateResult::into_check_result` now tags the produced `CheckResult` with `mutate`, attaches numeric evidence for `kill_pct`, `kill_pct_threshold`, `mutants_killed`, `mutants_survived`, `mutants_timeout`, and (when survivors exist) emits the first survivor as `Evidence::FileRef` pointing at its `(file, line)` for one-click navigation.
- Deterministic ordering: survivors sorted by `(file, line)`; file breakdown sorted by `file`.
- Allow-list reclassifies known-survivors as killed for kill-rate purposes — the user has explicitly declared them acceptable.
- New `producer` module exposing `MutateProducer`: a `dev_report::Producer` adapter. Subprocess failures map to a single `CheckResult::fail("mutate::<subject>", Severity::Critical)` tagged `mutate` + `subprocess`. The producer test that exercises the real subprocess pipeline is `#[ignore]`d (would deadlock on the workspace target-dir lock).
- 19 unit tests across `lib.rs`, `runner.rs`, `producer.rs`. Coverage includes: kill-rate math (with and without timeouts in the denominator), threshold pass / fail with `Severity::Warning`, `meets()` helper alignment, first-survivor evidence attachment, weakest-files ordering, `FileBreakdown::kill_pct`, JSON round-trip on `MutateResult`, NDJSON parsing for both `"Caught"` and `{"Caught": ...}` outcome shapes, baseline-scenario filtering, non-JSON line tolerance, unrecognized outcome handling (counted in total but not classified), per-file breakdown aggregation, survivor ordering, missing-file fallback.
- 8 integration tests in `tests/smoke.rs` plus one `#[ignore]`d real-subprocess test (documents the `CARGO_TARGET_DIR` workaround).
- Examples: `basic.rs` (graceful tool-missing handling), `with_threshold.rs` (constructed result; demonstrates `meets` and `weakest_files`), `with_limits.rs` (workspace + jobs + timeout + filters + allow-list), `producer.rs` (gated by `DEV_MUTATE_EXAMPLE_RUN`).

### Changed

- README rewritten: removes the "subprocess integration lands in 0.9.1" disclaimer, documents the builder surface, the kill-rate math, the per-file breakdown, the allow-list workflow, and the producer integration. MSRV pinned at 1.85.
- REPS.md tightened: the "SHOULD provide" items (subprocess integration, per-file breakdown, survivor-list attachment via `Evidence::FileRef`) become MUST-have for 0.9.x.
- CI workflow: new `integration` job installs `cargo-mutants` via `taiki-e/install-action@v2` and verifies the tool runs. Sibling-clone of `../dev-report` in every job; `actions/checkout@v5` everywhere.

### Dependencies

- Added: `serde` 1.0 (derive feature), `serde_json` 1.0. Required for parsing `cargo mutants --json` NDJSON output and for serializing `MutateResult` / `SurvivingMutant` / `FileBreakdown`.
- Added: `tempfile` 3 as a `dev-dependency`.

### Note

`0.1.0` was a name-claim publish with a stub `execute()` returning an empty result. The public API surface changed in one breaking way:

1. `MutateResult` gained a `files: Vec<FileBreakdown>` field. Callers that constructed `MutateResult` struct literals in 0.1.0 must add the field (initialize to `Vec::new()` if irrelevant).

2. `SurvivingMutant` gained an optional `function` field. The field is `#[serde(default, skip_serializing_if = "Option::is_none")]` so JSON round-trips remain backward-compatible.

The constructor surface (`MutateRun::new`, `MutateThreshold::min_kill_pct`, `MutateResult::kill_pct`, `into_check_result`) is unchanged.

[Unreleased]: https://github.com/jamesgober/dev-mutate/compare/v0.9.0...HEAD
[0.9.0]: https://github.com/jamesgober/dev-mutate/releases/tag/v0.9.0
[0.1.0]: https://github.com/jamesgober/dev-mutate/releases/tag/v0.1.0
