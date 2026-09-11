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

// ── Tests ────────────────────────────────────────────────────────────────

/// Position of a Skill source in the deterministic Expected-set order.
fn source_rank(source: &str) -> u8 {
    match source {
        "autopilot" => 0,
        "vendor" => 1,
        "upstream" => 2,
        other => panic!("unknown Skill source {other:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The repo's own source tree enumerates to a complete, deterministically
    /// ordered Expected set: every entry resolves every Skill file it owns,
    /// and no entry is a failed entry (ADR-0045).
    #[test]
    fn repo_tree_resolves_every_skill_file() {
        let entries = skill_index::discover_skills(&project_root())
            .expect("enumerate the Expected set from the repo tree");

        assert!(
            !entries.is_empty(),
            "the repo tree must own at least one skill"
        );

        let failed: Vec<String> = entries
            .iter()
            .filter(|entry| entry.is_failed())
            .map(|entry| {
                let missing: Vec<String> = entry
                    .skill_files
                    .iter()
                    .filter_map(|file| match &file.resolution {
                        skill_index::ResolutionStatus::Missing { reason } => Some(format!(
                            "{} ({:?}): {reason}",
                            file.path.display(),
                            file.variant
                        )),
                        skill_index::ResolutionStatus::Resolved => None,
                    })
                    .collect();
                format!("{}: {}", entry.name, missing.join("; "))
            })
            .collect();
        assert!(
            failed.is_empty(),
            "every entry must resolve all its Skill files in the repo tree:\n{failed:#?}"
        );

        // Entry order: autopilot, then vendor, then upstream; name-sorted
        // within each group.
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

        // Skill-file order: root fallback first, then variants in runtime order.
        let runtime_rank = |variant: &str| {
            skill_index::RUNTIME_VARIANTS
                .iter()
                .position(|known| *known == variant)
                .unwrap_or_else(|| panic!("unknown runtime variant {variant}"))
        };
        for entry in &entries {
            assert_eq!(
                entry.skill_files.first().and_then(|f| f.variant.as_deref()),
                None,
                "entry '{}' must list its root fallback skill file first",
                entry.name
            );
            let ranks: Vec<usize> = entry
                .skill_files
                .iter()
                .skip(1)
                .filter_map(|file| file.variant.as_deref())
                .map(runtime_rank)
                .collect();
            let mut sorted = ranks.clone();
            sorted.sort_unstable();
            assert_eq!(
                ranks, sorted,
                "entry '{}' must list variants in runtime order",
                entry.name
            );
        }

        let mut names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
        names.sort_unstable();
        let mut deduped = names.clone();
        deduped.dedup();
        assert_eq!(names, deduped, "duplicate skill names in the Expected set");
    }
}
