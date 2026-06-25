#!/usr/bin/env rust-script
//! ```cargo
//! [dependencies]
//! validation = { path = "../crates/validation" }
//! serde_json = { version = "1", features = ["preserve_order"] }
//! chrono = "0.4"
//! ```

use std::collections::HashMap;
use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

use chrono::Utc;
use validation::{parse_frontmatter, validate_skill, ValidationResult};

/// Write a line into a `String` via `fmt::Write`.  Allocation into a
/// `String` is infallible, so we discard the `Result` to keep the
/// report-building code noise-free.
macro_rules! wln {
    ($dst:expr) => {
        let _ = writeln!($dst);
    };
    ($dst:expr, $($arg:tt)*) => {
        let _ = writeln!($dst, $($arg)*);
    };
}

// ── Types ──────────────────────────────────────────────────────────────────

struct Skill {
    name: String,
    relative_path: String,
    source: String, // "upstream" | "autopilot"
}

struct SkillResult {
    result: ValidationResult,
    /// Frontmatter fields for autopilot display (runAs / allowed-tools)
    frontmatter: Option<HashMap<String, String>>,
}

// ── Helper: project root ───────────────────────────────────────────────────
///
/// Derive the project root from the script's own location, matching the bash
/// `run.sh` behaviour (`SCRIPT_DIR` → `PROJECT_ROOT`).  `file!()` is the
/// absolute path of this source file at compile time, so the call works
/// regardless of which directory the user runs the script from.
fn project_root() -> PathBuf {
    let script_dir = Path::new(file!())
        .parent()
        .expect("validation/run.rs has a parent directory");
    script_dir
        .parent()
        .expect("validation/ directory has a parent (project root)")
        .to_path_buf()
}

// ── Skill discovery ────────────────────────────────────────────────────────

fn discover_skills(root: &Path) -> Vec<Skill> {
    let mut skills: Vec<Skill> = Vec::new();
    discover_upstream(root, &mut skills);
    discover_autopilot(root, &mut skills);
    skills
}

fn discover_upstream(root: &Path, skills: &mut Vec<Skill>) {
    let lock_path = root.join(".skill-lock.json");
    if !lock_path.exists() {
        return;
    }
    let content = match fs::read_to_string(&lock_path) {
        Ok(c) => c,
        Err(_) => return,
    };
    let parsed: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return,
    };
    let skills_map = match parsed.get("skills").and_then(|s| s.as_object()) {
        Some(m) => m,
        None => return,
    };
    // serde_json with preserve_order iterates in insertion order
    for (name, info) in skills_map {
        let skill_path = info.get("skillPath").and_then(|v| v.as_str()).unwrap_or("");
        if skill_path.is_empty() {
            continue;
        }
        let relative_path = format!("skills/upstream/{}", skill_path);
        skills.push(Skill {
            name: name.clone(),
            relative_path,
            source: "upstream".to_string(),
        });
    }
}

fn discover_autopilot(root: &Path, skills: &mut Vec<Skill>) {
    let autopilot_dir = root.join("skills/autopilot");
    if !autopilot_dir.is_dir() {
        return;
    }
    let mut entries: Vec<String> = Vec::new();
    if let Ok(read_dir) = fs::read_dir(&autopilot_dir) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let skill_md = path.join("SKILL.md");
                if skill_md.is_file() {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        entries.push(name.to_string());
                    }
                }
            }
        }
    }
    entries.sort(); // alphabetical, matches bash glob order
    for name in entries {
        let relative_path = format!("skills/autopilot/{}/SKILL.md", name);
        skills.push(Skill {
            name,
            relative_path,
            source: "autopilot".to_string(),
        });
    }
}

// ── Batch validation ───────────────────────────────────────────────────────

fn validate_all(root: &Path, skills: &[Skill]) -> Vec<SkillResult> {
    skills
        .iter()
        .map(|skill| {
            let full_path = root.join(&skill.relative_path);
            let content = match fs::read_to_string(&full_path) {
                Ok(c) => c,
                Err(_) => {
                    return SkillResult {
                        result: ValidationResult {
                            passed: false,
                            issues: vec![format!("File not found: {}", full_path.display())],
                        },
                        frontmatter: None,
                    };
                }
            };
            let validation_result = validate_skill(&content);
            let frontmatter = if skill.source == "autopilot" {
                parse_frontmatter(&content).ok()
            } else {
                None
            };
            SkillResult {
                result: validation_result,
                frontmatter,
            }
        })
        .collect()
}

