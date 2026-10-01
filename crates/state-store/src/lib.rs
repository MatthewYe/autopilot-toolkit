//! Shared run-state persistence substrate for the published CLIs.
//!
//! Both `distill-cli` and `director-cli` keep their authoritative run state in a
//! project-local, git-ignored directory (`.distill/`, `.director/`). This crate
//! owns the four invariants they must agree on and nothing else: the state
//! directory must be ignorable before anything is captured into it (ADR 0025
//! pattern), it must never be a symlink out of the worktree, the worktree itself
//! must be real, and every state write must be atomic.
//!
//! The state *schemas* are deliberately not here. Reading run state genuinely
//! differs between the two CLIs (missing `schema_version` handling,
//! `deny_unknown_fields` versus a flatten catch-all, forward-only migration
//! chains), so `read_state`, state paths, and distill's cross-process lock
//! protocol stay in their own crates. The caller supplies its directory name and
//! its exact `.gitignore` rule, which is also what keeps each CLI's refusal
//! messages byte-identical to what it printed before this crate existed.

use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Ensure `dir_name` under `worktree` can never become a trackable project file.
///
/// Fail closed unless the root `.gitignore` carries the exact `ignore_rule`, the
/// existing state directory stays inside the worktree, and neither is a symlink.
/// Appends the rule only when it is absent, and refuses if the state directory
/// already exists before that rule is effective — state written before the rule
/// would be visible to git.
pub fn ensure_ignored(worktree: &Path, dir_name: &str, ignore_rule: &str) -> Result<(), String> {
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
    let ignored = content.lines().any(|line| line.trim() == ignore_rule);
    if ignored {
        return ensure_path_safe(worktree, dir_name).map(|_| ());
    }

    if worktree.join(dir_name).exists() {
        return Err(format!(
            "{dir_name} already exists before ignore is effective"
        ));
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&gitignore)
        .map_err(|err| format!("cannot establish {ignore_rule} gitignore: {err}"))?;
    if !content.is_empty() && !content.ends_with('\n') {
        file.write_all(b"\n")
            .map_err(|err| format!("cannot update .gitignore: {err}"))?;
    }
    file.write_all(format!("{ignore_rule}\n").as_bytes())
        .map_err(|err| format!("cannot update .gitignore: {err}"))?;
    ensure_path_safe(worktree, dir_name).map(|_| ())
}

/// The state directory must be a real directory inside the worktree, never a
/// symlink that could redirect run state elsewhere.
///
/// Returns the validated joined path (the caller's own `worktree.join(dir_name)`);
/// a state directory that does not exist yet is not an error, it just has nothing
/// to validate. Canonicalize both sides before comparing so the check stays sound
/// under symlinked roots (e.g. macOS `/var` → `/private/var`) while still catching
/// a state directory that resolves out of the worktree.
pub fn ensure_path_safe(worktree: &Path, dir_name: &str) -> Result<PathBuf, String> {
    let dir = worktree.join(dir_name);
    let meta = match fs::symlink_metadata(&dir) {
        Ok(meta) => meta,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(dir),
        Err(err) => return Err(format!("cannot inspect {dir_name} safely: {err}")),
    };
    if meta.file_type().is_symlink() {
        return Err(format!("{dir_name} must not be a symlink"));
    }
    if !meta.is_dir() {
        return Err(format!("{dir_name} must be a directory"));
    }
    let canonical_worktree =
        fs::canonicalize(worktree).map_err(|err| format!("cannot canonicalize worktree: {err}"))?;
    let canonical_dir =
        fs::canonicalize(&dir).map_err(|err| format!("cannot canonicalize {dir_name}: {err}"))?;
    if !canonical_dir.starts_with(&canonical_worktree) {
        return Err(format!("{dir_name} must stay inside the worktree"));
    }
    Ok(dir)
}

/// Fail closed unless the target worktree is an existing directory whose state
/// directory is safe to use.
pub fn ensure_worktree(worktree: &Path, dir_name: &str) -> Result<(), String> {
    if !worktree.is_dir() {
        return Err(format!("worktree does not exist: {}", worktree.display()));
    }
    ensure_path_safe(worktree, dir_name).map(|_| ())
}

