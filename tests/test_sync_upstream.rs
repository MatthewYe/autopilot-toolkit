#!/usr/bin/env rust-script
//! ```cargo
//! [dependencies]
//! serde_json = "1"
//! ```
//!
//! Integration tests for scripts/sync-upstream.rs CLI contract.
//! Drives the real entry script via std::process::Command against a
//! fixture project root (PROJECT_ROOT) and a local fixture upstream repo
//! (AUTOPILOT_UPSTREAM_REPO), asserting exit codes and generated files.
//!
//! Run: rust-script --test tests/test_sync_upstream.rs

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn main() {
    println!("Run with: rust-script --test tests/test_sync_upstream.rs");
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

/// Find the actual project root — the directory containing scripts/sync-upstream.rs.
fn actual_project_root() -> PathBuf {
    let src = Path::new(file!());
    if let (Some(_tests_dir), Some(proj)) = (src.parent(), src.parent().and_then(|p| p.parent())) {
        let candidate = proj.to_path_buf();
        if candidate.join("scripts/sync-upstream.rs").exists() {
            return candidate;
        }
    }
    if let Ok(root) = std::env::var("PROJECT_ROOT") {
        let p = PathBuf::from(&root);
        if p.join("scripts/sync-upstream.rs").exists() {
            return p;
        }
    }
    panic!("Cannot find project root (scripts/sync-upstream.rs not found)");
}

/// Run scripts/sync-upstream.rs with PROJECT_ROOT and
/// AUTOPILOT_UPSTREAM_REPO pointed at the fixtures.
fn run_sync(script: &Path, project: &Path, repo: &Path, ref_: &str) -> (String, String, i32) {
    assert!(script.exists(), "sync-upstream.rs not found at {:?}", script);

    let output = Command::new("rust-script")
        // `-f`: the cache ignores path-dependency changes (`crates/*`,
        // upstream rust-script#122).
        .arg("-f")
        .arg(script)
        .arg(ref_)
        .env("PROJECT_ROOT", project)
        .env("AUTOPILOT_UPSTREAM_REPO", repo)
        .output()
        .expect("failed to run rust-script");

    (
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
        output.status.code().unwrap_or(-1),
    )
}

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(dir)
        .status()
        .expect("git failed to start");
    assert!(status.success(), "git {:?} failed in {:?}", args, dir);
}

fn write_skill(root: &Path, rel: &str, content: &str) {
    let dir = root.join(rel);
    fs::create_dir_all(&dir).expect("create skill dir");
    fs::write(dir.join("SKILL.md"), content).expect("write SKILL.md");
}

fn commit_all(repo: &Path) {
    git(repo, &["add", "-A"]);
    git(
        repo,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "--quiet",
            "-m",
            "fixture",
        ],
    );
}

/// Fixture upstream repo: skill-a (engineering) + allowlisted loop-me
/// (in-progress), tagged v1.
fn make_fixture_repo(repo: &Path) {
    fs::create_dir_all(repo).expect("create repo dir");
    git(repo, &["init", "--quiet"]);
    write_skill(repo, "skills/engineering/skill-a", "# Skill A\n");
    write_skill(repo, "skills/in-progress/loop-me", "# Loop Me\n");
    commit_all(repo);
    git(repo, &["tag", "v1"]);
}

