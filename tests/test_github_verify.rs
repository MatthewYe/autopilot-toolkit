#!/usr/bin/env rust-script
//! ```cargo
//! [dependencies]
//! ```
//!
//! Integration tests verifying autopilot-orchestrator SKILL.md defines all
//! required phases, state transitions, and dispatch chains (AC1, AC2, AC4, AC5).
//! These are CI-safe — they only read local files, never call gh CLI or GitHub API.
//!
//! #[test] functions: 13
//!
//! For environment diagnostics (gh installed, authenticated, git remote),
//! run: ./scripts/env-check.rs
//!
//! Run: rust-script --test tests/test_github_verify.rs

use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    println!("Run with: rust-script --test tests/test_github_verify.rs");
}

// ── Helpers ─────────────────────────────────────────────────────────────

/// Find the actual project root — the directory containing skills/autopilot/autopilot-orchestrator/reasonix/SKILL.md.
fn project_root() -> PathBuf {
    let src = Path::new(file!());
    if let (Some(_tests_dir), Some(proj)) = (src.parent(), src.parent().and_then(|p| p.parent())) {
        let candidate = proj.to_path_buf();
        if candidate
            .join("skills/autopilot/autopilot-orchestrator/reasonix/SKILL.md")
            .exists()
        {
            return candidate;
        }
    }
    if let Ok(root) = std::env::var("PROJECT_ROOT") {
        let p = PathBuf::from(&root);
        if p.join("skills/autopilot/autopilot-orchestrator/reasonix/SKILL.md")
            .exists()
        {
            return p;
        }
    }
    panic!("Cannot find project root (orchestrator reasonix SKILL.md not found)");
}

/// Read orchestrator SKILL.md content.
fn orchestrator_skill_path() -> PathBuf {
    project_root().join("skills/autopilot/autopilot-orchestrator/reasonix/SKILL.md")
}

fn read_orchestrator_skill() -> String {
    fs::read_to_string(orchestrator_skill_path()).expect("failed to read orchestrator SKILL.md")
}

/// Count occurrences of a pattern in text.
fn count_matches(text: &str, pattern: &str) -> usize {
    text.matches(pattern).count()
}

/// Check if text contains pattern.
fn contains(text: &str, pattern: &str) -> bool {
    text.contains(pattern)
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ═══════════════════════════════════════════════════════════════════════
    // AC1: No-arg scan finds ready-for-agent issues
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn ac1_skill_defines_scan_command() {
        let skill = read_orchestrator_skill();
        let count = count_matches(&skill, "gh issue list --label \"ready-for-agent\"");
        assert!(
            count >= 1,
            "SKILL.md must define 'gh issue list --label \"ready-for-agent\"' scan command"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // AC2: needs-info issues correctly identified and stopped
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn ac2_skill_defines_stop_behavior() {
        let skill = read_orchestrator_skill();

        let stop_patterns = ["非以上标签", "非以上状态", "回复当前状态并停止"];
        let found = stop_patterns.iter().any(|p| skill.contains(p));

        assert!(
            found,
            "SKILL.md must define stop behavior for non-ready/in-progress issues"
        );
    }

    #[test]
    fn ac2_scan_filters_only_ready_for_agent() {
        let skill = read_orchestrator_skill();
        let filter_count = count_matches(&skill, "label \"ready-for-agent\"");
        assert!(
            filter_count >= 1,
            "SKILL.md scan must filter only ready-for-agent (excludes needs-info)"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // AC4: State transition chain complete
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn ac4_skill_defines_state_transitions() {
        let skill = read_orchestrator_skill();

        let transition_keywords = ["ready-for-agent", "in-progress", "resolved", "needs-info"];

        let mut found_count = 0;
        for kw in &transition_keywords {
            if count_matches(&skill, kw) >= 1 {
                found_count += 1;
            }
        }

        assert!(
            found_count >= 4,
            "SKILL.md must define state transition mappings for all 4 states (found {})",
            found_count
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // AC5: Dispatch chain integrity
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn ac5_skill_defines_scan_phase() {
        let skill = read_orchestrator_skill();
        assert!(
            contains(&skill, "扫描模式"),
            "SKILL.md must define scan phase (扫描模式)"
        );
    }

    #[test]
    fn ac5_skill_defines_status_recognition() {
        let skill = read_orchestrator_skill();
        let has_status_check = skill
            .lines()
            .any(|l| l.contains("检查") && (l.contains("label") || l.contains("Status")));
        assert!(
            has_status_check,
            "SKILL.md must define status/label recognition phase (检查 + label/Status on same line)"
        );
    }

    #[test]
    fn ac5_skill_defines_state_transition_phase() {
        let skill = read_orchestrator_skill();
        let has_label_ops = contains(&skill, "add-label")
            || contains(&skill, "remove-label")
            || contains(&skill, "edit_file") && contains(&skill, "Status");
        assert!(
            has_label_ops,
            "SKILL.md must define state transition phase (add-label/remove-label or edit_file Status)"
        );
    }

    #[test]
    fn ac5_skill_defines_implementer_dispatch() {
        let skill = read_orchestrator_skill();
        let imp_count = count_matches(&skill, "autopilot-implementer");
        assert!(
            imp_count >= 1,
            "SKILL.md must define implementer dispatch (autopilot-implementer)"
        );
    }

    #[test]
    fn ac5_skill_defines_reviewer_dispatch() {
        let skill = read_orchestrator_skill();
        let rev_count = count_matches(&skill, "autopilot-reviewer");
        assert!(
            rev_count >= 1,
            "SKILL.md must define reviewer dispatch (autopilot-reviewer)"
        );
    }

    #[test]
    fn ac5_skill_defines_retry_limit() {
        let skill = read_orchestrator_skill();
        let has_retry = contains(&skill, "最多 3 轮")
            || skill
                .lines()
                .any(|l| l.contains("retry_count") && l.contains("3"));
        assert!(has_retry, "SKILL.md must define retry limit (max 3 rounds)");
    }

    #[test]
    fn ac5_skill_defines_needs_info_fallback() {
        let skill = read_orchestrator_skill();
        let has_fallback = contains(&skill, "转为 needs-info")
            || (contains(&skill, "Status") && contains(&skill, "needs-info"));
        assert!(
            has_fallback,
            "SKILL.md must define needs-info fallback on exhaustion"
        );
    }

    #[test]
    fn ac5_skill_defines_empty_reply_handling() {
        let skill = read_orchestrator_skill();
        let has_empty = contains(&skill, "空回复处理")
            || (contains(&skill, "empty") && contains(&skill, "retry"));
        assert!(has_empty, "SKILL.md must define empty reply handling");
    }

    #[test]
    fn ac5_skill_defines_unparseable_reply_handling() {
        let skill = read_orchestrator_skill();
        let has_unparseable = contains(&skill, "解析容错") && contains(&skill, "不可解析");
        assert!(
            has_unparseable,
            "SKILL.md must define unparseable reply handling (解析容错...不可解析)"
        );
    }
}
