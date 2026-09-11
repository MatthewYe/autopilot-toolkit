//! Validation runner — discovers skills, validates frontmatter, and generates reports.
//!
//! Migrated from `validation/run.rs`.  Consumes `skill_index::discover_skills()`
//! — the Expected-set enumerator — for every skill source; failed entries are
//! reported as FAIL rather than silently dropped, including resolved entries
//! whose skill file is missing.
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

/// A skill ready for validation — flattened from an Expected-set entry with a
/// concrete file path.
#[derive(Debug, Clone)]
pub struct Skill {
    pub name: String,
    /// Relative path from project root to the SKILL.md file — the expected
    /// location even when the file is missing.
    pub relative_path: String,
    /// "upstream", "vendor", or "autopilot".
    pub source: String,
    /// Runtime variant: None for runtime-agnostic, Some("reasonix") etc.
    pub variant: Option<String>,
    /// Why this entry fails validation, if it does — an Expected-set
    /// resolution failure, or a missing skill file for a resolved entry.
    /// A failed entry fails validation immediately, without reading content.
    pub failed_reason: Option<String>,
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

// ── Skill expansion: Expected-set entries → Vec<Skill> ─────────────────────

/// Expand the Expected set into flat `Skill` entries with concrete file paths.
///
/// Failed entries keep their identity and carry the failure as
/// `failed_reason`, so validation reports them as FAIL instead of dropping
/// them.  Every Expected-set entry yields at least one `Skill`: a resolved
/// entry whose skill file is missing becomes a failed entry naming that file.
/// The one exemption is a codex variant that ships an `agent.toml`
/// custom-agent definition instead of a `SKILL.md` (reported as INFO by
/// [`check_codex_status`]); it yields no variant entry and no failure.
pub fn expand_skills(project_root: &Path) -> Result<Vec<Skill>, anyhow::Error> {
    let entries = skill_index::discover_skills(project_root)?;
    let mut skills: Vec<Skill> = Vec::new();

    for entry in entries {
        let relative_dir = entry
            .source_dir
            .strip_prefix(project_root)
            .unwrap_or(&entry.source_dir);

        match &entry.resolution {
            skill_index::ResolutionStatus::Missing { reason } => {
                skills.push(Skill {
                    name: entry.name.clone(),
                    relative_path: skill_relative_path(relative_dir, None),
                    source: entry.source.clone(),
                    variant: None,
                    failed_reason: Some(reason.clone()),
                });
            }
            skill_index::ResolutionStatus::Resolved => {
                skills.push(entry_skill(
                    &entry,
                    relative_dir,
                    None,
                    &entry.source_dir.join("SKILL.md"),
                ));
                for variant in &entry.variants {
                    let variant_skill = entry.source_dir.join(variant).join("SKILL.md");
                    if variant == "codex" && entry.codex_agent && !variant_skill.is_file() {
                        continue;
                    }
                    skills.push(entry_skill(
                        &entry,
                        relative_dir,
                        Some(variant),
                        &variant_skill,
                    ));
                }
            }
        }
    }

    Ok(skills)
}

/// Build the `Skill` for one Expected-set entry's skill file.
///
/// A missing file yields the failed-entry shape: the entry keeps its identity
/// and the reason names the file that should have been there.
fn entry_skill(
    entry: &skill_index::ExpectedSetEntry,
    relative_dir: &Path,
    variant: Option<&str>,
    skill_file: &Path,
) -> Skill {
    Skill {
        name: entry.name.clone(),
        relative_path: skill_relative_path(relative_dir, variant),
        source: entry.source.clone(),
        variant: variant.map(|v| v.to_string()),
        failed_reason: (!skill_file.is_file())
            .then(|| format!("skill file not found: {}", skill_file.display())),
    }
}

/// Project-relative path to a skill's SKILL.md, with an optional variant
/// subdirectory.
fn skill_relative_path(relative_dir: &Path, variant: Option<&str>) -> String {
    let path = match variant {
        Some(variant) => relative_dir.join(variant).join("SKILL.md"),
        None => relative_dir.join("SKILL.md"),
    };
    path.to_string_lossy().to_string()
}

// ── Batch validation ───────────────────────────────────────────────────────

/// Validate every skill in the list against its file content.
///
/// Failed Expected-set entries fail immediately with the entry's reason;
/// everything else is read and validated from disk.
pub fn validate_all(project_root: &Path, skills: &[Skill]) -> Vec<SkillResult> {
    skills
        .iter()
        .map(|skill| {
            if let Some(reason) = &skill.failed_reason {
                return SkillResult {
                    result: ValidationResult {
                        passed: false,
                        issues: vec![reason.clone()],
                    },
                    frontmatter: None,
                };
            }
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
pub fn generate_report(
    skills: &[Skill],
    results: &[SkillResult],
    project_root: Option<&Path>,
) -> String {
    let sep = "=".repeat(70);
    let date_str = Utc::now().format("%Y-%m-%dT%H:%M:%S.000Z").to_string();

    let total = skills.len();
    let pass_count = results.iter().filter(|r| r.result.passed).count();
    let fail_count = total - pass_count;

    let (upstream_total, upstream_pass, upstream_fail) =
        count_by_source(skills, results, "upstream");
    let (autopilot_total, autopilot_pass, autopilot_fail) =
        count_by_source(skills, results, "autopilot");
    let (vendor_total, vendor_pass, vendor_fail) = count_by_source(skills, results, "vendor");

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

    // ── Vendor section ──
    wln!(report, "--- Vendor Skills ({}) ---", vendor_total);
    wln!(report, "Passed: {} / Failed: {}", vendor_pass, vendor_fail);
    wln!(report);
    write_skill_entries(&mut report, skills, results, "vendor", true, project_root);

    // ── Autopilot section ──
    wln!(report, "--- Autopilot Skills ({}) ---", autopilot_total);
    wln!(
        report,
        "Passed: {} / Failed: {}",
        autopilot_pass,
        autopilot_fail
    );
    wln!(report);
    write_skill_entries(
        &mut report,
        skills,
        results,
        "autopilot",
        false,
        project_root,
    );

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
///
/// The placeholder line only surfaces for callers that pass a `skills` list
/// which does not carry the skill's codex variant: through [`expand_skills`]
/// such a directory is already a failed entry, so validation FAILs it instead
/// of reporting INFO.
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
            failed_reason: None,
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
    fn report_shows_upstream_vendor_and_autopilot_sections() {
        let skills = vec![
            test_skill("up-skill", "upstream"),
            test_skill("vendor-skill", "vendor"),
            test_skill("auto-skill", "autopilot"),
        ];
        let results = vec![pass_result(), pass_result(), pass_result()];
        let report = generate_report(&skills, &results, None);
        assert!(report.contains("Upstream Skills"));
        assert!(report.contains("Vendor Skills"));
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
            failed_reason: None,
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
            failed_reason: None,
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
                failed_reason: None,
            },
            Skill {
                name: "codex-skill".to_string(),
                relative_path: "skills/autopilot/my-skill/codex/SKILL.md".to_string(),
                source: "autopilot".to_string(),
                variant: Some("codex".to_string()),
                failed_reason: None,
            },
            Skill {
                name: "kimi-skill".to_string(),
                relative_path: "skills/autopilot/my-skill/kimi/SKILL.md".to_string(),
                source: "autopilot".to_string(),
                variant: Some("kimi".to_string()),
                failed_reason: None,
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
    fn expand_skills_finds_all_sources() {
        let root = repo_root();
        let skills = expand_skills(root).expect("expand_skills should succeed");
        assert!(!skills.is_empty(), "should find at least some skills");

        let has_upstream = skills.iter().any(|s| s.source == "upstream");
        let has_vendor = skills.iter().any(|s| s.source == "vendor");
        let has_autopilot = skills.iter().any(|s| s.source == "autopilot");
        assert!(has_upstream, "should find upstream skills");
        assert!(has_vendor, "should find vendor skills");
        assert!(has_autopilot, "should find autopilot skills");

        let show_me = skills
            .iter()
            .find(|s| s.name == "show-me")
            .expect("show-me should be discovered as a vendor skill");
        assert_eq!(show_me.source, "vendor");
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

    // ── Failed entries (Expected-set resolution failures) ───────────────

    /// Write a temp fixture's `.skill-lock.json` from a raw skills map.
    fn write_skill_lock(root: &Path, skills_json: &str) {
        fs::write(
            root.join(".skill-lock.json"),
            format!("{{\"version\": 4, \"skills\": {}}}", skills_json),
        )
        .unwrap();
    }

    /// Add a valid autopilot skill so a fixture has one resolved entry.
    fn write_valid_autopilot_skill(root: &Path) {
        let skill_dir = root.join("skills/autopilot/fixture-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        write_skill_md(&skill_dir);
    }

    /// Write a valid SKILL.md into an existing skill directory.
    fn write_skill_md(dir: &Path) {
        fs::write(
            dir.join("SKILL.md"),
            "---\nname: fixture-skill\ndescription: fixture\n---\n",
        )
        .unwrap();
    }

    #[test]
    fn run_validation_fails_when_expected_entry_directory_is_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_valid_autopilot_skill(root);
        write_skill_lock(
            root,
            r#"{
                "ghost-skill": {
                    "sourceType": "github",
                    "skillPath": "skills/engineering/ghost-skill/SKILL.md",
                    "skillFolderHash": "abc123"
                }
            }"#,
        );

        let report = run_validation(root).expect("run_validation should succeed");
        assert!(
            report.has_failures,
            "an expected skill whose directory is missing must fail validation, got:\n{}",
            report.report
        );
        assert!(
            report.report.contains("[FAIL] ghost-skill"),
            "report must carry a [FAIL] entry for the missing skill, got:\n{}",
            report.report
        );
        assert!(
            report.report.contains("upstream skill directory not found"),
            "the failed entry's reason must be reported as an issue, got:\n{}",
            report.report
        );
        assert!(
            report.report.contains("[PASS] fixture-skill"),
            "resolved entries keep passing, got:\n{}",
            report.report
        );
    }

    #[test]
    fn run_validation_errors_on_malformed_lock() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_valid_autopilot_skill(root);
        fs::write(root.join(".skill-lock.json"), "{ not valid json").unwrap();

        let err = run_validation(root)
            .err()
            .expect("a malformed lock must be a hard error");
        assert!(
            err.to_string().contains(".skill-lock.json"),
            "the error should name the malformed lock file, got: {err}"
        );
    }

    #[test]
    fn run_validation_without_lock_has_no_upstream_skills() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_valid_autopilot_skill(root);

        let report = run_validation(root).expect("run_validation should succeed");
        assert!(
            !report.has_failures,
            "a missing lock means no upstream skills, not a failure, got:\n{}",
            report.report
        );
        assert!(
            report.report.contains("Upstream Skills (0)"),
            "no lock must yield zero upstream skills, got:\n{}",
            report.report
        );
        assert!(
            report.report.contains("[PASS] fixture-skill"),
            "the resolved autopilot skill still passes, got:\n{}",
            report.report
        );
    }

    // ── Entry-state matrix (one resolved entry per Skill source) ────────

    /// The states a single Expected-set entry can be in.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum EntryState {
        Present,
        MissingDirectory,
        MissingSkillMd,
        MalformedSkillPath,
    }

    /// Write a `.vendor-lock.json` pinning one fixture skill at the
    /// conventional vendor location.
    fn write_vendor_lock(root: &Path) {
        fs::write(
            root.join(".vendor-lock.json"),
            r#"{
                "version": 1,
                "skills": {
                    "fixture-skill": {
                        "sourceType": "github",
                        "skillPath": "plugins/fixture-skill/SKILL.md",
                        "skillFolderHash": "abc123",
                        "vendorPath": "skills/vendor/fixture-skill"
                    }
                }
            }"#,
        )
        .unwrap();
    }

    /// Build a one-entry fixture for `source` in `state` and run the full
    /// validation pipeline over it.
    ///
    /// States that cannot occur for a source are rejected by construction:
    /// autopilot entries come from a directory scan (their resolution never
    /// fails), and vendor entries resolve through `vendorPath` (a malformed
    /// `skillPath` cannot orphan them).
    fn matrix_case(source: &str, state: EntryState) -> (tempfile::TempDir, ValidationReport) {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        match source {
            "autopilot" => {
                assert!(
                    matches!(state, EntryState::Present | EntryState::MissingSkillMd),
                    "autopilot cannot be in state {state:?}"
                );
                let dir = root.join("skills/autopilot/fixture-skill");
                fs::create_dir_all(&dir).unwrap();
                if state == EntryState::Present {
                    write_skill_md(&dir);
                }
            }
            "upstream" => {
                let skill_path = if state == EntryState::MalformedSkillPath {
                    "skills/engineering/fixture-skill"
                } else {
                    "skills/engineering/fixture-skill/SKILL.md"
                };
                write_skill_lock(
                    root,
                    &format!(
                        r#"{{
                            "fixture-skill": {{
                                "sourceType": "github",
                                "skillPath": "{skill_path}",
                                "skillFolderHash": "abc123"
                            }}
                        }}"#
                    ),
                );
                let dir = root.join("skills/upstream/skills/engineering/fixture-skill");
                if matches!(state, EntryState::Present | EntryState::MissingSkillMd) {
                    fs::create_dir_all(&dir).unwrap();
                }
                if state == EntryState::Present {
                    write_skill_md(&dir);
                }
            }
            "vendor" => {
                assert!(
                    state != EntryState::MalformedSkillPath,
                    "vendor entries resolve through vendorPath"
                );
                write_vendor_lock(root);
                let dir = root.join("skills/vendor/fixture-skill");
                if matches!(state, EntryState::Present | EntryState::MissingSkillMd) {
                    fs::create_dir_all(&dir).unwrap();
                }
                if state == EntryState::Present {
                    write_skill_md(&dir);
                }
            }
            other => panic!("unknown Skill source: {other}"),
        }
        let report = run_validation(root).expect("run_validation should succeed");
        (tmp, report)
    }

    #[test]
    fn matrix_present_entries_pass_for_every_source() {
        for source in ["autopilot", "upstream", "vendor"] {
            let (_tmp, report) = matrix_case(source, EntryState::Present);
            assert!(
                !report.has_failures,
                "{source}: a present entry must pass, got:\n{}",
                report.report
            );
            assert!(
                report.report.contains("[PASS] fixture-skill"),
                "{source}: a present entry must be reported as PASS, got:\n{}",
                report.report
            );
        }
    }

    #[test]
    fn matrix_missing_directory_fails_for_lock_driven_sources() {
        for source in ["upstream", "vendor"] {
            let (_tmp, report) = matrix_case(source, EntryState::MissingDirectory);
            assert!(
                report.has_failures,
                "{source}: a missing directory must fail, got:\n{}",
                report.report
            );
            assert!(
                report.report.contains("[FAIL] fixture-skill"),
                "{source}: the failed entry must keep its identity, got:\n{}",
                report.report
            );
            assert!(
                report
                    .report
                    .contains(&format!("{source} skill directory not found")),
                "{source}: the failure must name the missing directory, got:\n{}",
                report.report
            );
        }
    }

    #[test]
    fn matrix_missing_skill_md_fails_for_every_source() {
        for (source, missing_file) in [
            ("autopilot", "skills/autopilot/fixture-skill/SKILL.md"),
            (
                "upstream",
                "skills/upstream/skills/engineering/fixture-skill/SKILL.md",
            ),
            ("vendor", "skills/vendor/fixture-skill/SKILL.md"),
        ] {
            let (_tmp, report) = matrix_case(source, EntryState::MissingSkillMd);
            assert!(
                report.has_failures,
                "{source}: a resolved entry without SKILL.md must fail, got:\n{}",
                report.report
            );
            assert!(
                report.report.contains("[FAIL] fixture-skill"),
                "{source}: the failed entry must keep its identity, got:\n{}",
                report.report
            );
            assert!(
                report.report.contains(missing_file),
                "{source}: the failure must name the missing file {missing_file}, got:\n{}",
                report.report
            );
        }
    }

    #[test]
    fn matrix_malformed_skill_path_fails_for_upstream() {
        let (_tmp, report) = matrix_case("upstream", EntryState::MalformedSkillPath);
        assert!(
            report.has_failures,
            "a malformed skillPath must fail, got:\n{}",
            report.report
        );
        assert!(
            report.report.contains("malformed skillPath"),
            "the failure must name the malformed skillPath, got:\n{}",
            report.report
        );
    }

    // ── Variant expansion tests (temp fixtures) ─────────────────────────

    /// Build a temp fixture with one autopilot skill carrying the given
    /// variant directories and a root SKILL.md, then expand it through the
    /// public interface.
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

        expand_skills(root).expect("expand_skills should succeed")
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
    fn codex_variant_with_agent_toml_yields_no_skill_entry() {
        // classify_skill only returns variants whose directories exist.
        // A codex variant may carry an agent.toml custom-agent definition
        // instead of a SKILL.md, so it is the one variant shape that is not
        // expected to carry a skill file.
        let tmp = tempfile::tempdir().unwrap();
        let codex_dir = tmp.path().join("skills/autopilot/fixture-skill/codex");
        fs::create_dir_all(&codex_dir).unwrap();
        fs::write(codex_dir.join("agent.toml"), "name = \"fixture\"\n").unwrap();

        // classify_skill returns variants based on directory existence
        let (_type, variants, _codex_agent) =
            skill_index::classify_skill(&tmp.path().join("skills/autopilot/fixture-skill"));
        // codex dir exists, so it's in variants
        assert!(variants.contains(&"codex".to_string()));

        // When expanding, the agent.toml-only codex variant produces no entry
        // (check_codex_status reports it as INFO), but the missing root
        // fallback SKILL.md is a named failure — the entry must not vanish.
        let skills = expand_skills(tmp.path()).expect("expand_skills should succeed");
        assert!(
            !skills.iter().any(|s| s.variant.as_deref() == Some("codex")),
            "an agent.toml-only codex variant must not produce a Skill entry, got: {:?}",
            skills.iter().map(|s| &s.relative_path).collect::<Vec<_>>()
        );
        let failed: Vec<&Skill> = skills
            .iter()
            .filter(|s| s.failed_reason.is_some())
            .collect();
        assert_eq!(
            failed.len(),
            1,
            "the missing root SKILL.md must yield exactly one failed entry, got: {:?}",
            skills.iter().map(|s| &s.relative_path).collect::<Vec<_>>()
        );
        assert!(
            failed[0]
                .failed_reason
                .as_deref()
                .is_some_and(|reason| reason.contains("skills/autopilot/fixture-skill/SKILL.md")),
            "the failed entry must name the missing root skill file, got: {:?}",
            failed[0].failed_reason
        );
    }

    #[test]
    fn variant_without_skill_md_fails_instead_of_being_dropped() {
        // A variant directory that carries no SKILL.md and no agent.toml
        // alternative is an incomplete variant, not a silently ignored one.
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let skill_dir = root.join("skills/autopilot/fixture-skill");
        fs::create_dir_all(skill_dir.join("reasonix")).unwrap();
        write_skill_md(&skill_dir);

        let report = run_validation(root).expect("run_validation should succeed");
        assert!(
            report.has_failures,
            "a variant without SKILL.md must fail validation, got:\n{}",
            report.report
        );
        assert!(
            report.report.contains("[PASS] fixture-skill\n"),
            "the root fallback keeps passing, got:\n{}",
            report.report
        );
        assert!(
            report.report.contains("[FAIL] fixture-skill (reasonix)"),
            "the failure must name the variant, got:\n{}",
            report.report
        );
        assert!(
            report
                .report
                .contains("skills/autopilot/fixture-skill/reasonix/SKILL.md"),
            "the failure must name the missing variant file, got:\n{}",
            report.report
        );
    }

    #[test]
    fn codex_agent_toml_variant_keeps_validation_green() {
        // The repo's own install model: a codex variant may ship an
        // agent.toml instead of a SKILL.md. That stays INFO, not FAIL.
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let skill_dir = root.join("skills/autopilot/fixture-skill");
        fs::create_dir_all(skill_dir.join("codex")).unwrap();
        write_skill_md(&skill_dir);
        fs::write(
            skill_dir.join("codex").join("agent.toml"),
            "name = \"fixture\"\n",
        )
        .unwrap();

        let report = run_validation(root).expect("run_validation should succeed");
        assert!(
            !report.has_failures,
            "an agent.toml-only codex variant must not fail validation, got:\n{}",
            report.report
        );
        assert!(
            report
                .report
                .contains("[INFO] fixture-skill: no codex/SKILL.md (uses agent.toml instead)"),
            "the codex variant stays informational, got:\n{}",
            report.report
        );
    }

    #[test]
    fn codex_variant_without_skill_md_or_agent_toml_fails_validation() {
        // A codex directory carrying neither SKILL.md nor agent.toml is an
        // incomplete variant, not a placeholder that passes: the pipeline
        // FAILs it by name, and no placeholder INFO line is emitted for it.
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let skill_dir = root.join("skills/autopilot/fixture-skill");
        fs::create_dir_all(skill_dir.join("codex")).unwrap();
        write_skill_md(&skill_dir);

        let report = run_validation(root).expect("run_validation should succeed");
        assert!(
            report.has_failures,
            "a codex variant without SKILL.md or agent.toml must fail, got:\n{}",
            report.report
        );
        assert!(
            report.report.contains("[FAIL] fixture-skill (codex)"),
            "the failure must name the codex variant, got:\n{}",
            report.report
        );
        assert!(
            report
                .report
                .contains("skills/autopilot/fixture-skill/codex/SKILL.md"),
            "the failure must name the missing codex skill file, got:\n{}",
            report.report
        );
        let skills = expand_skills(root).expect("expand_skills should succeed");
        assert!(
            check_codex_status(root, &skills).is_empty(),
            "the pipeline reports the codex directory as FAIL, not as a placeholder INFO line"
        );
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
            status
                .iter()
                .any(|l| l.contains("test-skill") && l.contains("agent.toml")),
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
        // No agent.toml, no SKILL.md — just an empty codex dir.  An empty
        // skills list still reaches this line; through the validation
        // pipeline the same directory FAILs (see
        // codex_variant_without_skill_md_or_agent_toml_fails_validation).

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
