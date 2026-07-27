#!/usr/bin/env rust-script
//! ```cargo
//! [dependencies]
//! validation-runner = { path = "../crates/validation-runner" }
//! shared = { path = "../crates/shared" }
//! ```

use std::fs;
use std::process;

fn main() {
    let root = shared::project_root();
    let report = validation_runner::run_validation(&root).expect("validation should succeed");

    // Print report to stdout (println! mirrors bash's `echo "$report"`)
    println!("{}", report.report);

    // Save to validation/report.txt
    let report_path = root.join("validation/report.txt");
    if let Err(e) = fs::write(&report_path, &report.report) {
        eprintln!(
            "Warning: failed to write report to {}: {}",
            report_path.display(),
            e
        );
    }

    println!();
    println!("Report saved to: validation/report.txt");

    // Exit 1 if any FAIL, else 0
    if report.has_failures {
        process::exit(1);
    }
}
