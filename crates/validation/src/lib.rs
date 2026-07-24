//! SKILL.md YAML frontmatter parser and validator.
//!
//! Pure string-in/string-out — never touches the filesystem.
//!
//! Public API:
//! - `parse_frontmatter(content)` → `Result<HashMap<String,String>, Vec<String>>`
//! - `validate_skill(content)` → `ValidationResult`

use std::collections::HashMap;

// ── Constants (mirror validate.sh) ──────────────────────────────────────────

/// OpenCode-specific fields that must NOT appear in a Reasonix SKILL.md.
const OPENCODE_FIELDS: &[&str] = &["compatibility", "mode", "permission", "hidden", "arguments"];

/// Allowed characters for the `name` field: starts with alnum, then alnum / dot / underscore / hyphen, 1-64 chars total.
fn name_is_valid(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.is_empty() || bytes.len() > 64 {
        return false;
    }
    // First char must be [a-zA-Z0-9]
    if !bytes[0].is_ascii_alphanumeric() {
        return false;
    }
    // Remaining chars: [a-zA-Z0-9._-]
    for &b in &bytes[1..] {
        if !(b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-') {
            return false;
        }
    }
    true
}

// ── Public types ────────────────────────────────────────────────────────────

/// Which runtime variant a skill targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillVariant {
    /// Reasonix runtime (rejects OpenCode-specific fields).
    Reasonix,
    /// Codex runtime (allows OpenCode-specific fields).
    Codex,
    /// Runtime-agnostic (rejects OpenCode-specific fields, same as Reasonix).
    Agnostic,
}

/// Result of `validate_skill`.
#[derive(Debug, PartialEq, Eq)]
pub struct ValidationResult {
    pub passed: bool,
    pub issues: Vec<String>,
}

// ── parse_frontmatter ───────────────────────────────────────────────────────

/// Parse YAML-like frontmatter from SKILL.md content.
///
/// Returns `Ok(fields)` on success, `Err(parse_errors)` if the delimiters are
/// malformed.
pub fn parse_frontmatter(content: &str) -> Result<HashMap<String, String>, Vec<String>> {
    let mut errors: Vec<String> = Vec::new();
    let mut fields: HashMap<String, String> = HashMap::new();

    let lines: Vec<&str> = content.lines().collect();

    // Check opening delimiter (line 0) — allow trailing \r for CRLF
    let first = lines
        .first()
        .map(|s| s.trim_end_matches('\r'))
        .unwrap_or("");
    if first != "---" {
        errors.push("Missing opening --- delimiter".to_string());
        return Err(errors);
    }

    // Find closing delimiter
    let end_idx = (1..lines.len()).find(|&i| {
        let trimmed = lines[i].trim_end_matches('\r').trim();
        trimmed == "---"
    });

    let end_idx = match end_idx {
        Some(i) => i,
        None => {
            errors.push("Missing closing --- delimiter".to_string());
            return Err(errors);
        }
    };

    // Parse key: value lines between delimiters
    let mut i = 1;
    while i < end_idx {
        let line = lines[i].trim_end_matches('\r');

        // Match key: value  (key starts with letter, then alnum/_/-, colon, optional space, value)
        if let Some((key, value)) = parse_key_value(line) {
            let mut value = value.trim().to_string();

            // Handle folded block scalar (> or >-)
            if value == ">" || value == ">-" {
                value = String::new();
                i += 1;
                while i < end_idx {
                    let cline = lines[i].trim_end_matches('\r');
                    if cline.starts_with("  ") || cline.starts_with('\t') {
                        let folded = cline.trim();
                        if value.is_empty() {
                            value = folded.to_string();
                        } else {
                            value.push(' ');
                            value.push_str(folded);
                        }
                        i += 1;
                    } else {
                        break;
                    }
                }
                i = i.saturating_sub(1); // step back; loop will increment
            }

            // Handle literal block scalar (| or |-)
            if value == "|" || value == "|-" {
                value = String::new();
                i += 1;
                while i < end_idx {
                    let cline = lines[i].trim_end_matches('\r');
                    if cline.starts_with("  ") || cline.starts_with('\t') {
                        let lit = cline.trim();
                        if value.is_empty() {
                            value = lit.to_string();
                        } else {
                            value.push('\n');
                            value.push_str(lit);
                        }
                        i += 1;
                    } else {
                        break;
                    }
                }
                i = i.saturating_sub(1);
            }

            // Store field (key is stored as-is, preserving original casing/hyphens)
            fields.insert(key.to_string(), value);
        }
        i += 1;
    }

    Ok(fields)
}

