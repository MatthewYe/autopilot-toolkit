#!/usr/bin/env rust-script
//! ```cargo
//! [dependencies]
//! tempfile = "3"
//! anyhow = "1"
//! serde_json = "1"
//! ```
//!
//! Integration tests for install.rs toolkit-setup orchestration flow.
//! Per ADR 0005: tests/*.rs rust-script files that exercise the CLI
//! via std::process::Command, asserting exit codes and output.
//!
//! Replaces: tests/test_toolkit_setup.sh (614 lines)
//!
//! #[test] functions: 6
//!
//! Run: rust-script --test tests/test_toolkit_setup.rs

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("Run with: rust-script --test tests/test_toolkit_setup.rs");
}

// ── Helpers ─────────────────────────────────────────────────────────────

/// Wrapper that cleans up on drop (double-insurance with tempfile).
struct TestContext {
    _temp: tempfile::TempDir,
    mock_root: PathBuf,
    mock_home: PathBuf,
    skills_dir: PathBuf,
}

impl TestContext {
    fn new(prefix: &str) -> Self {
        let temp = tempfile::TempDir::with_prefix(prefix).expect("create temp dir");
        let mock_root = temp.path().join("mock-project");
        let mock_home = temp.path().join("home");
        let skills_dir = mock_home.join(".agents/skills");

        fs::create_dir_all(&mock_root).expect("create mock_root");
        fs::create_dir_all(&mock_home).expect("create mock_home");
        fs::create_dir_all(&skills_dir).expect("create skills_dir");

        TestContext {
            _temp: temp,
            mock_root,
            mock_home,
            skills_dir,
        }
    }

    fn path(&self) -> &Path {
        self.mock_root.as_path()
    }

    fn skills_dir(&self) -> &Path {
        self.skills_dir.as_path()
    }

    fn home(&self) -> &Path {
        self.mock_home.as_path()
    }
}

/// Find the actual project root — the directory containing install.rs.
fn project_root() -> PathBuf {
    let src = Path::new(file!());
    if let (Some(_tests_dir), Some(proj)) = (src.parent(), src.parent().and_then(|p| p.parent())) {
        let candidate = proj.to_path_buf();
        if candidate.join("install.rs").exists() {
            return candidate;
        }
    }
    if let Ok(root) = std::env::var("PROJECT_ROOT") {
        let p = PathBuf::from(&root);
        if p.join("install.rs").exists() {
            return p;
        }
    }
    panic!("Cannot find project root (install.rs not found)");
}

fn install_script_path() -> PathBuf {
    project_root().join("install.rs")
}

/// Run install.rs sync with given env.
fn run_sync(
    install_script: &Path,
    home: &Path,
    skills_dir: &Path,
    project_root: &Path,
    name: &str,
    src: &Path,
) -> (String, String, i32) {
    let output = Command::new("rust-script")
        .arg(install_script)
        .arg("sync")
        .arg(name)
        .arg(src)
        .env("HOME", home)
        .env("AGENTS_SKILLS_DIR", skills_dir)
        .env("PROJECT_ROOT", project_root)
        .output()
        .expect("failed to run install.rs sync");

    (
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
        output.status.code().unwrap_or(-1),
    )
}

/// Run install.rs unlink with given env.
fn run_unlink(
    install_script: &Path,
    home: &Path,
    skills_dir: &Path,
    project_root: &Path,
    name: &str,
) -> (String, String, i32) {
    let output = Command::new("rust-script")
        .arg(install_script)
        .arg("unlink")
        .arg(name)
        .env("HOME", home)
        .env("AGENTS_SKILLS_DIR", skills_dir)
        .env("PROJECT_ROOT", project_root)
        .output()
        .expect("failed to run install.rs unlink");

    (
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
        output.status.code().unwrap_or(-1),
    )
}

/// Run install.rs link-principles with given env.
fn run_link_principles(
    install_script: &Path,
    home: &Path,
    principles_dir: &Path,
    src: &Path,
) -> (String, String, i32) {
    let output = Command::new("rust-script")
        .arg(install_script)
        .arg("link-principles")
        .arg(src)
        .env("HOME", home)
        .env("AGENTS_PRINCIPLES_DIR", principles_dir)
        .output()
        .expect("failed to run install.rs link-principles");

    (
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
        output.status.code().unwrap_or(-1),
    )
}