// ── Report generation ──────────────────────────────────────────────────────

fn generate_report(skills: &[Skill], results: &[SkillResult]) -> String {
    let sep = "=".repeat(70);
    let date_str = Utc::now().format("%Y-%m-%dT%H:%M:%S.000Z").to_string();

    let total = skills.len();
    let pass_count = results.iter().filter(|r| r.result.passed).count();
    let fail_count = total - pass_count;

    // Source-level counts
    let (upstream_total, upstream_pass, upstream_fail) =
        count_by_source(skills, results, "upstream");
    let (autopilot_total, autopilot_pass, autopilot_fail) =
        count_by_source(skills, results, "autopilot");

    let mut report = String::new();

    // ── Header ──
    wln!(report, "{}", sep);
    wln!(
        report,
        "FRONTMATTER VALIDATION REPORT — reasonix compatibility"
    );
    wln!(report, "{}", sep);
    wln!(report, "Date: {}", date_str);
    wln!(
        report,
        "Total skills validated: {} | Passed: {} | Failed: {}",
        total,
        pass_count,
        fail_count
    );
    wln!(report);

    // ── Upstream section ──
    wln!(report, "--- Upstream Skills ({}) ---", upstream_total);
    wln!(
        report,
        "Passed: {} / Failed: {}",
        upstream_pass,
        upstream_fail
    );
    wln!(report);
    write_skill_entries(&mut report, skills, results, "upstream", true);

    // ── Autopilot section ──
    wln!(report, "--- Autopilot Skills ({}) ---", autopilot_total);
    wln!(
        report,
        "Passed: {} / Failed: {}",
        autopilot_pass,
        autopilot_fail
    );
    wln!(report);
    write_skill_entries(&mut report, skills, results, "autopilot", false);

    // ── Global checks ──
    wln!(report, "{}", sep);
    wln!(report, "GLOBAL CHECKS");
    wln!(report, "{}", sep);
    wln!(report);

    // Check 1: 0 opencode-specific fields
    let oc_count: usize = results
        .iter()
        .map(|r| {
            r.result
                .issues
                .iter()
                .filter(|issue| issue.starts_with("OpenCode-specific field present:"))
                .count()
        })
        .sum();
    wln!(
        report,
        "Check: 0 opencode-specific fields across all {} skills",
        total
    );
    if oc_count == 0 {
        wln!(report, "Result: ✓ PASS");
    } else {
        wln!(
            report,
            "Result: ✗ FAIL — {} opencode field(s) found",
            oc_count
        );
    }
    wln!(report);

    // Check 2: all subagent skills have allowed-tools
    let sub_missing = find_subagent_missing_allowed_tools(skills);
    wln!(
        report,
        "Check: All subagent skills have allowed-tools defined"
    );
    if sub_missing.is_empty() {
        wln!(report, "Result: ✓ PASS");
    } else {
        wln!(
            report,
            "Result: ✗ FAIL — missing: {}",
            sub_missing.join(" ")
        );
    }
    wln!(report);

    // ── Overall result ──
    wln!(report, "{}", sep);
    wln!(report, "OVERALL RESULT");
    wln!(report, "{}", sep);
    if fail_count == 0 {
        wln!(report, "All skills PASS validation.");
    } else {
        wln!(
            report,
            "{} skill(s) FAIL validation. See individual entries above for issue details.",
            fail_count
        );
    }

    report
}

/// Returns (total, pass, fail) for a given source.
fn count_by_source(
    skills: &[Skill],
    results: &[SkillResult],
    source: &str,
) -> (usize, usize, usize) {
    let mut total = 0;
    let mut pass = 0;
    let mut fail = 0;
    for (skill, result) in skills.iter().zip(results.iter()) {
        if skill.source != source {
            continue;
        }
        total += 1;
        if result.result.passed {
            pass += 1;
        } else {
            fail += 1;
        }
    }
    (total, pass, fail)
}

