<h1 align="center">
    <strong>dev-mutate</strong>
    <br>
    <sup><sub>MUTATION TESTING FOR RUST</sub></sup>
</h1>

<p align="center">
    <a href="https://crates.io/crates/dev-mutate"><img alt="crates.io" src="https://img.shields.io/crates/v/dev-mutate.svg"></a>
    <a href="https://crates.io/crates/dev-mutate"><img alt="downloads" src="https://img.shields.io/crates/d/dev-mutate.svg"></a>
    <a href="https://docs.rs/dev-mutate"><img alt="docs.rs" src="https://docs.rs/dev-mutate/badge.svg"></a>
    <a href="https://github.com/jamesgober/dev-mutate/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/jamesgober/dev-mutate/actions/workflows/ci.yml/badge.svg"></a>
    <img alt="MSRV" src="https://img.shields.io/badge/msrv-1.85%2B-blue.svg?style=flat-square" title="Rust Version">
</p>

<p align="center">
    Test-suite quality verification via deliberate code mutations.<br>
    Part of the <code>dev-*</code> verification suite.
</p>

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

## The `dev-*` suite

See [`dev-tools`](https://github.com/jamesgober/dev-tools) for the
umbrella crate covering the full suite.

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
