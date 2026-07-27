#!/usr/bin/env rust-script
//! ```cargo
//! ```
//!
//! Static contract tests for the local Distill -> Autopilot handoff.

use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    println!("Run with: rust-script --test tests/test_distill_local_handoff.rs");
}

fn project_root() -> PathBuf {
    let source = Path::new(file!());
    source
        .parent()
        .and_then(Path::parent)
        .expect("test file should live under the project root")
        .to_path_buf()
}

fn read_skill(relative_path: &str) -> String {
    fs::read_to_string(project_root().join(relative_path))
        .unwrap_or_else(|error| panic!("cannot read {relative_path}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DISTILL_SKILLS: &[&str] = &[
        "skills/autopilot/autopilot-distill/SKILL.md",
        "skills/autopilot/autopilot-distill/codex/SKILL.md",
        "skills/autopilot/autopilot-distill/kimi/SKILL.md",
        "skills/autopilot/autopilot-distill/reasonix/SKILL.md",
    ];

    const ORCHESTRATOR_SKILLS: &[&str] = &[
        "skills/autopilot/autopilot-orchestrator/SKILL.md",
        "skills/autopilot/autopilot-orchestrator/codex/SKILL.md",
        "skills/autopilot/autopilot-orchestrator/kimi/SKILL.md",
        "skills/autopilot/autopilot-orchestrator/reasonix/SKILL.md",
    ];

    #[test]
    fn local_distill_issues_are_published_once_with_agent_ready_triage() {
        for path in DISTILL_SKILLS {
            let skill = read_skill(path);
            assert!(
                skill.contains("LOCAL_ISSUE_HANDOFF_CONTRACT"),
                "{path} must define the local issue handoff contract"
            );
            assert!(
                skill.contains("ready-for-agent"),
                "{path} must require the AFK-agent-ready triage state"
            );
            assert!(
                skill.contains("exactly once"),
                "{path} must assign local issue publication to exactly one owner"
            );
        }
    }

    #[test]
    fn orchestrator_deduplicates_identical_local_distill_issues() {
        for path in ORCHESTRATOR_SKILLS {
            let skill = read_skill(path);
            assert!(
                skill.contains("LOCAL_ISSUE_DEDUP_CONTRACT"),
                "{path} must define the local issue deduplication contract"
            );
            assert!(
                skill.contains(".scratch/distill-tracer/issues/"),
                "{path} must name the Distill local publication directory"
            );
            assert!(
                skill.contains("identical"),
                "{path} must deduplicate only identical local issue content"
            );
        }
    }
}
