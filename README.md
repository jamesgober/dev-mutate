<h1 align="center">
    <img width="99" alt="Rust logo" src="https://raw.githubusercontent.com/jamesgober/rust-collection/72baabd71f00e14aa9184efcb16fa3deddda3a0a/assets/rust-logo.svg">
    <br>
    <strong>dev-mutate</strong>
    <br>
    <sup><sub>MUTATION TESTING WITH KILL-RATE GATES</sub></sup>
</h1>
<p align="center">
    <a href="https://crates.io/crates/dev-mutate"><img alt="crates.io" src="https://img.shields.io/crates/v/dev-mutate.svg"></a>
    <a href="https://crates.io/crates/dev-mutate"><img alt="downloads" src="https://img.shields.io/crates/d/dev-mutate.svg"></a>
    <a href="https://github.com/jamesgober/dev-mutate/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/jamesgober/dev-mutate/actions/workflows/ci.yml/badge.svg"></a>
    <img alt="MSRV" src="https://img.shields.io/badge/MSRV-1.85%2B-blue.svg?style=flat-square" title="Rust Version">
    <a href="https://docs.rs/dev-mutate"><img alt="docs.rs" src="https://docs.rs/dev-mutate/badge.svg"></a>
</p>

<p align="center">
    <strong>Wraps <code>cargo-mutants</code>; computes kill rate, surfaces surviving mutants, gates against a threshold.</strong> Detect tests that pass without asserting anything.
</p>

<br>

<div align="center">
    <strong>Part of the <a href="https://crates.io/crates/dev-tools"><code>dev-*</code></a> verification collection.</strong><br>
    <sub>Also available as the <code>mutate</code> feature of the <a href="https://crates.io/crates/dev-tools"><code>dev-tools</code></a> umbrella crate &mdash; one dependency, every verification layer.</sub>
</div>

<br>

---

## What it does