/// Write `bytes` to `path` via a temp file plus rename, so a crash can never
/// leave a half-written state file and a concurrent writer can never observe a
/// partially written one.
///
/// The temp name carries a millisecond suffix and is opened with
/// `create_new(true)`: a leftover temp file from a crashed writer is never
/// silently reused or truncated, and two writers in the same millisecond cannot
/// interleave into one temp file — the second one fails instead. Publication
/// payload immutability is not this function's job: it is enforced by content
/// hash at the publication layer.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("cannot create {}: {err}", parent.display()))?;
    }
    let tmp = tmp_path(path);
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
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

fn tmp_path(path: &Path) -> PathBuf {
    let file = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("state");
    path.with_file_name(format!(".{file}.{}.tmp", now_millis()))
}

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Both real callers: distill's `.distill/` and the director's `.director/`.
    const DIRS: [(&str, &str); 2] = [(".distill", "/.distill/"), (".director", "/.director/")];

    fn temp_paths() -> (tempfile::TempDir, tempfile::TempDir) {
        (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap())
    }

    fn temp_names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    // ── 1. symlinked .gitignore ──────────────────────────────────────────────

    #[test]
    fn symlinked_gitignore_is_refused() {
        for (dir_name, rule) in DIRS {
            let (dir, elsewhere) = temp_paths();
            let target = elsewhere.path().join("gitignore");
            fs::write(&target, "target/\n").unwrap();
            std::os::unix::fs::symlink(&target, dir.path().join(".gitignore")).unwrap();

            let error = ensure_ignored(dir.path(), dir_name, rule).unwrap_err();
            assert_eq!(error, ".gitignore must not be a symlink");
            assert_eq!(fs::read_to_string(&target).unwrap(), "target/\n");
        }
    }

    // ── 2. rule missing ──────────────────────────────────────────────────────

    #[test]
    fn missing_rule_is_appended_exactly_once() {
        for (dir_name, rule) in DIRS {
            let (dir, _) = temp_paths();
            fs::write(dir.path().join(".gitignore"), "target/\n").unwrap();

            ensure_ignored(dir.path(), dir_name, rule).unwrap();
            ensure_ignored(dir.path(), dir_name, rule).unwrap();

            assert_eq!(
                fs::read_to_string(dir.path().join(".gitignore")).unwrap(),
                format!("target/\n{rule}\n")
            );
        }
    }

    #[test]
    fn missing_trailing_newline_is_repaired_before_appending() {
        for (dir_name, rule) in DIRS {
            let (dir, _) = temp_paths();
            fs::write(dir.path().join(".gitignore"), "target/").unwrap();

            ensure_ignored(dir.path(), dir_name, rule).unwrap();

            assert_eq!(
                fs::read_to_string(dir.path().join(".gitignore")).unwrap(),
                format!("target/\n{rule}\n")
            );
        }
    }

    #[test]
    fn absent_gitignore_is_created_with_just_the_rule() {
        for (dir_name, rule) in DIRS {
            let (dir, _) = temp_paths();

            ensure_ignored(dir.path(), dir_name, rule).unwrap();

            assert_eq!(
                fs::read_to_string(dir.path().join(".gitignore")).unwrap(),
                format!("{rule}\n")
            );
        }
    }

    // ── 3. rule already present ──────────────────────────────────────────────

    #[test]
    fn present_rule_is_never_duplicated() {
        for (dir_name, rule) in DIRS {
            let (dir, _) = temp_paths();
            let original = format!("# state\n{rule}\n");
            fs::write(dir.path().join(".gitignore"), &original).unwrap();

            ensure_ignored(dir.path(), dir_name, rule).unwrap();
            ensure_ignored(dir.path(), dir_name, rule).unwrap();

            assert_eq!(
                fs::read_to_string(dir.path().join(".gitignore")).unwrap(),
                original
            );
        }
    }

    #[test]
    fn present_rule_matches_after_trimming() {
        for (dir_name, rule) in DIRS {
            let (dir, _) = temp_paths();
            let original = format!("  {rule}  \n");
            fs::write(dir.path().join(".gitignore"), &original).unwrap();

            ensure_ignored(dir.path(), dir_name, rule).unwrap();

            assert_eq!(
                fs::read_to_string(dir.path().join(".gitignore")).unwrap(),
                original
            );
        }
    }

    #[test]
    fn ignored_branch_still_validates_the_state_directory() {
        for (dir_name, rule) in DIRS {
            let (dir, elsewhere) = temp_paths();
            fs::write(dir.path().join(".gitignore"), format!("{rule}\n")).unwrap();
            std::os::unix::fs::symlink(elsewhere.path(), dir.path().join(dir_name)).unwrap();

            let error = ensure_ignored(dir.path(), dir_name, rule).unwrap_err();
            assert_eq!(error, format!("{dir_name} must not be a symlink"));
        }
    }

    #[test]
    fn ignored_branch_accepts_an_existing_real_state_directory() {
        for (dir_name, rule) in DIRS {
            let (dir, _) = temp_paths();
            fs::write(dir.path().join(".gitignore"), format!("{rule}\n")).unwrap();
            fs::create_dir(dir.path().join(dir_name)).unwrap();

            ensure_ignored(dir.path(), dir_name, rule).unwrap();
        }
    }

    // ── 4. state directory exists before the rule is effective ───────────────

    #[test]
    fn unignored_existing_state_directory_stops_closed() {
        for (dir_name, rule) in DIRS {
            let (dir, _) = temp_paths();
            fs::create_dir(dir.path().join(dir_name)).unwrap();

            let error = ensure_ignored(dir.path(), dir_name, rule).unwrap_err();
            assert_eq!(
                error,
                format!("{dir_name} already exists before ignore is effective")
            );
            assert!(!dir.path().join(".gitignore").exists());
        }
    }

    // ── 5. escape attempts ───────────────────────────────────────────────────

    #[test]
    fn symlinked_state_directory_is_refused() {
        for (dir_name, _) in DIRS {
            let (dir, elsewhere) = temp_paths();
            std::os::unix::fs::symlink(elsewhere.path(), dir.path().join(dir_name)).unwrap();

            let error = ensure_path_safe(dir.path(), dir_name).unwrap_err();
            assert_eq!(error, format!("{dir_name} must not be a symlink"));
        }
    }

    #[test]
    fn non_directory_state_path_is_refused() {
        for (dir_name, _) in DIRS {
            let (dir, _) = temp_paths();
            fs::write(dir.path().join(dir_name), "not a directory").unwrap();

            let error = ensure_path_safe(dir.path(), dir_name).unwrap_err();
            assert_eq!(error, format!("{dir_name} must be a directory"));
        }
    }

    #[test]
    fn absent_state_directory_is_not_an_error() {
        for (dir_name, _) in DIRS {
            let (dir, _) = temp_paths();

            let checked = ensure_path_safe(dir.path(), dir_name).unwrap();
            assert_eq!(checked, dir.path().join(dir_name));
        }
    }

    #[test]
    fn worktree_reached_through_a_symlink_is_not_a_false_positive() {
        for (dir_name, _) in DIRS {
            let (dir, _) = temp_paths();
            let real = dir.path().join("real");
            fs::create_dir_all(real.join(dir_name)).unwrap();
            let link = dir.path().join("link");
            std::os::unix::fs::symlink(&real, &link).unwrap();

            ensure_path_safe(&link, dir_name).unwrap();
        }
    }

    #[test]
    fn worktree_must_be_a_real_directory() {
        let (dir, _) = temp_paths();
        let missing = dir.path().join("missing");

        let error = ensure_worktree(&missing, ".distill").unwrap_err();
        assert_eq!(
            error,
            format!("worktree does not exist: {}", missing.display())
        );
    }

    #[test]
    fn ensure_worktree_validates_the_state_directory() {
        for (dir_name, _) in DIRS {
            let (dir, elsewhere) = temp_paths();
            std::os::unix::fs::symlink(elsewhere.path(), dir.path().join(dir_name)).unwrap();

            let error = ensure_worktree(dir.path(), dir_name).unwrap_err();
            assert_eq!(error, format!("{dir_name} must not be a symlink"));
        }
    }

    // ── 6. oversized path ────────────────────────────────────────────────────

    #[test]
    fn oversized_worktree_path_fails_closed() {
        // Neither source implementation carried an explicit path-length guard:
        // the refusal comes from the OS (`ENAMETOOLONG`) surfacing through the
        // existing "cannot inspect ... safely" arm, which is the contract this
        // test pins down. No new length check is introduced.
        let long = PathBuf::from("/tmp").join("a".repeat(5000));

        let error = ensure_path_safe(&long, ".distill").unwrap_err();
        assert!(
            error.starts_with("cannot inspect .distill safely:"),
            "got: {error}"
        );

        let error = ensure_worktree(&long, ".director").unwrap_err();
        assert!(
            error.starts_with("worktree does not exist:"),
            "got: {error}"
        );
    }

    // ── 7. temp-name collision ───────────────────────────────────────────────

    #[test]
    fn occupied_temp_name_fails_closed_without_clobbering() {
        let (dir, _) = temp_paths();
        let target = dir.path().join("state.json");
        let mut refusal = None;
        let mut window = None;
        for _ in 0..32 {
            // Occupy the temp names this write could pick: a millisecond window
            // starting now. The call lands microseconds after the window is
            // filled, so the `create_new(true)` collision is effectively certain;
            // a machine stalling past the whole window just retries.
            let start = now_millis();
            for offset in 0..64_u128 {
                let candidate = dir
                    .path()
                    .join(format!(".state.json.{}.tmp", start + offset));
                if !candidate.exists() {
                    fs::write(&candidate, b"occupied").unwrap();
                }
            }
            match atomic_write(&target, b"payload") {
                Err(err) if err.contains("cannot create temp file") => {
                    refusal = Some(err);
                    window = Some(start);
                    break;
                }
                Ok(()) => {
                    // Landed outside the window: undo the successful write and retry.
                    fs::remove_file(&target).ok();
                }
                Err(other) => panic!("unexpected error: {other}"),
            }
        }

        let error = refusal.expect("a write into an occupied temp window must be refused");
        assert!(error.contains("cannot create temp file"), "got: {error}");
        assert!(
            !target.exists(),
            "the target must stay untouched when the temp file cannot be created"
        );
        for offset in 0..64_u128 {
            let candidate = dir
                .path()
                .join(format!(".state.json.{}.tmp", window.unwrap() + offset));
            assert_eq!(
                fs::read(&candidate).unwrap().as_slice(),
                b"occupied".as_slice(),
                "an occupied temp file must never be truncated"
            );
        }
    }

    // ── 8. success path ──────────────────────────────────────────────────────

    #[test]
    fn atomic_write_creates_parents_and_leaves_no_temp_file() {
        let (dir, _) = temp_paths();
        let path = dir.path().join("nested/run/state.json");

        atomic_write(&path, b"payload").unwrap();

        assert_eq!(fs::read(&path).unwrap().as_slice(), b"payload".as_slice());
        assert_eq!(
            temp_names(path.parent().unwrap()),
            vec!["state.json".to_string()]
        );
    }

    #[test]
    fn atomic_write_replaces_an_existing_file_without_temp_residue() {
        let (dir, _) = temp_paths();
        let path = dir.path().join("state.json");
        fs::write(&path, b"old").unwrap();

        atomic_write(&path, b"new").unwrap();

        assert_eq!(fs::read(&path).unwrap().as_slice(), b"new".as_slice());
        assert_eq!(temp_names(dir.path()), vec!["state.json".to_string()]);
    }

    // ── 9. rename failure cleanup ────────────────────────────────────────────

    #[test]
    fn failed_rename_removes_the_temp_file() {
        let (dir, _) = temp_paths();
        let target = dir.path().join("state.json");
        fs::create_dir(&target).unwrap();

        let error = atomic_write(&target, b"payload").unwrap_err();

        assert!(error.contains("cannot replace"), "got: {error}");
        assert!(target.is_dir());
        assert_eq!(temp_names(dir.path()), vec!["state.json".to_string()]);
    }
}
