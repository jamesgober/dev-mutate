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
</p>

<p align="center">
    Test-suite quality verification via deliberate code mutations.<br>
    Part of the <code>dev-*</code> verification suite.
</p>

---

## What it does

`dev-mutate` wraps `cargo-mutants` and emits results as
`dev-report::Report`. It answers the question: **is your test suite
actually testing what you think it is?**

## What is mutation testing?

A tool makes small deliberate changes to your code — flipping `<` to
`>`, changing `+` to `-`, removing a `return`, swapping a boolean.
Then it runs your tests against each mutation.

- **Killed mutant**: a test failed. Good — your tests caught the bug.
- **Surviving mutant**: all tests still passed despite the broken
  code. Bad — your tests aren't really testing that behavior.

The **kill rate** is the percent of mutants caught. High coverage with
a low kill rate means lots of tests but they don't assert enough.

## Quick start

```toml
[dependencies]
dev-mutate = "0.9"
```

```rust
use dev_mutate::{MutateRun, MutateThreshold};

let run = MutateRun::new("my-crate", "0.1.0");
let result = run.execute()?;

let threshold = MutateThreshold::min_kill_pct(70.0);
let check = result.into_check_result(threshold);
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Requirements

```bash
cargo install cargo-mutants
```

## Typical kill-rate targets

| Project type            | Reasonable target |
|-------------------------|-------------------|
| Library, production     | 70-80%            |
| Library, mature         | 85%+              |
| Application             | 50-60%            |
| Cryptography / security | 95%+              |

The kill-rate metric excludes timeout mutants from both numerator
and denominator since they don't tell us anything about test quality.

## The `dev-*` suite

See [`dev-tools`](https://github.com/jamesgober/dev-tools) for the
full suite.

## Status

`v0.9.0` is the foundation release: API shape defined, subprocess
integration lands in `0.9.1`. Production use is discouraged until
`1.0`.

## Minimum supported Rust version

`1.85` — pinned in `Cargo.toml` and verified by CI.

## License

Apache-2.0. See [LICENSE](LICENSE).