/// Write per-skill entries for one source group.
fn write_skill_entries(
    report: &mut String,
    skills: &[Skill],
    results: &[SkillResult],
    source: &str,
    show_checkmark: bool,
) {
    let root = project_root();
    for (skill, result) in skills.iter().zip(results.iter()) {
        if skill.source != source {
            continue;
        }
        let full_path = root.join(&skill.relative_path);
        if result.result.passed {
            wln!(report, "  [PASS] {}", skill.name);
            wln!(report, "       File: {}", full_path.display());
            if show_checkmark {
                wln!(report, "       ✓ All checks passed");
            } else {
                // Show runAs / allowed-tools for autopilot pass
                if let Some(ref fm) = result.frontmatter {
                    if let Some(run_as) = fm.get("runAs").filter(|v| !v.is_empty()) {
                        wln!(report, "       runAs: {}", run_as);
                    }
                    if let Some(tools) = fm.get("allowed-tools").filter(|v| !v.is_empty()) {
                        wln!(report, "       allowed-tools: {}", tools);
                    }
                }
            }
        } else {
            wln!(report, "  [FAIL] {}", skill.name);
            wln!(report, "       File: {}", full_path.display());
            for issue in &result.result.issues {
                wln!(report, "       Issue: {}", issue);
            }
        }
        wln!(report);
    }
}

