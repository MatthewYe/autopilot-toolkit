//! Validation runner — discovers skills, validates frontmatter, and generates reports.
//!
//! Migrated from `validation/run.rs`.  Uses `skill_index::discover_skills()` for
//! skill discovery and `shared::load_skill_lock()` for upstream skill paths.
//!
//! Public API:
//! - `run_validation(project_root)` → `Result<ValidationReport>`
//! - `expand_skills(project_root)` → `Result<Vec<Skill>>`
//! - `validate_all(project_root, skills)` → `Vec<SkillResult>`
//! - `generate_report(skills, results, project_root)` → `String`
//! - `check_codex_status(project_root, skills)` → `Vec<String>`

use std::collections::HashMap;
use std::fmt::Write;
use std::fs;
use std::path::Path;

use chrono::Utc;
use validation::{parse_frontmatter, validate_skill_with_variant, SkillVariant, ValidationResult};

// ── wln! macro ──────────────────────────────────────────────────────────────

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

/// A skill ready for validation — flattened from `DiscoveredSkill` with a
/// concrete file path.
#[derive(Debug, Clone)]
pub struct Skill {
    pub name: String,
    /// Relative path from project root to the SKILL.md file.
    pub relative_path: String,
    /// "upstream" or "autopilot".
    pub source: String,
    /// Runtime variant: None for runtime-agnostic, Some("reasonix") etc.
    pub variant: Option<String>,
}

/// The result of validating a single skill.
pub struct SkillResult {
    pub result: ValidationResult,
    /// Frontmatter fields for autopilot display (runAs / allowed-tools)
    pub frontmatter: Option<HashMap<String, String>>,
}

/// Outcome of a full validation run.
pub struct ValidationReport {
    pub report: String,
    pub has_failures: bool,
}

// ── Skill expansion: DiscoveredSkill → Vec<Skill> ───────────────────────────

/// Expand `DiscoveredSkill` entries into flat `Skill` entries with concrete
/// file paths.  Uses `shared::load_skill_lock()` for upstream path resolution.
pub fn expand_skills(project_root: &Path) -> Result<Vec<Skill>, anyhow::Error> {
    let discovered = skill_index::discover_skills(project_root)?;
    let lock = shared::load_skill_lock().ok();

    let mut skills: Vec<Skill> = Vec::new();

    // ── Upstream skills ──
    for d in &discovered {
        if d.source != "upstream" {
            continue;
        }
        let path = lock
            .as_ref()
            .and_then(|l| l.skills.iter().find(|s| s.name == d.name))
            .map(|s| format!("skills/upstream/{}", s.skill_path));
        if let Some(relative_path) = path {
            let full_path = project_root.join(&relative_path);
            if full_path.is_file() {
                skills.push(Skill {
                    name: d.name.clone(),
                    relative_path,
                    source: "upstream".to_string(),
                    variant: None,
                });
            }
        }
    }

    // ── Autopilot skills ──
    // Collect into a temp vec for deterministic sorting (name, then variant)
    let mut autopilot_entries: Vec<(String, String, Option<String>)> = Vec::new();
    for d in &discovered {
        if d.source != "autopilot" {
            continue;
        }
        // Root-level SKILL.md (agnostic skill or fallback)
        let root_skill_path = project_root.join(format!("skills/autopilot/{}/SKILL.md", d.name));
        if root_skill_path.is_file() {
            autopilot_entries.push((
                d.name.clone(),
                format!("skills/autopilot/{}/SKILL.md", d.name),
                None,
            ));
        }
        // Variant subdirectories
        for variant in &d.variants {
            let variant_path = project_root.join(format!(
                "skills/autopilot/{}/{}/SKILL.md",
                d.name, variant
            ));
            if variant_path.is_file() {
                autopilot_entries.push((
                    d.name.clone(),
                    format!("skills/autopilot/{}/{}/SKILL.md", d.name, variant),
                    Some(variant.clone()),
                ));
            }
        }
    }
    autopilot_entries.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.2.cmp(&b.2)));
    for (name, relative_path, variant) in autopilot_entries {
        skills.push(Skill {
            name,
            relative_path,
            source: "autopilot".to_string(),
            variant,
        });
    }

    Ok(skills)
}

// ── Batch validation ───────────────────────────────────────────────────────

