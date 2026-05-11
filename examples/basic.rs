//! Minimal example: run mutation testing and emit a verdict.
//!
//! Run with: `cargo run --example basic`

use dev_mutate::{MutateRun, MutateThreshold};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let run = MutateRun::new("example", "0.1.0");
    let result = run.execute()?;
    println!("Kill rate: {:.2}%", result.kill_pct());
    let check = result.into_check_result(MutateThreshold::min_kill_pct(70.0));
    println!("Verdict: {:?}", check.verdict);
    if let Some(d) = check.detail {
        println!("Detail:  {d}");
    }
    Ok(())
}
