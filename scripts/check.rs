#!/usr/bin/env rust-script
//! ```cargo
//! [dependencies]
//! skill-check = { path = "../crates/skill-check" }
//! shared = { path = "../crates/shared" }
//! ```

use std::process;

fn main() {
    let root = shared::project_root();

    // Read lock file and check skills
    let report = match skill_check::check_skills(&root) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("ERROR: {}", e);
            process::exit(1);
        }
    };

    // Verify the upstream pin alongside the lock hashes. The pin check is
    // skipped only when the project has no upstream snapshot at all.
    let pin = match skill_check::check_upstream_pin(&root) {
        Ok(pin) => pin,
        Err(e) => {
            eprintln!("ERROR: {}", e);
            process::exit(1);
        }
    };
    let pin_checked = pin.is_some();
    let mut results = report.results;
    if let Some(entry) = pin {
        results.push(entry);
    }

    // Print results
    for (name, result) in &results {
        println!("{}", skill_check::format_result(name, result));
    }

    // Update lock file if any FIX applied
    if !report.updated.is_empty() {
        if let Err(e) = skill_check::write_updated_lockfile(&root, &report.updated) {
            eprintln!("ERROR: {}", e);
            process::exit(1);
        }
    }

    // Update vendor lock file if any FIX applied
    if !report.updated_vendor.is_empty() {
        if let Err(e) = skill_check::write_updated_vendor_lockfile(&root, &report.updated_vendor) {
            eprintln!("ERROR: {}", e);
            process::exit(1);
        }
    }

    // Exit status. A checked pin is a real check even in a project without
    // github skill entries, so it forces the fail-closed verdict.
    let code = skill_check::determine_exit_code(&results, report.found_github || pin_checked);
    if code == 0 && !report.found_github && !pin_checked {
        println!("ALL PASS (no github skills found)");
    } else if code == 0 {
        println!();
        println!("ALL PASS");
    }
    process::exit(code);
}
