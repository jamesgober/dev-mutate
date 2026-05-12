//! Run mutation testing on the current crate and print the report.
//!
//! ```text
//! cargo install cargo-mutants
//! cargo run --example basic
//! ```
//!
//! If `cargo-mutants` is missing the example prints a clear error and
//! exits 0 so `cargo build --examples` keeps working without the tool.

use dev_mutate::{MutateError, MutateRun, MutateThreshold};

fn main() {
    let run = MutateRun::new("example", "0.1.0");
    let result = match run.execute() {
        Ok(r) => r,
        Err(MutateError::ToolNotInstalled) => {
            eprintln!(
                "cargo-mutants is not installed; install with `cargo install cargo-mutants`."
            );
            return;
        }
        Err(e) => {
            eprintln!("mutation run failed: {e}");
            return;
        }
    };
    println!(
        "kill rate: {:.2}% (killed {} / surviving {} / timeouts {})",
        result.kill_pct(),
        result.mutants_killed,
        result.mutants_survived,
        result.mutants_timeout
    );
    let check = result.into_check_result(MutateThreshold::min_kill_pct(70.0));
    println!("verdict: {:?}", check.verdict);
    if let Some(d) = check.detail {
        println!("detail:  {d}");
    }
}
