//! Demonstrate `MutateProducer` — the `dev_report::Producer` adapter.
//!
//! Construction is always safe and cheap. The actual subprocess
//! invocation is gated behind the `DEV_MUTATE_EXAMPLE_RUN` env var so
//! CI doesn't pay for a full mutation run on every example build.
//!
//! ```text
//! cargo run --example producer
//! DEV_MUTATE_EXAMPLE_RUN=1 cargo run --example producer
//! ```

use dev_mutate::{MutateProducer, MutateRun, MutateThreshold};
use dev_report::Producer;

fn main() {
    let producer = MutateProducer::new(
        MutateRun::new("my-crate", "0.1.0"),
        MutateThreshold::min_kill_pct(70.0),
    );
    println!("Constructed MutateProducer for 'my-crate' v0.1.0.");

    if std::env::var("DEV_MUTATE_EXAMPLE_RUN").is_ok() {
        let report = producer.produce();
        println!("{}", report.to_json().expect("serialize report"));
    } else {
        println!("Set DEV_MUTATE_EXAMPLE_RUN=1 to spawn `cargo mutants --json`");
        println!("and print the resulting JSON report.");
    }
}
