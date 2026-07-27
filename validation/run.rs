#!/usr/bin/env rust-script
//! ```cargo
//! [dependencies]
//! validation-runner = { path = "../crates/validation-runner" }
//! ```

use std::fs;
use std::path::{Path, PathBuf};
use std::process;

fn project_root() -> PathBuf {
    let script_dir = Path::new(file!())
        .parent()
        .expect("validation/run.rs has a parent directory");
    script_dir
        .parent()
        .expect("validation/ directory has a parent (project root)")
        .to_path_buf()
}

fn main() {
    let root = project_root();
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
