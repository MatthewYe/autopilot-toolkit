#!/usr/bin/env rust-script
//! ```cargo
//! [dependencies]
//! tempfile = "3"
//! ```
//!
//! Integration tests for deploy.rs CLI contract.
//! Run: rust-script --test tests/test_install.rs

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("Run with: rust-script --test tests/test_install.rs");
}

fn project_root() -> PathBuf {
    let src = Path::new(file!());
    if let (Some(_tests_dir), Some(proj)) = (src.parent(), src.parent().and_then(|p| p.parent())) {
        let candidate = proj.to_path_buf();
        if candidate.join("deploy.rs").exists() {
            return candidate;
        }
    }
    if let Ok(root) = std::env::var("PROJECT_ROOT") {
        let p = PathBuf::from(&root);
        if p.join("deploy.rs").exists() {
            return p;
        }
    }
    panic!("Cannot find project root (deploy.rs not found)");
}

fn install_script() -> PathBuf {
    project_root().join("deploy.rs")
}

fn run_deploy(
    args: &[&str],
    home: &Path,
    skills_dir: Option<&Path>,
    project_root_override: Option<&Path>,
) -> (String, String, i32) {
    let script = install_script();
    assert!(script.exists(), "deploy.rs not found at {:?}", script);

    let mut cmd = Command::new("rust-script");
    cmd.arg(&script);
    for a in args {
        cmd.arg(a);
    }
    cmd.env("HOME", home);
    if let Some(d) = skills_dir {
        cmd.env("AGENTS_SKILLS_DIR", d);
    }
    if let Some(r) = project_root_override {
        cmd.env("PROJECT_ROOT", r);
    }

    let output = cmd.output().expect("failed to run rust-script deploy.rs");
    (
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
        output.status.code().unwrap_or(-1),
    )
}

// ── setup mock project ──

