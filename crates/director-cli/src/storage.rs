//! Project-local state storage: the `.director/` directory, the exact
//! `.gitignore` rule that must protect it, and atomic writes.
//!
//! The `.director/` hygiene guards and the atomic write itself live in the
//! shared `state-store` substrate; this module binds them to this CLI's
//! directory name and rule, and keeps the git fingerprinting that the
//! director's resume protocol owns.

use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

/// The state directory this CLI keeps its run state in.
pub const DIRECTOR_DIR: &str = ".director";

/// The one rule that makes `.director/` effectively ignored.
pub const DIRECTOR_IGNORE_RULE: &str = "/.director/";

/// Review rounds allowed per gate layer before Escalation (ADR 0048).
pub const DEFAULT_ROUND_CAP: u64 = 3;

/// Failed Worker dispatches allowed per ticket before Escalation (ADR 0048):
/// the failed attempt plus exactly one same-Worker retry.
pub const DISPATCH_RETRY_BUDGET: u64 = 1;

/// What the worktree looked like when the run state was last written: the
/// committed HEAD, its tree, and the checked-out branch. Resume compares this
/// against the live worktree, so a boundary commit the Director made is
/// distinguishable from context drift somebody else introduced (ADR 0035
/// pattern).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorktreeFingerprint {
    pub head: String,
    pub tree: String,
    pub branch: String,
}

impl WorktreeFingerprint {
    /// One line per differing field, so a refusal can name both sides.
    pub fn drift(&self, live: &Self) -> Vec<String> {
        let mut drift = Vec::new();
        if self.head != live.head {
            drift.push(format!("HEAD: recorded {}, live {}", self.head, live.head));
        }
        if self.tree != live.tree {
            drift.push(format!("tree: recorded {}, live {}", self.tree, live.tree));
        }
        if self.branch != live.branch {
            drift.push(format!(
                "branch: recorded {}, live {}",
                self.branch, live.branch
            ));
        }
        drift
    }
}

/// Capture the worktree's HEAD, tree hash, and checked-out branch. The CLI
/// stays offline: this reads the local repository and nothing else.
pub fn capture_fingerprint(worktree: &Path) -> Result<WorktreeFingerprint, String> {
    Ok(WorktreeFingerprint {
        head: git_rev_parse(worktree, &["HEAD"])?,
        tree: git_rev_parse(worktree, &["HEAD^{tree}"])?,
        branch: git_rev_parse(worktree, &["--abbrev-ref", "HEAD"])?,
    })
}

/// `git rev-parse <args...>` inside the worktree, refused unless it succeeds
/// and prints something — a run state that cannot be pinned to a worktree
/// state is not worth writing.
fn git_rev_parse(worktree: &Path, args: &[&str]) -> Result<String, String> {
    let what = args.join(" ");
    let output = Command::new("git")
        .arg("rev-parse")
        .args(args)
        .current_dir(worktree)
        .output()
        .map_err(|err| format!("cannot run `git rev-parse {what}`: {err}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!(
            "`git rev-parse {what}` failed in {}: {detail}",
            worktree.display()
        ));
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if value.is_empty() {
        return Err(format!(
            "`git rev-parse {what}` returned nothing in {}",
            worktree.display()
        ));
    }
    Ok(value)
}

/// Fail closed unless run state can never become a trackable project file:
/// the root `.gitignore` must carry the exact [`DIRECTOR_IGNORE_RULE`], the
/// existing `.director/` path must stay inside the worktree, and neither may
/// be a symlink.
///
/// Thin adapter over the shared [`state_store`] substrate: `.director` and
/// [`DIRECTOR_IGNORE_RULE`] are this CLI's vocabulary, and the refusal messages
/// stay exactly the ones the director has always printed.
pub fn ensure_director_ignored(worktree: &Path) -> Result<(), String> {
    state_store::ensure_ignored(worktree, DIRECTOR_DIR, DIRECTOR_IGNORE_RULE)
}

/// Fail closed unless the target worktree is an existing directory whose
/// `.director/` state directory is safe to use.
pub fn ensure_worktree(worktree: &Path) -> Result<(), String> {
    state_store::ensure_worktree(worktree, DIRECTOR_DIR)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn ignore_rule_is_established_exactly_once() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(".gitignore"), "target/\n").unwrap();
        ensure_director_ignored(dir.path()).unwrap();
        ensure_director_ignored(dir.path()).unwrap();
        let content = fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        assert_eq!(content, format!("target/\n{DIRECTOR_IGNORE_RULE}\n"));
    }

    #[test]
    fn ignore_rule_appends_a_missing_trailing_newline_first() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(".gitignore"), "target/").unwrap();
        ensure_director_ignored(dir.path()).unwrap();
        let content = fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        assert_eq!(content, format!("target/\n{DIRECTOR_IGNORE_RULE}\n"));
    }

    #[test]
    fn unignored_existing_state_directory_stops_closed() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join(".director")).unwrap();
        let error = ensure_director_ignored(dir.path()).unwrap_err();
        assert!(error.contains("before ignore is effective"), "got: {error}");
    }

    #[test]
    fn symlinked_state_directory_stops_closed() {
        let dir = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join(".gitignore"),
            format!("{DIRECTOR_IGNORE_RULE}\n"),
        )
        .unwrap();
        std::os::unix::fs::symlink(elsewhere.path(), dir.path().join(".director")).unwrap();
        let error = ensure_director_ignored(dir.path()).unwrap_err();
        assert!(error.contains("must not be a symlink"), "got: {error}");
    }
}