/// Set up a mock project with known skills and .skill-lock.json.
fn setup_mock_project(ctx: &TestContext) {
    let root = ctx.path();

    // Create upstream skill directories
    let upstream_skills = ["skill-a", "skill-b", "skill-c"];
    for s in &upstream_skills {
        let dir = root.join("skills/upstream/skills/engineering").join(s);
        fs::create_dir_all(&dir).expect("create upstream skill dir");
        let md = format!(
            "---\nname: {}\ndescription: Mock upstream skill {}\n---\n# {}\n",
            s, s, s
        );
        fs::write(dir.join("SKILL.md"), md).expect("write SKILL.md");
    }

    // Create autopilot skill directories
    let autopilot_skills = ["auto-x", "auto-y"];
    for s in &autopilot_skills {
        let dir = root.join("skills/autopilot").join(s);
        fs::create_dir_all(&dir).expect("create autopilot skill dir");
        let md = format!(
            "---\nname: {}\ndescription: Mock autopilot skill {}\n---\n# {}\n",
            s, s, s
        );
        fs::write(dir.join("SKILL.md"), md).expect("write SKILL.md");
    }

    // Create principles directory
    let principles_dir = root.join("principles");
    fs::create_dir_all(&principles_dir).expect("create principles dir");
    fs::write(principles_dir.join("karpathy.md"), "Be curious.\n").expect("write principles");

    // Create .skill-lock.json
    let lock = serde_json::json!({
        "version": 3,
        "skills": {
            "skill-a": {
                "source": "mock/skills",
                "sourceType": "github",
                "sourceUrl": "https://example.com/mock.git",
                "skillPath": "skills/engineering/skill-a/SKILL.md",
                "skillFolderHash": "aaa111",
                "pluginName": "mock-skills",
                "installedAt": "2026-01-01T00:00:00.000Z",
                "updatedAt": "2026-01-01T00:00:00.000Z"
            },
            "skill-b": {
                "source": "mock/skills",
                "sourceType": "github",
                "sourceUrl": "https://example.com/mock.git",
                "skillPath": "skills/engineering/skill-b/SKILL.md",
                "skillFolderHash": "bbb222",
                "pluginName": "mock-skills",
                "installedAt": "2026-01-01T00:00:00.000Z",
                "updatedAt": "2026-01-01T00:00:00.000Z"
            },
            "skill-c": {
                "source": "mock/skills",
                "sourceType": "github",
                "sourceUrl": "https://example.com/mock.git",
                "skillPath": "skills/engineering/skill-c/SKILL.md",
                "skillFolderHash": "ccc333",
                "pluginName": "mock-skills",
                "installedAt": "2026-01-01T00:00:00.000Z",
                "updatedAt": "2026-01-01T00:00:00.000Z"
            }
        },
        "dismissed": {}
    });
    let content = serde_json::to_string_pretty(&lock).unwrap() + "\n";
    fs::write(root.join(".skill-lock.json"), content).expect("write lockfile");
}

/// Check symlink state for a skill. Returns one of: "correct", "missing", "wrong_target", "broken", "real_dir".
fn skill_state(name: &str, expected_src: &Path, skills_dir: &Path) -> &'static str {
    let target = skills_dir.join(name);

    if !target.exists() && !target.is_symlink() {
        return "missing";
    }

    if target.exists() && !target.is_symlink() {
        return "real_dir";
    }

    // It's a symlink
    let link_target = match fs::read_link(&target) {
        Ok(lt) => lt,
        Err(_) => return "broken",
    };

    if link_target != expected_src {
        return "wrong_target";
    }

    if !target.is_dir() {
        return "broken";
    }

    "correct"
}

/// Derive the expected skill set (name → source dir) from a mock project.
fn derive_expected_set(mock_root: &Path) -> Vec<(String, PathBuf)> {
    let mut expected: Vec<(String, PathBuf)> = Vec::new();

    // Autopilot skills from filesystem
    let autopilot_dir = mock_root.join("skills/autopilot");
    if autopilot_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(&autopilot_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() && path.join("SKILL.md").exists() {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        let raw = path.to_path_buf();
                        expected.push((name.to_string(), raw));
                    }
                }
            }
        }
    }

    // Upstream skills from .skill-lock.json
    let lock_path = mock_root.join(".skill-lock.json");
    if lock_path.exists() {
        if let Ok(content) = fs::read_to_string(&lock_path) {
            if let Ok(data) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(skills) = data.get("skills").and_then(|s| s.as_object()) {
                    for (name, info) in skills {
                        if let Some(skill_path) = info.get("skillPath").and_then(|v| v.as_str()) {
                            // skillPath like "skills/engineering/skill-a/SKILL.md"
                            let parent = Path::new(skill_path).parent().unwrap_or(Path::new("."));
                            let src_dir = mock_root.join("skills/upstream").join(parent);
                            let raw = src_dir.to_path_buf();
                            expected.push((name.clone(), raw));
                        }
                    }
                }
            }
        }
    }

    expected
}