/// Try to parse a "key: value" line. Returns `Some((key, value))` on match.
fn parse_key_value(line: &str) -> Option<(&str, &str)> {
    // Key must start with [a-zA-Z], followed by [a-zA-Z0-9_-]*
    let colon_pos = line.find(':')?;
    let key = &line[..colon_pos];
    let value = &line[colon_pos + 1..];

    // Validate key format
    let key_bytes = key.as_bytes();
    if key_bytes.is_empty() || !key_bytes[0].is_ascii_alphabetic() {
        return None;
    }
    for &b in key_bytes {
        if !(b.is_ascii_alphanumeric() || b == b'_' || b == b'-') {
            return None;
        }
    }

    // Trim optional leading space from value (colon may be followed by space)
    let value = value.strip_prefix(' ').unwrap_or(value);

    Some((key, value))
}

// ── strict YAML check ───────────────────────────────────────────────────────

/// Extract the raw frontmatter block (between the `---` delimiters).
/// Returns `None` when the delimiters are missing or malformed.
fn frontmatter_block(content: &str) -> Option<String> {
    let lines: Vec<&str> = content.lines().collect();
    let first = lines.first().map(|s| s.trim_end_matches('\r'))?;
    if first != "---" {
        return None;
    }
    let end_idx = (1..lines.len()).find(|&i| lines[i].trim_end_matches('\r').trim() == "---")?;
    Some(lines[1..end_idx].join("\n"))
}

/// Parse the frontmatter block with a strict YAML parser. Returns an issue
/// list — empty when the block is valid YAML. The hand-rolled line parser in
/// `parse_frontmatter` is intentionally lenient; this gate catches frontmatter
/// that strict parsers (e.g. Kimi Code's skill loader) would reject, such as
/// unquoted `: ` inside a plain scalar.
fn strict_yaml_issues(content: &str) -> Vec<String> {
    let block = match frontmatter_block(content) {
        Some(b) => b,
        // Delimiter problems are reported by parse_frontmatter already.
        None => return Vec::new(),
    };
    match serde_yaml::from_str::<serde_yaml::Value>(&block) {
        Ok(_) => Vec::new(),
        Err(e) => vec![format!("Invalid YAML frontmatter: {e}")],
    }
}

// ── validate_skill ──────────────────────────────────────────────────────────

/// Validate SKILL.md frontmatter content (Reasonix-compatible default).
///
/// Runs `parse_frontmatter` then applies 5 validation checks.
/// Equivalent to `validate_skill_with_variant(content, SkillVariant::Reasonix)`.
pub fn validate_skill(content: &str) -> ValidationResult {
    validate_skill_with_variant(content, SkillVariant::Reasonix)
}

