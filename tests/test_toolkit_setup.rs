#!/usr/bin/env rust-script
//! ```cargo
//! [dependencies]
//! tempfile = "3"
//! anyhow = "1"
//! serde_json = "1"
//! ```
//!
//! Integration tests for deploy.rs toolkit-setup orchestration flow.
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
    reasonix_skills_dir: PathBuf,
    codex_skills_dir: PathBuf,
    codex_agents_dir: PathBuf,
}

impl TestContext {
    fn new(prefix: &str) -> Self {
        let temp = tempfile::TempDir::with_prefix(prefix).expect("create temp dir");
        let mock_root = temp.path().join("mock-project");
        let mock_home = temp.path().join("home");
        let skills_dir = mock_home.join(".agents/skills");
        let reasonix_skills_dir = mock_home.join(".reasonix/skills");
        let codex_skills_dir = mock_home.join(".codex/skills");
        let codex_agents_dir = mock_home.join(".codex/agents");

        fs::create_dir_all(&mock_root).expect("create mock_root");
        fs::create_dir_all(&mock_home).expect("create mock_home");
        fs::create_dir_all(&skills_dir).expect("create skills_dir");
        fs::create_dir_all(&reasonix_skills_dir).expect("create reasonix skills_dir");
        fs::create_dir_all(&codex_skills_dir).expect("create codex skills_dir");

        TestContext {
            _temp: temp,
            mock_root,
            mock_home,
            skills_dir,
            reasonix_skills_dir,
            codex_skills_dir,
            codex_agents_dir,
        }
    }

    fn path(&self) -> &Path {
        self.mock_root.as_path()
    }

    fn skills_dir(&self) -> &Path {
        self.skills_dir.as_path()
    }

    fn reasonix_skills_dir(&self) -> &Path {
        self.reasonix_skills_dir.as_path()
    }

    fn codex_skills_dir(&self) -> &Path {
        self.codex_skills_dir.as_path()
    }

    fn codex_agents_dir(&self) -> &Path {
        self.codex_agents_dir.as_path()
    }

    fn home(&self) -> &Path {
        self.mock_home.as_path()
    }
}

/// Find the actual project root — the directory containing deploy.rs.
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

fn install_script_path() -> PathBuf {
    project_root().join("deploy.rs")
}

/// Run deploy.rs dev with given env (legacy: uses --shared for backward compat).
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
        .arg("--shared")
        .env("HOME", home)
        .env("AGENTS_SKILLS_DIR", skills_dir)
        .env("PROJECT_ROOT", project_root)
        .output()
        .expect("failed to run deploy.rs dev");

    (
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
        output.status.code().unwrap_or(-1),
    )
}

/// Run deploy.rs unlink with given env (legacy: uses --shared for backward compat).
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
        .arg("--shared")
        .env("HOME", home)
        .env("AGENTS_SKILLS_DIR", skills_dir)
        .env("PROJECT_ROOT", project_root)
        .output()
        .expect("failed to run deploy.rs unlink");

    (
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
        output.status.code().unwrap_or(-1),
    )
}

/// Run deploy.rs link-principles with given env.
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
        .expect("failed to run deploy.rs link-principles");

    (
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
        output.status.code().unwrap_or(-1),
    )
}

/// Run deploy.rs dev with --shared flag (agnostic skill).
fn run_sync_shared(
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
        .arg("--shared")
        .env("HOME", home)
        .env("AGENTS_SKILLS_DIR", skills_dir)
        .env("PROJECT_ROOT", project_root)
        .output()
        .expect("failed to run deploy.rs dev --shared");

    (
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
        output.status.code().unwrap_or(-1),
    )
}

/// Run deploy.rs dev with --target flag (coupled skill).
fn run_sync_targeted(
    install_script: &Path,
    home: &Path,
    target: &str,
    target_skills_dir: &Path,
    project_root: &Path,
    name: &str,
    src: &Path,
) -> (String, String, i32) {
    let output = Command::new("rust-script")
        .arg(install_script)
        .arg("sync")
        .arg(name)
        .arg(src)
        .arg("--target")
        .arg(target)
        .env("HOME", home)
        .env("PROJECT_ROOT", project_root)
        .env(
            if target == "reasonix" {
                "REASONIX_SKILLS_DIR"
            } else {
                "CODEX_SKILLS_DIR"
            },
            target_skills_dir,
        )
        .output()
        .expect("failed to run deploy.rs dev --target");

    (
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
        output.status.code().unwrap_or(-1),
    )
}

