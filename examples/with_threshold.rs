//! Demonstrate `MutateThreshold` against a constructed `MutateResult`.
//! No subprocess; no external tooling required.
//!
//! ```text
//! cargo run --example with_threshold
//! ```

use dev_mutate::{FileBreakdown, MutateResult, MutateThreshold, SurvivingMutant};

fn main() {
    let result = MutateResult {
        name: "demo".into(),
        version: "0.1.0".into(),
        mutants_total: 120,
        mutants_killed: 88,
        mutants_survived: 22,
        mutants_timeout: 10,
        survivors: vec![SurvivingMutant {
            file: "src/parser.rs".into(),
            line: 142,
            description: "replace `<` with `<=`".into(),
            function: Some("validate_range".into()),
        }],
        files: vec![
            FileBreakdown {
                file: "src/parser.rs".into(),
                killed: 30,
                survived: 10,
                timeout: 2,
            },
            FileBreakdown {
                file: "src/codec.rs".into(),
                killed: 58,
                survived: 12,
                timeout: 8,
            },
        ],
    };

    println!("kill rate: {:.2}%", result.kill_pct());

    for threshold in [
        MutateThreshold::min_kill_pct(70.0),
        MutateThreshold::min_kill_pct(85.0),
    ] {
        let MutateThreshold::MinKillPct(t) = threshold;
        let passes = result.meets(threshold);
        println!(
            "  threshold >= {:>5.1}%  -> {}",
            t,
            if passes { "Pass" } else { "Fail" }
        );
    }

    println!("\nweakest files (ascending kill rate):");
    for f in result.weakest_files(5) {
        println!(
            "  {:<20} {:.2}%  (killed {}, survived {})",
            f.file,
            f.kill_pct(),
            f.killed,
            f.survived
        );
    }
}
