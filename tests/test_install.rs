#!/usr/bin/env rust-script
//! ```cargo
//! [dependencies]
//! tempfile = "3"
//! ```
//!
//! Integration tests for install.rs CLI contract.
//! Merges test_install.sh + test_install_rs.sh into a single Rust test suite
//! (≥78 assertions). Exercises sync, unlink, link-principles, and parameter
//! validation via std::process::Command.
//!
//! Run: rust-script --test tests/test_install.rs

use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("Run with: rust-script --test tests/test_install.rs");
}

// ── Helpers ─────────────────────────────────────────────────────────────

/// Find the actual project root — the directory containing install.rs.
fn project_root() -> PathBuf {
    // file!() gives "tests/test_install.rs" relative to project root
    let src = Path::new(file!());
    if let (Some(_tests_dir), Some(proj)) = (src.parent(), src.parent().and_then(|p| p.parent())) {
        let candidate = proj.to_path_buf();
        if candidate.join("install.rs").exists() {
            return candidate;
        }
    }
    // Fallback: try env var
    if let Ok(root) = std::env::var("PROJECT_ROOT") {
        let p = PathBuf::from(&root);
        if p.join("install.rs").exists() {
            return p;
        }
    }
    panic!("Cannot find project root (install.rs not found)");
}

/// Path to the install.rs script under test.
fn install_script() -> PathBuf {
    project_root().join("install.rs")
}

/// Run install.rs with given args and environment variables.
/// Returns (stdout, stderr, exit_code).
fn run_install(
    args: &[&str],
    home: &Path,
    agents_skills_dir: Option<&Path>,
    agents_principles_dir: Option<&Path>,
    project_root: Option<&Path>,
) -> (String, String, i32) {
    let script = install_script();
    assert!(script.exists(), "install.rs not found at {:?}", script);

    let mut cmd = Command::new("rust-script");
    cmd.arg(&script);

    for a in args {
        cmd.arg(a);
    }

    cmd.env("HOME", home);

    if let Some(d) = agents_skills_dir {
        cmd.env("AGENTS_SKILLS_DIR", d);
    }
    if let Some(d) = agents_principles_dir {
        cmd.env("AGENTS_PRINCIPLES_DIR", d);
    }
    if let Some(r) = project_root {
        cmd.env("PROJECT_ROOT", r);
    }

    let output = cmd.output().expect("failed to run rust-script install.rs");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let code = output.status.code().unwrap_or(-1);

    (stdout, stderr, code)
}