/// Validate SKILL.md frontmatter content for a specific runtime variant.
///
/// Codex variants skip the OpenCode-specific field check (Check 3).
/// Reasonix and Agnostic variants include it.
pub fn validate_skill_with_variant(content: &str, variant: SkillVariant) -> ValidationResult {
    let mut issues: Vec<String> = Vec::new();

    // Parse first
    let fields = match parse_frontmatter(content) {
        Ok(f) => f,
        Err(parse_errors) => {
            return ValidationResult {
                passed: false,
                issues: parse_errors,
            };
        }
    };

    // Check 0: frontmatter must be valid YAML (strict parsers reject skills
    // whose frontmatter only the lenient line parser accepts)
    issues.extend(strict_yaml_issues(content));

    // Check 1: Required fields
    if fields.get("name").is_none_or(|v| v.is_empty()) {
        issues.push("Missing required field: name".to_string());
    }
    if fields.get("description").is_none_or(|v| v.is_empty()) {
        issues.push("Missing required field: description".to_string());
    }

    // Check 2: Name format
    if let Some(name) = fields.get("name") {
        if !name.is_empty() && !name_is_valid(name) {
            issues.push(format!(
                "Name \"{name}\" does not match pattern ^[a-zA-Z0-9][a-zA-Z0-9._-]{{0,63}}$"
            ));
        }
    }

    // Check 3: No opencode fields (skip for Codex variants)
    if variant != SkillVariant::Codex {
        for &field in OPENCODE_FIELDS {
            if let Some(val) = fields.get(field) {
                if !val.is_empty() {
                    issues.push(format!("OpenCode-specific field present: {field}"));
                }
            }
        }
    }

    // Check 4: runAs valid
    if let Some(run_as) = fields.get("runAs") {
        if !run_as.is_empty() && run_as != "inline" && run_as != "subagent" {
            issues.push(format!(
                "Invalid runAs value \"{run_as}\" — must be \"inline\" or \"subagent\""
            ));
        }
    }

    // Check 5: allowed-tools for subagents
    if fields.get("runAs").is_some_and(|v| v == "subagent")
        && fields.get("allowed-tools").is_none_or(|v| v.is_empty())
    {
        issues.push("runAs is \"subagent\" but allowed-tools is not defined".to_string());
    }

    let passed = issues.is_empty();
    ValidationResult { passed, issues }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Helpers ─────────────────────────────────────────────────────────

    /// Assert that validation passes for the given content.
    fn assert_pass(content: &str) -> ValidationResult {
        let result = validate_skill(content);
        assert!(
            result.passed,
            "expected pass but got issues: {:?}",
            result.issues
        );
        result
    }

    /// Assert that validation fails and issues contain the expected substring.
    fn assert_fail(content: &str, expected_substr: &str) -> ValidationResult {
        let result = validate_skill(content);
        assert!(
            !result.passed,
            "expected fail but got pass (issues: {:?})",
            result.issues
        );
        let found = result
            .issues
            .iter()
            .any(|issue| issue.contains(expected_substr));
        assert!(
            found,
            "expected issue containing \"{}\" but got: {:?}",
            expected_substr, result.issues
        );
        result
    }

    // ── Test cases (ported from validation/validate.test.sh) ────────────

    // Check 1: Required fields

    #[test]
    fn fails_when_name_is_missing() {
        assert_fail(
            "---
description: A test skill
---
# Test",
            "name",
        );
    }

    #[test]
    fn fails_when_description_is_missing() {
        assert_fail(
            "---
name: test-skill
---
# Test",
            "description",
        );
    }

    #[test]
    fn passes_with_valid_minimal_frontmatter() {
        assert_pass(
            "---
name: my-skill
description: Does something useful.
---
# My Skill",
        );
    }

    // Check 2: Name format

    #[test]
    fn fails_when_name_starts_with_non_alphanumeric() {
        assert_fail(
            "---
name: _bad-name
description: A test
---
# Test",
            "Name",
        );
    }

    #[test]
    fn fails_when_name_exceeds_64_characters() {
        assert_fail(
            "---
name: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
description: A test
---
# Test",
            "Name",
        );
    }

    #[test]
    fn accepts_valid_name_with_dots_and_hyphens() {
        assert_pass(
            "---
name: my-skill.v2_test
description: A test
---
# Test",
        );
    }

    // Check 3: No opencode fields

    #[test]
    fn accepts_disable_model_invocation_valid_reasonix_field() {
        assert_pass(
            "---
name: test-skill
description: A test
disable-model-invocation: true
---
# Test",
        );
    }

    #[test]
    fn fails_when_compatibility_is_present() {
        assert_fail(
            "---
name: test-skill
description: A test
compatibility: \">=1.0\"
---
# Test",
            "compatibility",
        );
    }

    #[test]
    fn fails_when_multiple_opencode_fields_are_present() {
        assert_fail(
            "---
name: test-skill
description: A test
mode: chat
hidden: true
---
# Test",
            "mode",
        );
    }

    // Check 4: runAs valid

    #[test]
    fn accepts_runas_inline() {
        assert_pass(
            "---
name: test-skill
description: A test
runAs: inline
---
# Test",
        );
    }

    #[test]
    fn accepts_runas_subagent_with_allowed_tools() {
        assert_pass(
            "---
name: test-skill
description: A test
runAs: subagent
allowed-tools: read, write
---
# Test",
        );
    }

    #[test]
    fn fails_when_runas_has_invalid_value() {
        assert_fail(
            "---
name: test-skill
description: A test
runAs: agent
---
# Test",
            "runAs",
        );
    }

    // Check 5: allowed-tools for subagents

    #[test]
    fn fails_when_runas_is_subagent_but_allowed_tools_missing() {
        assert_fail(
            "---
name: test-skill
description: A test
runAs: subagent
---
# Test",
            "allowed-tools",
        );
    }

    #[test]
    fn accepts_subagent_with_todo_allowed_tools() {
        assert_pass(
            "---
name: test-skill
description: A test
runAs: subagent
allowed-tools: TODO
---
# Test",
        );
    }

    // Check 6: Frontmatter well-formed

    #[test]
    fn fails_when_no_opening_delimiter() {
        assert_fail(
            "name: test-skill
description: A test
---
# Test",
            "opening",
        );
    }

    #[test]
    fn fails_when_no_closing_delimiter() {
        assert_fail(
            "---
name: test-skill
description: A test
# Test",
            "closing",
        );
    }

    // Complex cases

    #[test]
    fn reports_multiple_issues_at_once() {
        let result = assert_fail(
            "---
name: _bad-name
compatibility: \">1.0\"
runAs: agent
---
# Test",
            "description",
        );
        // Should report multiple issues (at least description + name + compatibility + runAs)
        assert!(
            result.issues.len() >= 2,
            "expected multiple issues, got: {:?}",
            result.issues
        );
    }

    // ── Variant-aware validation tests ─────────────────────────────────

    #[test]
    fn codex_variant_allows_opencode_fields() {
        // Codex SKILL.md may legitimately contain compatibility, mode, etc.
        let content = "---
name: test-skill
description: A test
compatibility: \">=1.0\"
mode: chat
disable-model-invocation: true
---
# Test";
        let result = validate_skill_with_variant(content, SkillVariant::Codex);
        assert!(
            result.passed,
            "codex variant should allow opencode fields, got: {:?}",
            result.issues
        );
    }

    #[test]
    fn reasonix_variant_rejects_opencode_fields() {
        let content = "---
name: test-skill
description: A test
compatibility: \">=1.0\"
---
# Test";
        let result = validate_skill_with_variant(content, SkillVariant::Reasonix);
        assert!(
            !result.passed,
            "reasonix variant should reject opencode fields"
        );
        assert!(
            result.issues.iter().any(|i| i.contains("compatibility")),
            "should report compatibility as opencode field"
        );
    }

    #[test]
    fn agnostic_variant_rejects_opencode_fields() {
        let content = "---
name: test-skill
description: A test
hidden: true
---
# Test";
        let result = validate_skill_with_variant(content, SkillVariant::Agnostic);
        assert!(
            !result.passed,
            "agnostic variant should reject opencode fields"
        );
        assert!(
            result.issues.iter().any(|i| i.contains("hidden")),
            "should report hidden as opencode field"
        );
    }

    #[test]
    fn default_validate_skill_still_rejects_opencode_fields() {
        // Backward compatibility: validate_skill defaults to Reasonix behavior
        let result = validate_skill(
            "---
name: test-skill
description: A test
compatibility: \">=1.0\"
---
# Test",
        );
        assert!(
            !result.passed,
            "default validate_skill should reject opencode fields"
        );
    }

    // ── Strict YAML frontmatter check ─────────────────────────────────

    #[test]
    fn fails_when_description_contains_unquoted_colon() {
        // Real-world regression: autopilot-orchestrator/autopilot-reviewer were
        // silently dropped by strict YAML parsers because the plain scalar
        // contained ": " (e.g. "loop: scan → ...").
        assert_fail(
            "---
name: test-skill
description: Resolution loop: scan → implement → review. Use when processing issues.
---
# Test",
            "YAML",
        );
    }

    #[test]
    fn passes_when_colon_description_is_quoted() {
        assert_pass(
            "---
name: test-skill
description: \"Resolution loop: scan → implement → review. Use when processing issues.\"
---
# Test",
        );
    }

    #[test]
    fn fails_when_frontmatter_is_malformed_yaml() {
        assert_fail(
            "---
name: test-skill
description: A test
  bad-indent: true
---
# Test",
            "YAML",
        );
    }

    // ── Additional parse_frontmatter unit tests ────────────────────────

    #[test]
    fn parse_handles_crlf_line_endings() {
        let content = "---\r\nname: test-skill\r\ndescription: A test\r\n---\r\n# Body";
        let fields = parse_frontmatter(content).unwrap();
        assert_eq!(fields.get("name").unwrap(), "test-skill");
        assert_eq!(fields.get("description").unwrap(), "A test");
    }

    #[test]
    fn parse_handles_folded_block_scalar() {
        let content = "---
name: test-skill
description: >
  This is a long
  description that spans
  multiple lines.
---
# Body";
        let fields = parse_frontmatter(content).unwrap();
        assert_eq!(
            fields.get("description").unwrap(),
            "This is a long description that spans multiple lines."
        );
    }

    #[test]
    fn parse_handles_literal_block_scalar() {
        let content = "---
name: test-skill
description: |
  Line one
  Line two
  Line three
---
# Body";
        let fields = parse_frontmatter(content).unwrap();
        assert_eq!(
            fields.get("description").unwrap(),
            "Line one\nLine two\nLine three"
        );
    }
}