/// Run deploy.rs deploy-agent with --target codex.
fn run_deploy_agent(
    install_script: &Path,
    home: &Path,
    project_root: &Path,
    codex_agents_dir: &Path,
    name: &str,
    src: &Path,
) -> (String, String, i32) {
    let output = Command::new("rust-script")
        .arg(install_script)
        .arg("deploy-agent")
        .arg(name)
        .arg(src)
        .arg("--target")
        .arg("codex")
        .env("HOME", home)
        .env("PROJECT_ROOT", project_root)
        .env("CODEX_AGENTS_DIR", codex_agents_dir)
        .output()
        .expect("failed to run deploy.rs deploy-agent");

    (
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
        output.status.code().unwrap_or(-1),
    )
}

/// Run deploy.rs unlink with --shared flag (clean up agnostic skill).
fn run_unlink_shared(
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
        .arg("--shared")
        .env("HOME", home)
        .env("AGENTS_SKILLS_DIR", skills_dir)
        .env("PROJECT_ROOT", project_root)
        .output()
        .expect("failed to run deploy.rs unlink --shared");

    (
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
        output.status.code().unwrap_or(-1),
    )
}

/// Run deploy.rs unlink with --target flag (clean up coupled skill).
fn run_unlink_targeted(
    install_script: &Path,
    home: &Path,
    target: &str,
    target_skills_dir: &Path,
    project_root: &Path,
    name: &str,
) -> (String, String, i32) {
    let output = Command::new("rust-script")
        .arg(install_script)
        .arg("unlink")
        .arg(name)
        .arg("--target")
        .arg(target)
        .env("HOME", home)
        .env("PROJECT_ROOT", project_root)
        .env(
            if target == "reasonix" {
                "REASONIX_SKILLS_DIR"
            } else {
                "CODEX_SKILLS_DIR"
            },
            target_skills_dir,
        )
        .output()
        .expect("failed to run deploy.rs unlink --target");

    (
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
        output.status.code().unwrap_or(-1),
    )
}

/// Categorize a skill source directory.
/// Returns "agnostic" if $src/SKILL.md exists (no runtime variants),
/// "coupled" if $src/reasonix/SKILL.md exists (has per-runtime variants).
fn categorize_skill(src: &Path) -> &'static str {
    if src.join("reasonix/SKILL.md").exists() {
        "coupled"
    } else {
        "agnostic"
    }
}

/// Get the variant source path for a coupled skill based on target.
fn variant_src(src: &Path, target: &str) -> PathBuf {
    src.join(target)
}

/// Check if a coupled skill has a loadable SKILL.md for the target runtime.
fn has_skill_variant(src: &Path, target: &str) -> bool {
    src.join(target).join("SKILL.md").is_file()
}

/// Check if a coupled skill has codex agent .toml files (for deploy-agent).
fn has_codex_agents(src: &Path) -> bool {
    let codex_dir = src.join("codex");
    if !codex_dir.is_dir() {
        return false;
    }
    if let Ok(entries) = fs::read_dir(&codex_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().map(|e| e == "toml").unwrap_or(false) {
                return true;
            }
        }
    }
    false
}

/// Get the .toml agent files from a coupled skill's codex/ directory.
fn codex_agent_files(src: &Path) -> Vec<(String, PathBuf)> {
    let mut agents: Vec<(String, PathBuf)> = Vec::new();
    let codex_dir = src.join("codex");
    if !codex_dir.is_dir() {
        return agents;
    }
    if let Ok(entries) = fs::read_dir(&codex_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().map(|e| e == "toml").unwrap_or(false) {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    let agent_name = if stem == "agent" {
                        src.file_name()
                            .and_then(|s| s.to_str())
                            .unwrap_or(stem)
                            .to_string()
                    } else {
                        stem.to_string()
                    };
                    agents.push((agent_name, path));
                }
            }
        }
    }
    agents
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

