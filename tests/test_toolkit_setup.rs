#!/usr/bin/env rust-script
//! ```cargo
//! [dependencies]
//! skill-index = { path = "../crates/skill-index" }
//! ```
//!
//! Integration tests for the toolkit-setup flow.
//!
//! The Expected set is owned by `crates/skill-index` (ADR 0044); this suite
//! consumes the public enumerator instead of deriving a parallel copy.
//!
//! #[test] functions: 1
//!
//! Run: rust-script --test tests/test_toolkit_setup.rs

use std::path::{Path, PathBuf};

fn main() {
    println!("Run with: rust-script --test tests/test_toolkit_setup.rs");
}

/// Find the actual project root — the directory containing deploy.rs.
fn project_root() -> PathBuf {
    let src = Path::new(file!());
    if let (Some(_tests_dir), Some(proj)) = (src.parent(), src.parent().and_then(|p| p.parent())) {
        let candidate = proj.to_path_buf();
        if candidate.join("deploy.rs").exists() {
            return candidate;
        }
    }
    if let Ok(root) = std::env::var("PROJECT_ROOT") {
        let p = PathBuf::from(&root);
        if p.join("deploy.rs").exists() {
            return p;
        }
    }
    panic!("Cannot find project root (deploy.rs not found)");
}

/// Position of a Skill source in the deterministic Expected-set order.
fn source_rank(source: &str) -> u8 {
    match source {
        "autopilot" => 0,
        "vendor" => 1,
        "upstream" => 2,
        other => panic!("unknown Skill source {other:?}"),
    }
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// The repo's own source tree enumerates to a complete, deterministically
    /// ordered Expected set — no failed entries, no duplicate names.
    #[test]
    fn expected_set_resolves_from_the_repo_tree() {
        let entries = skill_index::discover_skills(&project_root())
            .expect("enumerate the Expected set from the repo tree");

        assert!(
            !entries.is_empty(),
            "the repo tree must own at least one skill"
        );

        let failed: Vec<String> = entries
            .iter()
            .filter_map(|entry| match &entry.resolution {
                skill_index::ResolutionStatus::Missing { reason } => {
                    Some(format!("{}: {reason}", entry.name))
                }
                skill_index::ResolutionStatus::Resolved => None,
            })
            .collect();
        assert!(
            failed.is_empty(),
            "every entry must resolve in the repo tree:\n{failed:#?}"
        );

        // Deterministic order: autopilot, then vendor, then upstream,
        // name-sorted within each group.
        let order: Vec<(u8, &str)> = entries
            .iter()
            .map(|entry| (source_rank(&entry.source), entry.name.as_str()))
            .collect();
        let mut sorted = order.clone();
        sorted.sort_unstable();
        assert_eq!(
            order, sorted,
            "entries must be in deterministic source-then-name order"
        );

        let mut names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
        names.sort_unstable();
        let mut deduped = names.clone();
        deduped.dedup();
        assert_eq!(names, deduped, "duplicate skill names in the Expected set");
    }
}
