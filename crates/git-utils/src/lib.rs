//! Git utilities for the autopilot-toolkit workspace.
//!
//! Provides:
//! - `compute_tree_hash(folder)` — compute a deterministic git tree hash for a directory
//!
//! Uses temporary git repos internally (no dependency on the host repo's git state).

use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Compute the git tree hash of a directory by creating a temporary git
/// repo, adding all files, and running `git write-tree`.
///
/// Returns the 40-char hex SHA-1 of the tree, or a `String` error.
pub fn compute_tree_hash(folder: &Path) -> Result<String, String> {
    if !folder.is_dir() {
        return Err(format!("folder not found: {}", folder.display()));
    }

    let n = TEMP_COUNTER.fetch_add(1, Ordering::SeqCst);
    let tmp = std::env::temp_dir().join(format!("git-utils-hash-{}-{}", std::process::id(), n));
    fs::create_dir(&tmp).map_err(|e| format!("cannot create temp dir: {}", e))?;

    let result = (|| -> Result<String, String> {
        run_git(&tmp, &["init", "--quiet"])?;
        run_git_worktree(&tmp, folder, &["add", "-A"])?;
        let hash = run_git_stdout(&tmp, &["write-tree"])?;
        Ok(hash.trim().to_string())
    })();

    let _ = fs::remove_dir_all(&tmp);
    result
}

// ── Private git helpers ─────────────────────────────────────────────────────

fn run_git(git_dir: &Path, args: &[&str]) -> Result<(), String> {
    run_git_inner(git_dir, None, args, false).map(|_| ())
}

fn run_git_worktree(git_dir: &Path, work_tree: &Path, args: &[&str]) -> Result<(), String> {
    run_git_inner(git_dir, Some(work_tree), args, false).map(|_| ())
}

fn run_git_stdout(git_dir: &Path, args: &[&str]) -> Result<String, String> {
    run_git_inner(git_dir, None, args, true)
}

fn run_git_inner(
    git_dir: &Path,
    work_tree: Option<&Path>,
    args: &[&str],
    capture_stdout: bool,
) -> Result<String, String> {
    let mut cmd = Command::new("git");
    cmd.arg(format!("--git-dir={}", git_dir.display()));
    if let Some(wt) = work_tree {
        cmd.arg(format!("--work-tree={}", wt.display()));
    }
    cmd.args(args);

    let output = cmd.output().map_err(|e| format!("git error: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git error: {}", stderr.trim()));
    }

    if capture_stdout {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Ok(String::new())
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TempDir(std::path::PathBuf);

    impl TempDir {
        fn new() -> Self {
            let n = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
            let dir = std::env::temp_dir()
                .join(format!("git-utils-test-{}-{}", std::process::id(), n));
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

    fn write_file(dir: &Path, name: &str, content: &str) {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, content).unwrap();
    }

    #[test]
    fn compute_tree_hash_non_existent_folder() {
        let result = compute_tree_hash(Path::new("/nonexistent/path/12345"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("folder not found"));
    }

    #[test]
    fn compute_tree_hash_empty_dir() {
        let tmp = TempDir::new();
        let sub = tmp.path().join("empty_skill");
        fs::create_dir_all(&sub).unwrap();

        let hash = compute_tree_hash(&sub).expect("hash empty dir");
        assert!(!hash.is_empty(), "hash should not be empty");
    }

    #[test]
    fn compute_tree_hash_deterministic() {
        let tmp = TempDir::new();
        let sub = tmp.path().join("some_skill");
        fs::create_dir_all(&sub).unwrap();
        write_file(&sub, "SKILL.md", "# Test Skill\n\nHello world.\n");

        let hash1 = compute_tree_hash(&sub).expect("hash1");
        let hash2 = compute_tree_hash(&sub).expect("hash2");
        assert_eq!(hash1, hash2, "same content should produce same hash");
    }

    #[test]
    fn compute_tree_hash_changes_with_content() {
        let tmp = TempDir::new();
        let sub = tmp.path().join("mutable_skill");
        fs::create_dir_all(&sub).unwrap();
        write_file(&sub, "SKILL.md", "version 1\n");

        let hash1 = compute_tree_hash(&sub).expect("hash1");

        write_file(&sub, "SKILL.md", "version 2\n");
        let hash2 = compute_tree_hash(&sub).expect("hash2");

        assert_ne!(hash1, hash2, "different content should produce different hash");
    }
}