/// Fixture project root: empty lock + a stale upstream tree.
fn make_project(project: &Path) {
    let lock = serde_json::json!({"version": 4, "skills": {}, "dismissed": {}});
    fs::write(
        project.join(".skill-lock.json"),
        serde_json::to_string_pretty(&lock).unwrap() + "\n",
    )
    .expect("write lock");
    write_skill(
        project,
        "skills/upstream/skills/engineering/stale-skill",
        "# Stale\n",
    );
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn sync_script_path() -> PathBuf {
        actual_project_root().join("scripts/sync-upstream.rs")
    }

    #[test]
    fn sync_happy_path_exits_zero_and_replaces_tree() {
        let repo = TempDir::new("sync-it-repo");
        make_fixture_repo(repo.path());
        let project = TempDir::new("sync-it-project");
        make_project(project.path());

        let (stdout, stderr, code) = run_sync(&sync_script_path(), project.path(), repo.path(), "v1");
        assert_eq!(code, 0, "sync should exit 0, stdout: {} stderr: {}", stdout, stderr);
        assert!(
            stdout.contains("Sync complete — all checks PASS."),
            "should report success, got: {}",
            stdout
        );
        assert!(stdout.contains("+ skill-a"), "should list added skill, got: {}", stdout);
        assert!(
            stdout.contains("in-progress allowlist: shipping loop-me"),
            "should carry the allowlist note, got: {}",
            stdout
        );

        // Tree replaced.
        let upstream = project.path().join("skills/upstream");
        assert!(upstream.join("skills/engineering/skill-a/SKILL.md").is_file());
        assert!(
            !upstream.join("skills/engineering/stale-skill").exists(),
            "stale tree must be replaced"
        );

        // Lock written with both skills.
        let lock = fs::read_to_string(project.path().join(".skill-lock.json")).unwrap();
        assert!(lock.contains("\"skill-a\""), "lock should contain skill-a: {}", lock);
        assert!(lock.contains("\"loop-me\""), "lock should contain loop-me: {}", lock);
        assert!(
            lock.contains("skills/in-progress/loop-me/SKILL.md"),
            "loop-me should keep its in-progress path: {}",
            lock
        );

        // Re-sync at the same ref: byte-identical lock, exit 0.
        let before = fs::read(project.path().join(".skill-lock.json")).unwrap();
        let (stdout2, stderr2, code2) =
            run_sync(&sync_script_path(), project.path(), repo.path(), "v1");
        assert_eq!(code2, 0, "second sync should exit 0: {} {}", stdout2, stderr2);
        let after = fs::read(project.path().join(".skill-lock.json")).unwrap();
        assert_eq!(before, after, "same-ref re-sync must produce an empty lock diff");
    }

    #[test]
    fn sync_fails_when_allowlisted_skill_missing_at_ref() {
        let repo = TempDir::new("sync-it-repo-noallow");
        fs::create_dir_all(repo.path()).unwrap();
        git(repo.path(), &["init", "--quiet"]);
        write_skill(repo.path(), "skills/engineering/skill-a", "# Skill A\n");
        commit_all(repo.path());
        git(repo.path(), &["tag", "v1"]);

        let project = TempDir::new("sync-it-project-noallow");
        make_project(project.path());
        let lock_before = fs::read(project.path().join(".skill-lock.json")).unwrap();

        let (stdout, stderr, code) = run_sync(&sync_script_path(), project.path(), repo.path(), "v1");
        assert_eq!(code, 1, "missing allowlisted skill must fail: {} {}", stdout, stderr);
        assert!(
            stderr.contains("loop-me"),
            "stderr should name the missing allowlisted skill: {}",
            stderr
        );
        // Nothing was mutated.
        let lock_after = fs::read(project.path().join(".skill-lock.json")).unwrap();
        assert_eq!(lock_before, lock_after, "lock must be untouched on failure");
        assert!(project
            .path()
            .join("skills/upstream/skills/engineering/stale-skill")
            .exists());
    }

    #[test]
    fn sync_fails_on_malformed_existing_lock() {
        let repo = TempDir::new("sync-it-repo-badlock");
        make_fixture_repo(repo.path());
        let project = TempDir::new("sync-it-project-badlock");
        make_project(project.path());
        fs::write(project.path().join(".skill-lock.json"), "{ not json").unwrap();

        let (stdout, stderr, code) = run_sync(&sync_script_path(), project.path(), repo.path(), "v1");
        assert_eq!(code, 1, "malformed lock must be a hard error: {} {}", stdout, stderr);
        assert!(
            stderr.contains("unparseable lock"),
            "stderr should identify the lock problem: {}",
            stderr
        );
        assert!(project
            .path()
            .join("skills/upstream/skills/engineering/stale-skill")
            .exists());
    }

    #[test]
    fn usage_error_without_ref() {
        let output = Command::new("rust-script")
            .arg("-f")
            .arg(sync_script_path())
            .output()
            .expect("failed to run rust-script");
        let code = output.status.code().unwrap_or(-1);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(code, 1, "missing REF should exit 1");
        assert!(
            stderr.contains("AUTOPILOT_UPSTREAM_REPO"),
            "usage should document the repo override: {}",
            stderr
        );
    }
}
