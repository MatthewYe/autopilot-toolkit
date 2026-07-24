#!/usr/bin/env rust-script
//! ```cargo
//! [dependencies]
//! ```
//!
//! Integration tests verifying autopilot-orchestrator SKILL.md variants define
//! all required phases, state transitions, and dispatch chains.
//! These are CI-safe — they only read local files, never call gh CLI or GitHub API.
//!
//! #[test] functions: 26
//!
//! For environment diagnostics (gh installed, authenticated, git remote),
//! run: ./scripts/env-check.rs
//!
//! Run: rust-script --test tests/test_github_verify.rs

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

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

fn codex_orchestrator_skill_path() -> PathBuf {
    project_root().join("skills/autopilot/autopilot-orchestrator/codex/SKILL.md")
}

fn read_codex_orchestrator_skill() -> String {
    fs::read_to_string(codex_orchestrator_skill_path())
        .expect("failed to read Codex orchestrator SKILL.md")
}

fn codex_audit_skill_path() -> PathBuf {
    project_root().join("skills/autopilot/audit-autopilot/codex/SKILL.md")
}

fn read_codex_audit_skill() -> String {
    fs::read_to_string(codex_audit_skill_path())
        .expect("failed to read Codex audit-autopilot SKILL.md")
}

fn reasonix_skill_path(name: &str) -> PathBuf {
    project_root().join(format!("skills/autopilot/{name}/reasonix/SKILL.md"))
}

fn read_reasonix_skill(name: &str) -> String {
    fs::read_to_string(reasonix_skill_path(name))
        .unwrap_or_else(|e| panic!("failed to read Reasonix skill {name}: {e}"))
}

fn codex_agent_path(name: &str) -> PathBuf {
    project_root().join(format!("skills/autopilot/{name}/codex/agent.toml"))
}

fn read_codex_agent(name: &str) -> String {
    fs::read_to_string(codex_agent_path(name))
        .unwrap_or_else(|e| panic!("failed to read Codex agent {name}: {e}"))
}

fn frontmatter_value(markdown: &str, key: &str) -> String {
    let needle = format!("{key}: ");
    let raw = markdown
        .lines()
        .find_map(|line| line.strip_prefix(&needle))
        .unwrap_or_else(|| panic!("frontmatter key {key} not found"));
    // A YAML double/single-quoted scalar parses to its inner text — compare
    // parsed values, not raw source text.
    raw.strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .or_else(|| raw.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
        .unwrap_or(raw)
        .to_string()
}

fn toml_string_value(toml: &str, key: &str) -> String {
    let needle = format!("{key} = ");
    let mut lines = toml.lines();
    while let Some(line) = lines.next() {
        let Some(value) = line.strip_prefix(&needle) else {
            continue;
        };
        if let Some(body) = value.strip_prefix("\"\"\"") {
            if let Some(end) = body.find("\"\"\"") {
                return body[..end].to_string();
            }
            let mut output = String::new();
            output.push_str(body);
            for continuation in lines.by_ref() {
                if let Some(end) = continuation.find("\"\"\"") {
                    output.push('\n');
                    output.push_str(&continuation[..end]);
                    return output;
                }
                output.push('\n');
                output.push_str(continuation);
            }
            panic!("unterminated multiline TOML string for {key}");
        }
        if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
            return value[1..value.len() - 1].to_string();
        }
        panic!("TOML key {key} is not a string value");
    }
    panic!("TOML key {key} not found");
}