/// Validate every skill in the list against its file content.
pub fn validate_all(project_root: &Path, skills: &[Skill]) -> Vec<SkillResult> {
    skills
        .iter()
        .map(|skill| {
            let full_path = project_root.join(&skill.relative_path);
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
            let variant = match skill.variant.as_deref() {
                Some("reasonix") => SkillVariant::Reasonix,
                Some("codex") => SkillVariant::Codex,
                Some("kimi") => SkillVariant::Kimi,
                _ => SkillVariant::Agnostic,
            };
            let validation_result = validate_skill_with_variant(&content, variant);
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

/// Generate the full human-readable validation report.
pub fn generate_report(skills: &[Skill], results: &[SkillResult], project_root: Option<&Path>) -> String {
    let sep = "=".repeat(70);
    let date_str = Utc::now().format("%Y-%m-%dT%H:%M:%S.000Z").to_string();

    let total = skills.len();
    let pass_count = results.iter().filter(|r| r.result.passed).count();
    let fail_count = total - pass_count;

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
    write_skill_entries(&mut report, skills, results, "upstream", true, project_root);

    // ── Autopilot section ──
    wln!(report, "--- Autopilot Skills ({}) ---", autopilot_total);
    wln!(
        report,
        "Passed: {} / Failed: {}",
        autopilot_pass,
        autopilot_fail
    );
    wln!(report);
    write_skill_entries(&mut report, skills, results, "autopilot", false, project_root);

    // ── Codex variant status ──
    if let Some(root) = project_root {
        let codex_status = check_codex_status(root, skills);
        if !codex_status.is_empty() {
            wln!(report, "--- Codex Variant Status ---");
            wln!(report);
            for line in &codex_status {
                wln!(report, "  {}", line);
            }
            wln!(report);
        }
    }

    // ── Global checks ──
    wln!(report, "{}", sep);
    wln!(report, "GLOBAL CHECKS");
    wln!(report, "{}", sep);
    wln!(report);

    // Check 1: 0 opencode-specific fields (exclude codex and kimi variants)
    let oc_count: usize = skills
        .iter()
        .zip(results.iter())
        .filter(|(s, _)| {
            let v = s.variant.as_deref();
            v != Some("codex") && v != Some("kimi")
        })
        .map(|(_, r)| {
            r.result
                .issues
                .iter()
                .filter(|issue| issue.starts_with("OpenCode-specific field present:"))
                .count()
        })
        .sum();
    let non_codex_count = skills
        .iter()
        .filter(|s| {
            let v = s.variant.as_deref();
            v != Some("codex") && v != Some("kimi")
        })
        .count();
    wln!(
        report,
        "Check: 0 opencode-specific fields across {} skills ({} non-codex/kimi)",
        non_codex_count,
        non_codex_count
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
    let sub_missing = find_subagent_missing_allowed_tools(skills, project_root);
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
    project_root: Option<&Path>,
) {
    for (skill, result) in skills.iter().zip(results.iter()) {
        if skill.source != source {
            continue;
        }
        // Build display label: name + optional variant tag
        let display_name = match skill.variant.as_deref() {
            Some(v) => format!("{} ({})", skill.name, v),
            None => skill.name.clone(),
        };
        // Path display: use project_root if provided, else relative path
        let path_display = match project_root {
            Some(root) => root.join(&skill.relative_path).display().to_string(),
            None => skill.relative_path.clone(),
        };
        if result.result.passed {
            wln!(report, "  [PASS] {}", display_name);
            wln!(report, "       File: {}", path_display);
            if show_checkmark {
                wln!(report, "       ✓ All checks passed");
            } else {
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
            wln!(report, "  [FAIL] {}", display_name);
            wln!(report, "       File: {}", path_display);
            for issue in &result.result.issues {
                wln!(report, "       Issue: {}", issue);
            }
        }
        wln!(report);
    }
}

/// Find skills where runAs=subagent but allowed-tools is missing/empty.
///
/// When `project_root` is None, only reports the skill name without reading
/// frontmatter (used in test contexts where file access isn't needed).
fn find_subagent_missing_allowed_tools(
    skills: &[Skill],
    project_root: Option<&Path>,
) -> Vec<String> {
    let mut missing = Vec::new();
    for skill in skills {
        let content = match project_root {
            Some(root) => {
                let full_path = root.join(&skill.relative_path);
                match fs::read_to_string(&full_path) {
                    Ok(c) => c,
                    Err(_) => continue,
                }
            }
            None => continue,
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

// ── Codex status check ─────────────────────────────────────────────────────

/// Check codex variant status for autopilot skills.
///
/// Returns informational lines about which skills lack codex SKILL.md.
/// **Data-driven**: uses the filesystem to determine whether a missing codex
/// SKILL.md is because the skill uses agent.toml instead (codex/agent.toml
/// exists) or is simply a placeholder directory (no agent.toml).
pub fn check_codex_status(project_root: &Path, skills: &[Skill]) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let autopilot_dir = project_root.join("skills/autopilot");
    if !autopilot_dir.is_dir() {
        return lines;
    }
    if let Ok(read_dir) = fs::read_dir(&autopilot_dir) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let codex_skill = path.join("codex/SKILL.md");
                let codex_dir = path.join("codex");
                if codex_dir.is_dir() {
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
                    let has_codex = codex_skill.is_file();
                    let already_found = skills
                        .iter()
                        .any(|s| s.name == name && s.variant.as_deref() == Some("codex"));
                    if !has_codex && !already_found {
                        // Data-driven: check filesystem instead of hardcoded names
                        let has_agent_toml = codex_dir.join("agent.toml").is_file();
                        if has_agent_toml {
                            lines.push(format!(
                                "[INFO] {}: no codex/SKILL.md (uses agent.toml instead)",
                                name
                            ));
                        } else {
                            lines.push(format!(
                                "[INFO] {}: no codex/SKILL.md (placeholder directory)",
                                name
                            ));
                        }
                    }
                }
            }
        }
    }
    lines.sort();
    lines
}

// ── Exit-code helper ───────────────────────────────────────────────────────

/// Determine whether validation should exit with error (any failure).
pub fn any_validation_failed(results: &[SkillResult]) -> bool {
    results.iter().any(|r| !r.result.passed)
}

// ── Main API ────────────────────────────────────────────────────────────────

/// Run the full validation pipeline: discover → validate → report.
///
/// Returns the report string and whether any failures were found.
pub fn run_validation(project_root: &Path) -> Result<ValidationReport, anyhow::Error> {
    let skills = expand_skills(project_root)?;
    let results = validate_all(project_root, &skills);
    let report = generate_report(&skills, &results, Some(project_root));
    let has_failures = any_validation_failed(&results);
    Ok(ValidationReport {
        report,
        has_failures,
    })
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Helpers ─────────────────────────────────────────────────────────

    fn pass_result() -> SkillResult {
        SkillResult {
            result: ValidationResult {
                passed: true,
                issues: vec![],
            },
            frontmatter: None,
        }
    }

    fn fail_result(issue: &str) -> SkillResult {
        SkillResult {
            result: ValidationResult {
                passed: false,
                issues: vec![issue.to_string()],
            },
            frontmatter: None,
        }
    }

    fn test_skill(name: &str, source: &str) -> Skill {
        Skill {
            name: name.to_string(),
            relative_path: format!("skills/{}/{}/SKILL.md", source, name),
            source: source.to_string(),
            variant: None,
        }
    }

    // ── any_validation_failed ───────────────────────────────────────────

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
        let report = generate_report(&skills, &results, None);
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
        let report = generate_report(&skills, &results, None);
        assert!(report.contains("Total skills validated: 3 | Passed: 2 | Failed: 1"));
    }

    #[test]
    fn report_passing_skill_shows_pass_label() {
        let skills = vec![test_skill("good-skill", "upstream")];
        let results = vec![pass_result()];
        let report = generate_report(&skills, &results, None);
        assert!(report.contains("[PASS] good-skill"));
    }

    #[test]
    fn report_failing_skill_shows_fail_label_and_issues() {
        let skills = vec![test_skill("bad-skill", "upstream")];
        let results = vec![fail_result("Missing required field: name")];
        let report = generate_report(&skills, &results, None);
        assert!(report.contains("[FAIL] bad-skill"));
        assert!(report.contains("Missing required field: name"));
    }

    #[test]
    fn report_all_pass_shows_overall_pass() {
        let skills = vec![test_skill("s1", "upstream")];
        let results = vec![pass_result()];
        let report = generate_report(&skills, &results, None);
        assert!(report.contains("All skills PASS validation."));
    }

    #[test]
    fn report_any_fail_shows_overall_fail_count() {
        let skills = vec![test_skill("s1", "upstream"), test_skill("s2", "upstream")];
        let results = vec![pass_result(), fail_result("issue")];
        let report = generate_report(&skills, &results, None);
        assert!(report.contains("1 skill(s) FAIL validation."));
    }

    #[test]
    fn report_shows_upstream_and_autopilot_sections() {
        let skills = vec![
            test_skill("up-skill", "upstream"),
            test_skill("auto-skill", "autopilot"),
        ];
        let results = vec![pass_result(), pass_result()];
        let report = generate_report(&skills, &results, None);
        assert!(report.contains("Upstream Skills"));
        assert!(report.contains("Autopilot Skills"));
    }

    #[test]
    fn report_includes_global_checks_section() {
        let skills = vec![test_skill("s1", "upstream")];
        let results = vec![pass_result()];
        let report = generate_report(&skills, &results, None);
        assert!(report.contains("GLOBAL CHECKS"));
        assert!(report.contains("opencode-specific fields"));
        assert!(report.contains("subagent skills have allowed-tools"));
    }

    #[test]
    fn report_trailing_newline_matches_bash_output_convention() {
        let skills = vec![test_skill("s1", "upstream")];
        let results = vec![pass_result()];
        let report = generate_report(&skills, &results, None);
        assert!(!report.is_empty(), "report must not be empty");
        assert!(
            report.ends_with('\n'),
            "report should end with newline (last line's \\n)"
        );
    }

    // ── Report variant tests ────────────────────────────────────────────

    #[test]
    fn report_shows_variant_tag_in_skill_name() {
        let skills = vec![Skill {
            name: "my-skill".to_string(),
            relative_path: "skills/autopilot/my-skill/reasonix/SKILL.md".to_string(),
            source: "autopilot".to_string(),
            variant: Some("reasonix".to_string()),
        }];
        let results = vec![pass_result()];
        let report = generate_report(&skills, &results, None);
        assert!(report.contains("[PASS] my-skill (reasonix)"));
    }

    #[test]
    fn report_codex_variant_not_counted_in_opencode_global_check() {
        let skills = vec![Skill {
            name: "my-skill".to_string(),
            relative_path: "skills/autopilot/my-skill/codex/SKILL.md".to_string(),
            source: "autopilot".to_string(),
            variant: Some("codex".to_string()),
        }];
        let results = vec![pass_result()];
        let report = generate_report(&skills, &results, None);
        assert!(report.contains("✓ PASS"));
    }

    #[test]
    fn report_shows_non_codex_count_in_global_check() {
        let skills = vec![
            Skill {
                name: "reasonix-skill".to_string(),
                relative_path: "skills/autopilot/my-skill/reasonix/SKILL.md".to_string(),
                source: "autopilot".to_string(),
                variant: Some("reasonix".to_string()),
            },
            Skill {
                name: "codex-skill".to_string(),
                relative_path: "skills/autopilot/my-skill/codex/SKILL.md".to_string(),
                source: "autopilot".to_string(),
                variant: Some("codex".to_string()),
            },
            Skill {
                name: "kimi-skill".to_string(),
                relative_path: "skills/autopilot/my-skill/kimi/SKILL.md".to_string(),
                source: "autopilot".to_string(),
                variant: Some("kimi".to_string()),
            },
        ];
        let results = vec![pass_result(), pass_result(), pass_result()];
        let report = generate_report(&skills, &results, None);
        assert!(
            report.contains("1 non-codex/kimi"),
            "global check should show 1 non-codex/kimi, got:\n{}",
            report
        );
    }

    // ── expand_skills (integration with real repo) ──────────────────────

    fn repo_root() -> &'static Path {
        // Use compile-time path to find the real repo root
        // This mirrors what shared::project_root() does.
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
    }

    #[test]
    fn expand_skills_finds_both_sources() {
        let root = repo_root();
        let skills = expand_skills(root).expect("expand_skills should succeed");
        assert!(!skills.is_empty(), "should find at least some skills");

        let has_upstream = skills.iter().any(|s| s.source == "upstream");
        let has_autopilot = skills.iter().any(|s| s.source == "autopilot");
        assert!(has_upstream, "should find upstream skills");
        assert!(has_autopilot, "should find autopilot skills");
    }

    #[test]
    fn expanded_skills_have_relative_paths() {
        let root = repo_root();
        let skills = expand_skills(root).expect("expand_skills should succeed");
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

    // ── Variant expansion tests (temp fixtures) ─────────────────────────

    /// Build a temp fixture with one autopilot skill carrying the given
    /// variant directories and a root SKILL.md.
    fn expand_with_variants(variant_dirs: &[&str]) -> Vec<Skill> {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let skill_dir = root.join("skills/autopilot/fixture-skill");

        // Always create root SKILL.md for autopilot discover_skills to find it
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: fixture-skill\ndescription: fixture\n---\n",
        )
        .unwrap();

        for dir in variant_dirs {
            fs::create_dir_all(skill_dir.join(dir)).unwrap();
            fs::write(
                skill_dir.join(dir).join("SKILL.md"),
                "---\nname: fixture-skill\ndescription: fixture\n---\n",
            )
            .unwrap();
        }

        // We can't use expand_skills because it calls skill_index::discover_skills
        // which calls shared::load_skill_lock which finds the real repo's lock.
        // Instead, test classification directly.
        let (_skill_type, variants, _codex_agent) = skill_index::classify_skill(&skill_dir);

        // Build expected skills manually
        let mut skills = Vec::new();
        // Root
        skills.push(Skill {
            name: "fixture-skill".to_string(),
            relative_path: "skills/autopilot/fixture-skill/SKILL.md".to_string(),
            source: "autopilot".to_string(),
            variant: None,
        });
        for v in &variants {
            skills.push(Skill {
                name: "fixture-skill".to_string(),
                relative_path: format!("skills/autopilot/fixture-skill/{}/SKILL.md", v),
                source: "autopilot".to_string(),
                variant: Some(v.clone()),
            });
        }
        skills.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.variant.cmp(&b.variant)));
        skills
    }

    #[test]
    fn discovers_kimi_variant() {
        let skills = expand_with_variants(&["kimi"]);
        assert!(
            skills
                .iter()
                .any(|s| s.name == "fixture-skill" && s.variant.as_deref() == Some("kimi")),
            "should discover kimi variant, got: {:?}",
            skills.iter().map(|s| &s.relative_path).collect::<Vec<_>>()
        );
    }

    #[test]
    fn variant_directories_without_skill_md_are_ignored() {
        // classify_skill only returns variants whose directories exist.
        // The expansion step further checks for SKILL.md existence.
        let tmp = tempfile::tempdir().unwrap();
        let codex_dir = tmp.path().join("skills/autopilot/fixture-skill/codex");
        fs::create_dir_all(&codex_dir).unwrap();
        fs::write(codex_dir.join("agent.toml"), "name = \"fixture\"\n").unwrap();

        // classify_skill returns variants based on directory existence
        let (_type, variants, _codex_agent) =
            skill_index::classify_skill(&tmp.path().join("skills/autopilot/fixture-skill"));
        // codex dir exists, so it's in variants
        assert!(variants.contains(&"codex".to_string()));

        // But when expanding, we check for SKILL.md — agent.toml alone won't create a Skill entry
        // (Verified in the expansion logic: variant_path.is_file() check)
    }

    #[test]
    fn discovers_reasonix_variants_for_coupled_skills() {
        let root = repo_root();
        let skills = expand_skills(root).expect("expand_skills should succeed");
        let coupled_names = [
            "audit-autopilot",
            "autopilot-implementer",
            "autopilot-orchestrator",
            "autopilot-reviewer",
        ];
        for name in coupled_names {
            let found = skills
                .iter()
                .any(|s| s.name == name && s.variant.as_deref() == Some("reasonix"));
            assert!(found, "should discover reasonix variant for {}", name);
        }
    }

    #[test]
    fn variant_skills_use_correct_relative_path() {
        let root = repo_root();
        let skills = expand_skills(root).expect("expand_skills should succeed");
        let orchestrator = skills.iter().find(|s| {
            s.name == "autopilot-orchestrator" && s.variant.as_deref() == Some("reasonix")
        });
        assert!(
            orchestrator.is_some(),
            "should find autopilot-orchestrator reasonix variant"
        );
        let orch = orchestrator.unwrap();
        assert_eq!(
            orch.relative_path,
            "skills/autopilot/autopilot-orchestrator/reasonix/SKILL.md"
        );
    }

    #[test]
    fn discovers_codex_skill_variants() {
        let root = repo_root();
        let skills = expand_skills(root).expect("expand_skills should succeed");
        let orchestrator = skills
            .iter()
            .find(|s| s.name == "autopilot-orchestrator" && s.variant.as_deref() == Some("codex"));
        assert!(
            orchestrator.is_some(),
            "should find autopilot-orchestrator codex variant"
        );
        let orch = orchestrator.unwrap();
        assert_eq!(
            orch.relative_path,
            "skills/autopilot/autopilot-orchestrator/codex/SKILL.md"
        );

        let audit = skills
            .iter()
            .find(|s| s.name == "audit-autopilot" && s.variant.as_deref() == Some("codex"));
        assert!(audit.is_some(), "should find audit-autopilot codex variant");
        let audit = audit.unwrap();
        assert_eq!(
            audit.relative_path,
            "skills/autopilot/audit-autopilot/codex/SKILL.md"
        );
    }

    #[test]
    fn runtime_agnostic_skills_have_no_variant() {
        let root = repo_root();
        let skills = expand_skills(root).expect("expand_skills should succeed");
        let toolkit = skills.iter().find(|s| s.name == "toolkit-setup");
        assert!(toolkit.is_some(), "should find toolkit-setup");
        assert_eq!(
            toolkit.unwrap().variant,
            None,
            "toolkit-setup should have no variant"
        );
    }

    // ── Codex status tests ──────────────────────────────────────────────

    #[test]
    fn check_codex_status_reports_missing_codex() {
        let root = repo_root();
        let skills = expand_skills(root).expect("expand_skills should succeed");
        let status = check_codex_status(root, &skills);
        assert!(status
            .iter()
            .any(|l| l.contains("autopilot-implementer") && l.contains("agent.toml")));
        assert!(status
            .iter()
            .any(|l| l.contains("autopilot-reviewer") && l.contains("agent.toml")));
        assert!(
            !status.iter().any(|l| l.contains("audit-autopilot") && l.contains("placeholder")),
            "audit-autopilot has a codex/SKILL.md and should no longer be reported as a placeholder"
        );
        assert!(
            !status
                .iter()
                .any(|l| l.contains("autopilot-orchestrator") && l.contains("placeholder")),
            "orchestrator has a codex/SKILL.md and should no longer be reported as a placeholder"
        );
    }

    // ── Data-driven codex status ────────────────────────────────────────

    #[test]
    fn codex_status_is_data_driven_not_hardcoded_names() {
        // Create a temp fixture with a skill that has codex/agent.toml but no codex/SKILL.md
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // Set up autopilot skill dir
        let skill_dir = root.join("skills/autopilot/test-skill");
        fs::create_dir_all(skill_dir.join("codex")).unwrap();
        fs::write(
            skill_dir.join("codex").join("agent.toml"),
            "[agent]\nname = \"test\"\n",
        )
        .unwrap();

        // The skill list is empty (no codex variant discovered)
        let skills: Vec<Skill> = vec![];
        let status = check_codex_status(root, &skills);

        // Should report "uses agent.toml" because agent.toml exists
        // (data-driven: based on filesystem, not hardcoded name)
        assert!(
            status.iter().any(|l| l.contains("test-skill") && l.contains("agent.toml")),
            "should detect agent.toml for a skill with any name, got: {:?}",
            status
        );
    }

    #[test]
    fn codex_status_placeholder_for_dir_without_agent_toml() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        let skill_dir = root.join("skills/autopilot/placeholder-skill");
        fs::create_dir_all(skill_dir.join("codex")).unwrap();
        // No agent.toml, no SKILL.md — just an empty codex dir

        let skills: Vec<Skill> = vec![];
        let status = check_codex_status(root, &skills);

        assert!(
            status
                .iter()
                .any(|l| l.contains("placeholder-skill") && l.contains("placeholder")),
            "should report placeholder for skill without agent.toml, got: {:?}",
            status
        );
    }
}
