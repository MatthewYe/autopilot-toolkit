//! Project-local state storage: the `.director/` directory, the exact
//! `.gitignore` rule that must protect it, and atomic writes.

use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::Path;

/// The one rule that makes `.director/` effectively ignored.
pub const DIRECTOR_IGNORE_RULE: &str = "/.director/";

/// Review rounds allowed per gate layer before Escalation (ADR 0048).
pub const DEFAULT_ROUND_CAP: u64 = 3;

/// Failed Worker dispatches allowed per ticket before Escalation (ADR 0048):
/// the failed attempt plus exactly one same-Worker retry.
pub const DISPATCH_RETRY_BUDGET: u64 = 1;

/// Fail closed unless run state can never become a trackable project file:
/// the root `.gitignore` must carry the exact [`DIRECTOR_IGNORE_RULE`], the
/// existing `.director/` path must stay inside the worktree, and neither may
/// be a symlink.
pub fn ensure_director_ignored(worktree: &Path) -> Result<(), String> {
    let gitignore = worktree.join(".gitignore");
    if let Ok(meta) = fs::symlink_metadata(&gitignore) {
        if meta.file_type().is_symlink() {
            return Err(".gitignore must not be a symlink".to_string());
        }
    }

    let content = match fs::read_to_string(&gitignore) {
        Ok(content) => content,
        Err(err) if err.kind() == ErrorKind::NotFound => String::new(),
        Err(err) => return Err(format!("cannot read .gitignore safely: {err}")),
    };
    let ignored = content
        .lines()
        .any(|line| line.trim() == DIRECTOR_IGNORE_RULE);
    if ignored {
        return ensure_director_path_safe(worktree);
    }

    if worktree.join(".director").exists() {
        return Err(".director already exists before ignore is effective".to_string());
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&gitignore)
        .map_err(|err| format!("cannot establish {DIRECTOR_IGNORE_RULE} gitignore: {err}"))?;
    if !content.is_empty() && !content.ends_with('\n') {
        file.write_all(b"\n")
            .map_err(|err| format!("cannot update .gitignore: {err}"))?;
    }
    file.write_all(format!("{DIRECTOR_IGNORE_RULE}\n").as_bytes())
        .map_err(|err| format!("cannot update .gitignore: {err}"))?;
    ensure_director_path_safe(worktree)
}

/// `.director/` must be a real directory inside the worktree, never a symlink
/// that could redirect run state elsewhere.
pub fn ensure_director_path_safe(worktree: &Path) -> Result<(), String> {
    let director = worktree.join(".director");
    let meta = match fs::symlink_metadata(&director) {
        Ok(meta) => meta,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(format!("cannot inspect .director safely: {err}")),
    };
    if meta.file_type().is_symlink() {
        return Err(".director must not be a symlink".to_string());
    }
    if !meta.is_dir() {
        return Err(".director must be a directory".to_string());
    }
    // Canonicalize both sides: comparison stays sound under symlinked roots
    // (e.g. macOS `/var` → `/private/var`) and still catches a `.director/`
    // that resolves out of the worktree.
    let worktree =
        fs::canonicalize(worktree).map_err(|err| format!("cannot canonicalize worktree: {err}"))?;
    let director = fs::canonicalize(&director)
        .map_err(|err| format!("cannot canonicalize .director: {err}"))?;
    if !director.starts_with(&worktree) {
        return Err(".director must stay inside the worktree".to_string());
    }
    Ok(())
}

/// Fail closed unless the target worktree is an existing directory.
pub fn ensure_worktree(worktree: &Path) -> Result<(), String> {
    if !worktree.is_dir() {
        return Err(format!("worktree does not exist: {}", worktree.display()));
    }
    ensure_director_path_safe(worktree)
}

/// Write `bytes` to `path` via a temp file plus rename so a crash can never
/// leave a half-written state file.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("cannot create {}: {err}", parent.display()))?;
    }
    let tmp = path.with_extension("json.tmp");
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    let mut file = options
        .open(&tmp)
        .map_err(|err| format!("cannot create temp file {}: {err}", tmp.display()))?;
    file.write_all(bytes)
        .map_err(|err| format!("cannot write temp file {}: {err}", tmp.display()))?;
    file.sync_all()
        .map_err(|err| format!("cannot sync temp file {}: {err}", tmp.display()))?;
    drop(file);
    fs::rename(&tmp, path).map_err(|err| {
        fs::remove_file(&tmp).ok();
        format!("cannot replace {}: {err}", path.display())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