fn assert_toml_parseable(path: &Path) {
    let output = Command::new("python3")
        .arg("-c")
        .arg("import sys, tomllib; tomllib.load(open(sys.argv[1], 'rb'))")
        .arg(path)
        .output()
        .expect("failed to run python3 tomllib");
    assert!(
        output.status.success(),
        "TOML should parse with tomllib: {}\n{}",
        path.display(),
        String::from_utf8_lossy(&output.stderr)
    );
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

    #[test]
    fn codex_variant_exists_with_valid_frontmatter() {
        let skill = read_codex_orchestrator_skill();
        assert!(
            skill.starts_with("---\nname: autopilot-orchestrator\n"),
            "Codex variant must start with valid frontmatter"
        );
        assert!(
            skill.contains("description:"),
            "Codex variant frontmatter must include description"
        );
    }

    #[test]
    fn codex_variant_uses_spawn_agent_dispatch() {
        let skill = read_codex_orchestrator_skill();
        assert!(
            contains(&skill, "spawn agent autopilot-implementer"),
            "Codex variant must dispatch implementer via spawn agent"
        );
        assert!(
            contains(&skill, "spawn agent autopilot-reviewer"),
            "Codex variant must dispatch reviewer via spawn agent"
        );
    }

    #[test]
    fn codex_variant_excludes_reasonix_dispatch_terms() {
        let skill = read_codex_orchestrator_skill();
        for forbidden in ["run_skill", "complete_step", "runAs"] {
            assert!(
                !contains(&skill, forbidden),
                "Codex variant must not contain Reasonix-specific term {forbidden}"
            );
        }
    }

    #[test]
    fn codex_variant_preserves_core_workflow_sections() {
        let skill = read_codex_orchestrator_skill();
        for required in [
            "Issue 来源识别",
            "PRD 检测与跳过",
            "扫描模式",
            "Phase 1: 调度循环",
            "最多 3 轮",
            "交叉 Issue Suggestion 匹配",
            "Phase 2: 全局 Meta-Review",
            "FINAL_ACCEPTANCE_REPORT",
        ] {
            assert!(
                contains(&skill, required),
                "Codex variant must preserve workflow section: {required}"
            );
        }
    }

    #[test]
    fn codex_orchestrator_references_are_loadable_from_variant_dir() {
        let skill = read_codex_orchestrator_skill();
        assert!(
            !contains(&skill, "skills/autopilot/autopilot-orchestrator/references"),
            "Codex orchestrator must not reference repo-relative orchestrator reference paths"
        );

        for reference in [
            "references/suggestion-matching.md",
            "references/acceptance-report.md",
        ] {
            assert!(
                contains(&skill, reference),
                "Codex orchestrator must reference {reference}"
            );
            assert!(
                project_root()
                    .join("skills/autopilot/autopilot-orchestrator/codex")
                    .join(reference)
                    .is_file(),
                "Codex orchestrator reference must be loadable from variant dir: {reference}"
            );
        }
    }

    #[test]
    fn codex_audit_variant_exists_with_valid_frontmatter_and_references() {
        let skill = read_codex_audit_skill();
        let reasonix = read_reasonix_skill("audit-autopilot");
        assert!(
            skill.starts_with("---\nname: audit-autopilot\n"),
            "Codex audit variant must start with valid frontmatter"
        );
        assert_eq!(
            frontmatter_value(&skill, "description"),
            frontmatter_value(&reasonix, "description")
        );

        let references = project_root().join("skills/autopilot/audit-autopilot/codex/references");
        assert!(
            references.join("questions.md").is_file(),
            "Codex audit variant must include references/questions.md"
        );
        assert!(
            references.join("report-template.md").is_file(),
            "Codex audit variant must include references/report-template.md"
        );
    }

    #[test]
    fn codex_audit_variant_preserves_reasonix_audit_methodology() {
        let skill = read_codex_audit_skill();
        for required in [
            "three layers of fidelity",
            "9 analysis questions",
            "Layer 1 (Fidelity)",
            "Layer 2 (Errors)",
            "Layer 3 (Friction & Drift)",
            "Phase 2",
            "fidelity percentage",
            "evidence anchor",
        ] {
            assert!(
                contains(&skill, required),
                "Codex audit variant must preserve audit method term: {required}"
            );
        }

        let questions = fs::read_to_string(
            project_root().join("skills/autopilot/audit-autopilot/codex/references/questions.md"),
        )
        .expect("failed to read Codex audit questions");
        for question in [
            "Q1: Intent Translation",
            "Q2: AC Coverage",
            "Q3: Report Credibility",
            "Q4: Unfixed Criticals",
            "Q5: Verdict Consistency",
            "Q6: Suggestion Chain Integrity",
            "Q7: Retry Efficacy",
            "Q8: Scope Creep",
            "Q9: TDD Discipline",
        ] {
            assert!(
                contains(&questions, question),
                "Codex audit references must preserve {question}"
            );
        }
    }

    #[test]
    fn codex_audit_variant_uses_codex_session_placeholders() {
        let skill = read_codex_audit_skill();
        assert!(
            count_matches(&skill, "TODO: codex session export — TBD") >= 2,
            "Codex audit variant must mark unresolved Codex session mechanism with TODO placeholders"
        );
        for forbidden in [
            "reasonix session export",
            "reasonix session list",
            "list_sessions",
            "read_session",
        ] {
            assert!(
                !contains(&skill, forbidden),
                "Codex audit variant must not reference Reasonix session tool {forbidden}"
            );
        }
    }

    #[test]
    fn codex_audit_variant_excludes_reasonix_workflow_terms() {
        let skill = read_codex_audit_skill();
        for forbidden in ["run_skill", "complete_step", "runAs"] {
            assert!(
                !contains(&skill, forbidden),
                "Codex audit variant must not contain Reasonix-specific term {forbidden}"
            );
        }
    }

    #[test]
    fn codex_agent_tomls_exist_and_parse() {
        for name in ["autopilot-implementer", "autopilot-reviewer"] {
            let path = codex_agent_path(name);
            assert!(
                path.is_file(),
                "Codex custom agent TOML must exist at {}",
                path.display()
            );
            assert_toml_parseable(&path);
        }
    }

    #[test]
    fn codex_implementer_agent_matches_contract() {
        let agent = read_codex_agent("autopilot-implementer");
        let instructions = toml_string_value(&agent, "developer_instructions");
        let reasonix = read_reasonix_skill("autopilot-implementer");

        assert_eq!(toml_string_value(&agent, "name"), "autopilot-implementer");
        assert_eq!(
            toml_string_value(&agent, "description"),
            frontmatter_value(&reasonix, "description")
        );
        assert!(
            ["gpt-5.4", "gpt-5.5"].contains(&toml_string_value(&agent, "model").as_str()),
            "implementer model must be gpt-5.4 or gpt-5.5"
        );
        assert_eq!(toml_string_value(&agent, "sandbox_mode"), "workspace-write");
        for required in [
            "~/.agents/principles/karpathy.md",
            "Think Before Coding",
            "Principles 2, 3, and 4",
        ] {
            assert!(
                contains(&instructions, required),
                "implementer developer_instructions must consume shared principles: {required}"
            );
            assert!(
                contains(&reasonix, required),
                "Reasonix implementer must consume shared principles: {required}"
            );
        }

        for required in [
            "Contract Reading",
            "red-green-refactor",
            "Diagnose Flow",
            "Self-review",
            "IMPLEMENTER_REPORT",
        ] {
            assert!(
                contains(&instructions, required),
                "implementer developer_instructions must cover {required}"
            );
        }
        assert!(
            contains(&instructions, "[resolved|rejected|deferred] 来源 <source_issue> round <N>:"),
            "implementer SUGGESTION_RESOLUTIONS format must use the orchestrator parser's 来源 token"
        );
        assert!(
            !contains(
                &instructions,
                "[resolved|rejected|deferred] source <issue-slug> round <N>:"
            ),
            "implementer must not emit the old source-token SUGGESTION_RESOLUTIONS format"
        );
    }

    #[test]
    fn codex_reviewer_agent_matches_contract() {
        let agent = read_codex_agent("autopilot-reviewer");
        let instructions = toml_string_value(&agent, "developer_instructions");
        let reasonix = read_reasonix_skill("autopilot-reviewer");

        assert_eq!(toml_string_value(&agent, "name"), "autopilot-reviewer");
        assert_eq!(
            toml_string_value(&agent, "description"),
            frontmatter_value(&reasonix, "description")
        );
        assert!(
            ["gpt-5.4", "gpt-5.5"].contains(&toml_string_value(&agent, "model").as_str()),
            "reviewer model must be gpt-5.4 or gpt-5.5"
        );
        assert_eq!(toml_string_value(&agent, "sandbox_mode"), "read-only");
        for required in [
            "~/.agents/principles/karpathy.md",
            "Think Before Judging",
            "Principles 2 and 4",
        ] {
            assert!(
                contains(&instructions, required),
                "reviewer developer_instructions must consume shared principles: {required}"
            );
        }
        for required in [
            "~/.agents/principles/karpathy.md",
            "Think Before Judging",
            "Principles 2, 4",
        ] {
            assert!(
                contains(&reasonix, required),
                "Reasonix reviewer must consume shared principles: {required}"
            );
        }

        for required in [
            "Four-Axis Review",
            "Behavior alignment",
            "TDD discipline",
            "code quality",
            "plan fidelity",
            "Critical",
            "Important",
            "Suggestion",
            "REVIEWER_REPORT",
        ] {
            assert!(
                contains(&instructions, required),
                "reviewer developer_instructions must cover {required}"
            );
        }
    }

    #[test]
    fn codex_agents_exclude_reasonix_terms_and_external_skill_dependencies() {
        for name in ["autopilot-implementer", "autopilot-reviewer"] {
            let agent = read_codex_agent(name);
            for forbidden in ["run_skill", "complete_step", "runAs", "skill(name"] {
                assert!(
                    !contains(&agent, forbidden),
                    "Codex agent {name} must not contain forbidden term {forbidden}"
                );
            }
            for forbidden_field in ["\nmode =", "\nhidden ="] {
                assert!(
                    !contains(&agent, forbidden_field),
                    "Codex agent {name} must not contain unsupported top-level field {forbidden_field}"
                );
            }
        }
    }
}