fn setup_mock_project(root: &Path) {
    // Copy deploy.rs to mock project
    fs::copy(install_script(), root.join("deploy.rs")).unwrap();

    // Create minimal autopilot skill
    let agnostic = root.join("skills/autopilot/test-skill");
    fs::create_dir_all(&agnostic).unwrap();
    fs::write(
        agnostic.join("SKILL.md"),
        "---\nname: test-skill\ndescription: test\n---\n",
    )
    .unwrap();

    // Create coupled skill with variants
    let coupled = root.join("skills/autopilot/coupled-skill");
    for variant in &["reasonix", "codex", "kimi"] {
        let vdir = coupled.join(variant);
        fs::create_dir_all(&vdir).unwrap();
        fs::write(
            vdir.join("SKILL.md"),
            format!(
                "---\nname: coupled-skill\ndescription: {} variant\n---\n",
                variant
            ),
        )
        .unwrap();
    }
    // Codex agent.toml
    fs::write(
        coupled.join("codex/agent.toml"),
        "[agent]\nname = \"coupled-skill\"\n",
    )
    .unwrap();

    // Create principles/
    let principles = root.join("principles");
    fs::create_dir_all(&principles).unwrap();
    fs::write(principles.join("karpathy.md"), "# Principles\n").unwrap();

    // Create .skill-lock.json (empty, so no upstream skills)
    fs::write(
        root.join(".skill-lock.json"),
        r#"{"version":4,"skills":{}}"#,
    )
    .unwrap();

    // Create templates/install.sh.in
    let templates = root.join("templates");
    fs::create_dir_all(&templates).unwrap();
    fs::write(
        templates.join("install.sh.in"),
        "#!/bin/bash\nVERSION=\"__VERSION__\"\n",
    )
    .unwrap();
    fs::write(
        templates.join("uninstall.sh"),
        "#!/bin/bash\necho uninstall\n",
    )
    .unwrap();

    // Create bootstrap.sh
    fs::write(root.join("bootstrap.sh"), "#!/bin/bash\necho bootstrap\n").unwrap();

    // Init git repo so get_version works
    let _ = Command::new("git")
        .args(["init"])
        .current_dir(root)
        .output();
    let _ = Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(root)
        .output();
    let _ = Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(root)
        .output();
    let _ = Command::new("git")
        .args(["add", "-A"])
        .current_dir(root)
        .output();
    let _ = Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(root)
        .output();
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dev_creates_symlinks() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let skills = home.join(".agents/skills");
        let project = tmp.path().join("project");
        fs::create_dir_all(&project).unwrap();
        setup_mock_project(&project);

        let (out, err, code) = run_deploy(&["dev"], &home, Some(&skills), Some(&project));

        assert_eq!(
            code, 0,
            "dev should exit 0, stderr: {}, stdout: {}",
            err, out
        );

        // Agnostic skill symlinked to ~/.agents/skills/
        let link = skills.join("test-skill");
        assert!(link.is_symlink(), "test-skill should be a symlink");
        assert!(link.is_dir(), "symlink should resolve to a directory");

        // Coupled skill: kimi variant in ~/.agents/skills/
        let coupled_link = skills.join("coupled-skill");
        assert!(
            coupled_link.is_symlink(),
            "coupled-skill should be a symlink"
        );
    }

    #[test]
    fn dev_clean_removes_symlinks() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let skills = home.join(".agents/skills");
        let project = tmp.path().join("project");
        fs::create_dir_all(&project).unwrap();
        setup_mock_project(&project);

        // First dev
        let (_, _, code) = run_deploy(&["dev"], &home, Some(&skills), Some(&project));
        assert_eq!(code, 0);
        assert!(skills.join("test-skill").is_symlink());

        // Then dev-clean
        let (out, err, code) = run_deploy(&["dev-clean"], &home, Some(&skills), Some(&project));
        assert_eq!(
            code, 0,
            "dev-clean should exit 0, stderr: {}, stdout: {}",
            err, out
        );

        assert!(
            !skills.join("test-skill").exists(),
            "symlink should be removed"
        );
    }

    #[test]
    fn pack_creates_tarball() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let skills = home.join(".agents/skills");
        let project = tmp.path().join("project");
        fs::create_dir_all(&project).unwrap();
        setup_mock_project(&project);

        let (out, err, code) = run_deploy(&["pack"], &home, Some(&skills), Some(&project));

        assert_eq!(
            code, 0,
            "pack should exit 0, stderr: {}, stdout: {}",
            err, out
        );

        let tarball = project.join("dist/autopilot-toolkit.tar.gz");
        assert!(
            tarball.is_file(),
            "tarball should exist at {:?}, out: {}",
            tarball,
            out
        );
    }

    #[test]
    fn pack_creates_install_script() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let skills = home.join(".agents/skills");
        let project = tmp.path().join("project");
        fs::create_dir_all(&project).unwrap();
        setup_mock_project(&project);

        let (_, _, code) = run_deploy(&["pack"], &home, Some(&skills), Some(&project));
        assert_eq!(code, 0);

        let install_sh = project.join("dist/install.sh");
        assert!(install_sh.is_file(), "install.sh should exist");
        let content = fs::read_to_string(&install_sh).unwrap();
        assert!(
            !content.contains("__VERSION__"),
            "install.sh should have substituted VERSION"
        );
    }

    #[test]
    fn dev_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let skills = home.join(".agents/skills");
        let project = tmp.path().join("project");
        fs::create_dir_all(&project).unwrap();
        setup_mock_project(&project);

        // First run
        let (_, _, code) = run_deploy(&["dev"], &home, Some(&skills), Some(&project));
        assert_eq!(code, 0);

        let _mtime1 = fs::symlink_metadata(skills.join("test-skill"))
            .unwrap()
            .modified()
            .unwrap();

        // Second run
        let (out, err, code) = run_deploy(&["dev"], &home, Some(&skills), Some(&project));
        assert_eq!(
            code, 0,
            "second dev should exit 0, stderr: {}, stdout: {}",
            err, out
        );

        let link = skills.join("test-skill");
        assert!(link.is_symlink(), "symlink should still exist");
    }

    #[test]
    fn unknown_subcommand_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let skills = home.join(".agents/skills");

        let (out, err, code) = run_deploy(&["nonexistent"], &home, Some(&skills), None);

        assert_ne!(code, 0, "unknown subcommand should exit non-zero");
        let combined = format!("{}{}", out, err);
        assert!(
            combined.contains("unknown subcommand"),
            "should mention unknown subcommand, got: {}",
            combined
        );
    }

    #[test]
    fn no_args_runs_pack() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let skills = home.join(".agents/skills");
        let project = tmp.path().join("project");
        fs::create_dir_all(&project).unwrap();
        setup_mock_project(&project);

        // No args: runs pack then release.
        // Pack should always succeed; release may fail (no gh, no remote).
        let (out, _err, _code) = run_deploy(&[], &home, Some(&skills), Some(&project));

        // Pack should have run — verify tarball exists
        let tarball = project.join("dist/autopilot-toolkit.tar.gz");
        assert!(
            tarball.is_file(),
            "pack should have run, tarball not found. stdout: {}",
            out
        );
    }
}
