# dev-mutate — API Reference

> Hand-written reference. Mirrors `cargo doc --open` output with
> curated examples and structure.

## Table of contents

- [`MutateRun`](#mutaterun)
  - [`MutateRun::new`](#mutaterunnew)
  - [`MutateRun::execute`](#mutaterunexecute)
- [`SurvivingMutant`](#survivingmutant)
- [`MutateResult`](#mutateresult)
  - [Fields](#mutateresult-fields)
  - [`MutateResult::kill_pct`](#mutateresultkill_pct)
  - [`MutateResult::into_check_result`](#mutateresultinto_check_result)
- [`MutateThreshold`](#mutatethreshold)
  - [`MutateThreshold::MinKillPct`](#mutatethresholdminkillpct)
  - [`MutateThreshold::min_kill_pct`](#mutatethresholdmin_kill_pct)
- [`MutateError`](#mutateerror)

---

## `MutateRun`

```rust
pub struct MutateRun { /* private */ }
```

### `MutateRun::new`

```rust
pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self
```

| Parameter | Type                | Description    |
|-----------|---------------------|----------------|
| `name`    | `impl Into<String>` | Crate name.    |
| `version` | `impl Into<String>` | Crate version. |

```rust
use dev_mutate::MutateRun;

let run = MutateRun::new("my-crate", "0.1.0");
```

### `MutateRun::execute`

```rust
pub fn execute(&self) -> Result<MutateResult, MutateError>
```

Run mutation testing via `cargo-mutants`.

---

## `SurvivingMutant`

```rust
pub struct SurvivingMutant {
    pub file: String,
    pub line: u32,
    pub description: String,
}
```

A mutation the test suite did NOT catch. Tells you exactly where
test coverage is weak.

---

## `MutateResult`

```rust
pub struct MutateResult {
    pub name: String,
    pub version: String,
    pub mutants_total: u64,
    pub mutants_killed: u64,
    pub mutants_survived: u64,
    pub mutants_timeout: u64,
    pub survivors: Vec<SurvivingMutant>,
}
```

### MutateResult fields

| Field             | Type                    | Description                                       |
|-------------------|-------------------------|---------------------------------------------------|
| `name`            | `String`                | Crate name.                                       |
| `version`         | `String`                | Crate version.                                    |
| `mutants_total`   | `u64`                   | Total mutants generated.                          |
| `mutants_killed`  | `u64`                   | Mutants the test suite caught.                    |
| `mutants_survived`| `u64`                   | Mutants the test suite missed.                    |
| `mutants_timeout` | `u64`                   | Mutants that timed out (excluded from kill rate). |
| `survivors`       | `Vec<SurvivingMutant>`  | Details of surviving mutants.                     |

### `MutateResult::kill_pct`

```rust
pub fn kill_pct(&self) -> f64
```

Kill rate as a percent in `[0.0, 100.0]`. Formula:

```text
kill_pct = killed / (killed + survived) * 100
```

Timeouts are excluded from both numerator and denominator since
they don't tell us anything about test quality.

```rust
use dev_mutate::MutateResult;

let r = MutateResult {
    name: "x".into(), version: "0.1.0".into(),
    mutants_total: 110, mutants_killed: 80,
    mutants_survived: 20, mutants_timeout: 10,
    survivors: vec![],
};
// 80 / (80 + 20) = 80%, timeouts excluded.
assert!((r.kill_pct() - 80.0).abs() < 0.0001);
```

### `MutateResult::into_check_result`

```rust
pub fn into_check_result(self, threshold: MutateThreshold) -> CheckResult
```

Convert this result into a `dev-report::CheckResult` against the
threshold. The kill rate is attached as `Evidence::Numeric` so
downstream consumers can sort or graph it.

```rust
use dev_mutate::{MutateResult, MutateThreshold};

# let r = MutateResult {
#     name: "x".into(), version: "0.1.0".into(),
#     mutants_total: 100, mutants_killed: 85,
#     mutants_survived: 15, mutants_timeout: 0,
#     survivors: vec![],
# };
let check = r.into_check_result(MutateThreshold::min_kill_pct(80.0));
```

---

## `MutateThreshold`

```rust
pub enum MutateThreshold {
    MinKillPct(f64),
}
```

### `MutateThreshold::MinKillPct`

Fail if `result.kill_pct() < pct`.

### `MutateThreshold::min_kill_pct`

```rust
pub fn min_kill_pct(pct: f64) -> Self
```

Build a kill-rate threshold.

```rust
use dev_mutate::MutateThreshold;

let t = MutateThreshold::min_kill_pct(70.0);
```

Typical targets:

| Project type            | Reasonable threshold |
|-------------------------|----------------------|
| Library, production     | 70-80%               |
| Library, mature         | 85%+                 |
| Application             | 50-60%               |
| Cryptography / security | 95%+                 |

---

## `MutateError`

```rust
pub enum MutateError {
    ToolNotInstalled,
    SubprocessFailed(String),
    ParseError(String),
}
```

Tool remediation: `cargo install cargo-mutants`.
