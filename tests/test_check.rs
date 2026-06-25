#!/usr/bin/env rust-script
//! ```cargo
//! [dependencies]
//! serde = { version = "1", features = ["derive"] }
//! serde_json = "1"
//! ```
//!
//! Integration tests for scripts/check.rs CLI contract.
//! Per ADR 0005: tests/*.rs rust-script files that exercise the CLI
//! via std::process::Command, asserting exit codes and output.
//!
//! Run: rust-script --test tests/test_check.rs

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn main() {
    println!("Run with: rust-script --test tests/test_check.rs");
}

// ── Helpers ─────────────────────────────────────────────────────────────

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new(prefix: &str) -> Self {
        let n = TEMP_COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("{}-{}-{}", prefix, std::process::id(), n));
        fs::create_dir_all(&dir).expect("create temp dir");
        TempDir(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Find the actual project root — the directory containing scripts/check.rs.
fn actual_project_root() -> PathBuf {
    // file!() gives "tests/test_check.rs" relative to project root
    let src = Path::new(file!());
    // tests/test_check.rs → parent = tests/ → parent = project root
    if let (Some(_tests_dir), Some(proj)) = (src.parent(), src.parent().and_then(|p| p.parent())) {
        let candidate = proj.to_path_buf();
        if candidate.join("scripts/check.rs").exists() {
            return candidate;
        }
    }
    // Fallback: try env var
    if let Ok(root) = std::env::var("PROJECT_ROOT") {
        let p = PathBuf::from(&root);
        if p.join("scripts/check.rs").exists() {
            return p;
        }
    }
    panic!("Cannot find project root (scripts/check.rs not found)");
}

/// Run scripts/check.rs with PROJECT_ROOT set to synthetic_root.
/// check_script_path is the location of scripts/check.rs in the actual project.
fn run_check(check_script: &Path, synthetic_project: &Path) -> (String, String, i32) {
    assert!(check_script.exists(), "check.rs not found at {:?}", check_script);

    let output = Command::new("rust-script")
        .arg(check_script)
        .env("PROJECT_ROOT", synthetic_project)
        .output()
        .expect("failed to run rust-script");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let code = output.status.code().unwrap_or(-1);

    (stdout, stderr, code)
}

/// Write .skill-lock.json with given skills.
fn write_lockfile(dir: &Path, skills: &serde_json::Value) {
    let lock = serde_json::json!({
        "version": 4,
        "skills": skills
    });
    let content = serde_json::to_string_pretty(&lock).unwrap() + "\n";
    fs::write(dir.join(".skill-lock.json"), content).expect("write lockfile");
}

/// Create a minimal skill directory with a SKILL.md file.
fn create_skill_dir(base: &Path, rel_path: &str, content: &str) {
    let dir = base.join(rel_path);
    fs::create_dir_all(&dir).expect("create skill dir");
    fs::write(dir.join("SKILL.md"), content).expect("write SKILL.md");
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn check_script_path() -> PathBuf {
        actual_project_root().join("scripts/check.rs")
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Test: ALL PASS when hashes match
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn all_pass_when_hashes_match() {
        let tmp = TempDir::new("check-test-pass");
        let root = tmp.path();

        create_skill_dir(root, "skills/upstream/skills/engineering/tdd", "# TDD Skill\n");

        let skills = serde_json::json!({
            "tdd": {
                "sourceType": "github",
                "skillPath": "skills/engineering/tdd/SKILL.md",
                "skillFolderHash": "TODO-recalculated"
            }
        });
        write_lockfile(root, &skills);

        // First run: should FIX the hash
        let (stdout1, _stderr1, code1) = run_check(&check_script_path(), root);
        assert_eq!(code1, 0, "FIX run should exit 0, stdout: {}", stdout1);
        assert!(stdout1.contains("FIX: tdd →"), "should contain FIX line, got: {}", stdout1);
        assert!(stdout1.contains("PASS: tdd"), "should also contain PASS, got: {}", stdout1);
        assert!(stdout1.contains("ALL PASS"), "should contain ALL PASS, got: {}", stdout1);

        // Second run: should be all PASS
        let (stdout2, _stderr2, code2) = run_check(&check_script_path(), root);
        assert_eq!(code2, 0, "second run should exit 0, got: {}", stdout2);
        assert!(stdout2.contains("PASS: tdd"), "should contain PASS: tdd, got: {}", stdout2);
        assert!(stdout2.contains("ALL PASS"), "should contain ALL PASS, got: {}", stdout2);
        assert!(!stdout2.contains("FAIL"), "should not contain FAIL, got: {}", stdout2);
        assert!(!stdout2.contains("FIX"), "should not contain FIX on second run, got: {}", stdout2);
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Test: FAIL when hash mismatches
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn fail_when_hash_mismatches() {
        let tmp = TempDir::new("check-test-fail");
        let root = tmp.path();

        create_skill_dir(root, "skills/upstream/skills/engineering/tdd", "# TDD Skill\n");

        let skills = serde_json::json!({
            "tdd": {
                "sourceType": "github",
                "skillPath": "skills/engineering/tdd/SKILL.md",
                "skillFolderHash": "0000000000000000000000000000000000000000"
            }
        });
        write_lockfile(root, &skills);

        let (stdout, _stderr, code) = run_check(&check_script_path(), root);
        assert_eq!(code, 1, "FAIL run should exit 1, stdout: {}", stdout);
        assert!(stdout.contains("FAIL: tdd"), "should contain FAIL: tdd, got: {}", stdout);
        assert!(stdout.contains("computed:"), "should contain computed hash, got: {}", stdout);
        assert!(
            stdout.contains("lockfile: 0000000000000000000000000000000000000000"),
            "should mention lockfile hash, got: {}",
            stdout
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Test: no github skills → ALL PASS (no github skills found)
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn all_pass_when_no_github_skills() {
        let tmp = TempDir::new("check-test-nogithub");
        let root = tmp.path();

        let skills = serde_json::json!({
            "my-local": {
                "sourceType": "local",
                "skillPath": "skills/autopilot/my-local/SKILL.md",
                "skillFolderHash": "somehash"
            }
        });
        write_lockfile(root, &skills);

        let (stdout, _stderr, code) = run_check(&check_script_path(), root);
        assert_eq!(code, 0, "no-github should exit 0, stdout: {}", stdout);
        assert!(
            stdout.contains("ALL PASS (no github skills found)"),
            "should say no github skills, got: {}",
            stdout
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Test: FIX + PASS when TODO-recalculated (no spurious FAIL)
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn fix_then_pass_no_fail() {
        let tmp = TempDir::new("check-test-fix");
        let root = tmp.path();

        create_skill_dir(root, "skills/upstream/skills/engineering/tdd", "# TDD Skill\n");
        create_skill_dir(root, "skills/upstream/skills/engineering/triage", "# Triage\n");

        let skills = serde_json::json!({
            "tdd": {
                "sourceType": "github",
                "skillPath": "skills/engineering/tdd/SKILL.md",
                "skillFolderHash": "TODO-recalculated"
            },
            "triage": {
                "sourceType": "github",
                "skillPath": "skills/engineering/triage/SKILL.md",
                "skillFolderHash": "TODO-recalculated"
            }
        });
        write_lockfile(root, &skills);

        let (stdout, _stderr, code) = run_check(&check_script_path(), root);
        assert_eq!(code, 0, "FIX run should exit 0, stdout: {}", stdout);

        assert!(stdout.contains("FIX: tdd →"), "tdd should have FIX, got: {}", stdout);
        assert!(stdout.contains("FIX: triage →"), "triage should have FIX, got: {}", stdout);
        assert!(stdout.contains("PASS: tdd"), "tdd should PASS after FIX, got: {}", stdout);
        assert!(stdout.contains("PASS: triage"), "triage should PASS after FIX, got: {}", stdout);
        assert!(!stdout.contains("FAIL"), "should not contain FAIL, got: {}", stdout);
        assert!(stdout.contains("ALL PASS"), "should contain ALL PASS, got: {}", stdout);

        // Verify lockfile was updated
        let lock_content = fs::read_to_string(root.join(".skill-lock.json")).unwrap();
        assert!(
            !lock_content.contains("TODO-recalculated"),
            "lockfile should not contain TODO-recalculated anymore"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Test: output format matches bash script exactly
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn output_format_matches_bash_script() {
        let tmp = TempDir::new("check-test-format");
        let root = tmp.path();

        create_skill_dir(root, "skills/upstream/skills/engineering/tdd", "# TDD\n");

        let skills = serde_json::json!({
            "tdd": {
                "sourceType": "github",
                "skillPath": "skills/engineering/tdd/SKILL.md",
                "skillFolderHash": "badhash"
            }
        });
        write_lockfile(root, &skills);

        let (stdout, _stderr, code) = run_check(&check_script_path(), root);
        assert_eq!(code, 1, "should exit 1");

        let fail_line = stdout
            .lines()
            .find(|l| l.starts_with("FAIL:"))
            .expect("should have FAIL line");
        assert!(
            fail_line.starts_with("FAIL: tdd (computed: "),
            "FAIL format wrong: {}",
            fail_line
        );
        assert!(
            fail_line.ends_with(")"),
            "FAIL line should end with ')', got: {}",
            fail_line
        );
        assert!(
            fail_line.contains(", lockfile: badhash)"),
            "FAIL should mention lockfile hash, got: {}",
            fail_line
        );

        // No trailing ALL PASS when there's a FAIL
        assert!(
            !stdout.contains("ALL PASS"),
            "should not contain ALL PASS on failure"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Test: SKIP when skillPath format is unexpected
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn skip_on_unexpected_skill_path() {
        let tmp = TempDir::new("check-test-skip");
        let root = tmp.path();

        let skills = serde_json::json!({
            "bad-skill": {
                "sourceType": "github",
                "skillPath": "not-a-valid-skill-path",
                "skillFolderHash": "abc123"
            }
        });
        write_lockfile(root, &skills);

        let (stdout, _stderr, code) = run_check(&check_script_path(), root);
        // No github skills with valid paths → behaves like no github skills
        assert_eq!(code, 0, "should exit 0, stdout: {}", stdout);
        assert!(
            stdout.contains("SKIP: bad-skill — unexpected skillPath format: not-a-valid-skill-path"),
            "should contain SKIP line, got: {}",
            stdout
        );
    }
}
