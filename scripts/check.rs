#!/usr/bin/env rust-script
//! ```cargo
//! [dependencies]
//! skill-check = { path = "../crates/skill-check" }
//! ```

use std::path::{Path, PathBuf};
use std::process;

fn project_root() -> PathBuf {
    let script_dir = Path::new(file!())
        .parent()
        .expect("scripts/check.rs has a parent directory");
    script_dir
        .parent()
        .expect("scripts/ directory has a parent (project root)")
        .to_path_buf()
}

fn main() {
    let root = std::env::var("PROJECT_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| project_root());

    // Read lock file and check skills
    let report = match skill_check::check_skills(&root) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("ERROR: {}", e);
            process::exit(1);
        }
    };

    // Print results
    for (name, result) in &report.results {
        println!("{}", skill_check::format_result(name, result));
    }

    // Update lock file if any FIX applied
    if !report.updated.is_empty() {
        if let Err(e) = skill_check::write_updated_lockfile(&root, &report.updated) {
            eprintln!("ERROR: {}", e);
            process::exit(1);
        }
    }

    // Exit status
    let code = skill_check::determine_exit_code(&report.results, report.found_github);
    if code == 0 && !report.found_github {
        println!("ALL PASS (no github skills found)");
    } else if code == 0 {
        println!();
        println!("ALL PASS");
    }
    process::exit(code);
}