/// Full execute: sync all expected + link-principles. Returns combined output.
fn run_toolkit_setup_execute(
    install_script: &Path,
    ctx: &TestContext,
    expected: &[(String, PathBuf)],
) -> String {
    let mut lines: Vec<String> = Vec::new();

    // Sync all expected skills
    for (name, src) in expected {
        let state = skill_state(name, src, ctx.skills_dir());
        match state {
            "missing" | "broken" | "wrong_target" => {
                let (_stdout, _stderr, _code) = run_sync(
                    install_script,
                    ctx.home(),
                    ctx.skills_dir(),
                    ctx.path(),
                    name,
                    src,
                );
                lines.push(format!("  SYNC {} -> {}", name, src.display()));
            }
            "real_dir" => {
                lines.push(format!(
                    "  WARN: {} is a real directory at {}/{} — skipping",
                    name,
                    ctx.skills_dir().display(),
                    name
                ));
            }
            "correct" => {
                // No-op
            }
            _ => {}
        }
    }

    // Find and unlink orphaned symlinks
    let expected_names: Vec<&str> = expected.iter().map(|(n, _)| n.as_str()).collect();
    if ctx.skills_dir().is_dir() {
        if let Ok(entries) = fs::read_dir(ctx.skills_dir()) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_symlink() {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        if !expected_names.contains(&name) {
                            if let Ok(link_target) = fs::read_link(&path) {
                                let proj = ctx.path().to_path_buf();
                                if link_target.starts_with(&proj) {
                                    let (_stdout, _stderr, _code) = run_unlink(
                                        install_script,
                                        ctx.home(),
                                        ctx.skills_dir(),
                                        ctx.path(),
                                        name,
                                    );
                                    lines.push(format!("  UNLINK {} (orphaned)", name));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Link principles
    let principles_src = ctx.path().join("principles");
    if principles_src.is_dir() {
        let principles_dir = ctx.home().join(".agents/principles");
        let _ = run_link_principles(install_script, ctx.home(), &principles_dir, &principles_src);
        lines.push(format!("  LINK-PRINCIPLES -> {}", principles_src.display()));
    }

    lines.join("\n")
}

/// Run verification: check all expected skills and produce a report.
fn run_toolkit_setup_verify(
    ctx: &TestContext,
    expected: &[(String, PathBuf)],
) -> String {
    let mut report: Vec<String> = Vec::new();
    let mut missing_count = 0;
    let mut damaged_count = 0;
    let total = expected.len();
    let mut all_pass = true;

    for (name, src) in expected {
        let state = skill_state(name, src, ctx.skills_dir());
        match state {
            "correct" => {
                report.push(format!("  [PASS] {}", name));
            }
            "missing" => {
                report.push(format!("  [FAIL] {} — missing", name));
                missing_count += 1;
                all_pass = false;
            }
            "wrong_target" => {
                let actual = fs::read_link(ctx.skills_dir().join(name))
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|_| "?".to_string());
                report.push(format!(
                    "  [FAIL] {} -> {} (expected {})",
                    name,
                    actual,
                    src.display()
                ));
                damaged_count += 1;
                all_pass = false;
            }
            "broken" => {
                report.push(format!("  [FAIL] {} — broken symlink", name));
                damaged_count += 1;
                all_pass = false;
            }
            "real_dir" => {
                report.push(format!(
                    "  [WARN] {} — real directory (not a symlink)",
                    name
                ));
                damaged_count += 1;
                all_pass = false;
            }
            _ => {}
        }
    }

    report.push(format!(
        "  Total: {} expected, {} missing, {} damaged",
        total, missing_count, damaged_count
    ));

    if all_pass {
        report.push("  ALL PASS".to_string());
    } else {
        report.push("  FIXES NEEDED".to_string());
    }

    report.join("\n")
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn install_script() -> PathBuf {
        install_script_path()
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Test 1: First install — all skills missing, full sync + link-principles
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn first_install_all_skills_missing() {
        let ctx = TestContext::new("toolkit-test1");
        setup_mock_project(&ctx);
        let install = install_script();

        let expected = derive_expected_set(ctx.path());

        // Verify expected set has 5 skills (3 upstream + 2 autopilot)
        assert_eq!(
            expected.len(),
            5,
            "expected 5 skills, got {}: {:?}",
            expected.len(),
            expected
        );

        // Verify all skills are initially missing
        for (name, src) in &expected {
            let state = skill_state(name, src, ctx.skills_dir());
            assert_eq!(
                state, "missing",
                "skill {} should be missing initially, but was {}",
                name, state
            );
        }

        // Execute toolkit-setup
        let exec_result = run_toolkit_setup_execute(&install, &ctx, &expected);

        // Verify sync happened for all 5 skills
        let sync_count = exec_result.matches("SYNC").count();
        assert_eq!(
            sync_count, 5,
            "first install should sync all 5 skills, synced {}:\n{}",
            sync_count, exec_result
        );

        // Verify link-principles was called
        assert!(
            exec_result.contains("LINK-PRINCIPLES"),
            "should contain LINK-PRINCIPLES:\n{}",
            exec_result
        );

        // Verify all skills are now correct
        for (name, src) in &expected {
            let state = skill_state(name, src, ctx.skills_dir());
            assert_eq!(
                state, "correct",
                "skill {} should be correct after first install, but was {}",
                name, state
            );
        }

        // Verify output contains specific skill names
        for s in &["skill-a", "skill-b", "skill-c", "auto-x", "auto-y"] {
            assert!(
                exec_result.contains(s),
                "output should mention {}, got:\n{}",
                s,
                exec_result
            );
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Test 2: Update scenario — only incremental changes
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn update_incremental_changes_only() {
        let ctx = TestContext::new("toolkit-test2");
        setup_mock_project(&ctx);
        let install = install_script();

        let expected = derive_expected_set(ctx.path());

        // Pre-setup: install all skills correctly first
        let _ = run_toolkit_setup_execute(&install, &ctx, &expected);

        // Now simulate an update: break one symlink, wrong target for another
        // Break skill-a: create a broken symlink (target does not exist)
        let skill_a_target = ctx.skills_dir().join("skill-a");
        fs::remove_file(&skill_a_target).unwrap_or(());
        std::os::unix::fs::symlink("/tmp/nonexistent", &skill_a_target)
            .expect("create broken symlink for skill-a");

        // Wrong target for skill-b: point to skill-c's source
        let skill_b_target = ctx.skills_dir().join("skill-b");
        fs::remove_file(&skill_b_target).unwrap_or(());
        let skill_c_src = ctx.path().join("skills/upstream/skills/engineering/skill-c");
        std::os::unix::fs::symlink(&skill_c_src, &skill_b_target)
            .expect("create wrong target symlink for skill-b");

        // Re-run toolkit-setup
        let exec_result = run_toolkit_setup_execute(&install, &ctx, &expected);

        // Verify only broken/wrong-target skills were fixed (2 syncs)
        let sync_count = exec_result.matches("SYNC").count();
        assert_eq!(
            sync_count, 2,
            "update should only sync 2 changed skills, synced {}:\n{}",
            sync_count, exec_result
        );

        // Verify skill-a and skill-b were mentioned in output
        assert!(
            exec_result.contains("skill-a"),
            "output should mention skill-a:\n{}",
            exec_result
        );
        assert!(
            exec_result.contains("skill-b"),
            "output should mention skill-b:\n{}",
            exec_result
        );

        // Verify all skills are correct after update
        for (name, src) in &expected {
            let state = skill_state(name, src, ctx.skills_dir());
            assert_eq!(
                state, "correct",
                "skill {} should be correct after update, but was {}",
                name, state
            );
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Test 3: Orphaned symlink cleanup
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn orphaned_symlink_cleanup() {
        let ctx = TestContext::new("toolkit-test3");
        setup_mock_project(&ctx);
        let install = install_script();

        let expected = derive_expected_set(ctx.path());

        // Pre-setup: install all skills correctly
        let _ = run_toolkit_setup_execute(&install, &ctx, &expected);

        // Create an orphaned symlink (pointing under PROJECT_ROOT, not in expected set)
        let old_skill_dir = ctx.path().join("skills/upstream/skills/engineering/old-skill");
        fs::create_dir_all(&old_skill_dir).expect("create old-skill dir");
        fs::write(old_skill_dir.join("SKILL.md"), "# Old Skill\n").expect("write old SKILL.md");

        let orphan_target = ctx.skills_dir().join("old-skill");
        std::os::unix::fs::symlink(&old_skill_dir, &orphan_target)
            .expect("create orphan symlink");

        // Verify orphan exists before execution
        assert!(
            orphan_target.is_symlink(),
            "orphan symlink should exist before setup"
        );

        // Re-run toolkit-setup
        let exec_result = run_toolkit_setup_execute(&install, &ctx, &expected);

        // Verify orphan was unlinked
        assert!(
            !orphan_target.exists(),
            "orphan symlink should be removed after setup"
        );

        // Verify UNLINK was reported with specific name
        assert!(
            exec_result.contains("UNLINK") && exec_result.contains("old-skill"),
            "output should report UNLINK with old-skill:\n{}",
            exec_result
        );

        // Verify all expected skills still correct
        for (name, src) in &expected {
            let state = skill_state(name, src, ctx.skills_dir());
            assert_eq!(
                state, "correct",
                "skill {} should still be correct after orphan cleanup, but was {}",
                name, state
            );
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Test 4: Real-directory conflict is reported as WARN
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn real_directory_conflict_reported_as_warn() {
        let ctx = TestContext::new("toolkit-test4");
        setup_mock_project(&ctx);
        let install = install_script();

        let expected = derive_expected_set(ctx.path());

        // Pre-setup: install skill-a and skill-b correctly
        for (name, src) in &expected {
            if name == "skill-a" || name == "skill-b" {
                let _ = run_sync(&install, ctx.home(), ctx.skills_dir(), ctx.path(), name, src);
            }
        }

        // Create a real directory at skill-c's location
        let skill_c_dir = ctx.skills_dir().join("skill-c");
        fs::create_dir_all(&skill_c_dir).expect("create real dir for skill-c");
        fs::write(skill_c_dir.join("important.txt"), "precious data").expect("write precious data");

        // Re-run toolkit-setup
        let exec_result = run_toolkit_setup_execute(&install, &ctx, &expected);

        // Verify WARN is reported for the real directory conflict
        assert!(
            exec_result.contains("WARN") && exec_result.contains("real directory"),
            "output should report WARN for real dir:\n{}",
            exec_result
        );
        assert!(
            exec_result.contains("skill-c"),
            "WARN should mention skill-c:\n{}",
            exec_result
        );

        // Verify real directory is preserved
        assert!(skill_c_dir.is_dir(), "real directory should still exist");
        assert!(
            skill_c_dir.join("important.txt").exists(),
            "precious file should be preserved"
        );
        assert!(
            !skill_c_dir.is_symlink(),
            "real dir should not be replaced by symlink"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Test 5: ALL PASS when final verification succeeds
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn all_pass_on_successful_verification() {
        let ctx = TestContext::new("toolkit-test5");
        setup_mock_project(&ctx);
        let install = install_script();

        let expected = derive_expected_set(ctx.path());

        // Pre-setup: install all skills correctly
        let _ = run_toolkit_setup_execute(&install, &ctx, &expected);

        // Run verification
        let verify_result = run_toolkit_setup_verify(&ctx, &expected);

        // Verify ALL PASS is in output
        assert!(
            verify_result.contains("ALL PASS"),
            "verification should contain ALL PASS:\n{}",
            verify_result
        );

        // Verify each skill is listed as PASS
        for s in &["skill-a", "skill-b", "skill-c", "auto-x", "auto-y"] {
            assert!(
                verify_result.contains(&format!("[PASS] {}", s)),
                "verification should list {} as PASS:\n{}",
                s,
                verify_result
            );
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Test 6: Verification reports failures correctly
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn verification_reports_failures() {
        let ctx = TestContext::new("toolkit-test6");
        setup_mock_project(&ctx);
        let install = install_script();

        let expected = derive_expected_set(ctx.path());

        // Install only some skills, leave others missing
        // Install skill-a only
        for (name, src) in &expected {
            if name == "skill-a" {
                let _ = run_sync(&install, ctx.home(), ctx.skills_dir(), ctx.path(), name, src);
            }
        }

        // Run verification
        let verify_result = run_toolkit_setup_verify(&ctx, &expected);

        // Verify it does NOT say ALL PASS (some skills missing)
        assert!(
            !verify_result.contains("ALL PASS"),
            "verification should NOT say ALL PASS when broken:\n{}",
            verify_result
        );
        assert!(
            verify_result.contains("FIXES NEEDED"),
            "verification should say FIXES NEEDED:\n{}",
            verify_result
        );

        // Verify PASS for the installed skill
        assert!(
            verify_result.contains("[PASS] skill-a"),
            "verification should list skill-a as PASS:\n{}",
            verify_result
        );

        // Verify FAIL for missing skills
        assert!(
            verify_result.contains("FAIL") && verify_result.contains("missing"),
            "verification should list missing skills as FAIL:\n{}",
            verify_result
        );
    }
}