`dev-mutate` wraps [`cargo-mutants`](https://crates.io/crates/cargo-mutants)
and emits results as a [`dev-report::Report`](https://docs.rs/dev-report).
It answers the question: **is your test suite actually testing what
you think it is?**

## What is mutation testing?

A tool makes small deliberate changes to your code — flipping `<` to
`>`, changing `+` to `-`, removing a `return`, swapping a boolean.
Then it runs your tests against each mutation.

- **Killed mutant**: a test failed. Good — your tests caught the bug.
- **Surviving mutant**: all tests still passed despite the broken
  code. Bad — your tests aren't really testing that behavior.

The **kill rate** is the percent of mutants caught. High coverage
with a low kill rate means lots of tests but they don't assert
enough.

## Quick start

```toml
[dependencies]
dev-mutate = "0.9"
```

One-time tool install:

```bash
cargo install cargo-mutants
```

Drive it from code:

```rust,no_run
use dev_mutate::{MutateRun, MutateThreshold};

let run = MutateRun::new("my-crate", "0.1.0");
let result = run.execute()?;
let threshold = MutateThreshold::min_kill_pct(70.0);
let check = result.into_check_result(threshold);
println!("{:?} {:?}", check.verdict, check.detail);
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Builder surface

| Method                            | What it does                                                          |
|-----------------------------------|------------------------------------------------------------------------|
| `in_dir(path)`                    | Run `cargo mutants` from a different directory.                       |
| `workspace()`                     | Pass `--workspace` (mutate every workspace member).                   |
| `jobs(n)`                         | Pass `--jobs <N>` (parallel mutation runs).                            |
| `timeout(Duration)`               | Per-mutant timeout (`--timeout <secs>`).                              |
| `exclude_re(pattern)`             | Skip files matching the regex (`--exclude-re <pattern>`). Repeatable. |
| `file(pattern)`                   | Restrict to matching files (`--file <pattern>`). Repeatable.          |
| `allow(description)` / `allow_all(iter)` | Reclassify known survivors as killed (e.g. `replace + with -`). |

## Kill rate

```text
kill_pct = killed / (killed + survived) * 100
```

Timeouts are **excluded** from both numerator and denominator —
they don't reflect test quality, they reflect test speed.

## Typical kill-rate targets

| Project type            | Reasonable target |
|-------------------------|-------------------|
| Library, production     | 70–80%            |
| Library, mature         | 85%+              |
| Application             | 50–60%            |
| Cryptography / security | 95%+              |

## Per-file breakdown

`MutateResult::files` is a sorted list of `FileBreakdown` records,
one per source file. Use `weakest_files(n)` to spotlight the lowest
kill-rate hotspots:

```rust
use dev_mutate::MutateResult;
# let result: MutateResult = unimplemented!();
for f in result.weakest_files(5) {
    println!("{:<30} {:.1}%  (killed {}, survived {})",
             f.file, f.kill_pct(), f.killed, f.survived);
}
```

## Allow-list known false positives

```rust,no_run
use dev_mutate::{MutateRun, MutateThreshold};

let run = MutateRun::new("my-crate", "0.1.0")
    .allow("replace `+` with `-`")
    .allow_all(["replace `<` with `<=`", "delete `!`"]);
let _result = run.execute()?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

Allow-listed mutations are reclassified as *killed* (the user has
explicitly declared them acceptable), so the kill rate goes up
accordingly.

## `Producer` integration

`MutateProducer` plugs the run into a multi-producer pipeline driven
by [`dev-tools`](https://github.com/jamesgober/dev-tools):

```rust,no_run
use dev_mutate::{MutateProducer, MutateRun, MutateThreshold};
use dev_report::Producer;

let producer = MutateProducer::new(
    MutateRun::new("my-crate", "0.1.0"),
    MutateThreshold::min_kill_pct(70.0),
);
let report = producer.produce();
println!("{}", report.to_json().unwrap());
```

Subprocess failures map to a single failing `CheckResult` named
`mutate::<subject>` with `Severity::Critical` — the pipeline keeps
running.

## Target-dir-lock note

Running `MutateRun::execute()` from inside another `cargo`
invocation that already holds the workspace target-dir lock will
deadlock — `cargo mutants` itself drives `cargo test` repeatedly.
Use a separate target dir:

```bash
CARGO_TARGET_DIR=/tmp/mutate-target cargo run --example basic
CARGO_TARGET_DIR=/tmp/mutate-target cargo test -- --ignored
```

## Wire format

`MutateResult`, `SurvivingMutant`, and `FileBreakdown` are all
`serde`-derived. JSON uses `snake_case` field names:

```json
{
  "name": "my-crate",
  "version": "0.1.0",
  "mutants_total": 120,
  "mutants_killed": 88,
  "mutants_survived": 22,
  "mutants_timeout": 10,
  "survivors": [
    {
      "file": "src/parser.rs",
      "line": 142,
      "description": "replace `<` with `<=`",
      "function": "validate_range"
    }
  ],
  "files": [
    { "file": "src/parser.rs", "killed": 30, "survived": 10, "timeout": 2 }
  ]
}
```

## Examples

| File                              | What it shows                                                |
|-----------------------------------|---------------------------------------------------------------|
| `examples/basic.rs`               | Run against the current crate; graceful tool-missing handling. |
| `examples/with_threshold.rs`      | Constructed result; demonstrates `meets` and `weakest_files`. |
| `examples/with_limits.rs`         | `workspace` + `jobs` + `timeout` + filters + allow-list.      |
| `examples/producer.rs`            | `MutateProducer` (gated by `DEV_MUTATE_EXAMPLE_RUN`).         |

## The `dev-*` collection

`dev-mutate` ships independently and is also re-exported by the
[`dev-tools`](https://crates.io/crates/dev-tools) umbrella crate as
the `mutate` feature. Sister crates cover the other verification
dimensions:

- [`dev-report`](https://crates.io/crates/dev-report) &mdash; report schema everything emits
- [`dev-fixtures`](https://crates.io/crates/dev-fixtures) &mdash; deterministic test fixtures
- [`dev-bench`](https://crates.io/crates/dev-bench) &mdash; performance and regression detection
- [`dev-async`](https://crates.io/crates/dev-async) &mdash; async runtime verification
- [`dev-stress`](https://crates.io/crates/dev-stress) &mdash; stress and soak workloads
- [`dev-chaos`](https://crates.io/crates/dev-chaos) &mdash; fault injection and recovery testing
- [`dev-coverage`](https://crates.io/crates/dev-coverage) &mdash; code coverage with regression gates
- [`dev-security`](https://crates.io/crates/dev-security) &mdash; CVE / license / banned-crate audit
- [`dev-deps`](https://crates.io/crates/dev-deps) &mdash; unused / outdated dep detection
- [`dev-ci`](https://crates.io/crates/dev-ci) &mdash; GitHub Actions workflow generator
- [`dev-fuzz`](https://crates.io/crates/dev-fuzz) &mdash; fuzz testing workflow
- [`dev-flaky`](https://crates.io/crates/dev-flaky) &mdash; flaky-test detection

## Status

`v0.9.x` is the pre-1.0 stabilization line. Feature-complete for
mutation testing, per-file breakdown, threshold, allow-list, and
producer integration. `1.0` will pin the public API and the
kill-rate computation.

## Minimum supported Rust version

`1.85` — pinned in `Cargo.toml` via `rust-version` and verified by
the MSRV job in CI.

## License

Apache-2.0. See [LICENSE](LICENSE).




<!-- COPYRIGHT
---------------------------------->
<div align="center">
    <br>
    <h2></h2>
    Copyright &copy; 2026 James Gober.
</div>