/// Helper: read symlink target, returning None if not a symlink.
fn read_link_target(path: &Path) -> Option<PathBuf> {
    if path.is_symlink() {
        fs::read_link(path).ok()
    } else {
        None
    }
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ═══════════════════════════════════════════════════════════════════════
    // Parameter validation: no args / wrong args / unknown subcommand
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn no_args_prints_usage_and_exits_nonzero() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path().join("home");
        fs::create_dir_all(&home).unwrap();

        let (out, err, code) = run_install(&[], &home, None, None, None);

        assert_ne!(code, 0, "no-args should exit non-zero");
        let combined = format!("{}{}", out, err);
        assert!(
            combined.contains("Usage:") || combined.to_lowercase().contains("sync"),
            "no-args should print usage mentioning sync, got: {}",
            combined
        );
    }

    #[test]
    fn sync_zero_args_exits_nonzero() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path().join("home");
        fs::create_dir_all(&home).unwrap();

        let (out, err, code) = run_install(&["sync"], &home, None, None, None);

        assert_ne!(code, 0, "sync with 0 args should exit non-zero");
        let combined = format!("{}{}", out, err);
        assert!(
            combined.to_lowercase().contains("requires exactly two"),
            "sync-0-args should print 'requires exactly two', got: {}",
            combined
        );
    }

    #[test]
    fn sync_one_arg_exits_nonzero() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path().join("home");
        fs::create_dir_all(&home).unwrap();

        let (out, err, code) = run_install(&["sync", "onlyname"], &home, None, None, None);

        assert_ne!(code, 0, "sync with 1 arg should exit non-zero");
        let combined = format!("{}{}", out, err);
        assert!(
            combined.to_lowercase().contains("requires exactly two"),
            "sync-1-arg should print 'requires exactly two', got: {}",
            combined
        );
    }

    #[test]
    fn sync_three_args_exits_nonzero() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path().join("home");
        fs::create_dir_all(&home).unwrap();

        let (out, err, code) = run_install(&["sync", "a", "b", "c"], &home, None, None, None);

        assert_ne!(code, 0, "sync with 3 args should exit non-zero");
        let combined = format!("{}{}", out, err);
        assert!(
            combined.to_lowercase().contains("requires exactly two"),
            "sync-3-args should print 'requires exactly two', got: {}",
            combined
        );
    }

    #[test]
    fn unknown_subcommand_exits_nonzero() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path().join("home");
        fs::create_dir_all(&home).unwrap();

        let (out, err, code) = run_install(&["bogus", "arg1"], &home, None, None, None);

        assert_ne!(code, 0, "unknown subcommand should exit non-zero");
        let combined = format!("{}{}", out, err);
        assert!(
            combined.to_lowercase().contains("unknown") || combined.to_lowercase().contains("usage"),
            "unknown subcommand should print error about unknown/usage, got: {}",
            combined
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // sync: fresh create
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn fresh_sync_creates_symlink() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path().join("home");
        let skills = home.join(".agents/skills");
        let src = tmp.path().join("source-skills/my-skill");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("SKILL.md"), "# My Skill\n").unwrap();

        let (_out, _err, code) = run_install(
            &["sync", "my-skill", &src.to_string_lossy()],
            &home,
            Some(&skills),
            None,
            None,
        );

        assert_eq!(code, 0, "fresh sync should exit 0");
        assert!(skills.is_dir(), "skills dir should be created");
        let link = skills.join("my-skill");
        assert!(link.is_symlink(), "symlink should exist");
        assert!(link.is_dir(), "symlink should be valid (target exists)");
        assert_eq!(
            read_link_target(&link).unwrap(),
            src,
            "symlink should point to correct source"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // sync: idempotent re-sync
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn idempotent_resync_preserves_symlink() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path().join("home");
        let skills = home.join(".agents/skills");
        let src = tmp.path().join("source-skills/my-skill");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("SKILL.md"), "# My Skill\n").unwrap();

        // First sync
        run_install(
            &["sync", "my-skill", &src.to_string_lossy()],
            &home,
            Some(&skills),
            None,
            None,
        );

        // Second sync (idempotent)
        let (_out, _err, code) = run_install(
            &["sync", "my-skill", &src.to_string_lossy()],
            &home,
            Some(&skills),
            None,
            None,
        );

        assert_eq!(code, 0, "idempotent re-sync should exit 0");
        let link = skills.join("my-skill");
        assert!(link.is_symlink(), "symlink should still exist");
        assert!(link.is_dir(), "symlink should still be valid");
        assert_eq!(
            read_link_target(&link).unwrap(),
            src,
            "symlink should still point to correct source"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // sync: broken symlink repair
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn broken_symlink_repair() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path().join("home");
        let skills = home.join(".agents/skills");
        let src = tmp.path().join("source-skills/my-skill");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("SKILL.md"), "# My Skill\n").unwrap();

        // Create initial symlink
        run_install(
            &["sync", "my-skill", &src.to_string_lossy()],
            &home,
            Some(&skills),
            None,
            None,
        );

        let link = skills.join("my-skill");
        // Break the symlink by removing the source
        fs::remove_dir_all(&src).unwrap();
        assert!(link.is_symlink(), "symlink should still exist");
        assert!(!link.is_dir(), "symlink should be broken (target missing)");

        // Recreate source
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("SKILL.md"), "# My Skill Restored\n").unwrap();

        // Repair
        let (_out, _err, code) = run_install(
            &["sync", "my-skill", &src.to_string_lossy()],
            &home,
            Some(&skills),
            None,
            None,
        );

        assert_eq!(code, 0, "broken symlink repair should exit 0");
        assert!(link.is_symlink(), "symlink should exist after repair");
        assert!(link.is_dir(), "symlink should be valid after repair");
        assert_eq!(
            read_link_target(&link).unwrap(),
            src,
            "repaired symlink should point to correct source"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // sync: wrong-target symlink replacement
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn wrong_target_symlink_replacement() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path().join("home");
        let skills = home.join(".agents/skills");
        let src = tmp.path().join("source-skills/my-skill");
        let other_src = tmp.path().join("source-skills/other-skill");

        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("SKILL.md"), "# My Skill\n").unwrap();
        fs::create_dir_all(&other_src).unwrap();
        fs::write(other_src.join("SKILL.md"), "# Other Skill\n").unwrap();

        // Ensure skills dir exists
        fs::create_dir_all(&skills).unwrap();

        // Manually create a symlink pointing to the wrong target
        symlink(&other_src, skills.join("my-skill")).unwrap();

        // Now sync to the original src — should replace
        let (_out, _err, code) = run_install(
            &["sync", "my-skill", &src.to_string_lossy()],
            &home,
            Some(&skills),
            None,
            None,
        );

        assert_eq!(code, 0, "wrong-target replacement should exit 0");
        let link = skills.join("my-skill");
        assert!(link.is_symlink(), "symlink should exist after replacement");
        assert_eq!(
            read_link_target(&link).unwrap(),
            src,
            "symlink should now point to correct source"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // sync: real directory conflict
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn real_directory_conflict_refuses_overwrite() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path().join("home");
        let skills = home.join(".agents/skills");
        let src = tmp.path().join("source-skills/conflict-skill");

        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("SKILL.md"), "# Conflict Skill\n").unwrap();

        // Create a real directory at the target location
        let conflict_dir = skills.join("conflict-skill");
        fs::create_dir_all(&conflict_dir).unwrap();
        fs::write(conflict_dir.join("important.txt"), "precious data\n").unwrap();

        let (out, err, code) = run_install(
            &["sync", "conflict-skill", &src.to_string_lossy()],
            &home,
            Some(&skills),
            None,
            None,
        );

        assert_ne!(code, 0, "real-dir conflict should exit non-zero");
        let combined = format!("{}{}", out, err);
        assert!(
            combined.to_lowercase().contains("real directory"),
            "should warn about real directory, got: {}",
            combined
        );
        assert!(conflict_dir.is_dir(), "real directory should still exist");
        assert!(
            conflict_dir.join("important.txt").is_file(),
            "precious file should be preserved"
        );
        assert!(!conflict_dir.is_symlink(), "no symlink should be created over real dir");
    }

    // ═══════════════════════════════════════════════════════════════════════
    // sync: missing source directory
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn missing_source_warns_and_exits_zero() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path().join("home");
        let skills = home.join(".agents/skills");
        let nonexistent = tmp.path().join("nonexistent-src");

        let (out, err, code) = run_install(
            &["sync", "ghost-skill", &nonexistent.to_string_lossy()],
            &home,
            Some(&skills),
            None,
            None,
        );

        assert_eq!(code, 0, "missing source should exit 0");
        let combined = format!("{}{}", out, err);
        assert!(
            combined.to_lowercase().contains("not exist"),
            "should warn about source not existing, got: {}",
            combined
        );
        assert!(
            !skills.join("ghost-skill").exists(),
            "no symlink should be created for missing source"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // unlink: parameter validation
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn unlink_zero_args_exits_nonzero() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path().join("home");
        fs::create_dir_all(&home).unwrap();

        let (out, err, code) = run_install(&["unlink"], &home, None, None, None);

        assert_ne!(code, 0, "unlink with 0 args should exit non-zero");
        let combined = format!("{}{}", out, err);
        assert!(
            combined.to_lowercase().contains("requires exactly one"),
            "unlink-0-args should print 'requires exactly one', got: {}",
            combined
        );
    }

    #[test]
    fn unlink_two_args_exits_nonzero() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path().join("home");
        fs::create_dir_all(&home).unwrap();

        let (out, err, code) = run_install(&["unlink", "a", "b"], &home, None, None, None);

        assert_ne!(code, 0, "unlink with 2 args should exit non-zero");
        let combined = format!("{}{}", out, err);
        assert!(
            combined.to_lowercase().contains("requires exactly one"),
            "unlink-2-args should print 'requires exactly one', got: {}",
            combined
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // unlink: PROJECT_ROOT symlink removal
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn unlink_project_root_symlink_removes() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path().join("home");
        let skills = home.join(".agents/skills");
        let src = tmp.path().join("source-skills/to-remove");
        let proj_root = tmp.path().to_path_buf();

        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("SKILL.md"), "# To Remove\n").unwrap();

        // Create a symlink whose target is under PROJECT_ROOT
        fs::create_dir_all(&skills).unwrap();
        symlink(&src, skills.join("to-remove")).unwrap();

        let (_out, _err, code) = run_install(
            &["unlink", "to-remove"],
            &home,
            Some(&skills),
            None,
            Some(&proj_root),
        );

        assert_eq!(code, 0, "unlink PROJECT_ROOT symlink should exit 0");
        assert!(
            !skills.join("to-remove").exists(),
            "symlink should be removed"
        );
        assert!(src.is_dir(), "source directory should be preserved");
        assert!(
            src.join("SKILL.md").is_file(),
            "source file should be preserved"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // unlink: non-PROJECT_ROOT symlink preserved
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn unlink_non_project_root_symlink_preserved() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path().join("home");
        let skills = home.join(".agents/skills");
        let external_target = tmp.path().join("external-target");

        fs::create_dir_all(&external_target).unwrap();
        fs::create_dir_all(&skills).unwrap();

        // Create a symlink pointing to a location — we don't set PROJECT_ROOT,
        // so the default PROJECT_ROOT will be the install.rs parent directory,
        // which is definitely not under tmp.
        symlink(&external_target, skills.join("external-link")).unwrap();

        let (out, err, code) = run_install(
            &["unlink", "external-link"],
            &home,
            Some(&skills),
            None,
            None,
        );

        // The symlink target (under /tmp) is not under the default PROJECT_ROOT,
        // so it should be preserved. Exit 0 is expected.
        assert_eq!(code, 0, "unlink non-PROJECT_ROOT symlink should exit 0");
        let combined = format!("{}{}", out, err);
        // Also check that the symlink is preserved
        let link = skills.join("external-link");
        assert!(
            link.is_symlink(),
            "non-PROJECT_ROOT symlink should be preserved, combined output: {}",
            combined
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // unlink: non-existent target no-op
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn unlink_nonexistent_target_noop() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path().join("home");
        let skills = home.join(".agents/skills");
        fs::create_dir_all(&skills).unwrap();

        let (_out, _err, code) = run_install(
            &["unlink", "nothing-here"],
            &home,
            Some(&skills),
            None,
            None,
        );

        assert_eq!(code, 0, "unlink non-existent target should exit 0");
    }

    // ═══════════════════════════════════════════════════════════════════════
    // unlink: real directory no-op
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn unlink_real_directory_noop() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path().join("home");
        let skills = home.join(".agents/skills");
        let real_dir = skills.join("my-real-dir");

        fs::create_dir_all(&real_dir).unwrap();
        fs::write(real_dir.join("data.txt"), "keep me\n").unwrap();

        let (_out, _err, code) = run_install(
            &["unlink", "my-real-dir"],
            &home,
            Some(&skills),
            None,
            None,
        );

        assert_eq!(code, 0, "unlink real directory should exit 0");
        assert!(real_dir.is_dir(), "real directory should be preserved");
        assert!(
            real_dir.join("data.txt").is_file(),
            "real directory file should be preserved"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // link-principles: parameter validation
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn link_principles_zero_args_exits_nonzero() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path().join("home");
        fs::create_dir_all(&home).unwrap();

        let (out, err, code) = run_install(&["link-principles"], &home, None, None, None);

        assert_ne!(code, 0, "link-principles with 0 args should exit non-zero");
        let combined = format!("{}{}", out, err);
        assert!(
            combined.to_lowercase().contains("requires exactly one"),
            "link-principles-0-args should print 'requires exactly one', got: {}",
            combined
        );
    }

    #[test]
    fn link_principles_two_args_exits_nonzero() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path().join("home");
        fs::create_dir_all(&home).unwrap();

        let (out, err, code) = run_install(&["link-principles", "a", "b"], &home, None, None, None);

        assert_ne!(code, 0, "link-principles with 2 args should exit non-zero");
        let combined = format!("{}{}", out, err);
        assert!(
            combined.to_lowercase().contains("requires exactly one"),
            "link-principles-2-args should print 'requires exactly one', got: {}",
            combined
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // link-principles: fresh creation
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn fresh_link_principles_creates_symlink() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path().join("home");
        let principles = home.join(".agents/principles");
        let src_principles = tmp.path().join("my-principles");

        fs::create_dir_all(&src_principles).unwrap();
        fs::write(src_principles.join("karpathy.md"), "Be curious.\n").unwrap();

        let (_out, _err, code) = run_install(
            &["link-principles", &src_principles.to_string_lossy()],
            &home,
            None,
            Some(&principles),
            None,
        );

        assert_eq!(code, 0, "link-principles fresh should exit 0");
        assert!(principles.is_symlink(), "principles symlink should exist");
        assert!(principles.is_dir(), "principles symlink should be valid");
        assert_eq!(
            read_link_target(&principles).unwrap(),
            src_principles,
            "principles symlink should point to source"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // link-principles: idempotent
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn idempotent_link_principles_preserves_symlink() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path().join("home");
        let principles = home.join(".agents/principles");
        let src_principles = tmp.path().join("my-principles");

        fs::create_dir_all(&src_principles).unwrap();
        fs::write(src_principles.join("karpathy.md"), "Be curious.\n").unwrap();

        // First call
        run_install(
            &["link-principles", &src_principles.to_string_lossy()],
            &home,
            None,
            Some(&principles),
            None,
        );

        // Second call (idempotent)
        let (_out, _err, code) = run_install(
            &["link-principles", &src_principles.to_string_lossy()],
            &home,
            None,
            Some(&principles),
            None,
        );

        assert_eq!(code, 0, "link-principles idempotent should exit 0");
        assert!(principles.is_symlink(), "principles symlink should still exist");
        assert!(principles.is_dir(), "principles symlink should still be valid");
        assert_eq!(
            read_link_target(&principles).unwrap(),
            src_principles,
            "principles symlink should still point to source"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // link-principles: broken symlink repair
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn link_principles_broken_symlink_repair() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path().join("home");
        let principles = home.join(".agents/principles");
        let src_principles = tmp.path().join("my-principles");

        fs::create_dir_all(&src_principles).unwrap();
        fs::write(src_principles.join("karpathy.md"), "Be curious.\n").unwrap();

        // Create initial symlink
        run_install(
            &["link-principles", &src_principles.to_string_lossy()],
            &home,
            None,
            Some(&principles),
            None,
        );

        // Break the symlink by removing the source
        fs::remove_dir_all(&src_principles).unwrap();
        assert!(principles.is_symlink(), "principles symlink should still exist");
        assert!(!principles.is_dir(), "principles symlink should be broken");

        // Call link-principles while source is still missing — should detect broken
        // symlink, remove it, and warn about missing source.
        let (out, err, code) = run_install(
            &["link-principles", &src_principles.to_string_lossy()],
            &home,
            None,
            Some(&principles),
            None,
        );

        assert_eq!(code, 0, "link-principles broken repair should exit 0");
        let combined = format!("{}{}", out, err);
        assert!(
            combined.to_lowercase().contains("not exist"),
            "should warn about source not existing, got: {}",
            combined
        );
        assert!(
            !principles.exists(),
            "broken symlink should be removed"
        );

        // Recreate source and run link-principles — should create fresh symlink
        fs::create_dir_all(&src_principles).unwrap();
        fs::write(src_principles.join("karpathy.md"), "Be curious. (restored)\n").unwrap();

        let (_out2, _err2, code2) = run_install(
            &["link-principles", &src_principles.to_string_lossy()],
            &home,
            None,
            Some(&principles),
            None,
        );

        assert_eq!(code2, 0, "link-principles fresh after repair should exit 0");
        assert!(
            principles.is_symlink() && principles.is_dir(),
            "principles symlink should be valid after repair"
        );
        assert_eq!(
            read_link_target(&principles).unwrap(),
            src_principles,
            "repaired principles symlink should point to source"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // link-principles: wrong-target symlink replacement
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn link_principles_wrong_target_replacement() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path().join("home");
        let principles = home.join(".agents/principles");
        let src_principles = tmp.path().join("my-principles");
        let other_principles = tmp.path().join("other-principles");

        fs::create_dir_all(&src_principles).unwrap();
        fs::write(src_principles.join("karpathy.md"), "Be curious.\n").unwrap();
        fs::create_dir_all(&other_principles).unwrap();
        fs::write(other_principles.join("README.md"), "Other principles.\n").unwrap();

        // Ensure parent dir exists
        fs::create_dir_all(principles.parent().unwrap()).unwrap();

        // Manually create symlink to wrong target
        symlink(&other_principles, &principles).unwrap();

        let (_out, _err, code) = run_install(
            &["link-principles", &src_principles.to_string_lossy()],
            &home,
            None,
            Some(&principles),
            None,
        );

        assert_eq!(code, 0, "link-principles wrong-target should exit 0");
        assert!(
            principles.is_symlink(),
            "principles symlink should exist after replacement"
        );
        assert_eq!(
            read_link_target(&principles).unwrap(),
            src_principles,
            "principles symlink should now point to correct source"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // link-principles: real directory conflict
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn link_principles_real_directory_conflict() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path().join("home");
        let principles = home.join(".agents/principles");
        let src_principles = tmp.path().join("my-principles");

        fs::create_dir_all(&src_principles).unwrap();
        fs::write(src_principles.join("karpathy.md"), "Be curious.\n").unwrap();

        // Create a real directory at the principles location
        fs::create_dir_all(&principles).unwrap();
        fs::write(principles.join("personal.md"), "precious principles\n").unwrap();

        let (out, err, code) = run_install(
            &["link-principles", &src_principles.to_string_lossy()],
            &home,
            None,
            Some(&principles),
            None,
        );

        assert_ne!(code, 0, "link-principles real-dir conflict should exit non-zero");
        let combined = format!("{}{}", out, err);
        assert!(
            combined.to_lowercase().contains("real directory"),
            "should warn about real directory, got: {}",
            combined
        );
        assert!(principles.is_dir(), "real principles directory should be preserved");
        assert!(
            principles.join("personal.md").is_file(),
            "real principles file should be preserved"
        );
        assert!(!principles.is_symlink(), "no symlink should be created over real principles dir");
    }

    // ═══════════════════════════════════════════════════════════════════════
    // link-principles: missing source directory
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn link_principles_missing_source_warns() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path().join("home");
        let principles = home.join(".agents/principles");
        let nonexistent = tmp.path().join("nonexistent-principles");

        // Ensure principles does not exist beforehand (start clean)
        let _ = fs::remove_dir_all(&principles);

        let (out, err, code) = run_install(
            &["link-principles", &nonexistent.to_string_lossy()],
            &home,
            None,
            Some(&principles),
            None,
        );

        assert_eq!(code, 0, "link-principles missing source should exit 0");
        let combined = format!("{}{}", out, err);
        assert!(
            combined.to_lowercase().contains("not exist"),
            "should warn about source not existing, got: {}",
            combined
        );
        assert!(
            !principles.exists(),
            "no symlink should be created for missing source"
        );
    }
}
