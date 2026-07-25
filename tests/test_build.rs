#!/usr/bin/env rust-script
//! ```cargo
//! [dependencies]
//! serde = { version = "1", features = ["derive"] }
//! serde_json = "1"
//! tempfile = "3"
//! walkdir = "2"
//! ```
//!
//! Integration tests for deploy.rs pack subcommand.
//! Build -> extract -> verify structure and metadata (AC 9).
//!
//! Run: rust-script --test tests/test_build.rs

use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
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

fn setup_mock_project_for_distill(root: &Path) {
    fs::copy(install_script(), root.join("deploy.rs")).unwrap();

    let distill_skill = root.join("skills/autopilot/autopilot-distill");
    for variant in &["codex", "kimi", "reasonix"] {
        let dir = distill_skill.join(variant);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("SKILL.md"),
            format!(
                "---\nname: autopilot-distill\ndescription: Distill {}\n---\nRun ~/.agents/skills/.autopilot/bin/distill.\n",
                variant
            ),
        )
        .unwrap();
    }

    let cli = root.join("crates/distill-cli/src");
    fs::create_dir_all(&cli).unwrap();
    fs::write(
        root.join("crates/distill-cli/Cargo.toml"),
        "[package]\nname = \"distill-cli\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[[bin]]\nname = \"distill\"\npath = \"src/main.rs\"\n",
    )
    .unwrap();
    fs::write(cli.join("main.rs"), "fn main() {}\n").unwrap();

    fs::create_dir_all(root.join("templates")).unwrap();
    fs::write(
        root.join("templates/install.sh.in"),
        "#!/bin/bash\nVERSION=\"__VERSION__\"\n",
    )
    .unwrap();
    fs::write(root.join("templates/uninstall.sh"), "#!/bin/bash\n").unwrap();
    fs::write(root.join("bootstrap.sh"), "#!/bin/bash\n").unwrap();
    fs::create_dir_all(root.join("principles")).unwrap();
    fs::write(root.join("principles/karpathy.md"), "# Principles\n").unwrap();
    fs::write(
        root.join(".skill-lock.json"),
        r#"{"version":4,"skills":{}}"#,
    )
    .unwrap();
    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/*\"]\n",
    )
    .unwrap();

    Command::new("git")
        .args(["init"])
        .current_dir(root)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(root)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(root)
        .output()
        .unwrap();
    Command::new("git")
        .args(["add", "-A"])
        .current_dir(root)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(root)
        .output()
        .unwrap();
}

fn setup_full_pack_project(root: &Path) {
    fs::create_dir_all(root).unwrap();
    let source = project_root();
    for file in [
        "deploy.rs",
        "bootstrap.sh",
        ".skill-lock.json",
        "Cargo.toml",
    ] {
        fs::copy(source.join(file), root.join(file)).unwrap();
    }
    fs::create_dir_all(root.join("crates/distill-cli")).unwrap();
    fs::copy(
        source.join("crates/distill-cli/Cargo.toml"),
        root.join("crates/distill-cli/Cargo.toml"),
    )
    .unwrap();
    for directory in ["templates", "skills", "principles"] {
        let status = Command::new("cp")
            .args([
                "-R",
                &source.join(directory).to_string_lossy(),
                &root.join(directory).to_string_lossy(),
            ])
            .status()
            .expect("fixture directory copy should run");
        assert!(
            status.success(),
            "fixture directory {directory} should copy"
        );
    }

    Command::new("git")
        .args(["init"])
        .current_dir(root)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(root)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(root)
        .output()
        .unwrap();
    Command::new("git")
        .args(["add", "-A"])
        .current_dir(root)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(root)
        .output()
        .unwrap();
}

fn write_mock_distill_artifacts(root: &Path) {
    for platform in &["darwin-arm64", "linux-arm64", "linux-x64"] {
        let artifact = root
            .join("dist")
            .join("distill")
            .join(platform)
            .join("distill");
        fs::create_dir_all(artifact.parent().unwrap()).unwrap();
        fs::write(
            &artifact,
            format!("#!/usr/bin/env bash\necho distill {}\n", platform),
        )
        .unwrap();
        fs::set_permissions(&artifact, fs::Permissions::from_mode(0o755)).unwrap();
    }
}

#[derive(Debug, Deserialize)]
struct Manifest {
    version: String,
    skills: HashMap<String, SkillEntry>,
    #[serde(default)]
    executables: HashMap<String, ExecutableEntry>,
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

#[derive(Debug, Deserialize)]
struct ExecutableEntry {
    platforms: HashMap<String, String>,
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
        __distill_artifacts_command_builds_supported_targets();
        __distill_artifacts_command_installs_targets_before_build_and_derives_linux_linker();
        __release_stops_when_target_install_fails();
        __release_builds_packs_and_publishes_once_in_order();
        __no_args_release_builds_packs_and_publishes_once_in_order();
        __pack_fails_when_distill_artifact_set_is_incomplete();
        __build_creates_dist_dir_if_missing();
        __build_exits_nonzero_when_not_in_git_repo();
        __sync_still_works_after_build_changes();
        eprintln!("All tests passed!");
    }

    fn __build_produces_tarball() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path().join("project");
        setup_full_pack_project(&root);
        let dist_dir = root.join("dist");

        let git_hash = git_rev_parse(&root);
        assert!(!git_hash.is_empty(), "git rev-parse should return a hash");

        let tarball_name = "autopilot-toolkit.tar.gz";
        let tarball_path = dist_dir.join(tarball_name);
        write_mock_distill_artifacts(&root);

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
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path().join("project");
        setup_full_pack_project(&root);
        let git_hash = git_rev_parse(&root);
        assert!(!git_hash.is_empty());

        let dist_dir = root.join("dist");
        write_mock_distill_artifacts(&root);

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
        let extract_tmp = tempfile::tempdir().expect("tempdir");
        let extract_dir = extract_tmp.path().join("extracted");
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
        let install_sh = root.join("dist").join("install.sh");
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
            "autopilot-distill",
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
            "autopilot-distill",
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

        for coupled_name in &[
            "autopilot-distill",
            "autopilot-implementer",
            "autopilot-reviewer",
            "autopilot-orchestrator",
            "audit-autopilot",
        ] {
            let discoverable_count = walkdir::WalkDir::new(skills_dir.join(coupled_name))
                .into_iter()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_type().is_file() && entry.file_name() == "SKILL.md")
                .count();
            assert_eq!(
                discoverable_count, 1,
                "{} should expose exactly one discoverable SKILL.md",
                coupled_name
            );
        }
        let implementer_router =
            fs::read_to_string(skills_dir.join("autopilot-implementer/SKILL.md")).unwrap();
        assert!(
            implementer_router.contains("runAs: subagent")
                && implementer_router.contains("allowed-tools:"),
            "shared router must preserve Reasonix execution metadata"
        );

        for variant in &["codex", "kimi", "reasonix"] {
            let skill = fs::read_to_string(
                skills_dir
                    .join("autopilot-distill")
                    .join("runtime")
                    .join(variant)
                    .join("INSTRUCTIONS.md"),
            )
            .unwrap_or_else(|err| {
                panic!(
                    "autopilot-distill {} INSTRUCTIONS.md should be readable: {}",
                    variant, err
                )
            });
            assert!(
                skill.contains("name: autopilot-distill"),
                "autopilot-distill {} variant should expose the renamed skill identity",
                variant
            );
            assert!(
                skill.contains(".autopilot/bin/distill"),
                "autopilot-distill {} variant should tell the runtime how to execute the installed binary",
                variant
            );
        }

        let distill = manifest
            .executables
            .get("distill")
            .expect("manifest should own distill executable");
        assert!(
            !distill.platforms.contains_key("darwin-x64"),
            "manifest should not advertise the unsupported x86_64 macOS artifact"
        );
        for platform in &["darwin-arm64", "linux-arm64", "linux-x64"] {
            let rel = distill
                .platforms
                .get(*platform)
                .unwrap_or_else(|| panic!("distill platform {} should be in manifest", platform));
            let executable = autopilot_dir.join(rel);
            assert!(
                executable.is_file(),
                "distill executable for {} should exist at {:?}",
                platform,
                executable
            );
            assert!(
                fs::metadata(&executable).unwrap().permissions().mode() & 0o111 != 0,
                "distill executable for {} should be executable",
                platform
            );
        }
    }

    fn __distill_artifacts_command_builds_supported_targets() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path().join("project");
        fs::create_dir_all(&root).unwrap();
        setup_mock_project_for_distill(&root);
        let fake_bin = tmp.path().join("bin");
        let log = tmp.path().join("cargo.log");
        fs::create_dir_all(&fake_bin).unwrap();
        let cargo = fake_bin.join("cargo");
        fs::write(
            &cargo,
            format!(
                r#"#!/usr/bin/env bash
set -euo pipefail
echo "$*" >> "{}"
echo "arm64_linker=${{CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER:-}}" >> "{}"
echo "x64_linker=${{CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER:-}}" >> "{}"
target=""
while [[ $# -gt 0 ]]; do
  if [[ "$1" == "--target" ]]; then
    shift
    target="$1"
  fi
  shift || true
done
mkdir -p "target/${{target}}/release"
printf '#!/usr/bin/env bash\necho distill %s\n' "${{target}}" > "target/${{target}}/release/distill"
chmod +x "target/${{target}}/release/distill"
"#,
                log.display(),
                log.display(),
                log.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&cargo, fs::Permissions::from_mode(0o755)).unwrap();

        let path = format!("{}:{}", fake_bin.display(), std::env::var("PATH").unwrap());
        let output = Command::new("rust-script")
            .arg(install_script())
            .arg("distill-artifacts")
            .env("PROJECT_ROOT", &root)
            .env("PATH", path)
            .env("DISTILL_LINUX_ARM64_LINKER", "/toolchains/aarch64-linker")
            .env("DISTILL_LINUX_X64_LINKER", "/toolchains/x86_64-linker")
            .output()
            .expect("failed to run deploy.rs distill-artifacts");
        assert!(
            output.status.success(),
            "distill-artifacts should succeed, stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let logged = fs::read_to_string(&log).unwrap();
        for triple in &[
            "aarch64-apple-darwin",
            "aarch64-unknown-linux-musl",
            "x86_64-unknown-linux-musl",
        ] {
            assert!(
                logged.contains(&format!("--target {}", triple)),
                "cargo should build target {}, log:\n{}",
                triple,
                logged
            );
        }
        assert!(
            !logged.contains("--target x86_64-apple-darwin"),
            "cargo should not build the unsupported x86_64 macOS target, log:\n{}",
            logged
        );
        assert!(logged.contains("arm64_linker=/toolchains/aarch64-linker"));
        assert!(logged.contains("x64_linker=/toolchains/x86_64-linker"));
    }

    fn __distill_artifacts_command_installs_targets_before_build_and_derives_linux_linker() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path().join("project");
        fs::create_dir_all(&root).unwrap();
        setup_mock_project_for_distill(&root);
        let fake_bin = tmp.path().join("bin");
        let log = tmp.path().join("toolchain.log");
        fs::create_dir_all(&fake_bin).unwrap();

        let rustup = fake_bin.join("rustup");
        fs::write(
            &rustup,
            format!(
                r#"#!/usr/bin/env bash
set -euo pipefail
echo "rustup $*" >> "{}"
"#,
                log.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&rustup, fs::Permissions::from_mode(0o755)).unwrap();

        let rustc = fake_bin.join("rustc");
        fs::write(
            &rustc,
            r#"#!/usr/bin/env bash
set -euo pipefail
if [[ "$*" == "--print sysroot" ]]; then
  echo "/mock/rust/sysroot"
elif [[ "$*" == "-vV" ]]; then
  echo "host: test-host-triple"
fi
"#,
        )
        .unwrap();
        fs::set_permissions(&rustc, fs::Permissions::from_mode(0o755)).unwrap();

        let cargo = fake_bin.join("cargo");
        fs::write(
            &cargo,
            format!(
                r#"#!/usr/bin/env bash
set -euo pipefail
echo "cargo $*" >> "{}"
echo "arm64_linker=${{CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER:-}}" >> "{}"
echo "x64_linker=${{CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER:-}}" >> "{}"
target=""
while [[ $# -gt 0 ]]; do
  if [[ "$1" == "--target" ]]; then
    shift
    target="$1"
  fi
  shift || true
done
mkdir -p "target/${{target}}/release"
printf '#!/usr/bin/env bash\necho distill %s\n' "${{target}}" > "target/${{target}}/release/distill"
chmod +x "target/${{target}}/release/distill"
"#,
                log.display(),
                log.display(),
                log.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&cargo, fs::Permissions::from_mode(0o755)).unwrap();

        let path = format!("{}:{}", fake_bin.display(), std::env::var("PATH").unwrap());
        let output = Command::new("rust-script")
            .arg(install_script())
            .arg("distill-artifacts")
            .env("PROJECT_ROOT", &root)
            .env("PATH", &path)
            .output()
            .expect("failed to run deploy.rs distill-artifacts");
        assert!(
            output.status.success(),
            "distill-artifacts should succeed, stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let logged = fs::read_to_string(&log).unwrap();
        for triple in &[
            "aarch64-apple-darwin",
            "aarch64-unknown-linux-musl",
            "x86_64-unknown-linux-musl",
        ] {
            let rustup_pos = logged
                .find(&format!("rustup target add {}", triple))
                .unwrap_or_else(|| {
                    panic!("rustup should install target {}, log:\n{}", triple, logged)
                });
            let cargo_pos = logged
                .find(&format!(
                    "cargo build --release --bin distill --target {}",
                    triple
                ))
                .unwrap_or_else(|| {
                    panic!("cargo should build target {}, log:\n{}", triple, logged)
                });
            assert!(
                rustup_pos < cargo_pos,
                "rustup target add should run before cargo build for {}, log:\n{}",
                triple,
                logged
            );
        }
        let derived = "/mock/rust/sysroot/lib/rustlib/test-host-triple/bin/rust-lld";
        assert!(
            logged.contains(&format!("arm64_linker={derived}")),
            "arm64 should use derived rust-lld, log:\n{}",
            logged
        );
        assert!(
            logged.contains(&format!("x64_linker={derived}")),
            "x64 should use derived rust-lld, log:\n{}",
            logged
        );

        fs::write(&log, "").unwrap();
        let shared_linker = "/toolchains/shared-linux-linker";
        let output = Command::new("rust-script")
            .arg(install_script())
            .arg("distill-artifacts")
            .env("PROJECT_ROOT", &root)
            .env("PATH", &path)
            .env("DISTILL_LINUX_LINKER", shared_linker)
            .output()
            .expect("failed to run deploy.rs with shared Linux linker");
        assert!(
            output.status.success(),
            "distill-artifacts should accept DISTILL_LINUX_LINKER, stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let logged = fs::read_to_string(&log).unwrap();
        assert!(
            logged.contains(&format!("arm64_linker={shared_linker}")),
            "arm64 should use DISTILL_LINUX_LINKER, log:\n{}",
            logged
        );
        assert!(
            logged.contains(&format!("x64_linker={shared_linker}")),
            "x64 should use DISTILL_LINUX_LINKER, log:\n{}",
            logged
        );
    }

    fn __release_builds_packs_and_publishes_once_in_order() {
        assert_release_builds_packs_and_publishes_once_in_order(&["release"], false);
    }

    fn __no_args_release_builds_packs_and_publishes_once_in_order() {
        assert_release_builds_packs_and_publishes_once_in_order(&[], false);
    }

    fn __release_stops_when_target_install_fails() {
        assert_release_builds_packs_and_publishes_once_in_order(&["release"], true);
    }

    fn assert_release_builds_packs_and_publishes_once_in_order(
        deploy_args: &[&str],
        fail_linux_arm64_target_install: bool,
    ) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path().join("project");
        fs::create_dir_all(&root).unwrap();
        setup_mock_project_for_distill(&root);
        let fake_bin = tmp.path().join("bin");
        let log = tmp.path().join("release.log");
        fs::create_dir_all(&fake_bin).unwrap();

        fs::write(
            fake_bin.join("rustup"),
            format!(
                "#!/usr/bin/env bash\nset -euo pipefail\necho \"rustup $*\" >> \"{}\"\n{}\n",
                log.display(),
                if fail_linux_arm64_target_install {
                    "if [[ \"$*\" == *aarch64-unknown-linux-musl* ]]; then exit 42; fi"
                } else {
                    ""
                }
            ),
        )
        .unwrap();
        fs::set_permissions(fake_bin.join("rustup"), fs::Permissions::from_mode(0o755)).unwrap();

        fs::write(
            fake_bin.join("rustc"),
            "#!/usr/bin/env bash\nif [[ \"$*\" == \"--print sysroot\" ]]; then echo \"/mock/rust/sysroot\"; elif [[ \"$*\" == \"-vV\" ]]; then echo \"host: test-host-triple\"; fi\n",
        )
        .unwrap();
        fs::set_permissions(fake_bin.join("rustc"), fs::Permissions::from_mode(0o755)).unwrap();

        fs::write(
            fake_bin.join("cargo"),
            format!(
                r#"#!/usr/bin/env bash
set -euo pipefail
echo "cargo $*" >> "{}"
target=""
while [[ $# -gt 0 ]]; do
  if [[ "$1" == "--target" ]]; then shift; target="$1"; fi
  shift || true
done
mkdir -p "target/${{target}}/release"
printf '#!/usr/bin/env bash\necho distill %s\n' "${{target}}" > "target/${{target}}/release/distill"
chmod +x "target/${{target}}/release/distill"
"#,
                log.display()
            ),
        )
        .unwrap();
        fs::set_permissions(fake_bin.join("cargo"), fs::Permissions::from_mode(0o755)).unwrap();

        fs::write(
            fake_bin.join("git"),
            format!(
                r#"#!/usr/bin/env bash
set -euo pipefail
echo "git $*" >> "{}"
case "$*" in
  "rev-parse HEAD") echo "1234567890abcdef1234567890abcdef12345678" ;;
  "remote get-url origin") echo "git@github.com:test/repo.git" ;;
  tag*|push*) exit 0 ;;
  *) exit 0 ;;
esac
"#,
                log.display()
            ),
        )
        .unwrap();
        fs::set_permissions(fake_bin.join("git"), fs::Permissions::from_mode(0o755)).unwrap();

        fs::write(
            fake_bin.join("gh"),
            format!(
                r#"#!/usr/bin/env bash
set -euo pipefail
echo "gh $*" >> "{}"
if [[ "$*" == release\ view* ]]; then
  exit 1
fi
exit 0
"#,
                log.display()
            ),
        )
        .unwrap();
        fs::set_permissions(fake_bin.join("gh"), fs::Permissions::from_mode(0o755)).unwrap();

        let path = format!("{}:{}", fake_bin.display(), std::env::var("PATH").unwrap());
        let mut command = Command::new("rust-script");
        command.arg(install_script());
        for arg in deploy_args {
            command.arg(arg);
        }
        let output = command
            .env("PROJECT_ROOT", &root)
            .env("PATH", path)
            .output()
            .expect("failed to run deploy.rs release path");
        if fail_linux_arm64_target_install {
            assert!(
                !output.status.success(),
                "release should fail when rustup target add fails"
            );
            let logged = fs::read_to_string(&log).unwrap();
            assert!(
                logged.contains("rustup target add aarch64-unknown-linux-musl"),
                "release should attempt the failing target install, log:\n{}",
                logged
            );
            assert!(
                !logged.contains(
                    "cargo build --release --bin distill --target aarch64-unknown-linux-musl"
                ),
                "release must stop before the corresponding Cargo build, log:\n{}",
                logged
            );
            assert!(
                !logged.contains("gh release create"),
                "release must not publish after target installation fails, log:\n{}",
                logged
            );
            return;
        }
        assert!(
            output.status.success(),
            "release should succeed, stdout: {}, stderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        let logged = fs::read_to_string(&log).unwrap();
        assert_eq!(
            logged
                .matches("cargo build --release --bin distill --target")
                .count(),
            3,
            "release should build each supported binary once, log:\n{}",
            logged
        );
        assert_eq!(
            logged.matches("gh release create").count(),
            1,
            "release should publish once, log:\n{}",
            logged
        );
        let first_cargo = logged
            .find("cargo build --release --bin distill --target")
            .expect("release should build artifacts");
        let tag = logged
            .find("git tag -f")
            .expect("release should create tag after pack");
        let publish = logged
            .find("gh release create")
            .expect("release should publish");
        assert!(
            first_cargo < tag,
            "artifact build should precede publication prep, log:\n{}",
            logged
        );
        assert!(
            tag < publish,
            "tag push should precede GitHub publication, log:\n{}",
            logged
        );

        let manifest_bytes = {
            let extract_tmp = tempfile::tempdir().expect("tempdir");
            let extract_dir = extract_tmp.path().join("extracted");
            fs::create_dir_all(&extract_dir).unwrap();
            let status = Command::new("tar")
                .args([
                    "-xzf",
                    &root.join("dist/autopilot-toolkit.tar.gz").to_string_lossy(),
                    "-C",
                    &extract_dir.to_string_lossy(),
                ])
                .status()
                .expect("tar extract failed");
            assert!(status.success(), "release tarball should extract");
            fs::read_to_string(extract_dir.join(".autopilot/manifest.json")).unwrap()
        };
        let manifest: Manifest = serde_json::from_str(&manifest_bytes).unwrap();
        assert!(manifest.skills.contains_key("autopilot-distill"));
        assert!(!manifest.skills.contains_key("distill"));
        assert!(manifest.executables.contains_key("distill"));
    }

    fn __pack_fails_when_distill_artifact_set_is_incomplete() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let project = tmp.path().join("project");
        fs::create_dir_all(&project).unwrap();
        setup_mock_project_for_distill(&project);

        let artifact = project.join("dist/distill/darwin-arm64/distill");
        fs::create_dir_all(artifact.parent().unwrap()).unwrap();
        fs::write(&artifact, "#!/usr/bin/env bash\necho darwin-arm64\n").unwrap();
        fs::set_permissions(&artifact, fs::Permissions::from_mode(0o755)).unwrap();

        let (out, err, code) = run_build(&["pack"], Some(&project));
        assert_ne!(
            code, 0,
            "pack should fail when only one Distill platform artifact exists, stdout: {}, stderr: {}",
            out, err
        );
        assert!(
            err.contains("missing Distill CLI artifact"),
            "pack should report missing Distill artifact, stderr: {}",
            err
        );
    }

    fn __build_creates_dist_dir_if_missing() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path().join("project");
        setup_full_pack_project(&root);
        let dist_dir = root.join("dist");
        write_mock_distill_artifacts(&root);

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

        // Copy crates/ (needed by deploy.rs path dep on skill-index)
        let real_crates = project_root().join("crates");
        if real_crates.exists() {
            let status = Command::new("cp")
                .args([
                    "-r",
                    &real_crates.to_string_lossy(),
                    &mock_root.join("crates").to_string_lossy(),
                ])
                .status()
                .expect("cp crates failed");
            assert!(status.success());
        }

        // Copy Cargo.toml + Cargo.lock (needed by workspace resolution)
        for f in &["Cargo.toml", "Cargo.lock"] {
            let real = project_root().join(f);
            if real.exists() {
                fs::copy(&real, mock_root.join(f)).unwrap();
            }
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
        assert!(
            skills.join("autopilot-distill").is_symlink(),
            "dev should symlink the renamed autopilot-distill skill"
        );
        assert!(
            !skills.join("distill").exists(),
            "dev should not create the old distill skill symlink"
        );
    }
}
