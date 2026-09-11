//! Validation runner — discovers skills, validates frontmatter, and generates reports.
//!
//! Migrated from `validation/run.rs`.  Consumes `skill_index::discover_skills()`
//! — the Expected-set enumerator — for every skill source (ADR 0044, ADR 0045).
//! Validation targets are the Skill files the enumerator resolved, so a missing
//! file fails by name instead of dropping out of the report.
//!
//! Public API:
//! - `run_validation(project_root)` → `Result<ValidationReport>`
//! - `build_validation_targets(project_root)` → `Result<Vec<ValidationTarget>>`
//! - `validate_all(project_root, targets)` → `Vec<(ValidationTarget, SkillResult)>`
//! - `generate_report(validated, project_root)` → `String`
//! - `check_codex_status(entries)` → `Vec<String>`

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

/// One validation target: a Skill file the enumerator resolved, together with
/// the Expected-set entry that owns it.
///
/// The entry is owned here so a report row carries identity and result
/// together — there is no positional pairing between two lists (ADR 0045).
#[derive(Debug, Clone)]
pub struct ValidationTarget {
    /// The Expected-set entry that owns this file.
    pub entry: skill_index::ExpectedSetEntry,
    /// The skill file under validation (expected path, even when missing).
    pub file: skill_index::SkillFile,
    /// Project-relative path used for display and file access.
    pub relative_path: String,
}

impl ValidationTarget {
    pub fn name(&self) -> &str {
        &self.entry.name
    }

    /// "upstream", "vendor", or "autopilot".
    pub fn source(&self) -> &str {
        &self.entry.source
    }

