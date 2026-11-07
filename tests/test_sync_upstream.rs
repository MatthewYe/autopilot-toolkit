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
    run_sync_args(script, project, repo, &[ref_])
}

/// Run scripts/sync-upstream.rs with explicit CLI args (e.g. `--force`).
fn run_sync_args(
    script: &Path,
    project: &Path,
    repo: &Path,
    args: &[&str],
) -> (String, String, i32) {
    assert!(
        script.exists(),
        "sync-upstream.rs not found at {:?}",
        script
    );

    let output = Command::new("rust-script")
        // `-f`: the cache ignores path-dependency changes (`crates/*`,
        // upstream rust-script#122).
        .arg("-f")
        .arg(script)
        .args(args)
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

/// A valid SKILL.md body: the sync now runs the full validation gate, so the
/// integration fixtures need frontmatter that passes it.
fn fixture_skill_md(name: &str, description: &str, heading: &str) -> String {
    format!(
        "---\nname: {}\ndescription: {}\n---\n# {}\n",
        name, description, heading
    )
}

/// Fixture upstream repo: skill-a (engineering) + allowlisted loop-me
/// (in-progress), tagged v1.
fn make_fixture_repo(repo: &Path) {
    fs::create_dir_all(repo).expect("create repo dir");
    git(repo, &["init", "--quiet"]);
    write_skill(
        repo,
        "skills/engineering/skill-a",
        &fixture_skill_md("skill-a", "Fixture skill A.", "Skill A"),
    );
    write_skill(
        repo,
        "skills/in-progress/loop-me",
        &fixture_skill_md("loop-me", "Fixture in-progress skill.", "Loop Me"),
    );
    commit_all(repo);
    git(repo, &["tag", "v1"]);
}

/// Fixture upstream repo whose skill-a has no frontmatter, so the post-sync
/// validation gate fails and the sync must roll back.
fn make_fixture_repo_with_invalid_skill(repo: &Path) {
    fs::create_dir_all(repo).expect("create repo dir");
    git(repo, &["init", "--quiet"]);
    write_skill(
        repo,
        "skills/engineering/skill-a",
        "# Skill A without frontmatter\n",
    );
    write_skill(
        repo,
        "skills/in-progress/loop-me",
        &fixture_skill_md("loop-me", "Fixture in-progress skill.", "Loop Me"),
    );
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

/// Turn the fixture project root into a git worktree whose current tree is
/// committed (so the dirty-tree precondition can be exercised).
fn init_project_git(project: &Path) {
    git(project, &["init", "--quiet"]);
    commit_all(project);
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

        let (stdout, stderr, code) =
            run_sync(&sync_script_path(), project.path(), repo.path(), "v1");
        assert_eq!(
            code, 0,
            "sync should exit 0, stdout: {} stderr: {}",
            stdout, stderr
        );
        assert!(
            stdout.contains("Sync complete — all checks PASS."),
            "should report success, got: {}",
            stdout
        );
        assert!(
            stdout.contains("+ skill-a"),
            "should list added skill, got: {}",
            stdout
        );
        assert!(
            stdout.contains("in-progress allowlist: shipping loop-me"),
            "should carry the allowlist note, got: {}",
            stdout
        );

        // Tree replaced.
        let upstream = project.path().join("skills/upstream");
        assert!(upstream
            .join("skills/engineering/skill-a/SKILL.md")
            .is_file());
        assert!(
            !upstream.join("skills/engineering/stale-skill").exists(),
            "stale tree must be replaced"
        );

        // Lock written with both skills.
        let lock = fs::read_to_string(project.path().join(".skill-lock.json")).unwrap();
        assert!(
            lock.contains("\"skill-a\""),
            "lock should contain skill-a: {}",
            lock
        );
        assert!(
            lock.contains("\"loop-me\""),
            "lock should contain loop-me: {}",
            lock
        );
        assert!(
            lock.contains("skills/in-progress/loop-me/SKILL.md"),
            "loop-me should keep its in-progress path: {}",
            lock
        );

        // Re-sync at the same ref: byte-identical lock, exit 0.
        let before = fs::read(project.path().join(".skill-lock.json")).unwrap();
        let (stdout2, stderr2, code2) =
            run_sync(&sync_script_path(), project.path(), repo.path(), "v1");
        assert_eq!(
            code2, 0,
            "second sync should exit 0: {} {}",
            stdout2, stderr2
        );
        let after = fs::read(project.path().join(".skill-lock.json")).unwrap();
        assert_eq!(
            before, after,
            "same-ref re-sync must produce an empty lock diff"
        );
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

        let (stdout, stderr, code) =
            run_sync(&sync_script_path(), project.path(), repo.path(), "v1");
        assert_eq!(
            code, 1,
            "missing allowlisted skill must fail: {} {}",
            stdout, stderr
        );
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

        let (stdout, stderr, code) =
            run_sync(&sync_script_path(), project.path(), repo.path(), "v1");
        assert_eq!(
            code, 1,
            "malformed lock must be a hard error: {} {}",
            stdout, stderr
        );
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

    // ── Failure atomicity ────────────────────────────────────────────────

    #[test]
    fn sync_rolls_back_when_validation_gate_fails() {
        let repo = TempDir::new("sync-it-repo-invalid");
        make_fixture_repo_with_invalid_skill(repo.path());
        let project = TempDir::new("sync-it-project-invalid");
        make_project(project.path());
        let lock_before = fs::read(project.path().join(".skill-lock.json")).unwrap();
        let stale = project
            .path()
            .join("skills/upstream/skills/engineering/stale-skill/SKILL.md");
        let stale_before = fs::read(&stale).unwrap();

        let (stdout, stderr, code) =
            run_sync(&sync_script_path(), project.path(), repo.path(), "v1");
        assert_eq!(
            code, 1,
            "a failed validation gate must exit 1: {} {}",
            stdout, stderr
        );
        assert!(
            stderr.contains("post-sync validation failed"),
            "stderr should name the failed gate: {}",
            stderr
        );
        assert!(
            stderr.contains("pre-sync state restored"),
            "stderr should report the rollback: {}",
            stderr
        );

        // Pre-sync state restored byte-identically (the error is recoverable
        // by rerunning after fixing the upstream ref).
        assert_eq!(fs::read(&stale).unwrap(), stale_before);
        assert_eq!(
            fs::read(project.path().join(".skill-lock.json")).unwrap(),
            lock_before
        );
        assert!(
            !project.path().join(".upstream-pin.json").exists(),
            "a rolled-back sync must not leave a pin behind"
        );
    }

    #[test]
    fn sync_refuses_dirty_worktree() {
        let repo = TempDir::new("sync-it-repo-dirty");
        make_fixture_repo(repo.path());
        let project = TempDir::new("sync-it-project-dirty");
        make_project(project.path());
        init_project_git(project.path());
        let stale = project
            .path()
            .join("skills/upstream/skills/engineering/stale-skill/SKILL.md");
        fs::write(&stale, "locally edited\n").unwrap();

        let (stdout, stderr, code) =
            run_sync(&sync_script_path(), project.path(), repo.path(), "v1");
        assert_eq!(
            code, 1,
            "a dirty worktree must be refused: {} {}",
            stdout, stderr
        );
        assert!(stderr.contains("refusing to sync"), "{}", stderr);
        assert!(stderr.contains("stale-skill"), "{}", stderr);
        assert_eq!(
            fs::read_to_string(&stale).unwrap(),
            "locally edited\n",
            "a refused sync must not touch local edits"
        );
    }

    #[test]
    fn sync_force_overwrites_dirty_worktree() {
        let repo = TempDir::new("sync-it-repo-force");
        make_fixture_repo(repo.path());
        let project = TempDir::new("sync-it-project-force");
        make_project(project.path());
        init_project_git(project.path());
        let stale = project
            .path()
            .join("skills/upstream/skills/engineering/stale-skill/SKILL.md");
        fs::write(&stale, "locally edited\n").unwrap();

        let (stdout, stderr, code) = run_sync_args(
            &sync_script_path(),
            project.path(),
            repo.path(),
            &["--force", "v1"],
        );
        assert_eq!(
            code, 0,
            "--force sync should succeed: {} {}",
            stdout, stderr
        );
        assert!(
            stdout.contains("warning: --force overwrote local changes"),
            "should warn about overwritten local changes: {}",
            stdout
        );
        assert!(stdout.contains("stale-skill"), "{}", stdout);
        assert!(
            stdout.contains("Sync complete — all checks PASS."),
            "{}",
            stdout
        );
        assert!(!stale.exists(), "forced sync replaces the stale tree");
    }
}
