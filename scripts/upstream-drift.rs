#!/usr/bin/env rust-script
//! ```cargo
//! [dependencies]
//! upstream-sync = { path = "../crates/upstream-sync" }
//! shared = { path = "../crates/shared" }
//! serde_json = "1"
//! ```
//!
//! Compare the committed `.upstream-pin.json` with the newest release tag of
//! the upstream repository and describe the gap.
//!
//! This is the thin CLI behind `.github/workflows/upstream-drift.yml`; the
//! comparison itself lives in `upstream_sync::drift` so it is unit-tested.
//! The command is read-only: it reads the pin, asks the upstream repository
//! for its tags, and prints a report (optionally as JSON for the workflow to
//! turn into a tracking issue). It never syncs, commits, or opens PRs.
//!
//! Usage:
//!   rust-script scripts/upstream-drift.rs [--repo <URL>] [--tag <TAG>] [--json]
//!
//! `--tag` pretends `TAG` is the newest upstream release without touching
//! the network (`workflow_dispatch`'s `simulate_tag` input). `--json` prints
//! one object: drifted, pinned, latest, title, body.

use std::path::Path;
use std::process;

use upstream_sync::drift::{self, DriftCheck};
use upstream_sync::DEFAULT_UPSTREAM_REPO;

fn print_usage() {
    eprintln!("Usage: rust-script scripts/upstream-drift.rs [--repo <URL>] [--tag <TAG>] [--json]");
    eprintln!();
    eprintln!("Compares .upstream-pin.json with the newest upstream release tag.");
    eprintln!("  --repo <URL>  Upstream git URL (default: {DEFAULT_UPSTREAM_REPO})");
    eprintln!("  --tag <TAG>   Pretend TAG is the newest release (skips the network)");
    eprintln!("  --json        Print a machine-readable report (used by the workflow)");
    eprintln!();
    eprintln!("Exit status: 0 when the check ran (drift or not), 1 on error.");
}

fn run(
    project_root: &Path,
    repo_url: &str,
    simulated_tag: Option<String>,
    json: bool,
) -> Result<(), String> {
    let pin = drift::load_pin(project_root)?;
    let tags = match simulated_tag {
        Some(tag) => vec![tag],
        None => drift::fetch_release_tags(repo_url)?,
    };
    let check = drift::evaluate(pin, &tags)?;

    if json {
        let payload = serde_json::json!({
            "drifted": check.drifted,
            "pinned": {
                "ref": check.pinned.ref_name,
                "version": check.pinned.version,
                "resolvedCommit": check.pinned.resolved_commit,
                "syncedAt": check.pinned.synced_at,
            },
            "latest": {
                "tag": check.latest.tag,
                "version": check.latest.version.to_string(),
            },
            "title": drift::issue_title(&check),
            "body": drift::issue_body(&check),
        });
        println!("{payload}");
    } else {
        render_human(&check);
    }
    Ok(())
}

fn render_human(check: &DriftCheck) {
    if check.drifted {
        println!(
            "DRIFT: pinned {} ({}) is behind upstream release {}",
            check.pinned.ref_name, check.pinned.version, check.latest.tag
        );
        println!("  title: {}", drift::issue_title(check));
        println!();
        print!("{}", drift::issue_body(check));
    } else {
        println!(
            "No drift: pinned {} is at {} and the newest upstream release is {}.",
            check.pinned.ref_name, check.pinned.version, check.latest.tag
        );
    }
}

fn main() {
    let mut repo_url: Option<String> = None;
    let mut simulated_tag: Option<String> = None;
    let mut json = false;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json" => json = true,
            "--repo" => match args.next() {
                Some(value) => repo_url = Some(value),
                None => {
                    eprintln!("Missing value for --repo");
                    print_usage();
                    process::exit(1);
                }
            },
            "--tag" => match args.next() {
                Some(value) => simulated_tag = Some(value),
                None => {
                    eprintln!("Missing value for --tag");
                    print_usage();
                    process::exit(1);
                }
            },
            "--help" | "-h" => {
                print_usage();
                process::exit(0);
            }
            other => {
                eprintln!("Unknown argument: {other}");
                print_usage();
                process::exit(1);
            }
        }
    }

    let repo_url = repo_url
        .or_else(|| std::env::var("AUTOPILOT_UPSTREAM_REPO").ok())
        .unwrap_or_else(|| DEFAULT_UPSTREAM_REPO.to_string());

    if let Err(err) = run(&shared::project_root(), &repo_url, simulated_tag, json) {
        eprintln!("ERROR: {err}");
        process::exit(1);
    }
}
