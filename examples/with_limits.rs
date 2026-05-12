//! Configure per-mutant timeout, parallel job count, and filters.
//!
//! ```text
//! cargo run --example with_limits
//! ```

use std::time::Duration;

use dev_mutate::{MutateError, MutateRun, MutateThreshold};

fn main() {
    let run = MutateRun::new("example", "0.1.0")
        .workspace()
        .jobs(4)
        .timeout(Duration::from_secs(120))
        .exclude_re(r"^src/generated/")
        .file("src/lib.rs")
        .allow("replace `+` with `-`");

    match run.execute() {
        Ok(result) => {
            let check = result.into_check_result(MutateThreshold::min_kill_pct(75.0));
            println!("verdict: {:?}", check.verdict);
            if let Some(d) = check.detail {
                println!("detail:  {d}");
            }
        }
        Err(MutateError::ToolNotInstalled) => {
            eprintln!("cargo-mutants is not installed; skipping example.");
        }
        Err(e) => eprintln!("mutation run failed: {e}"),
    }
}
