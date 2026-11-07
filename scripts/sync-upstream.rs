#!/usr/bin/env rust-script
//! ```cargo
//! [dependencies]
//! chrono = "0.4"
//! upstream-sync = { path = "../crates/upstream-sync" }
//! shared = { path = "../crates/shared" }
//! ```
//!
//! Sync the vendored upstream (skills/upstream/) to a pinned ref of
//! mattpocock/skills. Replaces the entire upstream tree, recomputes all
//! skillFolderHash values, drops orphan entries, and adds new skills.
//! All logic lives in the `upstream-sync` crate; this is a thin CLI.
//!
//! Upstream keeps beta skills in `skills/in-progress/`, outside the plugin,
//! with no stability guarantee. The crate's IN_PROGRESS_ALLOWLIST names the
//! ones this toolkit ships anyway; they are tracked in `.skill-lock.json`
//! like any other upstream skill. A ref that does not contain an
//! allowlisted skill fails the sync before the tree or the lock is touched.
//!
//! Usage:
//!   rust-script scripts/sync-upstream.rs [--force] <REF>
//!
//! REF is required: a release tag, branch, or commit SHA. There is no
//! default, because a ref that predates an allowlisted skill would orphan it.
//!
//! --force proceeds even when the git worktree is dirty or `skills/upstream/`
//! holds untracked files (which the sync would overwrite or delete). The
//! overwritten paths are printed. Either way, the sync snapshots the
//! pre-sync state first and restores it if any post-sync gate fails.
//!
//! Environment:
//!   AUTOPILOT_UPSTREAM_REPO — override the upstream repo URL (tests point
//!   this at a local fixture repo; default: the real mattpocock/skills).

use std::env;
use std::process;

fn usage() -> ! {
    eprintln!("Usage: rust-script scripts/sync-upstream.rs [--force] <REF>");
    eprintln!();
    eprintln!("REF is a required mattpocock/skills release tag, branch, or commit SHA.");
    eprintln!("There is no default: a ref that predates an allowlisted skill would orphan it.");
    eprintln!();
    eprintln!("  --force  proceed despite local changes in the worktree; the paths the");
    eprintln!("           sync overwrites are printed. A failed sync still rolls back.");
    eprintln!();
    eprintln!("Environment:");
    eprintln!(
        "  AUTOPILOT_UPSTREAM_REPO  override the upstream repo URL (default: {})",
        upstream_sync::DEFAULT_UPSTREAM_REPO
    );
    process::exit(1);
}

fn main() {
    let mut force = false;
    let mut ref_arg: Option<String> = None;
    for arg in env::args().skip(1) {
        match arg.as_str() {
            "--force" => force = true,
            unknown if unknown.starts_with("--") => {
                eprintln!("Unknown flag: {}", unknown);
                usage();
            }
            other => {
                if ref_arg.is_some() {
                    eprintln!("Unexpected extra argument: {}", other);
                    usage();
                }
                ref_arg = Some(other.to_string());
            }
        }
    }
    let tag = match ref_arg {
        Some(tag) if !tag.trim().is_empty() => tag,
        _ => usage(),
    };
    let options = upstream_sync::SyncOptions { force };

    let project_root = shared::project_root();
    let repo_url = env::var("AUTOPILOT_UPSTREAM_REPO")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| upstream_sync::DEFAULT_UPSTREAM_REPO.to_string());
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

    println!("=== Sync upstream to {} ===", tag);
    println!("Project root: {}", project_root.display());
    println!("\nFetching {} ({}):", repo_url, tag);

    match upstream_sync::sync_upstream_with_options(&project_root, &tag, &repo_url, &now, &options)
    {
        Ok(outcome) => {
            println!("  resolved to commit {}", outcome.resolved_commit);

            if !outcome.forced_overwrites.is_empty() {
                println!("\nwarning: --force overwrote local changes:");
                for path in &outcome.forced_overwrites {
                    println!("  ! {}", path);
                }
            }

            println!("\nDiscovered {} skills:", outcome.discovered.len());
            for skill in &outcome.discovered {
                println!("    {:.<36} {}", skill.name, skill.skill_path);
                println!("      {}{}", " ".repeat(36), skill.skill_folder_hash);
            }
            for note in &outcome.allowlist_notes {
                println!("  {}", note);
            }

            if !outcome.removed.is_empty() {
                println!("\nOrphan skills (removed from lock file):");
                for name in &outcome.removed {
                    println!("  - {}", name);
                }
            }
            if !outcome.added.is_empty() {
                println!("\nNew skills (added to lock file):");
                for name in &outcome.added {
                    println!("  + {}", name);
                }
            }
            if !outcome.updated.is_empty() {
                println!("\nUpdated skills (hash changed):");
                for name in &outcome.updated {
                    println!("  ~ {}", name);
                }
            }

            println!("\nReplaced {}", outcome.upstream_dir.display());
            println!("  written {}", outcome.lock_path.display());

            println!("\n=== Post-sync check ===");
            for line in &outcome.check_lines {
                println!("{}", line);
            }
            println!("\nSync complete — all checks PASS.");
        }
        Err(e) => {
            eprintln!("\nERROR: {}", e);
            process::exit(1);
        }
    }
}
