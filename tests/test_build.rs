#!/usr/bin/env rust-script
//! ```cargo
//! [dependencies]
//! serde = { version = "1", features = ["derive"] }
//! serde_json = "1"
//! tempfile = "3"
//! ```
//!
//! Integration tests for deploy.rs pack subcommand.
//! Build -> extract -> verify structure and metadata (AC 9).
//!
//! Run: rust-script --test tests/test_build.rs

use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("Run with: rust-script --test tests/test_build.rs");
}

// ── Helpers ─────────────────────────────────────────────────────────────

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

fn run_build(args: &[&str], project_root_override: Option<&Path>) -> (String, String, i32) {
    let script = install_script();
    assert!(script.exists(), "deploy.rs not found at {:?}", script);

    let mut cmd = Command::new("rust-script");
    cmd.arg(&script);
    for a in args {
        cmd.arg(a);
    }
    if let Some(r) = project_root_override {
        cmd.env("PROJECT_ROOT", r);
    }

    let output = cmd.output().expect("failed to run rust-script deploy.rs");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let code = output.status.code().unwrap_or(-1);

    (stdout, stderr, code)
}

fn git_rev_parse(repo: &Path) -> String {
    let output = Command::new("git")
        .args(["-C", &repo.to_string_lossy(), "rev-parse", "HEAD"])
        .output()
        .expect("git rev-parse failed");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

#[derive(Debug, Deserialize)]
struct Manifest {
    version: String,
    skills: HashMap<String, SkillEntry>,
}

#[derive(Debug, Deserialize)]
struct SkillEntry {
    #[serde(rename = "type")]
    skill_type: String,
    #[serde(default)]
    variants: Vec<String>,
    #[serde(default)]
    codex_agent: bool,
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_tests() {
        eprintln!("Running tests sequentially...");
        __build_produces_tarball();
        __build_tarball_structure_and_metadata();
        __build_creates_dist_dir_if_missing();
        __build_exits_nonzero_when_not_in_git_repo();
        __sync_still_works_after_build_changes();
        eprintln!("All tests passed!");
    }

    fn __build_produces_tarball() {
        let root = project_root();
        let dist_dir = root.join("dist");

        // Clean dist if it exists from a previous run
        if dist_dir.exists() {
            fs::remove_dir_all(&dist_dir).unwrap();
        }

        let git_hash = git_rev_parse(&root);
        assert!(!git_hash.is_empty(), "git rev-parse should return a hash");

        let tarball_name = "autopilot-toolkit.tar.gz";
        let tarball_path = dist_dir.join(tarball_name);

        // Run build
        let (out, err, code) = run_build(&["pack"], Some(&root));

        eprintln!("DEBUG pack exit code: {}", code);
        eprintln!("DEBUG pack stdout: {}", out);
        eprintln!("DEBUG pack stderr: {}", err);
        eprintln!("DEBUG project_root: {:?}", root);
        eprintln!(
            "DEBUG deploy.rs exists: {}",
            root.join("deploy.rs").exists()
        );
        eprintln!(
            "DEBUG templates/install.sh.in exists: {}",
            root.join("templates/install.sh.in").exists()
        );
        eprintln!(
            "DEBUG .skill-lock.json exists: {}",
            root.join(".skill-lock.json").exists()
        );

        assert_eq!(
            code, 0,
            "pack should exit 0, stderr: {}, stdout: {}",
            err, out
        );

        // AC 1: tarball exists
        assert!(
            tarball_path.is_file(),
            "tarball not found at {:?}, stdout: {}, stderr: {}",
            tarball_path,
            out,
            err
        );
    }

    fn __build_tarball_structure_and_metadata() {
        let _ = std::fs::remove_dir_all(project_root().join("dist"));
        let root = project_root();
        let git_hash = git_rev_parse(&root);
        assert!(!git_hash.is_empty());

        let dist_dir = root.join("dist");
        if dist_dir.exists() {
            fs::remove_dir_all(&dist_dir).unwrap();
        }

        let (out, err, code) = run_build(&["pack"], Some(&root));
        eprintln!("DEBUG2 pack exit code: {}", code);
        eprintln!("DEBUG2 pack stderr: {}", err);
        assert_eq!(
            code, 0,
            "pack should exit 0, stderr: {}, stdout: {}",
            err, out
        );

        let tarball_path = dist_dir.join("autopilot-toolkit.tar.gz");
        assert!(tarball_path.is_file());

        // Extract to temp dir
        let tmp = tempfile::tempdir().expect("tempdir");
        let extract_dir = tmp.path().join("extracted");
        fs::create_dir_all(&extract_dir).unwrap();

        let status = Command::new("tar")
            .args([
                "-xzf",
                &tarball_path.to_string_lossy(),
                "-C",
                &extract_dir.to_string_lossy(),
            ])
            .status()
            .expect("tar extract failed");
        assert!(status.success(), "tar extract should succeed");

        // ── AC 3: .autopilot/ contains bootstrap.sh, manifest.json, .version, .skill-lock.json ──

        let autopilot_dir = extract_dir.join(".autopilot");
        assert!(autopilot_dir.is_dir(), ".autopilot/ should exist");

        assert!(
            autopilot_dir.join("bootstrap.sh").is_file(),
            ".autopilot/bootstrap.sh should exist"
        );
        assert!(
            autopilot_dir.join("manifest.json").is_file(),
            ".autopilot/manifest.json should exist"
        );
        assert!(
            autopilot_dir.join(".version").is_file(),
            ".autopilot/.version should exist"
        );
        assert!(
            autopilot_dir.join(".skill-lock.json").is_file(),
            ".autopilot/.skill-lock.json should exist"
        );

        // ── AC 7: dist/install.sh is executable and embeds correct version ──
        let install_sh = project_root().join("dist").join("install.sh");
        assert!(install_sh.is_file(), "dist/install.sh should exist");
        let metadata = fs::metadata(&install_sh).unwrap();
        // Check executable bit (on Unix)
        use std::os::unix::fs::PermissionsExt;
        assert!(
            metadata.permissions().mode() & 0o111 != 0,
            "install.sh should be executable"
        );

        let install_content = fs::read_to_string(&install_sh).unwrap();
        assert!(
            install_content.contains(&git_hash),
            "install.sh should contain version hash '{}', got content: ...{}...",
            git_hash,
            &install_content[..install_content.len().min(200)]
        );
        assert!(
            !install_content.contains("__VERSION__"),
            "install.sh should not contain raw __VERSION__ placeholder"
        );

        // ── AC 6: .version matches git rev-parse HEAD ──
        let version_file = autopilot_dir.join(".version");
        let version_content = fs::read_to_string(&version_file)
            .unwrap()
            .trim()
            .to_string();
        assert_eq!(version_content, git_hash, ".version should match git hash");

        // ── AC 2: skills/ directory structure ──
        let skills_dir = extract_dir.join("skills");
        assert!(skills_dir.is_dir(), "skills/ should exist");

        // ── AC 4: principles/ exists ──
        assert!(
            extract_dir.join("principles").is_dir(),
            "principles/ should exist"
        );

        // ── AC 5: manifest.json classification ──
        let manifest_path = autopilot_dir.join("manifest.json");
        let manifest_bytes = fs::read_to_string(&manifest_path).unwrap();
        let manifest: Manifest =
            serde_json::from_str(&manifest_bytes).expect("manifest.json should be valid JSON");

        assert_eq!(
            manifest.version, git_hash,
            "manifest.version should match git hash"
        );

        // Check autopilot skills are present and correctly classified
        // Agnostic skills
        assert!(
            manifest.skills.contains_key("toolkit-setup"),
            "toolkit-setup should be in manifest"
        );
        let tks = &manifest.skills["toolkit-setup"];
        assert_eq!(
            tks.skill_type, "agnostic",
            "toolkit-setup should be agnostic"
        );
        assert!(!tks.codex_agent, "toolkit-setup should not be codex_agent");

        assert!(
            manifest.skills.contains_key("zoom-out"),
            "zoom-out should be in manifest"
        );
        let zo = &manifest.skills["zoom-out"];
        assert_eq!(zo.skill_type, "agnostic", "zoom-out should be agnostic");

        // Coupled skills
        for coupled_name in &[
            "autopilot-implementer",
            "autopilot-reviewer",
            "autopilot-orchestrator",
            "audit-autopilot",
        ] {
            assert!(
                manifest.skills.contains_key(*coupled_name),
                "{} should be in manifest",
                coupled_name
            );
            let entry = &manifest.skills[*coupled_name];
            assert_eq!(
                entry.skill_type, "coupled",
                "{} should be coupled, got {}",
                coupled_name, entry.skill_type
            );
            assert!(
                !entry.variants.is_empty(),
                "{} should have variants",
                coupled_name
            );
        }

        // implementer and reviewer should have codex_agent = true
        assert!(
            manifest.skills["autopilot-implementer"].codex_agent,
            "implementer should be codex_agent"
        );
        assert!(
            manifest.skills["autopilot-reviewer"].codex_agent,
            "reviewer should be codex_agent"
        );

        // Check upstream skills are present
        let upstream_expected = &[
            "diagnosing-bugs",
            "grill-with-docs",
            "improve-codebase-architecture",
            "prototype",
            "setup-matt-pocock-skills",
            "tdd",
            "to-issues",
            "to-prd",
            "triage",
            "ask-matt",
            "codebase-design",
            "domain-modeling",
            "implement",
            "resolving-merge-conflicts",
            "grill-me",
            "grilling",
            "handoff",
            "teach",
            "writing-great-skills",
        ];
        for name in upstream_expected {
            assert!(
                manifest.skills.contains_key(*name),
                "upstream skill '{}' should be in manifest",
                name
            );
            let entry = &manifest.skills[*name];
            assert_eq!(
                entry.skill_type, "upstream",
                "'{}' should be upstream, got {}",
                name, entry.skill_type
            );
            assert!(!entry.codex_agent, "'{}' should not be codex_agent", name);
        }

        // Verify upstream skill dirs exist as flat directories in skills/
        for name in upstream_expected {
            assert!(
                skills_dir.join(name).is_dir(),
                "skills/{} directory should exist for upstream skill",
                name
            );
        }

        // Verify all autopilot skill dirs exist
        for name in &[
            "toolkit-setup",
            "zoom-out",
            "autopilot-implementer",
            "autopilot-reviewer",
            "autopilot-orchestrator",
            "audit-autopilot",
        ] {
            assert!(
                skills_dir.join(name).is_dir(),
                "skills/{} directory should exist for autopilot skill",
                name
            );
        }
    }

    fn __build_creates_dist_dir_if_missing() {
        let _ = std::fs::remove_dir_all(project_root().join("dist"));
        let root = project_root();
        let dist_dir = root.join("dist");

        if dist_dir.exists() {
            fs::remove_dir_all(&dist_dir).unwrap();
        }

        let (out, err, code) = run_build(&["pack"], Some(&root));
        eprintln!("DEBUG2 pack exit code: {}", code);
        eprintln!("DEBUG2 pack stderr: {}", err);
        assert_eq!(
            code, 0,
            "pack should exit 0, stderr: {}, stdout: {}",
            err, out
        );

        assert!(dist_dir.is_dir(), "dist/ should be created");
    }

    fn __build_exits_nonzero_when_not_in_git_repo() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let mock_root = tmp.path().join("mock-project");
        fs::create_dir_all(&mock_root).unwrap();

        // Copy deploy.rs to mock project
        let real_install = install_script();
        fs::copy(&real_install, mock_root.join("deploy.rs")).unwrap();

        // Copy templates dir
        let real_templates = project_root().join("templates");
        if real_templates.exists() {
            let mock_templates = mock_root.join("templates");
            fs::create_dir_all(&mock_templates).unwrap();
            for entry in fs::read_dir(&real_templates).unwrap() {
                let entry = entry.unwrap();
                fs::copy(entry.path(), mock_templates.join(entry.file_name())).unwrap();
            }
        }

        // Copy bootstrap.sh
        let real_bootstrap = project_root().join("bootstrap.sh");
        if real_bootstrap.exists() {
            fs::copy(&real_bootstrap, mock_root.join("bootstrap.sh")).unwrap();
        }

        // Copy .skill-lock.json
        let real_lock = project_root().join(".skill-lock.json");
        if real_lock.exists() {
            fs::copy(&real_lock, mock_root.join(".skill-lock.json")).unwrap();
        }

        // Copy skills dirs (needed for build scanning)
        let real_skills = project_root().join("skills");
        if real_skills.exists() {
            let mock_skills = mock_root.join("skills");
            // Use cp -r for recursive copy
            let status = Command::new("cp")
                .args([
                    "-r",
                    &real_skills.to_string_lossy(),
                    &mock_skills.to_string_lossy(),
                ])
                .status()
                .expect("cp failed");
            assert!(status.success());
        }

        // Copy principles/
        let real_principles = project_root().join("principles");
        if real_principles.exists() {
            let status = Command::new("cp")
                .args([
                    "-r",
                    &real_principles.to_string_lossy(),
                    &mock_root.join("principles").to_string_lossy(),
                ])
                .status()
                .expect("cp failed");
            assert!(status.success());
        }

        let mock_install = mock_root.join("deploy.rs");

        let mut cmd = Command::new("rust-script");
        cmd.arg(&mock_install);
        cmd.arg("pack");
        cmd.env("PROJECT_ROOT", &mock_root);

        let output = cmd.output().expect("failed to run");
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let code = output.status.code().unwrap_or(-1);

        assert_ne!(code, 0, "build outside git repo should exit non-zero");
        assert!(
            stderr.to_lowercase().contains("git") || stderr.to_lowercase().contains("version"),
            "should mention git/version error, got: {}",
            stderr
        );
    }

    fn __sync_still_works_after_build_changes() {
        // AC 8: deploy.rs dev still works (dev flow unchanged)
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path().join("home");
        let skills = home.join(".agents/skills");

        let script = install_script();

        let mut cmd = Command::new("rust-script");
        cmd.arg(&script);
        cmd.arg("dev");
        cmd.env("HOME", &home);
        cmd.env("AGENTS_SKILLS_DIR", &skills);

        let output = cmd.output().expect("failed to run");
        let code = output.status.code().unwrap_or(-1);
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        assert_eq!(code, 0, "dev should exit 0, stderr: {}", stderr);
        // dev auto-discovers skills from the real project and symlinks them
        let count = std::fs::read_dir(&skills).unwrap().count();
        assert!(
            count > 0,
            "dev should create at least one symlink, got {}",
            count
        );
    }
}