/// Find skills where runAs=subagent but allowed-tools is missing/empty.
fn find_subagent_missing_allowed_tools(skills: &[Skill]) -> Vec<String> {
    let root = project_root();
    let mut missing = Vec::new();
    for skill in skills {
        let full_path = root.join(&skill.relative_path);
        let content = match fs::read_to_string(&full_path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        if let Ok(fm) = parse_frontmatter(&content) {
            if fm.get("runAs").is_some_and(|v| v == "subagent")
                && fm.get("allowed-tools").is_none_or(|v| v.is_empty())
            {
                missing.push(skill.name.clone());
            }
        }
    }
    missing
}

// ── Main ───────────────────────────────────────────────────────────────────

/// Determine whether validation should exit with error (any failure).
/// Extracted for testability.
fn any_validation_failed(results: &[SkillResult]) -> bool {
    results.iter().any(|r| !r.result.passed)
}

fn main() {
    let root = project_root();
    let skills = discover_skills(&root);
    let results = validate_all(&root, &skills);
    let report = generate_report(&skills, &results);

    // Print report to stdout (println! mirrors bash's `echo "$report"` which
    // appends a newline on top of the report's own trailing newline)
    println!("{}", report);

    // Save to validation/report.txt
    let report_path = root.join("validation/report.txt");
    if let Err(e) = fs::write(&report_path, &report) {
        eprintln!(
            "Warning: failed to write report to {}: {}",
            report_path.display(),
            e
        );
    }

    println!();
    println!("Report saved to: validation/report.txt");

    // Exit 1 if any FAIL, else 0
    if any_validation_failed(&results) {
        process::exit(1);
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Helpers ─────────────────────────────────────────────────────────

    /// Build a minimal passing SkillResult.
    fn pass_result() -> SkillResult {
        SkillResult {
            result: ValidationResult {
                passed: true,
                issues: vec![],
            },
            frontmatter: None,
        }
    }

    /// Build a failing SkillResult with a single issue.
    fn fail_result(issue: &str) -> SkillResult {
        SkillResult {
            result: ValidationResult {
                passed: false,
                issues: vec![issue.to_string()],
            },
            frontmatter: None,
        }
    }

    /// Build a Skill for testing.
    fn test_skill(name: &str, source: &str) -> Skill {
        Skill {
            name: name.to_string(),
            relative_path: format!("skills/{}/{}/SKILL.md", source, name),
            source: source.to_string(),
        }
    }

    // ── project_root ────────────────────────────────────────────────────

    #[test]
    fn project_root_contains_expected_markers() {
        let root = project_root();
        assert!(
            root.join("Cargo.toml").exists(),
            "project root must contain Cargo.toml"
        );
        assert!(
            root.join("validation").is_dir(),
            "project root must contain validation/ directory"
        );
        assert!(
            root.join(".skill-lock.json").exists(),
            "project root must contain .skill-lock.json"
        );
    }

    // ── any_validation_failed (exit-code branching) ─────────────────────

    #[test]
    fn all_pass_no_error_exit() {
        let results = vec![pass_result(), pass_result()];
        assert!(!any_validation_failed(&results));
    }

    #[test]
    fn any_fail_indicates_error_exit() {
        let results = vec![pass_result(), fail_result("missing name")];
        assert!(any_validation_failed(&results));
    }

    #[test]
    fn empty_results_no_error() {
        let results: Vec<SkillResult> = vec![];
        assert!(!any_validation_failed(&results));
    }

    // ── generate_report ─────────────────────────────────────────────────

    #[test]
    fn report_header_contains_expected_banner() {
        let skills = vec![test_skill("my-skill", "upstream")];
        let results = vec![pass_result()];
        let report = generate_report(&skills, &results);
        assert!(report.contains("FRONTMATTER VALIDATION REPORT — reasonix compatibility"));
        assert!(report.contains("=".repeat(70).as_str()));
        assert!(report.contains("Date: "));
    }

    #[test]
    fn report_shows_total_pass_fail_counts() {
        let skills = vec![
            test_skill("pass-1", "upstream"),
            test_skill("fail-1", "upstream"),
            test_skill("pass-2", "autopilot"),
        ];
        let results = vec![
            pass_result(),
            fail_result("missing description"),
            pass_result(),
        ];
        let report = generate_report(&skills, &results);
        assert!(report.contains("Total skills validated: 3 | Passed: 2 | Failed: 1"));
    }

    #[test]
    fn report_passing_skill_shows_pass_label() {
        let skills = vec![test_skill("good-skill", "upstream")];
        let results = vec![pass_result()];
        let report = generate_report(&skills, &results);
        assert!(report.contains("[PASS] good-skill"));
    }

    #[test]
    fn report_failing_skill_shows_fail_label_and_issues() {
        let skills = vec![test_skill("bad-skill", "upstream")];
        let results = vec![fail_result("Missing required field: name")];
        let report = generate_report(&skills, &results);
        assert!(report.contains("[FAIL] bad-skill"));
        assert!(report.contains("Missing required field: name"));
    }

    #[test]
    fn report_all_pass_shows_overall_pass() {
        let skills = vec![test_skill("s1", "upstream")];
        let results = vec![pass_result()];
        let report = generate_report(&skills, &results);
        assert!(report.contains("All skills PASS validation."));
    }

    #[test]
    fn report_any_fail_shows_overall_fail_count() {
        let skills = vec![test_skill("s1", "upstream"), test_skill("s2", "upstream")];
        let results = vec![pass_result(), fail_result("issue")];
        let report = generate_report(&skills, &results);
        assert!(report.contains("1 skill(s) FAIL validation."));
    }

    #[test]
    fn report_shows_upstream_and_autopilot_sections() {
        let skills = vec![
            test_skill("up-skill", "upstream"),
            test_skill("auto-skill", "autopilot"),
        ];
        let results = vec![pass_result(), pass_result()];
        let report = generate_report(&skills, &results);
        assert!(report.contains("Upstream Skills"));
        assert!(report.contains("Autopilot Skills"));
    }

    #[test]
    fn report_includes_global_checks_section() {
        let skills = vec![test_skill("s1", "upstream")];
        let results = vec![pass_result()];
        let report = generate_report(&skills, &results);
        assert!(report.contains("GLOBAL CHECKS"));
        assert!(report.contains("opencode-specific fields"));
        assert!(report.contains("subagent skills have allowed-tools"));
    }

    #[test]
    fn report_trailing_newline_matches_bash_output_convention() {
        let skills = vec![test_skill("s1", "upstream")];
        let results = vec![pass_result()];
        let report = generate_report(&skills, &results);
        // Bash `echo "$report"` outputs the report followed by a newline.
        // The Rust version uses `println!("{}", report)` which adds one.
        // So the report string itself should end with \n for the blank line
        // before OVERALL RESULT separator, and println! adds the final one.
        // We check that the report is non-empty and ends with a newline
        // (the OVERALL line's trailing \n).
        assert!(!report.is_empty(), "report must not be empty");
        assert!(
            report.ends_with('\n'),
            "report should end with newline (last line's \\n)"
        );
    }

    // ── Skill discovery (integration, uses real repository) ─────────────

    #[test]
    fn discover_skills_finds_both_sources() {
        let root = project_root();
        let skills = discover_skills(&root);
        assert!(!skills.is_empty(), "should find at least some skills");

        let has_upstream = skills.iter().any(|s| s.source == "upstream");
        let has_autopilot = skills.iter().any(|s| s.source == "autopilot");
        assert!(has_upstream, "should find upstream skills");
        assert!(has_autopilot, "should find autopilot skills");
    }

    #[test]
    fn discovered_skills_have_relative_paths() {
        let root = project_root();
        let skills = discover_skills(&root);
        for skill in &skills {
            let full_path = root.join(&skill.relative_path);
            assert!(
                full_path.exists(),
                "skill '{}' path '{}' must exist at {:?}",
                skill.name,
                skill.relative_path,
                full_path
            );
        }
    }
}
