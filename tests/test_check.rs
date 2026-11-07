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
    assert!(
        check_script.exists(),
        "check.rs not found at {:?}",
        check_script
    );

    let output = Command::new("rust-script")
        // `-f`: the cache ignores path-dependency changes (`crates/*`,
        // upstream rust-script#122).
        .arg("-f")
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

/// Write .vendor-lock.json with given skills.
fn write_vendor_lockfile(dir: &Path, skills: &serde_json::Value) {
    let lock = serde_json::json!({
        "version": 1,
        "skills": skills
    });
    let content = serde_json::to_string_pretty(&lock).unwrap() + "\n";
    fs::write(dir.join(".vendor-lock.json"), content).expect("write vendor lockfile");
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

        create_skill_dir(
            root,
            "skills/upstream/skills/engineering/tdd",
            "# TDD Skill\n",
        );

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
        assert!(
            stdout1.contains("FIX: tdd →"),
            "should contain FIX line, got: {}",
            stdout1
        );
        assert!(
            stdout1.contains("PASS: tdd"),
            "should also contain PASS, got: {}",
            stdout1
        );
        assert!(
            stdout1.contains("ALL PASS"),
            "should contain ALL PASS, got: {}",
            stdout1
        );

        // Second run: should be all PASS
        let (stdout2, _stderr2, code2) = run_check(&check_script_path(), root);
        assert_eq!(code2, 0, "second run should exit 0, got: {}", stdout2);
        assert!(
            stdout2.contains("PASS: tdd"),
            "should contain PASS: tdd, got: {}",
            stdout2
        );
        assert!(
            stdout2.contains("ALL PASS"),
            "should contain ALL PASS, got: {}",
            stdout2
        );
        assert!(
            !stdout2.contains("FAIL"),
            "should not contain FAIL, got: {}",
            stdout2
        );
        assert!(
            !stdout2.contains("FIX"),
            "should not contain FIX on second run, got: {}",
            stdout2
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Test: FAIL when hash mismatches
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn fail_when_hash_mismatches() {
        let tmp = TempDir::new("check-test-fail");
        let root = tmp.path();

        create_skill_dir(
            root,
            "skills/upstream/skills/engineering/tdd",
            "# TDD Skill\n",
        );

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
        assert!(
            stdout.contains("FAIL: tdd"),
            "should contain FAIL: tdd, got: {}",
            stdout
        );
        assert!(
            stdout.contains("computed:"),
            "should contain computed hash, got: {}",
            stdout
        );
        assert!(
            stdout.contains("lockfile: 0000000000000000000000000000000000000000"),
            "should mention lockfile hash, got: {}",
            stdout
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Test: vendor lock hash FIX then PASS
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn vendor_hash_fix_then_pass() {
        let tmp = TempDir::new("check-test-vendor");
        let root = tmp.path();

        create_skill_dir(root, "skills/vendor/show-me", "# Show Me\n");
        fs::write(
            root.join("skills/vendor/show-me/PROVENANCE.md"),
            "# Provenance\n",
        )
        .unwrap();
        write_lockfile(root, &serde_json::json!({}));
        write_vendor_lockfile(
            root,
            &serde_json::json!({
                "show-me": {
                    "sourceType": "github",
                    "skillPath": "plugins/show-me/skills/show-me/SKILL.md",
                    "skillFolderHash": "TODO-recalculated",
                    "vendorPath": "skills/vendor/show-me"
                }
            }),
        );

        let (stdout1, stderr1, code1) = run_check(&check_script_path(), root);
        assert_eq!(
            code1, 0,
            "FIX run should exit 0, stdout: {} stderr: {}",
            stdout1, stderr1
        );
        assert!(
            stdout1.contains("FIX: show-me →"),
            "first run should FIX vendor hash, stdout: {}",
            stdout1
        );
        assert!(
            stdout1.contains("PASS: show-me"),
            "first run should PASS after fix, stdout: {}",
            stdout1
        );

        let (stdout2, stderr2, code2) = run_check(&check_script_path(), root);
        assert_eq!(
            code2, 0,
            "second run should exit 0, stdout: {} stderr: {}",
            stdout2, stderr2
        );
        assert!(
            stdout2.contains("PASS: show-me"),
            "second run should PASS, stdout: {}",
            stdout2
        );
        assert!(
            !stdout2.contains("FIX: show-me"),
            "second run should not FIX, stdout: {}",
            stdout2
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Test: vendor lock hash mismatch FAILs
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn vendor_fail_when_hash_mismatches() {
        let tmp = TempDir::new("check-test-vendor-fail");
        let root = tmp.path();

        create_skill_dir(root, "skills/vendor/show-me", "# Show Me\n");
        fs::write(
            root.join("skills/vendor/show-me/PROVENANCE.md"),
            "# Provenance\n",
        )
        .unwrap();
        write_lockfile(root, &serde_json::json!({}));
        write_vendor_lockfile(
            root,
            &serde_json::json!({
                "show-me": {
                    "sourceType": "github",
                    "skillPath": "plugins/show-me/skills/show-me/SKILL.md",
                    "skillFolderHash": "0000000000000000000000000000000000000000",
                    "vendorPath": "skills/vendor/show-me"
                }
            }),
        );

        let (stdout, stderr, code) = run_check(&check_script_path(), root);
        assert_eq!(
            code, 1,
            "mismatch run should exit 1, stdout: {} stderr: {}",
            stdout, stderr
        );
        assert!(
            stdout.contains("FAIL: show-me"),
            "should report vendor FAIL, stdout: {}",
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

        create_skill_dir(
            root,
            "skills/upstream/skills/engineering/tdd",
            "# TDD Skill\n",
        );
        create_skill_dir(
            root,
            "skills/upstream/skills/engineering/triage",
            "# Triage\n",
        );

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

        assert!(
            stdout.contains("FIX: tdd →"),
            "tdd should have FIX, got: {}",
            stdout
        );
        assert!(
            stdout.contains("FIX: triage →"),
            "triage should have FIX, got: {}",
            stdout
        );
        assert!(
            stdout.contains("PASS: tdd"),
            "tdd should PASS after FIX, got: {}",
            stdout
        );
        assert!(
            stdout.contains("PASS: triage"),
            "triage should PASS after FIX, got: {}",
            stdout
        );
        assert!(
            !stdout.contains("FAIL"),
            "should not contain FAIL, got: {}",
            stdout
        );
        assert!(
            stdout.contains("ALL PASS"),
            "should contain ALL PASS, got: {}",
            stdout
        );

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
        // A skipped check fails closed: the hash was never verified, so the
        // gate must not pass silently.
        assert_eq!(code, 1, "SKIP must fail the check, stdout: {}", stdout);
        assert!(
            stdout
                .contains("SKIP: bad-skill — unexpected skillPath format: not-a-valid-skill-path"),
            "should contain SKIP line, got: {}",
            stdout
        );
        assert!(
            !stdout.contains("ALL PASS"),
            "should not contain ALL PASS on a skipped check"
        );
    }
    // ═══════════════════════════════════════════════════════════════════════
    // Upstream pin checks
    // ═══════════════════════════════════════════════════════════════════════

    const PIN_COMMIT: &str = "d1478fcb140db0b93c0c8df17bcfd311844749d0";

    fn write_readme(dir: &Path, ref_name: &str, commit: &str) {
        fs::write(
            dir.join("README.md"),
            format!(
                "# Fixture\n\nThe upstream snapshot is pinned to release {} `{}`.\n",
                ref_name, commit
            ),
        )
        .unwrap();
    }

    fn write_upstream_package(dir: &Path, version: &str) {
        let path = dir.join("skills/upstream/package.json");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            format!(
                "{{\n  \"name\": \"mattpocock-skills\",\n  \"version\": \"{}\"\n}}\n",
                version
            ),
        )
        .unwrap();
    }

    fn tag_pin(version: &str) -> serde_json::Value {
        serde_json::json!({
            "schemaVersion": 1,
            "source": "mattpocock/skills",
            "sourceUrl": "https://github.com/mattpocock/skills.git",
            "ref": "v1.3.2",
            "refKind": "tag",
            "resolvedCommit": PIN_COMMIT,
            "version": version,
            "syncedAt": "2026-11-07T10:00:00Z"
        })
    }

    fn write_pin(dir: &Path, pin: &serde_json::Value) {
        fs::write(
            dir.join(".upstream-pin.json"),
            serde_json::to_string_pretty(pin).unwrap() + "\n",
        )
        .unwrap();
    }

    fn pin_fixture(prefix: &str, package_version: &str) -> TempDir {
        let tmp = TempDir::new(prefix);
        write_lockfile(tmp.path(), &serde_json::json!({}));
        write_upstream_package(tmp.path(), package_version);
        write_readme(tmp.path(), "v1.3.2", PIN_COMMIT);
        tmp
    }

    #[test]
    fn fail_when_pin_is_missing() {
        let tmp = pin_fixture("check-test-pin-missing", "1.3.2");
        let (stdout, _stderr, code) = run_check(&check_script_path(), tmp.path());
        assert_eq!(code, 1, "missing pin must fail, stdout: {}", stdout);
        assert!(
            stdout.contains("FAIL: upstream-pin — missing .upstream-pin.json"),
            "should name the missing pin, got: {}",
            stdout
        );
        assert!(
            !stdout.contains("ALL PASS"),
            "missing pin must not report ALL PASS, got: {}",
            stdout
        );
    }

    #[test]
    fn pass_with_valid_tag_pin() {
        let tmp = pin_fixture("check-test-pin-pass", "1.3.2");
        write_pin(tmp.path(), &tag_pin("1.3.2"));
        let (stdout, _stderr, code) = run_check(&check_script_path(), tmp.path());
        assert_eq!(code, 0, "valid pin should pass, stdout: {}", stdout);
        assert!(
            stdout.contains("PASS: upstream-pin"),
            "should contain the pin PASS line, got: {}",
            stdout
        );
        assert!(
            stdout.contains("ALL PASS"),
            "should contain ALL PASS, got: {}",
            stdout
        );
    }

    #[test]
    fn fail_when_readme_omits_the_pin_commit() {
        let tmp = pin_fixture("check-test-pin-readme", "1.3.2");
        fs::write(tmp.path().join("README.md"), "pinned to v1.3.2\n").unwrap();
        write_pin(tmp.path(), &tag_pin("1.3.2"));
        let (stdout, _stderr, code) = run_check(&check_script_path(), tmp.path());
        assert_eq!(code, 1, "README disagreement must fail, stdout: {}", stdout);
        assert!(
            stdout.contains("does not mention the pinned commit"),
            "should name the README mismatch, got: {}",
            stdout
        );
    }

    #[test]
    fn fail_when_tag_pin_version_mismatches_the_tree() {
        let tmp = pin_fixture("check-test-pin-version", "1.3.3");
        write_pin(tmp.path(), &tag_pin("1.3.2"));
        let (stdout, _stderr, code) = run_check(&check_script_path(), tmp.path());
        assert_eq!(code, 1, "version mismatch must fail, stdout: {}", stdout);
        assert!(
            stdout.contains("does not match skills/upstream/package.json"),
            "should name the version mismatch, got: {}",
            stdout
        );
    }

    #[test]
    fn fail_when_pin_is_malformed() {
        let tmp = pin_fixture("check-test-pin-malformed", "1.3.2");
        fs::write(
            tmp.path().join(".upstream-pin.json"),
            "{\"schemaVersion\": 1, \"surprise\": true}\n",
        )
        .unwrap();
        let (stdout, _stderr, code) = run_check(&check_script_path(), tmp.path());
        assert_eq!(code, 1, "malformed pin must fail, stdout: {}", stdout);
        assert!(
            stdout.contains("FAIL: upstream-pin — malformed upstream pin"),
            "should name the malformed pin, got: {}",
            stdout
        );
    }
}