    /// Runtime variant: None for the root fallback, Some("reasonix") etc.
    pub fn variant(&self) -> Option<&str> {
        self.file.variant.as_deref()
    }
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

/// One validated target: a validation target together with its result.
pub type ValidatedSkill<'a> = (&'a ValidationTarget, &'a SkillResult);

// ── Validation targets: Expected-set Skill files ───────────────────────────

/// Build the validation targets for a project: one per Skill file the
/// Expected-set entries own, in enumeration order.
///
/// The enumerator already resolved which files each entry owns and which of
/// them are missing (ADR 0045), so this is a projection — no layout is
/// rebuilt here, and no target can be dropped.
pub fn build_validation_targets(
    project_root: &Path,
) -> Result<Vec<ValidationTarget>, anyhow::Error> {
    let entries = skill_index::discover_skills(project_root)?;
    let mut targets: Vec<ValidationTarget> = Vec::new();

    for entry in entries {
        for file in &entry.skill_files {
            targets.push(ValidationTarget {
                entry: entry.clone(),
                relative_path: relative_skill_path(project_root, &file.path),
                file: file.clone(),
            });
        }
    }

    Ok(targets)
}

/// Project-relative display path for a Skill file; falls back to the absolute
/// path when the file lives outside the project root.
fn relative_skill_path(project_root: &Path, path: &Path) -> String {
    path.strip_prefix(project_root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string()
}

// ── Batch validation ───────────────────────────────────────────────────────

/// Validate every target against its file content.
///
/// A target whose skill file is missing fails immediately with the
/// enumerator's reason; everything else is read and validated from disk.
/// Each result stays attached to the target it came from.
pub fn validate_all(
    project_root: &Path,
    targets: &[ValidationTarget],
) -> Vec<(ValidationTarget, SkillResult)> {
    targets
        .iter()
        .map(|target| {
            let result = match &target.file.resolution {
                skill_index::ResolutionStatus::Missing { reason } => SkillResult {
                    result: ValidationResult {
                        passed: false,
                        issues: vec![reason.clone()],
                    },
                    frontmatter: None,
                },
                skill_index::ResolutionStatus::Resolved => {
                    let full_path = project_root.join(&target.relative_path);
                    let content = match fs::read_to_string(&full_path) {
                        Ok(c) => c,
                        Err(_) => {
                            return (
                                target.clone(),
                                SkillResult {
                                    result: ValidationResult {
                                        passed: false,
                                        issues: vec![format!(
                                            "File not found: {}",
                                            full_path.display()
                                        )],
                                    },
                                    frontmatter: None,
                                },
                            );
                        }
                    };
                    // An agent definition is not a SKILL.md: it has no
                    // frontmatter contract to validate, so it counts as
                    // resolved for the report without being parsed.
                    let validation_result =
                        if target.file.kind == skill_index::SkillFileKind::AgentDefinition {
                            ValidationResult {
                                passed: true,
                                issues: vec![],
                            }
                        } else {
                            let variant = match target.variant() {
                                Some("reasonix") => SkillVariant::Reasonix,
                                Some("codex") => SkillVariant::Codex,
                                Some("kimi") => SkillVariant::Kimi,
                                _ => SkillVariant::Agnostic,
                            };
                            validate_skill_with_variant(&content, variant)
                        };
                    let frontmatter = if target.source() == "autopilot" {
                        parse_frontmatter(&content).ok()
                    } else {
                        None
                    };
                    SkillResult {
                        result: validation_result,
                        frontmatter,
                    }
                }
            };
            (target.clone(), result)
        })
        .collect()
}

// ── Report generation ──────────────────────────────────────────────────────

/// Generate the full human-readable validation report.
pub fn generate_report(validated: &[ValidatedSkill], project_root: Option<&Path>) -> String {
    let sep = "=".repeat(70);
    let date_str = Utc::now().format("%Y-%m-%dT%H:%M:%S.000Z").to_string();

    let total = validated.len();
    let pass_count = validated.iter().filter(|(_, r)| r.result.passed).count();
    let fail_count = total - pass_count;

    let (upstream_total, upstream_pass, upstream_fail) = count_by_source(validated, "upstream");
    let (autopilot_total, autopilot_pass, autopilot_fail) = count_by_source(validated, "autopilot");
    let (vendor_total, vendor_pass, vendor_fail) = count_by_source(validated, "vendor");

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
    write_skill_entries(&mut report, validated, "upstream", true, project_root);

    // ── Vendor section ──
    wln!(report, "--- Vendor Skills ({}) ---", vendor_total);
    wln!(report, "Passed: {} / Failed: {}", vendor_pass, vendor_fail);
    wln!(report);
    write_skill_entries(&mut report, validated, "vendor", true, project_root);

    // ── Autopilot section ──
    wln!(report, "--- Autopilot Skills ({}) ---", autopilot_total);
    wln!(
        report,
        "Passed: {} / Failed: {}",
        autopilot_pass,
        autopilot_fail
    );
    wln!(report);
    write_skill_entries(&mut report, validated, "autopilot", false, project_root);

    // ── Codex variant status ──
    {
        let entries: Vec<skill_index::ExpectedSetEntry> = validated
            .iter()
            .map(|(target, _)| target.entry.clone())
            .collect();
        let codex_status = check_codex_status(&entries);
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
    let oc_count: usize = validated
        .iter()
        .filter(|(target, _)| {
            let v = target.variant();
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
    let non_codex_count = validated
        .iter()
        .filter(|(target, _)| {
            let v = target.variant();
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
    let sub_missing = find_subagent_missing_allowed_tools(validated);
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
fn count_by_source(validated: &[ValidatedSkill], source: &str) -> (usize, usize, usize) {
    let mut total = 0;
    let mut pass = 0;
    let mut fail = 0;
    for (target, result) in validated {
        if target.source() != source {
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
    validated: &[ValidatedSkill],
    source: &str,
    show_checkmark: bool,
    project_root: Option<&Path>,
) {
    for (target, result) in validated {
        if target.source() != source {
            continue;
        }
        // Build display label: name + optional variant tag
        let display_name = match target.variant() {
            Some(variant) => format!("{} ({})", target.name(), variant),
            None => target.name().to_string(),
        };
        // Path display: use project_root if provided, else relative path
        let path_display = match project_root {
            Some(root) => root.join(&target.relative_path).display().to_string(),
            None => target.relative_path.clone(),
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
/// Reads the frontmatter captured during validation — the same bytes the
/// validation verdict was produced from — instead of re-reading files.
fn find_subagent_missing_allowed_tools(validated: &[ValidatedSkill]) -> Vec<String> {
    let mut missing = Vec::new();
    for (target, result) in validated {
        if let Some(fm) = &result.frontmatter {
            if fm.get("runAs").is_some_and(|v| v == "subagent")
                && fm.get("allowed-tools").is_none_or(|v| v.is_empty())
            {
                missing.push(target.name().to_string());
            }
        }
    }
    missing
}

// ── Codex status check ─────────────────────────────────────────────────────

/// Project the codex variant status from enumerated Expected-set entries.
///
/// A codex variant that ships an agent definition rather than a `SKILL.md`
/// is an informational note; a variant that ships neither is a missing skill
/// file, which validation already FAILs, so no placeholder line is emitted.
pub fn check_codex_status(entries: &[skill_index::ExpectedSetEntry]) -> Vec<String> {
    let mut lines: Vec<String> = entries
        .iter()
        .filter_map(|entry| {
            let file = entry.skill_file(Some("codex"))?;
            (file.kind == skill_index::SkillFileKind::AgentDefinition).then(|| {
                format!(
                    "[INFO] {}: no codex/SKILL.md (uses agent.toml instead)",
                    entry.name
                )
            })
        })
        .collect();
    lines.sort();
    lines.dedup();
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
    let targets = build_validation_targets(project_root)?;
    let validated = validate_all(project_root, &targets);
    let pairs: Vec<ValidatedSkill> = validated
        .iter()
        .map(|(target, result)| (target, result))
        .collect();
    let report = generate_report(&pairs, Some(project_root));
    let has_failures = validated.iter().any(|(_, result)| !result.result.passed);
    Ok(ValidationReport {
        report,
        has_failures,
    })
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

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

    /// A synthetic Expected-set entry owning one resolved root skill file.
    fn test_entry(name: &str, source: &str) -> skill_index::ExpectedSetEntry {
        let relative_dir = match source {
            "upstream" => format!("skills/upstream/skills/engineering/{name}"),
            other => format!("skills/{other}/{name}"),
        };
        skill_index::ExpectedSetEntry {
            name: name.to_string(),
            source: source.to_string(),
            skill_type: skill_index::SkillType::Agnostic,
            variants: vec![],
            codex_agent: false,
            source_dir: PathBuf::from(&relative_dir),
            resolution: skill_index::ResolutionStatus::Resolved,
            skill_files: vec![skill_index::SkillFile {
                variant: None,
                path: PathBuf::from(format!("{relative_dir}/SKILL.md")),
                kind: skill_index::SkillFileKind::Skill,
                resolution: skill_index::ResolutionStatus::Resolved,
            }],
        }
    }

    /// A synthetic Expected-set entry carrying one variant skill file.
    fn test_variant_entry(
        name: &str,
        source: &str,
        variant: &str,
    ) -> skill_index::ExpectedSetEntry {
        let mut entry = test_entry(name, source);
        let rel = format!("{}/{variant}/SKILL.md", entry.source_dir.display());
        entry.variants = vec![variant.to_string()];
        entry.skill_files.push(skill_index::SkillFile {
            variant: Some(variant.to_string()),
            path: PathBuf::from(&rel),
            kind: skill_index::SkillFileKind::Skill,
            resolution: skill_index::ResolutionStatus::Resolved,
        });
        entry
    }

    /// A synthetic validation target for a report test.
    fn test_target(name: &str, source: &str) -> ValidationTarget {
        test_target_with_variant(name, source, None)
    }

    fn test_target_with_variant(
        name: &str,
        source: &str,
        variant: Option<&str>,
    ) -> ValidationTarget {
        let entry = match variant {
            Some(variant) => test_variant_entry(name, source, variant),
            None => test_entry(name, source),
        };
        let file = match variant {
            Some(variant) => entry.skill_file(Some(variant)).unwrap().clone(),
            None => entry.skill_file(None).unwrap().clone(),
        };
        ValidationTarget {
            relative_path: file.path.to_string_lossy().to_string(),
            entry,
            file,
        }
    }

    /// Build (target, result) pairs straight from a synthetic target list.
    fn pairs_for(
        targets: &[ValidationTarget],
        results: Vec<SkillResult>,
    ) -> Vec<(ValidationTarget, SkillResult)> {
        targets.iter().cloned().zip(results).collect()
    }

    /// Render a report from owned pairs.
    fn render(validated: &[(ValidationTarget, SkillResult)]) -> String {
        let pairs: Vec<ValidatedSkill> = validated
            .iter()
            .map(|(target, result)| (target, result))
            .collect();
        generate_report(&pairs, None)
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
        let targets = vec![test_target("my-skill", "upstream")];
        let pairs = pairs_for(&targets, vec![pass_result()]);
        let report = render(&pairs);
        assert!(report.contains("FRONTMATTER VALIDATION REPORT — reasonix compatibility"));
        assert!(report.contains("=".repeat(70).as_str()));
        assert!(report.contains("Date: "));
    }

    #[test]
    fn report_shows_total_pass_fail_counts() {
        let targets = vec![
            test_target("pass-1", "upstream"),
            test_target("fail-1", "upstream"),
            test_target("pass-2", "autopilot"),
        ];
        let pairs = pairs_for(
            &targets,
            vec![
                pass_result(),
                fail_result("missing description"),
                pass_result(),
            ],
        );
        let report = render(&pairs);
        assert!(report.contains("Total skills validated: 3 | Passed: 2 | Failed: 1"));
    }

    #[test]
    fn report_passing_skill_shows_pass_label() {
        let targets = vec![test_target("good-skill", "upstream")];
        let pairs = pairs_for(&targets, vec![pass_result()]);
        let report = render(&pairs);
        assert!(report.contains("[PASS] good-skill"));
    }

    #[test]
    fn report_failing_skill_shows_fail_label_and_issues() {
        let targets = vec![test_target("bad-skill", "upstream")];
        let pairs = pairs_for(&targets, vec![fail_result("Missing required field: name")]);
        let report = render(&pairs);
        assert!(report.contains("[FAIL] bad-skill"));
        assert!(report.contains("Missing required field: name"));
    }

    #[test]
    fn report_all_pass_shows_overall_pass() {
        let targets = vec![test_target("s1", "upstream")];
        let pairs = pairs_for(&targets, vec![pass_result()]);
        let report = render(&pairs);
        assert!(report.contains("All skills PASS validation."));
    }

    #[test]
    fn report_any_fail_shows_overall_fail_count() {
        let targets = vec![test_target("s1", "upstream"), test_target("s2", "upstream")];
        let pairs = pairs_for(&targets, vec![pass_result(), fail_result("issue")]);
        let report = render(&pairs);
        assert!(report.contains("1 skill(s) FAIL validation."));
    }

    #[test]
    fn report_shows_upstream_vendor_and_autopilot_sections() {
        let targets = vec![
            test_target("up-skill", "upstream"),
            test_target("vendor-skill", "vendor"),
            test_target("auto-skill", "autopilot"),
        ];
        let pairs = pairs_for(&targets, vec![pass_result(), pass_result(), pass_result()]);
        let report = render(&pairs);
        assert!(report.contains("Upstream Skills"));
        assert!(report.contains("Vendor Skills"));
        assert!(report.contains("Autopilot Skills"));
    }

    #[test]
    fn report_includes_global_checks_section() {
        let targets = vec![test_target("s1", "upstream")];
        let pairs = pairs_for(&targets, vec![pass_result()]);
        let report = render(&pairs);
        assert!(report.contains("GLOBAL CHECKS"));
        assert!(report.contains("opencode-specific fields"));
        assert!(report.contains("subagent skills have allowed-tools"));
    }

    #[test]
    fn report_trailing_newline_matches_bash_output_convention() {
        let targets = vec![test_target("s1", "upstream")];
        let pairs = pairs_for(&targets, vec![pass_result()]);
        let report = render(&pairs);
        assert!(!report.is_empty(), "report must not be empty");
        assert!(
            report.ends_with('\n'),
            "report should end with newline (last line's \\n)"
        );
    }

    // ── Report variant tests ────────────────────────────────────────────

    #[test]
    fn report_shows_variant_tag_in_skill_name() {
        let targets = vec![test_target_with_variant(
            "my-skill",
            "autopilot",
            Some("reasonix"),
        )];
        let pairs = pairs_for(&targets, vec![pass_result()]);
        let report = render(&pairs);
        assert!(report.contains("[PASS] my-skill (reasonix)"));
    }

    #[test]
    fn report_codex_variant_not_counted_in_opencode_global_check() {
        let targets = vec![test_target_with_variant(
            "my-skill",
            "autopilot",
            Some("codex"),
        )];
        let pairs = pairs_for(&targets, vec![pass_result()]);
        let report = render(&pairs);
        assert!(report.contains("✓ PASS"));
    }

    #[test]
    fn report_shows_non_codex_count_in_global_check() {
        let targets = vec![
            test_target_with_variant("reasonix-skill", "autopilot", Some("reasonix")),
            test_target_with_variant("codex-skill", "autopilot", Some("codex")),
            test_target_with_variant("kimi-skill", "autopilot", Some("kimi")),
        ];
        let pairs = pairs_for(&targets, vec![pass_result(), pass_result(), pass_result()]);
        let report = render(&pairs);
        assert!(
            report.contains("1 non-codex/kimi"),
            "global check should show 1 non-codex/kimi, got:\n{}",
            report
        );
    }

    // ── build_validation_targets (integration with real repo) ───────────

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
    fn validation_targets_cover_all_sources() {
        let root = repo_root();
        let targets =
            build_validation_targets(root).expect("build_validation_targets should succeed");
        assert!(!targets.is_empty(), "should find at least some targets");

        let has_upstream = targets.iter().any(|t| t.source() == "upstream");
        let has_vendor = targets.iter().any(|t| t.source() == "vendor");
        let has_autopilot = targets.iter().any(|t| t.source() == "autopilot");
        assert!(has_upstream, "should find upstream skills");
        assert!(has_vendor, "should find vendor skills");
        assert!(has_autopilot, "should find autopilot skills");

        let show_me = targets
            .iter()
            .find(|t| t.name() == "show-me")
            .expect("show-me should be discovered as a vendor skill");
        assert_eq!(show_me.source(), "vendor");
    }

    #[test]
    fn validation_targets_have_resolved_relative_paths() {
        let root = repo_root();
        let targets =
            build_validation_targets(root).expect("build_validation_targets should succeed");
        for target in &targets {
            let full_path = root.join(&target.relative_path);
            assert!(
                full_path.exists(),
                "skill file '{}' path '{}' must exist at {:?}",
                target.name(),
                target.relative_path,
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
    /// variant directories and a root SKILL.md, then enumerate the validation
    /// targets through the public interface.
    fn targets_with_variants(variant_dirs: &[&str]) -> Vec<ValidationTarget> {
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

        build_validation_targets(root).expect("build_validation_targets should succeed")
    }

    #[test]
    fn discovers_kimi_variant() {
        let targets = targets_with_variants(&["kimi"]);
        assert!(
            targets
                .iter()
                .any(|t| t.name() == "fixture-skill" && t.variant() == Some("kimi")),
            "should discover kimi variant, got: {:?}",
            targets.iter().map(|t| &t.relative_path).collect::<Vec<_>>()
        );
    }

    #[test]
    fn codex_variant_with_agent_toml_yields_no_skill_entry() {
        // A codex variant that ships an agent.toml is an agent definition,
        // not a skill file under validation; the missing root SKILL.md is
        // still a named failure.
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("skills/autopilot/fixture-skill");
        let codex_dir = skill_dir.join("codex");
        fs::create_dir_all(&codex_dir).unwrap();
        fs::write(codex_dir.join("agent.toml"), "name = \"fixture\"\n").unwrap();

        let targets =
            build_validation_targets(tmp.path()).expect("build_validation_targets should succeed");
        let codex_target = targets
            .iter()
            .find(|t| t.variant() == Some("codex"))
            .expect("the agent definition is a target of its own kind");
        assert_eq!(
            codex_target.file.kind,
            skill_index::SkillFileKind::AgentDefinition,
            "an agent.toml-only codex variant must be an agent definition"
        );
        let failed: Vec<&ValidationTarget> = targets
            .iter()
            .filter(|t| {
                matches!(
                    t.file.resolution,
                    skill_index::ResolutionStatus::Missing { .. }
                )
            })
            .collect();
        assert_eq!(
            failed.len(),
            1,
            "the missing root SKILL.md must yield exactly one failed entry, got: {:?}",
            targets.iter().map(|t| &t.relative_path).collect::<Vec<_>>()
        );
        assert!(
            failed[0]
                .relative_path
                .contains("skills/autopilot/fixture-skill/SKILL.md"),
            "the failed target must name the missing root skill file, got: {:?}",
            failed[0].relative_path
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
        let targets =
            build_validation_targets(root).expect("build_validation_targets should succeed");
        let entries: Vec<skill_index::ExpectedSetEntry> =
            targets.iter().map(|t| t.entry.clone()).collect();
        assert!(
            check_codex_status(&entries).is_empty(),
            "the pipeline reports the codex directory as FAIL, not as a placeholder INFO line"
        );
    }

    #[test]
    fn discovers_reasonix_variants_for_coupled_skills() {
        let root = repo_root();
        let targets =
            build_validation_targets(root).expect("build_validation_targets should succeed");
        let coupled_names = [
            "audit-autopilot",
            "autopilot-implementer",
            "autopilot-orchestrator",
            "autopilot-reviewer",
        ];
        for name in coupled_names {
            let found = targets
                .iter()
                .any(|t| t.name() == name && t.variant() == Some("reasonix"));
            assert!(found, "should discover reasonix variant for {}", name);
        }
    }

    #[test]
    fn variant_skills_use_correct_relative_path() {
        let root = repo_root();
        let targets =
            build_validation_targets(root).expect("build_validation_targets should succeed");
        let orchestrator = targets
            .iter()
            .find(|t| t.name() == "autopilot-orchestrator" && t.variant() == Some("reasonix"));
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
        let targets =
            build_validation_targets(root).expect("build_validation_targets should succeed");
        let orchestrator = targets
            .iter()
            .find(|t| t.name() == "autopilot-orchestrator" && t.variant() == Some("codex"));
        assert!(
            orchestrator.is_some(),
            "should find autopilot-orchestrator codex variant"
        );
        let orch = orchestrator.unwrap();
        assert_eq!(
            orch.relative_path,
            "skills/autopilot/autopilot-orchestrator/codex/SKILL.md"
        );

        let audit = targets
            .iter()
            .find(|t| t.name() == "audit-autopilot" && t.variant() == Some("codex"));
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
        let targets =
            build_validation_targets(root).expect("build_validation_targets should succeed");
        let toolkit = targets.iter().find(|t| t.name() == "toolkit-setup");
        assert!(toolkit.is_some(), "should find toolkit-setup");
        assert_eq!(
            toolkit.unwrap().variant(),
            None,
            "toolkit-setup should have no variant"
        );
    }

    // ── Codex status tests ──────────────────────────────────────────────

    #[test]
    fn check_codex_status_reports_missing_codex() {
        let root = repo_root();
        let targets =
            build_validation_targets(root).expect("build_validation_targets should succeed");
        let entries: Vec<skill_index::ExpectedSetEntry> =
            targets.iter().map(|t| t.entry.clone()).collect();
        let status = check_codex_status(&entries);
        assert!(status
            .iter()
            .any(|l| l.contains("autopilot-implementer") && l.contains("agent.toml")));
        assert!(status
            .iter()
            .any(|l| l.contains("autopilot-reviewer") && l.contains("agent.toml")));
        assert!(
            !status
                .iter()
                .any(|l| l.contains("audit-autopilot") || l.contains("autopilot-orchestrator")),
            "skills with a codex/SKILL.md must not be reported as agent definitions, got: {:?}",
            status
        );
    }
}