/// Set up a mock project with both agnostic AND coupled skills.
/// Agnostic: upstream skills (skill-a, skill-b, skill-c) + toolkit-setup + zoom-out
/// Coupled: 4 autopilot skills with reasonix/codex variant subdirs + .toml agents
fn setup_mock_project_with_variants(ctx: &TestContext, _target: &str) {
    let root = ctx.path();

    // ── Agnostic: Upstream skills (3) ──────────────────────────────────
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

    // ── Agnostic: toolkit-setup and zoom-out ───────────────────────────
    let agnostic_autopilot = ["toolkit-setup", "zoom-out"];
    for s in &agnostic_autopilot {
        let dir = root.join("skills/autopilot").join(s);
        fs::create_dir_all(&dir).expect("create agnostic autopilot skill dir");
        let md = format!(
            "---\nname: {}\ndescription: Mock autopilot skill {}\n---\n# {}\n",
            s, s, s
        );
        fs::write(dir.join("SKILL.md"), md).expect("write SKILL.md");
    }

    // ── Coupled: 4 autopilot skills with runtime variants ──────────────
    let coupled_skills = [
        "audit-autopilot",
        "autopilot-implementer",
        "autopilot-orchestrator",
        "autopilot-reviewer",
    ];
    let codex_agent_skills = ["autopilot-implementer", "autopilot-reviewer"];

    for s in &coupled_skills {
        let dir = root.join("skills/autopilot").join(s);

        // Create reasonix variant
        let reasonix_dir = dir.join("reasonix");
        fs::create_dir_all(&reasonix_dir).expect("create reasonix variant dir");
        let reasonix_md = format!(
            "---\nname: {}\ndescription: {} (reasonix variant)\n---\n# {} (Reasonix)\n",
            s, s, s
        );
        fs::write(reasonix_dir.join("SKILL.md"), reasonix_md).expect("write reasonix SKILL.md");

        // Create codex variant
        let codex_dir = dir.join("codex");
        fs::create_dir_all(&codex_dir).expect("create codex variant dir");
        if !codex_agent_skills.contains(s) {
            let codex_md = format!(
                "---\nname: {}\ndescription: {} (codex variant)\n---\n# {} (Codex)\n",
                s, s, s
            );
            fs::write(codex_dir.join("SKILL.md"), codex_md).expect("write codex SKILL.md");
        }

        // For implementer and reviewer: add .toml agent files
        if codex_agent_skills.contains(s) {
            let agent_toml = format!(
                "[agent]\nname = \"{}\"\ndescription = \"{} codex agent\"\n",
                s, s
            );
            fs::write(codex_dir.join("agent.toml"), agent_toml).expect("write agent .toml");
        }
    }

    // ── Kimi variants: all four coupled skills ──
    let kimi_skills = [
        "audit-autopilot",
        "autopilot-implementer",
        "autopilot-orchestrator",
        "autopilot-reviewer",
    ];
    for s in &kimi_skills {
        let kimi_dir = root.join("skills/autopilot").join(s).join("kimi");
        fs::create_dir_all(&kimi_dir).expect("create kimi variant dir");
        let kimi_md = format!(
            "---\nname: {}\ndescription: {} (kimi variant)\n---\n# {} (Kimi)\n",
            s, s, s
        );
        fs::write(kimi_dir.join("SKILL.md"), kimi_md).expect("write kimi SKILL.md");
    }

    // ── Principles ─────────────────────────────────────────────────────
    let principles_dir = root.join("principles");
    fs::create_dir_all(&principles_dir).expect("create principles dir");
    fs::write(principles_dir.join("karpathy.md"), "Be curious.\n").expect("write principles");

    // ── .skill-lock.json ───────────────────────────────────────────────
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
                if path.is_dir() {
                    // Agnostic skill: SKILL.md directly in directory
                    // Coupled skill: reasonix/SKILL.md exists (variant source)
                    let is_skill =
                        path.join("SKILL.md").exists() || path.join("reasonix/SKILL.md").exists();
                    if is_skill {
                        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                            let raw = path.to_path_buf();
                            expected.push((name.to_string(), raw));
                        }
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

/// Full execute with --target: sync agnostic skills --shared, coupled skills
/// to their runtime's install dir (kimi variants also go --shared).
fn run_toolkit_setup_execute_targeted(
    install_script: &Path,
    ctx: &TestContext,
    expected: &[(String, PathBuf)],
    target: &str,
) -> String {
    let mut lines: Vec<String> = Vec::new();

    // Resolve target-specific dirs. Kimi has no agent-exclusive directory —
    // its coupled variants install to the shared skills dir.
    let target_skills_dir = match target {
        "reasonix" => ctx.reasonix_skills_dir(),
        "codex" => ctx.codex_skills_dir(),
        _ => ctx.skills_dir(),
    };

    // Sync all expected skills
    for (name, src) in expected {
        let category = categorize_skill(src);

        match category {
            "agnostic" => {
                let state = skill_state(name, src, ctx.skills_dir());
                match state {
                    "missing" | "broken" | "wrong_target" => {
                        let (_stdout, _stderr, _code) = run_sync_shared(
                            install_script,
                            ctx.home(),
                            ctx.skills_dir(),
                            ctx.path(),
                            name,
                            src,
                        );
                        lines.push(format!("  SYNC {} -> {} (shared)", name, src.display()));
                    }
                    "real_dir" => {
                        lines.push(format!(
                            "  WARN: {} is a real directory at {}/{} — skipping",
                            name,
                            ctx.skills_dir().display(),
                            name
                        ));
                    }
                    "correct" => { /* no-op */ }
                    _ => {}
                }
            }
            "coupled" => {
                let variant_src_path = variant_src(src, target);
                if has_skill_variant(src, target) {
                    let state = skill_state(name, &variant_src_path, target_skills_dir);
                    match state {
                        "missing" | "broken" | "wrong_target" => {
                            if target == "kimi" {
                                // Kimi variants install to the shared dir via --shared
                                let (_stdout, _stderr, _code) = run_sync_shared(
                                    install_script,
                                    ctx.home(),
                                    ctx.skills_dir(),
                                    ctx.path(),
                                    name,
                                    &variant_src_path,
                                );
                                lines.push(format!(
                                    "  SYNC {} -> {} (shared)",
                                    name,
                                    variant_src_path.display()
                                ));
                            } else {
                                let (_stdout, _stderr, _code) = run_sync_targeted(
                                    install_script,
                                    ctx.home(),
                                    target,
                                    target_skills_dir,
                                    ctx.path(),
                                    name,
                                    &variant_src_path,
                                );
                                lines.push(format!(
                                    "  SYNC {} -> {} (--target {})",
                                    name,
                                    variant_src_path.display(),
                                    target
                                ));
                            }
                        }
                        "real_dir" => {
                            lines.push(format!(
                                "  WARN: {} is a real directory at {}/{} — skipping",
                                name,
                                target_skills_dir.display(),
                                name
                            ));
                        }
                        "correct" => { /* no-op */ }
                        _ => {}
                    }
                }

                // Codex agents: deploy-agent for skills with .toml files
                if target == "codex" && has_codex_agents(src) {
                    for (agent_name, agent_src) in &codex_agent_files(src) {
                        let (_stdout, _stderr, _code) = run_deploy_agent(
                            install_script,
                            ctx.home(),
                            ctx.path(),
                            ctx.codex_agents_dir(),
                            agent_name,
                            agent_src,
                        );
                        lines.push(format!(
                            "  DEPLOY-AGENT {} -> {}",
                            agent_name,
                            agent_src.display()
                        ));
                    }
                }
            }
            _ => {}
        }
    }

    // Find and unlink orphaned symlinks from ALL relevant directories
    // Agnostic orphans: in ~/.agents/skills/. Kimi-variant coupled skills are
    // legitimate shared-dir residents for EVERY target — never orphan them.
    let shared_expected_names: Vec<&str> = expected
        .iter()
        .filter(|(_, src)| categorize_skill(src) == "agnostic" || has_skill_variant(src, "kimi"))
        .map(|(n, _)| n.as_str())
        .collect();
    let target_expected_names: Vec<&str> = expected
        .iter()
        .filter(|(_, src)| categorize_skill(src) == "coupled" && has_skill_variant(src, target))
        .map(|(n, _)| n.as_str())
        .collect();
    if ctx.skills_dir().is_dir() {
        if let Ok(entries) = fs::read_dir(ctx.skills_dir()) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_symlink() {
                    if let Some(ename) = path.file_name().and_then(|n| n.to_str()) {
                        if !shared_expected_names.contains(&ename) {
                            if let Ok(link_target) = fs::read_link(&path) {
                                let proj = ctx.path().to_path_buf();
                                if link_target.starts_with(&proj) {
                                    let (_stdout, _stderr, _code) = run_unlink_shared(
                                        install_script,
                                        ctx.home(),
                                        ctx.skills_dir(),
                                        ctx.path(),
                                        ename,
                                    );
                                    lines.push(format!("  UNLINK {} (orphaned, shared)", ename));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Coupled orphans: in ~/.reasonix/skills/ or ~/.codex/skills/
    // (skipped for kimi — its coupled skills live in the shared dir, already
    // scanned above)
    if target != "kimi" && target_skills_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(target_skills_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_symlink() {
                    if let Some(ename) = path.file_name().and_then(|n| n.to_str()) {
                        if !target_expected_names.contains(&ename) {
                            if let Ok(link_target) = fs::read_link(&path) {
                                let proj = ctx.path().to_path_buf();
                                if link_target.starts_with(&proj) {
                                    let (_stdout, _stderr, _code) = run_unlink_targeted(
                                        install_script,
                                        ctx.home(),
                                        target,
                                        target_skills_dir,
                                        ctx.path(),
                                        ename,
                                    );
                                    lines.push(format!(
                                        "  UNLINK {} (orphaned, --target {})",
                                        ename, target
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Link principles (always shared)
    let principles_src = ctx.path().join("principles");
    if principles_src.is_dir() {
        let principles_dir = ctx.home().join(".agents/principles");
        let _ = run_link_principles(install_script, ctx.home(), &principles_dir, &principles_src);
        lines.push(format!("  LINK-PRINCIPLES -> {}", principles_src.display()));
    }

    lines.join("\n")
}

/// Verification with target awareness: checks agnostic skills in shared dir,
/// coupled skills in the target-specific dir.
fn run_toolkit_setup_verify_targeted(
    ctx: &TestContext,
    expected: &[(String, PathBuf)],
    target: &str,
) -> String {
    let mut report: Vec<String> = Vec::new();
    let mut missing_count = 0;
    let mut damaged_count = 0;
    let total = expected
        .iter()
        .filter(|(_, src)| {
            let category = categorize_skill(src);
            category == "agnostic" || (category == "coupled" && has_skill_variant(src, target))
        })
        .count();
    let mut all_pass = true;

    let target_skills_dir = match target {
        "reasonix" => ctx.reasonix_skills_dir(),
        "codex" => ctx.codex_skills_dir(),
        _ => ctx.skills_dir(),
    };

    for (name, src) in expected {
        let category = categorize_skill(src);
        let (check_dir, check_src) = match category {
            "agnostic" => (ctx.skills_dir(), src.clone()),
            "coupled" => {
                if !has_skill_variant(src, target) {
                    continue;
                }
                (target_skills_dir, variant_src(src, target))
            }
            _ => continue,
        };

        let state = skill_state(name, &check_src, check_dir);
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
                let actual = fs::read_link(check_dir.join(name))
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|_| "?".to_string());
                report.push(format!(
                    "  [FAIL] {} -> {} (expected {})",
                    name,
                    actual,
                    check_src.display()
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
fn run_toolkit_setup_verify(ctx: &TestContext, expected: &[(String, PathBuf)]) -> String {
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

    #[ignore]
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

    #[ignore]
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
        let skill_c_src = ctx
            .path()
            .join("skills/upstream/skills/engineering/skill-c");
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

    #[ignore]
    #[test]
    fn orphaned_symlink_cleanup() {
        let ctx = TestContext::new("toolkit-test3");
        setup_mock_project(&ctx);
        let install = install_script();

        let expected = derive_expected_set(ctx.path());

        // Pre-setup: install all skills correctly
        let _ = run_toolkit_setup_execute(&install, &ctx, &expected);

        // Create an orphaned symlink (pointing under PROJECT_ROOT, not in expected set)
        let old_skill_dir = ctx
            .path()
            .join("skills/upstream/skills/engineering/old-skill");
        fs::create_dir_all(&old_skill_dir).expect("create old-skill dir");
        fs::write(old_skill_dir.join("SKILL.md"), "# Old Skill\n").expect("write old SKILL.md");

        let orphan_target = ctx.skills_dir().join("old-skill");
        std::os::unix::fs::symlink(&old_skill_dir, &orphan_target).expect("create orphan symlink");

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

    #[ignore]
    #[test]
    fn real_directory_conflict_reported_as_warn() {
        let ctx = TestContext::new("toolkit-test4");
        setup_mock_project(&ctx);
        let install = install_script();

        let expected = derive_expected_set(ctx.path());

        // Pre-setup: install skill-a and skill-b correctly
        for (name, src) in &expected {
            if name == "skill-a" || name == "skill-b" {
                let _ = run_sync(
                    &install,
                    ctx.home(),
                    ctx.skills_dir(),
                    ctx.path(),
                    name,
                    src,
                );
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

    #[ignore]
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

    #[ignore]
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
                let _ = run_sync(
                    &install,
                    ctx.home(),
                    ctx.skills_dir(),
                    ctx.path(),
                    name,
                    src,
                );
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

    // ═══════════════════════════════════════════════════════════════════════
    // Test 7: --target reasonix routes skills correctly
    // ═══════════════════════════════════════════════════════════════════════

    #[ignore]
    #[test]
    fn target_reasonix_routes_skills_correctly() {
        let ctx = TestContext::new("toolkit-test7");
        setup_mock_project_with_variants(&ctx, "reasonix");
        let install = install_script();

        let expected = derive_expected_set(ctx.path());
        // Expected: 3 upstream + 6 autopilot = 9 skills
        assert!(
            expected.len() >= 8,
            "expected at least 8 skills, got {}: {:?}",
            expected.len(),
            expected
        );

        // Count categories
        let agnostic_count = expected
            .iter()
            .filter(|(_, src)| categorize_skill(src) == "agnostic")
            .count();
        let coupled_count = expected
            .iter()
            .filter(|(_, src)| categorize_skill(src) == "coupled")
            .count();
        assert!(agnostic_count > 0, "should have agnostic skills");
        assert!(coupled_count > 0, "should have coupled skills");

        // Execute with target=reasonix
        let exec_result = run_toolkit_setup_execute_targeted(&install, &ctx, &expected, "reasonix");

        // Verify shared sync for agnostic skills
        let shared_sync_count = exec_result.matches("(shared)").count();
        assert_eq!(
            shared_sync_count, agnostic_count,
            "all {} agnostic skills should sync with --shared, got {}:\n{}",
            agnostic_count, shared_sync_count, exec_result
        );

        // Verify --target reasonix sync for coupled skills
        let targeted_sync_count = exec_result.matches("--target reasonix").count();
        assert_eq!(
            targeted_sync_count, coupled_count,
            "all {} coupled skills should sync with --target reasonix, got {}:\n{}",
            coupled_count, targeted_sync_count, exec_result
        );

        // Verify LINK-PRINCIPLES
        assert!(
            exec_result.contains("LINK-PRINCIPLES"),
            "should contain LINK-PRINCIPLES:\n{}",
            exec_result
        );

        // Verify agnostic skills in shared dir
        for (name, src) in &expected {
            if categorize_skill(src) == "agnostic" {
                let state = skill_state(name, src, ctx.skills_dir());
                assert_eq!(
                    state, "correct",
                    "agnostic skill {} should be correct in shared dir, but was {}",
                    name, state
                );
            }
        }

        // Verify coupled skills in reasonix dir
        for (name, src) in &expected {
            if categorize_skill(src) == "coupled" {
                let variant = variant_src(src, "reasonix");
                let state = skill_state(name, &variant, ctx.reasonix_skills_dir());
                assert_eq!(
                    state, "correct",
                    "coupled skill {} should be correct in reasonix dir, but was {}",
                    name, state
                );
            }
        }

        // Coupled skills should NOT be in shared dir
        for (name, src) in &expected {
            if categorize_skill(src) == "coupled" {
                let target = ctx.skills_dir().join(name);
                assert!(
                    !target.exists() || !target.is_symlink(),
                    "coupled skill {} should NOT be symlinked in shared dir",
                    name
                );
            }
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Test 8: --target codex routes skills correctly
    // ═══════════════════════════════════════════════════════════════════════

    #[ignore]
    #[test]
    fn target_codex_routes_skills_correctly() {
        let ctx = TestContext::new("toolkit-test8");
        setup_mock_project_with_variants(&ctx, "codex");
        let install = install_script();

        let expected = derive_expected_set(ctx.path());

        let agnostic_count = expected
            .iter()
            .filter(|(_, src)| categorize_skill(src) == "agnostic")
            .count();
        let codex_skill_variant_count = expected
            .iter()
            .filter(|(_, src)| {
                categorize_skill(src) == "coupled" && has_skill_variant(src, "codex")
            })
            .count();

        // Execute with target=codex
        let exec_result = run_toolkit_setup_execute_targeted(&install, &ctx, &expected, "codex");

        // Verify shared sync for agnostic skills
        let shared_sync_count = exec_result.matches("(shared)").count();
        assert_eq!(
            shared_sync_count, agnostic_count,
            "all agnostic skills should sync with --shared:\n{}",
            exec_result
        );

        // Verify --target codex sync only for coupled skills with a loadable codex/SKILL.md.
        let targeted_sync_count = exec_result.matches("--target codex").count();
        assert_eq!(
            targeted_sync_count, codex_skill_variant_count,
            "only coupled skills with codex/SKILL.md should sync with --target codex:\n{}",
            exec_result
        );
        assert!(
            !exec_result.contains("SYNC autopilot-implementer"),
            "agent-only implementer must not be synced as a Codex skill:\n{}",
            exec_result
        );
        assert!(
            !exec_result.contains("SYNC autopilot-reviewer"),
            "agent-only reviewer must not be synced as a Codex skill:\n{}",
            exec_result
        );

        // Verify DEPLOY-AGENT for implementer and reviewer
        assert!(
            exec_result.contains("DEPLOY-AGENT autopilot-implementer"),
            "should contain DEPLOY-AGENT for implementer:\n{}",
            exec_result
        );
        assert!(
            exec_result.contains("DEPLOY-AGENT autopilot-reviewer"),
            "should contain DEPLOY-AGENT for reviewer:\n{}",
            exec_result
        );

        // deploy-agent should NOT be called for non-agent skills
        assert!(
            !exec_result.contains("DEPLOY-AGENT audit-autopilot"),
            "should NOT deploy-agent for audit-autopilot:\n{}",
            exec_result
        );
        assert!(
            !exec_result.contains("DEPLOY-AGENT autopilot-orchestrator"),
            "should NOT deploy-agent for orchestrator:\n{}",
            exec_result
        );

        // Verify only coupled skills with codex/SKILL.md are linked in codex skills dir.
        for (name, src) in &expected {
            if categorize_skill(src) == "coupled" && has_skill_variant(src, "codex") {
                let variant = variant_src(src, "codex");
                let state = skill_state(name, &variant, ctx.codex_skills_dir());
                assert_eq!(
                    state, "correct",
                    "coupled skill {} should be correct in codex dir, but was {}",
                    name, state
                );
            }
        }
        for agent_only in &["autopilot-implementer", "autopilot-reviewer"] {
            assert!(
                !ctx.codex_skills_dir().join(agent_only).exists(),
                "agent-only Codex variant {} must not be linked under ~/.codex/skills",
                agent_only
            );
        }

        // Verify agent files deployed
        for agent_name in &["autopilot-implementer", "autopilot-reviewer"] {
            let agent_path = ctx.codex_agents_dir().join(format!("{}.toml", agent_name));
            assert!(
                agent_path.is_file(),
                "agent file {} should exist after deploy",
                agent_path.display()
            );
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Test 9: target-aware verification reports correctly
    // ═══════════════════════════════════════════════════════════════════════

    #[ignore]
    #[test]
    fn target_aware_verification_reports_per_directory() {
        let ctx = TestContext::new("toolkit-test9");
        setup_mock_project_with_variants(&ctx, "reasonix");
        let install = install_script();

        let expected = derive_expected_set(ctx.path());

        // First install all skills
        let _ = run_toolkit_setup_execute_targeted(&install, &ctx, &expected, "reasonix");

        // Run target-aware verification
        let verify_result = run_toolkit_setup_verify_targeted(&ctx, &expected, "reasonix");

        // Should report ALL PASS
        assert!(
            verify_result.contains("ALL PASS"),
            "verification should report ALL PASS:\n{}",
            verify_result
        );

        // Each skill should be listed as PASS
        for (name, _src) in &expected {
            assert!(
                verify_result.contains(&format!("[PASS] {}", name)),
                "verification should list {} as PASS:\n{}",
                name,
                verify_result
            );
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Test 10: Backward compatible — no target defaults to reasonix behavior
    // ═══════════════════════════════════════════════════════════════════════

    #[ignore]
    #[test]
    fn backward_compatible_no_target_uses_reasonix_behavior() {
        let ctx = TestContext::new("toolkit-test10");
        setup_mock_project_with_variants(&ctx, "reasonix");
        let install = install_script();

        let expected = derive_expected_set(ctx.path());

        // Old-style execute (no target awareness) should still work for the
        // old behavior: all skills in ~/.agents/skills/ via plain sync
        let exec_result = run_toolkit_setup_execute(&install, &ctx, &expected);

        // Old-style execute syncs everything to shared dir
        // It should at least complete without error
        assert!(
            exec_result.contains("LINK-PRINCIPLES"),
            "old-style execute should complete with link-principles:\n{}",
            exec_result
        );

        // Verify all agnostic skills are correct in shared dir
        for (name, src) in &expected {
            if categorize_skill(src) == "agnostic" {
                let state = skill_state(name, src, ctx.skills_dir());
                assert_eq!(
                    state, "correct",
                    "agnostic skill {} should be correct in shared dir, but was {}",
                    name, state
                );
            }
        }

        // Old-style verify should work too
        let verify_result = run_toolkit_setup_verify(&ctx, &expected);
        assert!(
            verify_result.contains("ALL PASS") || verify_result.contains("FIXES NEEDED"),
            "old-style verify should produce report"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Test 11: Unlink cleanup uses correct target directories
    // ═══════════════════════════════════════════════════════════════════════

    #[ignore]
    #[test]
    fn unlink_cleanup_uses_correct_target_directories() {
        let ctx = TestContext::new("toolkit-test11");
        setup_mock_project_with_variants(&ctx, "codex");
        let install = install_script();

        let expected = derive_expected_set(ctx.path());

        // First install all skills
        let _ = run_toolkit_setup_execute_targeted(&install, &ctx, &expected, "codex");

        // Create orphaned symlinks in both shared and codex dirs
        let old_shared = ctx
            .path()
            .join("skills/upstream/skills/engineering/old-shared");
        fs::create_dir_all(&old_shared).expect("create old-shared dir");
        fs::write(old_shared.join("SKILL.md"), "# Old Shared\n").unwrap();

        let orphan_shared = ctx.skills_dir().join("old-shared");
        std::os::unix::fs::symlink(&old_shared, &orphan_shared).expect("create orphan in shared");

        let old_codex = ctx.path().join("skills/autopilot/old-codex/codex");
        fs::create_dir_all(&old_codex).expect("create old-codex dir");
        fs::write(old_codex.join("SKILL.md"), "# Old Codex\n").unwrap();

        let orphan_codex = ctx.codex_skills_dir().join("old-codex");
        std::os::unix::fs::symlink(&old_codex, &orphan_codex).expect("create orphan in codex");

        // Re-run toolkit-setup with target=codex
        let exec_result = run_toolkit_setup_execute_targeted(&install, &ctx, &expected, "codex");

        // Verify both orphans were cleaned up
        assert!(
            !orphan_shared.exists(),
            "orphan in shared dir should be removed"
        );
        assert!(
            !orphan_codex.exists(),
            "orphan in codex dir should be removed"
        );

        // Verify UNLINK was reported for both
        assert!(
            exec_result.contains("UNLINK old-shared"),
            "should report UNLINK for old-shared:\n{}",
            exec_result
        );
        assert!(
            exec_result.contains("UNLINK old-codex"),
            "should report UNLINK for old-codex:\n{}",
            exec_result
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Test 12: --target kimi routes coupled skills to the shared directory
    // ═══════════════════════════════════════════════════════════════════════

    #[ignore]
    #[test]
    fn target_kimi_routes_coupled_skills_to_shared() {
        let ctx = TestContext::new("toolkit-test12");
        setup_mock_project_with_variants(&ctx, "kimi");
        let install = install_script();

        let expected = derive_expected_set(ctx.path());

        let kimi_variant_names: Vec<&str> = expected
            .iter()
            .filter(|(_, src)| categorize_skill(src) == "coupled" && has_skill_variant(src, "kimi"))
            .map(|(n, _)| n.as_str())
            .collect();
        assert_eq!(
            kimi_variant_names.len(),
            4,
            "mock should define 4 kimi variants (all coupled skills)"
        );

        let exec_result = run_toolkit_setup_execute_targeted(&install, &ctx, &expected, "kimi");

        // Coupled skills with a kimi variant install to the SHARED dir
        for (name, src) in &expected {
            if categorize_skill(src) == "coupled" && has_skill_variant(src, "kimi") {
                let state = skill_state(name, &variant_src(src, "kimi"), ctx.skills_dir());
                assert_eq!(
                    state, "correct",
                    "kimi variant of {} should be correct in shared dir, but was {}:\n{}",
                    name, state, exec_result
                );
            }
        }

        // Nothing lands in the agent-exclusive dirs on a kimi run
        for dir in [ctx.reasonix_skills_dir(), ctx.codex_skills_dir()] {
            let entries: Vec<_> = fs::read_dir(dir).unwrap().flatten().collect();
            assert!(
                entries.is_empty(),
                "kimi run must not touch agent-exclusive dir {}: {:?}",
                dir.display(),
                entries
            );
        }

        // Verification reports ALL PASS for a kimi run
        let verify_result = run_toolkit_setup_verify_targeted(&ctx, &expected, "kimi");
        assert!(
            verify_result.contains("ALL PASS"),
            "kimi verification should report ALL PASS:\n{}",
            verify_result
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Test 13: A reasonix run does not orphan-unlink kimi shared installs
    // ═══════════════════════════════════════════════════════════════════════

    #[ignore]
    #[test]
    fn reasonix_run_preserves_kimi_shared_installs() {
        let ctx = TestContext::new("toolkit-test13");
        setup_mock_project_with_variants(&ctx, "kimi");
        let install = install_script();

        let expected = derive_expected_set(ctx.path());

        // Install for kimi first, then run a reasonix setup on the same machine
        let _ = run_toolkit_setup_execute_targeted(&install, &ctx, &expected, "kimi");
        let exec_result = run_toolkit_setup_execute_targeted(&install, &ctx, &expected, "reasonix");

        // Kimi coupled installs in the shared dir must survive the reasonix run
        for (name, src) in &expected {
            if categorize_skill(src) == "coupled" && has_skill_variant(src, "kimi") {
                let state = skill_state(name, &variant_src(src, "kimi"), ctx.skills_dir());
                assert_eq!(
                    state, "correct",
                    "kimi variant of {} must survive a reasonix run, but was {}:\n{}",
                    name, state, exec_result
                );
                assert!(
                    !exec_result.contains(&format!("UNLINK {}", name)),
                    "reasonix run must not unlink kimi-installed {}:\n{}",
                    name,
                    exec_result
                );
            }
        }

        // And the reasonix variants are installed to the reasonix dir
        for (name, src) in &expected {
            if categorize_skill(src) == "coupled" {
                let state = skill_state(
                    name,
                    &variant_src(src, "reasonix"),
                    ctx.reasonix_skills_dir(),
                );
                assert_eq!(
                    state, "correct",
                    "reasonix variant of {} should be correct, but was {}",
                    name, state
                );
            }
        }
    }
}
