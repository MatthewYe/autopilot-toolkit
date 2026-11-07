//! Upstream sync — replaces the vendored upstream snapshot
//! (`skills/upstream/`) with a pinned ref of mattpocock/skills and rewrites
//! `.skill-lock.json` accordingly.
//!
//! Migrated from `scripts/sync-upstream.rs`. Uses `shared::SkillLock` as the
//! source-of-truth lock model (merge rules live in
//! [`SkillLock::replace_skills`]) and `skill_check::check_skills` for the
//! in-process post-sync self-check.
//!
//! Public API:
//! - `sync_upstream(project_root, upstream_ref, repo_url, now)` → `Result<SyncOutcome, String>`
//! - `sync_upstream_with_options(..., options)` → same, with `--force` support
//!
//! Failure atomicity: before mutating anything, the sync snapshots
//! `skills/upstream/`, `.skill-lock.json`, and `.upstream-pin.json` into a
//! temp dir. Any post-sync gate failure (skill check FAIL/SKIP, validation
//! suite FAIL) restores that snapshot, so a failed sync leaves the project
//! byte-identical to its pre-sync state. A panic in the mutation phase also
//! restores via the snapshot's `Drop` impl. A dirty worktree (or untracked
//! files under `skills/upstream/`) is refused before the clone unless
//! `SyncOptions::force` is set.
//!
//! Intentional behavior changes vs. the legacy script:
//! - An unparseable existing `.skill-lock.json` is a hard error (the legacy
//!   script warned and continued with an empty lock, misclassifying every
//!   skill as an orphan).
//! - `updatedAt` is stamped only on entries whose hash changed or that are
//!   new (the legacy script re-stamped every entry).
//! - The `dismissed` object is preserved verbatim instead of being reset to
//!   `{}` (a brand-new lock file starts with `dismissed: {}`).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use shared::{LockFlavor, LockedSkill, SkillLock};

/// Default upstream repository. Entry points may override it (tests inject a
/// local fixture repo; the CLI honors `AUTOPILOT_UPSTREAM_REPO`).
pub const DEFAULT_UPSTREAM_REPO: &str = "https://github.com/mattpocock/skills.git";
/// Value of the lock entries' `source` field.
pub const UPSTREAM_SOURCE: &str = "mattpocock/skills";
/// Plugin name written into every upstream lock entry.
pub const PLUGIN_NAME: &str = "mattpocock-skills";
/// Lockfile format version used when a new `.skill-lock.json` is created.
pub const SKILL_LOCK_VERSION: u32 = 4;

/// BUCKET_DIRS lists the subdirectories under the upstream repo's skills/
/// that contain shippable skills.
const BUCKET_DIRS: &[&str] = &["engineering", "productivity", "misc"];

/// Beta upstream skills this toolkit ships on purpose.
///
/// Upstream's `in-progress/` bucket is excluded from the plugin and can
/// change or disappear without warning, so nothing is picked up from it
/// implicitly: a skill ships only when its name is listed here. Remove a
/// name once upstream graduates it into a stable bucket (the stable entry
/// wins either way) or drops it (the sync fails until the allowlist is
/// updated).
const IN_PROGRESS_ALLOWLIST: &[&str] = &["loop-me"];

// ── Public types ───────────────────────────────────────────────────────────

/// Skills discovered in an upstream checkout, plus human-readable notes
/// about how the in-progress allowlist was applied.
#[derive(Debug)]
pub struct Discovery {
    /// Discovered skills, sorted by name. Timestamps are unset; the lock
    /// merge (`SkillLock::replace_skills`) assigns them.
    pub skills: Vec<LockedSkill>,
    /// Allowlist decisions, e.g. "shipping loop-me" or "loop-me graduated
    /// to a stable bucket, keeping the stable entry".
    pub allowlist_notes: Vec<String>,
}

/// Options for a sync run.
#[derive(Debug, Clone, Default)]
pub struct SyncOptions {
    /// Proceed even though the worktree is dirty or `skills/upstream/` holds
    /// untracked files (`--force` in the CLI). The sync overwrites local
    /// edits under `skills/upstream/` and in the lock/pin files; the paths
    /// it will overwrite are reported in [`SyncOutcome::forced_overwrites`].
    /// The pre-sync state is still backed up and restored if a gate fails.
    pub force: bool,
}

/// Everything an entry point needs to render the sync report.
#[derive(Debug)]
pub struct SyncOutcome {
    /// The ref that was requested.
    pub upstream_ref: String,
    /// The repo URL the ref was fetched from.
    pub repo_url: String,
    /// The commit the ref resolved to.
    pub resolved_commit: String,
    /// Skills discovered at the ref, sorted by name (post-merge timestamps).
    pub discovered: Vec<LockedSkill>,
    /// Names newly added to the lock.
    pub added: Vec<String>,
    /// Names whose `skillFolderHash` changed.
    pub updated: Vec<String>,
    /// Names kept with unchanged hash (and unchanged timestamps).
    pub unchanged: Vec<String>,
    /// Names removed from the lock (no longer present upstream).
    pub removed: Vec<String>,
    /// Allowlist decisions made during discovery.
    pub allowlist_notes: Vec<String>,
    /// Paths a `--force` run overwrote (empty unless local changes were
    /// present and `SyncOptions::force` was set).
    pub forced_overwrites: Vec<String>,
    /// Post-sync self-check results, formatted one per line.
    pub check_lines: Vec<String>,
    /// Path of the written lock file.
    pub lock_path: PathBuf,
    /// Path of the replaced upstream tree.
    pub upstream_dir: PathBuf,
}

// ── Sync ───────────────────────────────────────────────────────────────────

/// Sync `skills/upstream/` and `.skill-lock.json` under `project_root` to
/// `upstream_ref` of `repo_url`, with the default options (no `--force`).
pub fn sync_upstream(
    project_root: &Path,
    upstream_ref: &str,
    repo_url: &str,
    now: &str,
) -> Result<SyncOutcome, String> {
    sync_upstream_with_options(
        project_root,
        upstream_ref,
        repo_url,
        now,
        &SyncOptions::default(),
    )
}

/// Sync with explicit options.
///
/// Steps: check preconditions (a dirty worktree or untracked files under
/// `skills/upstream/` are refused unless `force` is set) → clone the ref into
/// a temp dir → discover skills → load the existing lock (missing file starts
/// a fresh one; an unparseable one is a hard error) → merge via
/// `SkillLock::replace_skills` → snapshot the pre-sync state → replace the
/// upstream tree → write the byte-stable lock → run the skill check
/// in-process (any FAIL **or** SKIP fails the sync) → run the full validation
/// suite. If any gate fails, the snapshot is restored and the error is
/// returned: a failed sync leaves the project byte-identical to its pre-sync
/// state.
///
/// `now` is the timestamp used for new/changed lock entries. The temp clone
/// and the snapshot are removed on every path, success or failure (unless the
/// restore itself fails, in which case the snapshot path is reported so it can
/// be recovered by hand).
pub fn sync_upstream_with_options(
    project_root: &Path,
    upstream_ref: &str,
    repo_url: &str,
    now: &str,
    options: &SyncOptions,
) -> Result<SyncOutcome, String> {
    if upstream_ref.trim().is_empty() {
        return Err("upstream ref must not be empty".to_string());
    }

    let upstream_dir = project_root.join("skills").join("upstream");
    let lock_path = project_root.join(shared::SKILL_LOCK_FILE);

    // 0. Preconditions, before the clone: never start a sync that would
    //    bury local work. Outside a git worktree the check cannot run; the
    //    backup below still protects the sync itself.
    let forced_overwrites = check_worktree_preconditions(project_root, options)?;

    // 1. Fetch upstream at the pinned ref (temp clone, cleaned up on drop).
    let (clone, resolved_commit) = clone_at_ref(repo_url, upstream_ref)?;

    // 2. Discover skills from the clone — before anything is replaced, so
    //    an allowlisted skill missing at this ref fails the sync cleanly.
    let discovery = discover_upstream_skills(clone.path(), repo_url)?;

    // 3. Load the existing lock. A missing file starts a fresh lock; an
    //    unparseable one is a hard error (continuing with an empty lock
    //    would misclassify every skill as an orphan).
    let mut lock = if lock_path.exists() {
        shared::load_skill_lock_at(project_root)
            .map_err(|e| format!("cannot sync with an unparseable lock file: {}", e))?
    } else {
        SkillLock {
            version: SKILL_LOCK_VERSION,
            source_type: None,
            skills: vec![],
            dismissed: Some(serde_json::json!({})),
            flavor: LockFlavor::Skill,
        }
    };

    // 4. Classify the merge, then apply it (timestamps are assigned by
    //    `replace_skills`).
    let previous: BTreeMap<String, String> = lock
        .skills
        .iter()
        .map(|s| (s.name.clone(), s.skill_folder_hash.clone()))
        .collect();
    let mut added: Vec<String> = Vec::new();
    let mut updated: Vec<String> = Vec::new();
    let mut unchanged: Vec<String> = Vec::new();
    for skill in &discovery.skills {
        match previous.get(&skill.name) {
            None => added.push(skill.name.clone()),
            Some(old_hash) if *old_hash == skill.skill_folder_hash => {
                unchanged.push(skill.name.clone())
            }
            Some(_) => updated.push(skill.name.clone()),
        }
    }
    let removed: Vec<String> = previous
        .keys()
        .filter(|n| !discovery.skills.iter().any(|s| &s.name == *n))
        .cloned()
        .collect();

    lock.replace_skills(discovery.skills.clone(), now);
    let discovered = lock.skills.clone();
    let allowlist_notes = discovery.allowlist_notes.clone();

    // 5. Snapshot the pre-sync state. Restored on any gate failure below,
    //    and on panic via the guard's `Drop` impl.
    let mut backup = Backup::create(project_root)?;

    let mutated = (move || -> Result<SyncOutcome, String> {
        // 5a. Replace the upstream tree.
        if upstream_dir.exists() {
            fs::remove_dir_all(&upstream_dir)
                .map_err(|e| format!("cannot remove old upstream tree: {}", e))?;
        }
        fs::create_dir_all(&upstream_dir)
            .map_err(|e| format!("cannot create upstream dir: {}", e))?;
        copy_dir_except_git(clone.path(), &upstream_dir)
            .map_err(|e| format!("cannot copy upstream tree: {}", e))?;

        // 5b. Write the lock (byte-stable serialization).
        fs::write(&lock_path, lock.to_bytes())
            .map_err(|e| format!("cannot write {}: {}", lock_path.display(), e))?;

        // 5c. Post-sync self-check, in-process. A SKIP means a check could
        //     not run — fail closed instead of passing silently.
        let report = skill_check::check_skills(project_root)?;
        if !report.updated.is_empty() {
            // Defensive: unreachable right after a sync, which just computed
            // the hashes itself — apply the repair rather than ignoring it.
            skill_check::write_updated_lockfile(project_root, &report.updated)?;
        }
        let check_lines: Vec<String> = report
            .results
            .iter()
            .map(|(name, result)| skill_check::format_result(name, result))
            .collect();
        if skill_check::any_not_pass(&report.results) {
            let failures: Vec<String> = report
                .results
                .iter()
                .filter(|(_, r)| {
                    matches!(
                        r,
                        skill_check::CheckResult::Fail(_) | skill_check::CheckResult::Skip(_)
                    )
                })
                .map(|(name, r)| skill_check::format_result(name, r))
                .collect();
            return Err(format!("post-sync check failed:\n{}", failures.join("\n")));
        }

        // 5d. Full validation gate (frontmatter, documented inventory,
        //     companion files). Any failure rolls the whole sync back.
        let validation = validation_runner::run_validation(project_root)
            .map_err(|e| format!("post-sync validation could not run: {}", e))?;
        if validation.has_failures {
            return Err(format!(
                "post-sync validation failed:\n{}",
                validation.report
            ));
        }

        Ok(SyncOutcome {
            upstream_ref: upstream_ref.to_string(),
            repo_url: repo_url.to_string(),
            resolved_commit,
            discovered,
            added,
            updated,
            unchanged,
            removed,
            allowlist_notes,
            forced_overwrites,
            check_lines,
            lock_path,
            upstream_dir,
        })
    })();

    match mutated {
        Ok(outcome) => {
            backup.disarm();
            Ok(outcome)
        }
        Err(err) => Err(backup.restore_after_failure(err)),
    }
}

// ── Skill discovery ────────────────────────────────────────────────────────

/// Walk an upstream checkout to discover all SKILL.md files: the stable
/// buckets plus the allowlisted in-progress skills.
///
/// Allowlist rules:
/// - A name that graduated into a stable bucket keeps its stable entry
///   (noted in `allowlist_notes`).
/// - An allowlisted name missing entirely at this ref is a hard error, so
///   an allowlisted skill can never be dropped from the lock by accident.
fn discover_upstream_skills(upstream_root: &Path, repo_url: &str) -> Result<Discovery, String> {
    let skills_dir = upstream_root.join("skills");
    if !skills_dir.is_dir() {
        return Err(format!(
            "skills/ not found in upstream at {}",
            upstream_root.display()
        ));
    }

    let mut map: BTreeMap<String, LockedSkill> = BTreeMap::new();

    for bucket in BUCKET_DIRS {
        let bucket_dir = skills_dir.join(bucket);
        if !bucket_dir.is_dir() {
            continue;
        }

        let entries = match fs::read_dir(&bucket_dir) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let skill_md = path.join("SKILL.md");
            if !skill_md.is_file() {
                continue;
            }

            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            if name.is_empty() {
                continue;
            }

            let skill_path = format!("skills/{}/{}/SKILL.md", bucket, name);
            map.insert(
                name.clone(),
                discovered_entry(&name, &path, &skill_path, repo_url)?,
            );
        }
    }

    // ── In-progress allowlist ──────────────────────────────────────────
    //
    // A name that graduated into a stable bucket is already in `map`, and the
    // stable entry wins. A name that vanished upstream fails the sync, so an
    // allowlisted skill can never be dropped from the lock by accident.
    let mut notes: Vec<String> = Vec::new();
    let in_progress_dir = skills_dir.join("in-progress");
    for name in IN_PROGRESS_ALLOWLIST {
        let dir = in_progress_dir.join(name);
        let has_in_progress_copy = dir.join("SKILL.md").is_file();

        if map.contains_key(*name) {
            let state = if has_in_progress_copy {
                "also exists in a stable bucket"
            } else {
                "graduated to a stable bucket"
            };
            notes.push(format!(
                "in-progress allowlist: {} {}, keeping the stable entry",
                name, state
            ));
            continue;
        }

        if !has_in_progress_copy {
            return Err(format!(
                "allowlisted in-progress skill '{}' is missing at this ref; sync a ref that \
                 contains it, or remove it from IN_PROGRESS_ALLOWLIST if upstream dropped it",
                name
            ));
        }

        let skill_path = format!("skills/in-progress/{}/SKILL.md", name);
        map.insert(
            name.to_string(),
            discovered_entry(name, &dir, &skill_path, repo_url)?,
        );
        notes.push(format!("in-progress allowlist: shipping {}", name));
    }

    Ok(Discovery {
        skills: map.into_values().collect(),
        allowlist_notes: notes,
    })
}

/// Build one lock entry for a discovered skill directory (timestamps unset).
fn discovered_entry(
    name: &str,
    dir: &Path,
    skill_path: &str,
    repo_url: &str,
) -> Result<LockedSkill, String> {
    let hash = git_utils::compute_tree_hash(dir)?;
    Ok(LockedSkill {
        name: name.to_string(),
        source: Some(UPSTREAM_SOURCE.to_string()),
        source_type: "github".to_string(),
        source_url: Some(repo_url.to_string()),
        skill_path: skill_path.to_string(),
        skill_folder_hash: hash,
        plugin_name: Some(PLUGIN_NAME.to_string()),
        ..Default::default()
    })
}

// ── Clone-at-ref ───────────────────────────────────────────────────────────

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Temp clone directory, removed on drop (success and failure paths alike).
struct TempClone {
    dir: PathBuf,
}

impl TempClone {
    fn path(&self) -> &Path {
        &self.dir
    }
}

impl Drop for TempClone {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// Fetch `upstream_ref` of `repo_url` into a fresh temp dir.
///
/// `git clone --branch` accepts branches and tags only. Init + fetch +
/// checkout also accepts a commit SHA, which is how the in-progress
/// allowlist is pinned to a main commit that has no release tag yet.
/// Returns the clone and the commit the ref resolved to.
fn clone_at_ref(repo_url: &str, upstream_ref: &str) -> Result<(TempClone, String), String> {
    let n = TEMP_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir =
        std::env::temp_dir().join(format!("upstream-sync-clone-{}-{}", std::process::id(), n));
    fs::create_dir_all(&dir).map_err(|e| format!("cannot create temp clone dir: {}", e))?;
    // From here on the guard cleans the temp dir on every return path.
    let clone = TempClone { dir };

    let steps: Vec<Vec<&str>> = vec![
        vec!["init", "--quiet"],
        vec!["remote", "add", "origin", repo_url],
        vec!["fetch", "--quiet", "--depth", "1", "origin", upstream_ref],
        vec!["checkout", "--quiet", "FETCH_HEAD"],
    ];
    for step in &steps {
        let output = Command::new("git")
            .args(step)
            .current_dir(clone.path())
            .output()
            .map_err(|e| format!("git failed to start: {}", e))?;
        if !output.status.success() {
            return Err(format!(
                "git {} failed: {}",
                step.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
    }

    let resolved = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(clone.path())
        .output()
        .map_err(|e| format!("git rev-parse failed to start: {}", e))?;
    if !resolved.status.success() {
        return Err(format!(
            "git rev-parse HEAD failed: {}",
            String::from_utf8_lossy(&resolved.stderr).trim()
        ));
    }

    Ok((
        clone,
        String::from_utf8_lossy(&resolved.stdout).trim().to_string(),
    ))
}

// ── File copy helpers ──────────────────────────────────────────────────────

/// Recursively copy `src` into `dst`, skipping `.git` directories and
/// preserving symlinks.
fn copy_dir_except_git(src: &Path, dst: &Path) -> std::io::Result<()> {
    if !src.is_dir() {
        return Ok(());
    }
    fs::create_dir_all(dst)?;

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str == ".git" {
            continue;
        }
        let src_path = entry.path();
        let dst_path = dst.join(&*name_str);

        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            std::os::unix::fs::symlink(fs::read_link(&src_path)?, &dst_path)?;
        } else if file_type.is_dir() {
            copy_dir_except_git(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

// ── Preconditions ──────────────────────────────────────────────────────────

/// Refuse a sync that would bury local work, and report what `--force`
/// would overwrite.
///
/// Two classes are inspected when the project root is inside a git worktree:
/// 1. Tracked files with uncommitted changes (`git status` without untracked
///    files). A sync rewrites the lock and the vendored tree, so an
///    uncommitted edit would be silently lost — refuse the whole run.
/// 2. Untracked (non-ignored) files under `skills/upstream/`: the tree is
///    replaced wholesale, so those files would be deleted.
///
/// With [`SyncOptions::force`], the run proceeds and the returned list names
/// the paths that the sync itself overwrites (paths under `skills/upstream/`,
/// the lock, and the pin), so the caller can print them. A project root that
/// is not inside a git worktree cannot be inspected; the gate is skipped
/// (the pre-sync snapshot still protects the sync).
fn check_worktree_preconditions(
    project_root: &Path,
    options: &SyncOptions,
) -> Result<Vec<String>, String> {
    let inside = Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(project_root)
        .output();
    let in_worktree = match &inside {
        Ok(output) => {
            output.status.success() && String::from_utf8_lossy(&output.stdout).trim() == "true"
        }
        Err(_) => false,
    };
    if !in_worktree {
        return Ok(Vec::new());
    }

    let tracked = run_git(
        project_root,
        &["status", "--porcelain", "--untracked-files=no"],
    )?;
    let untracked = run_git(
        project_root,
        &[
            "status",
            "--porcelain",
            "--untracked-files=all",
            "--",
            "skills/upstream",
        ],
    )?;

    let tracked_lines: Vec<String> = tracked
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(str::to_string)
        .collect();
    let untracked_lines: Vec<String> = untracked
        .lines()
        .filter(|line| line.starts_with("??"))
        .map(str::to_string)
        .collect();

    if tracked_lines.is_empty() && untracked_lines.is_empty() {
        return Ok(Vec::new());
    }

    let mut overwritten: Vec<String> = Vec::new();
    for line in tracked_lines.iter().chain(untracked_lines.iter()) {
        if let Some(path) = status_line_path(line) {
            if is_sync_owned_path(&path) {
                overwritten.push(path);
            }
        }
    }

    if options.force {
        return Ok(overwritten);
    }

    let mut problem = String::from("refusing to sync: the worktree has uncommitted changes:\n");
    for line in tracked_lines.iter().chain(untracked_lines.iter()) {
        problem.push_str("  ");
        problem.push_str(line);
        problem.push('\n');
    }
    problem.push_str(
        "\ncommit or stash them, or pass --force to sync anyway (the sync only overwrites \
         skills/upstream/, .skill-lock.json, and .upstream-pin.json; the pre-sync state is \
         backed up and restored if the sync fails)",
    );
    Err(problem)
}

/// Run git in `dir`, returning stdout; a failed command is an error.
fn run_git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .map_err(|e| format!("git failed to start: {}", e))?;
    if !output.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Extract the path from a `git status --porcelain` line (`XY PATH`, or
/// `XY OLD -> NEW` for renames; the overwritten path is NEW).
fn status_line_path(line: &str) -> Option<String> {
    if line.len() < 4 {
        return None;
    }
    let rest = &line[3..];
    let path = rest.rsplit(" -> ").next().unwrap_or(rest);
    Some(path.trim_matches('"').to_string())
}

/// Whether the sync itself writes this repo-relative path.
fn is_sync_owned_path(path: &str) -> bool {
    path == shared::SKILL_LOCK_FILE
        || path == shared::UPSTREAM_PIN_FILE
        || path == "skills/upstream"
        || path.starts_with("skills/upstream/")
}

// ── Failure rollback ───────────────────────────────────────────────────────

/// Pre-sync snapshot of everything the sync mutates, plus the rollback logic.
///
/// Created right before the first mutation; on a gate failure the caller asks
/// it to restore, and on success (`disarm`) the snapshot directory is removed.
/// `Drop` restores too, so a panic in the mutation phase cannot leave a
/// half-applied sync behind. If a restore fails, the snapshot directory is
/// kept and its path reported.
struct Backup {
    project_root: PathBuf,
    dir: PathBuf,
    upstream_existed: bool,
    lock_bytes: Option<Vec<u8>>,
    pin_bytes: Option<Vec<u8>>,
    armed: bool,
    keep: bool,
}

impl Backup {
    fn create(project_root: &Path) -> Result<Self, String> {
        let n = TEMP_COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir =
            std::env::temp_dir().join(format!("upstream-sync-backup-{}-{}", std::process::id(), n));
        fs::create_dir_all(&dir)
            .map_err(|e| format!("cannot create backup dir {}: {}", dir.display(), e))?;

        let upstream = project_root.join("skills").join("upstream");
        let upstream_existed = upstream.is_dir();
        if upstream_existed {
            copy_dir_except_git(&upstream, &dir.join("upstream")).map_err(|e| {
                let _ = fs::remove_dir_all(&dir);
                format!("cannot back up skills/upstream: {}", e)
            })?;
        }
        let lock_bytes = read_optional_file(&project_root.join(shared::SKILL_LOCK_FILE))?;
        let pin_bytes = read_optional_file(&project_root.join(shared::UPSTREAM_PIN_FILE))?;

        Ok(Backup {
            project_root: project_root.to_path_buf(),
            dir,
            upstream_existed,
            lock_bytes,
            pin_bytes,
            armed: true,
            keep: false,
        })
    }

    /// Forget the snapshot: the sync succeeded, so the pre-sync state is no
    /// longer needed (the directory is removed by `Drop`).
    fn disarm(&mut self) {
        self.armed = false;
    }

    /// Attempt to put the project back into its pre-sync state.
    fn restore(&mut self) -> Result<(), String> {
        let upstream = self.project_root.join("skills").join("upstream");
        if self.upstream_existed {
            if upstream.exists() {
                fs::remove_dir_all(&upstream)
                    .map_err(|e| format!("cannot remove failed-sync upstream tree: {}", e))?;
            }
            copy_dir_except_git(&self.dir.join("upstream"), &upstream)
                .map_err(|e| format!("cannot restore skills/upstream: {}", e))?;
        } else if upstream.exists() {
            fs::remove_dir_all(&upstream)
                .map_err(|e| format!("cannot remove failed-sync upstream tree: {}", e))?;
        }
        restore_optional_file(
            &self.project_root.join(shared::SKILL_LOCK_FILE),
            self.lock_bytes.as_deref(),
        )?;
        restore_optional_file(
            &self.project_root.join(shared::UPSTREAM_PIN_FILE),
            self.pin_bytes.as_deref(),
        )?;
        self.armed = false;
        Ok(())
    }

    /// Restore after a gate failure and fold the outcome into the error text.
    fn restore_after_failure(&mut self, err: String) -> String {
        match self.restore() {
            Ok(()) => format!(
                "{}\n\npre-sync state restored (skills/upstream, .skill-lock.json, \
                 .upstream-pin.json).",
                err
            ),
            Err(restore_err) => {
                // Drop must not delete the only copy of the pre-sync state.
                self.keep = true;
                format!(
                    "{}\n\nALSO FAILED to restore pre-sync state: {}\na snapshot remains at {}",
                    err,
                    restore_err,
                    self.dir.display()
                )
            }
        }
    }
}

impl Drop for Backup {
    fn drop(&mut self) {
        if self.armed && self.restore().is_err() {
            // Keep the snapshot directory for manual recovery.
            self.keep = true;
        }
        if !self.keep {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }
}

/// Read a file as raw bytes, treating "not found" as `None`.
fn read_optional_file(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("cannot read {} for backup: {}", path.display(), e)),
    }
}

/// Rewrite (or delete, when the pre-sync file did not exist) one file.
fn restore_optional_file(path: &Path, bytes: Option<&[u8]>) -> Result<(), String> {
    match bytes {
        Some(bytes) => {
            fs::write(path, bytes).map_err(|e| format!("cannot restore {}: {}", path.display(), e))
        }
        None => match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("cannot remove {}: {}", path.display(), e)),
        },
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::TempDir;

    const NOW: &str = "2026-10-01T09:00:00.000Z";
    const NOW2: &str = "2026-11-01T09:00:00.000Z";
    const OLD_INSTALLED: &str = "2026-01-01T00:00:00.000Z";
    const OLD_UPDATED: &str = "2026-01-02T00:00:00.000Z";
    const TEST_REPO: &str = "https://example.invalid/fixture.git";

    // ── Helpers ─────────────────────────────────────────────────────────

    fn write_skill(root: &Path, rel: &str, content: &str) {
        let dir = root.join(rel);
        fs::create_dir_all(&dir).expect("create skill dir");
        fs::write(dir.join("SKILL.md"), content).expect("write SKILL.md");
    }

    fn git(dir: &Path, args: &[&str]) {
        let status = Command::new("git")
            .args(args)
            .current_dir(dir)
            .status()
            .expect("git failed to start");
        assert!(status.success(), "git {:?} failed in {:?}", args, dir);
    }

    /// A valid SKILL.md body for a fixture skill: the sync now runs the full
    /// validation gate, so fixture frontmatter must pass it.
    fn fixture_skill_md(name: &str, description: &str, heading: &str) -> String {
        format!(
            "---\nname: {}\ndescription: {}\n---\n# {}\n",
            name, description, heading
        )
    }

    /// Stage, commit, and tag (`v1`) the fixture repo.
    fn commit_fixture(repo: &Path) {
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
        git(repo, &["tag", "v1"]);
    }

    /// A fixture upstream repo with skill-a (engineering), skill-b
    /// (productivity), and the allowlisted loop-me (in-progress), tagged v1.
    fn make_fixture_repo(repo: &Path) {
        fs::create_dir_all(repo).expect("create repo dir");
        git(repo, &["init", "--quiet"]);
        fs::write(repo.join("CLAUDE.md"), "# Upstream\n").expect("write CLAUDE.md");
        write_skill(
            repo,
            "skills/engineering/skill-a",
            &fixture_skill_md("skill-a", "Fixture skill A.", "Skill A v2"),
        );
        write_skill(
            repo,
            "skills/productivity/skill-b",
            &fixture_skill_md("skill-b", "Fixture skill B.", "Skill B"),
        );
        write_skill(
            repo,
            "skills/in-progress/loop-me",
            &fixture_skill_md("loop-me", "Fixture in-progress skill.", "Loop Me"),
        );
        commit_fixture(repo);
    }

    /// The same fixture, but skill-a has no frontmatter, so the post-sync
    /// validation gate must fail and the sync must roll back.
    fn make_fixture_repo_with_invalid_skill(repo: &Path) {
        fs::create_dir_all(repo).expect("create repo dir");
        git(repo, &["init", "--quiet"]);
        fs::write(repo.join("CLAUDE.md"), "# Upstream\n").expect("write CLAUDE.md");
        write_skill(
            repo,
            "skills/engineering/skill-a",
            "# Skill A without frontmatter\n",
        );
        write_skill(
            repo,
            "skills/productivity/skill-b",
            &fixture_skill_md("skill-b", "Fixture skill B.", "Skill B"),
        );
        write_skill(
            repo,
            "skills/in-progress/loop-me",
            &fixture_skill_md("loop-me", "Fixture in-progress skill.", "Loop Me"),
        );
        commit_fixture(repo);
    }

    /// A project root that is a real git worktree, so the dirty-tree
    /// preconditions can be exercised.
    fn init_project_git(project: &Path) {
        git(project, &["init", "--quiet"]);
        git(project, &["add", "-A"]);
        git(
            project,
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

    /// Byte snapshot of a directory tree, keyed by relative path (used to
    /// prove a failed sync restored the tree exactly).
    fn snapshot_dir(root: &Path) -> BTreeMap<String, Vec<u8>> {
        fn walk(dir: &Path, base: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
            for entry in fs::read_dir(dir).expect("read snapshot dir") {
                let entry = entry.expect("snapshot entry");
                let path = entry.path();
                let rel = path
                    .strip_prefix(base)
                    .expect("strip snapshot base")
                    .to_string_lossy()
                    .to_string();
                if entry.file_type().expect("snapshot file type").is_dir() {
                    walk(&path, base, out);
                } else {
                    out.insert(rel, fs::read(&path).expect("read snapshot file"));
                }
            }
        }
        let mut out = BTreeMap::new();
        walk(root, root, &mut out);
        out
    }

    /// A mini project root with an old upstream tree and a lock whose
    /// skill-a entry has a stale hash and old timestamps, plus an orphan
    /// and a non-empty `dismissed` object.
    fn make_project_root(project: &Path) {
        write_skill(
            project,
            "skills/upstream/skills/engineering/skill-a",
            "# Skill A v1\n",
        );
        write_skill(
            project,
            "skills/upstream/skills/engineering/orphan-skill",
            "# Orphan\n",
        );
        fs::write(project.join("skills/upstream/STALE.md"), "stale\n").expect("write stale file");
        let lock = serde_json::json!({
            "version": 4,
            "skills": {
                "skill-a": {
                    "source": "mattpocock/skills",
                    "sourceType": "github",
                    "sourceUrl": "https://github.com/mattpocock/skills.git",
                    "skillPath": "skills/engineering/skill-a/SKILL.md",
                    "skillFolderHash": "0000000000000000000000000000000000000000",
                    "pluginName": "mattpocock-skills",
                    "installedAt": OLD_INSTALLED,
                    "updatedAt": OLD_UPDATED
                },
                "orphan-skill": {
                    "sourceType": "github",
                    "skillPath": "skills/engineering/orphan-skill/SKILL.md",
                    "skillFolderHash": "1111111111111111111111111111111111111111"
                }
            },
            "dismissed": {"grumpy": {"reason": "no thanks"}}
        });
        let content = serde_json::to_string_pretty(&lock).unwrap() + "\n";
        fs::write(project.join(".skill-lock.json"), content).expect("write lock");
    }

    fn skill<'a>(lock: &'a SkillLock, name: &str) -> &'a LockedSkill {
        lock.skills
            .iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("skill '{}' missing from lock", name))
    }

    // ── copy_dir_except_git ─────────────────────────────────────────────

    #[test]
    fn copy_preserves_symlinks_and_excludes_git_metadata() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("source");
        let dst = tmp.path().join("snapshot");
        fs::create_dir_all(src.join(".git")).unwrap();
        fs::create_dir_all(src.join("skills/example")).unwrap();
        fs::write(src.join("CLAUDE.md"), "# Instructions\n").unwrap();
        fs::write(src.join("skills/example/SKILL.md"), "# Example\n").unwrap();
        std::os::unix::fs::symlink("CLAUDE.md", src.join("AGENTS.md")).unwrap();

        copy_dir_except_git(&src, &dst).unwrap();

        assert_eq!(
            fs::read_link(dst.join("AGENTS.md")).unwrap(),
            Path::new("CLAUDE.md")
        );
        assert_eq!(
            fs::read(dst.join("AGENTS.md")).unwrap(),
            b"# Instructions\n"
        );
        assert_eq!(
            fs::read(dst.join("skills/example/SKILL.md")).unwrap(),
            b"# Example\n"
        );
        assert!(!dst.join(".git").exists());
    }

    // ── discover_upstream_skills ────────────────────────────────────────

    #[test]
    fn discovers_skills_across_buckets_and_skips_non_skills() {
        let tmp = TempDir::new().unwrap();
        let up = tmp.path();
        write_skill(up, "skills/engineering/alpha", "# Alpha\n");
        write_skill(up, "skills/productivity/beta", "# Beta\n");
        // A directory without SKILL.md is skipped; an empty bucket is fine.
        fs::create_dir_all(up.join("skills/engineering/no-skill-md")).unwrap();
        fs::create_dir_all(up.join("skills/misc")).unwrap();
        // Keep the allowlist silent.
        write_skill(up, "skills/in-progress/loop-me", "# Loop\n");

        let d = discover_upstream_skills(up, TEST_REPO).expect("discover");
        let names: Vec<&str> = d.skills.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["alpha", "beta", "loop-me"]);

        let alpha = &d.skills[0];
        assert_eq!(alpha.skill_path, "skills/engineering/alpha/SKILL.md");
        assert_eq!(
            alpha.skill_folder_hash,
            git_utils::compute_tree_hash(&up.join("skills/engineering/alpha")).unwrap()
        );
        assert_eq!(alpha.source.as_deref(), Some("mattpocock/skills"));
        assert_eq!(alpha.source_type, "github");
        assert_eq!(alpha.source_url.as_deref(), Some(TEST_REPO));
        assert_eq!(alpha.plugin_name.as_deref(), Some("mattpocock-skills"));
        assert_eq!(d.skills[1].skill_path, "skills/productivity/beta/SKILL.md");
    }

    #[test]
    fn discover_errors_when_skills_dir_missing() {
        let tmp = TempDir::new().unwrap();
        let err = discover_upstream_skills(tmp.path(), TEST_REPO).unwrap_err();
        assert!(
            err.contains("skills/"),
            "error should mention skills/: {}",
            err
        );
    }

    // ── allowlist matrix ────────────────────────────────────────────────

    #[test]
    fn allowlist_graduated_with_leftover_copy_keeps_stable_entry() {
        let tmp = TempDir::new().unwrap();
        let up = tmp.path();
        write_skill(up, "skills/engineering/loop-me", "# stable\n");
        write_skill(up, "skills/in-progress/loop-me", "# beta\n");

        let d = discover_upstream_skills(up, TEST_REPO).expect("discover");
        assert_eq!(d.skills.len(), 1);
        assert_eq!(
            d.skills[0].skill_path,
            "skills/engineering/loop-me/SKILL.md"
        );
        assert_eq!(
            d.skills[0].skill_folder_hash,
            git_utils::compute_tree_hash(&up.join("skills/engineering/loop-me")).unwrap(),
            "stable copy wins over the in-progress copy"
        );
        assert!(
            d.allowlist_notes
                .iter()
                .any(|n| n.contains("loop-me") && n.contains("stable entry")),
            "expected a keep-stable note, got: {:?}",
            d.allowlist_notes
        );
    }

    #[test]
    fn allowlist_graduated_without_in_progress_copy_keeps_stable_entry() {
        let tmp = TempDir::new().unwrap();
        let up = tmp.path();
        write_skill(up, "skills/productivity/loop-me", "# graduated\n");

        let d = discover_upstream_skills(up, TEST_REPO).expect("discover");
        assert_eq!(d.skills.len(), 1);
        assert_eq!(
            d.skills[0].skill_path,
            "skills/productivity/loop-me/SKILL.md"
        );
        assert!(
            d.allowlist_notes
                .iter()
                .any(|n| n.contains("loop-me") && n.contains("graduated")),
            "expected a graduated note, got: {:?}",
            d.allowlist_notes
        );
    }

    #[test]
    fn allowlist_missing_at_ref_is_hard_error() {
        let tmp = TempDir::new().unwrap();
        let up = tmp.path();
        write_skill(up, "skills/engineering/alpha", "# Alpha\n");

        let err = discover_upstream_skills(up, TEST_REPO).unwrap_err();
        assert!(
            err.contains("loop-me"),
            "error should name the skill: {}",
            err
        );
        assert!(
            err.contains("missing"),
            "error should say the allowlisted skill is missing: {}",
            err
        );
    }

    #[test]
    fn allowlist_in_progress_skill_ships_with_in_progress_path() {
        let tmp = TempDir::new().unwrap();
        let up = tmp.path();
        write_skill(up, "skills/in-progress/loop-me", "# Loop Me\n");

        let d = discover_upstream_skills(up, TEST_REPO).expect("discover");
        assert_eq!(d.skills.len(), 1);
        assert_eq!(d.skills[0].name, "loop-me");
        assert_eq!(
            d.skills[0].skill_path,
            "skills/in-progress/loop-me/SKILL.md"
        );
        assert!(
            d.allowlist_notes
                .iter()
                .any(|n| n.contains("shipping") && n.contains("loop-me")),
            "expected a shipping note, got: {:?}",
            d.allowlist_notes
        );
    }

    // ── sync_upstream full chain ────────────────────────────────────────

    #[test]
    fn sync_replaces_tree_and_merges_lock() {
        let repo = TempDir::new().unwrap();
        make_fixture_repo(repo.path());
        let project = TempDir::new().unwrap();
        make_project_root(project.path());

        let outcome = sync_upstream(project.path(), "v1", repo.path().to_str().unwrap(), NOW)
            .expect("sync should succeed");

        // Tree replaced: new content in, stale content out.
        let upstream = project.path().join("skills/upstream");
        assert_eq!(
            fs::read_to_string(upstream.join("skills/engineering/skill-a/SKILL.md")).unwrap(),
            fixture_skill_md("skill-a", "Fixture skill A.", "Skill A v2")
        );
        assert_eq!(
            fs::read_to_string(upstream.join("CLAUDE.md")).unwrap(),
            "# Upstream\n"
        );
        assert!(
            !upstream.join("STALE.md").exists(),
            "stale file must be gone"
        );
        assert!(
            !upstream.join("skills/engineering/orphan-skill").exists(),
            "orphan dir must be gone"
        );
        assert!(!upstream.join(".git").exists(), ".git must not be copied");

        // Lock merged per the timestamp rules.
        let lock = shared::load_skill_lock_at(project.path()).expect("load written lock");
        let names: Vec<&str> = lock.skills.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["loop-me", "skill-a", "skill-b"]);

        let a = skill(&lock, "skill-a");
        assert_eq!(
            a.skill_folder_hash,
            git_utils::compute_tree_hash(&repo.path().join("skills/engineering/skill-a")).unwrap()
        );
        assert_eq!(
            a.installed_at.as_deref(),
            Some(OLD_INSTALLED),
            "installedAt preserved"
        );
        assert_eq!(
            a.updated_at.as_deref(),
            Some(NOW),
            "changed hash re-stamps updatedAt"
        );

        for name in ["skill-b", "loop-me"] {
            let s = skill(&lock, name);
            assert_eq!(s.installed_at.as_deref(), Some(NOW));
            assert_eq!(s.updated_at.as_deref(), Some(NOW));
        }
        assert_eq!(
            skill(&lock, "loop-me").skill_path,
            "skills/in-progress/loop-me/SKILL.md"
        );
        assert_eq!(
            lock.dismissed,
            Some(serde_json::json!({"grumpy": {"reason": "no thanks"}})),
            "dismissed preserved verbatim"
        );

        // Outcome classification.
        assert_eq!(outcome.added, vec!["loop-me", "skill-b"]);
        assert_eq!(outcome.updated, vec!["skill-a"]);
        assert_eq!(outcome.removed, vec!["orphan-skill"]);
        assert!(outcome.unchanged.is_empty());
        assert!(!outcome.resolved_commit.is_empty());
        assert!(
            outcome
                .allowlist_notes
                .iter()
                .any(|n| n.contains("loop-me")),
            "outcome should carry the allowlist note"
        );
        assert!(
            outcome.check_lines.iter().all(|l| l.starts_with("PASS:")),
            "post-sync check must be all PASS, got: {:?}",
            outcome.check_lines
        );
        assert_eq!(outcome.check_lines.len(), 3);
    }

    #[test]
    fn sync_same_ref_twice_is_byte_identical() {
        let repo = TempDir::new().unwrap();
        make_fixture_repo(repo.path());
        let project = TempDir::new().unwrap();
        make_project_root(project.path());

        sync_upstream(project.path(), "v1", repo.path().to_str().unwrap(), NOW)
            .expect("first sync");
        let bytes1 = fs::read(project.path().join(".skill-lock.json")).unwrap();

        let outcome2 = sync_upstream(project.path(), "v1", repo.path().to_str().unwrap(), NOW2)
            .expect("second sync");
        let bytes2 = fs::read(project.path().join(".skill-lock.json")).unwrap();

        assert_eq!(
            bytes1, bytes2,
            "re-sync at the same ref must not change the lock bytes"
        );
        assert!(outcome2.added.is_empty());
        assert!(
            outcome2.updated.is_empty(),
            "nothing changed → no updatedAt re-stamp"
        );
        assert!(outcome2.removed.is_empty());
        assert_eq!(outcome2.unchanged, vec!["loop-me", "skill-a", "skill-b"]);
    }

    #[test]
    fn sync_with_malformed_existing_lock_is_hard_error() {
        let repo = TempDir::new().unwrap();
        make_fixture_repo(repo.path());
        let project = TempDir::new().unwrap();
        make_project_root(project.path());
        fs::write(project.path().join(".skill-lock.json"), "{ not json").unwrap();

        let err =
            sync_upstream(project.path(), "v1", repo.path().to_str().unwrap(), NOW).unwrap_err();
        assert!(
            err.contains(".skill-lock.json") || err.contains("invalid"),
            "error should identify the malformed lock: {}",
            err
        );
        // Failure happens before the tree is touched.
        assert!(project.path().join("skills/upstream/STALE.md").exists());
    }

    #[test]
    fn sync_without_existing_lock_creates_fresh_one() {
        let repo = TempDir::new().unwrap();
        make_fixture_repo(repo.path());
        let project = TempDir::new().unwrap();
        fs::create_dir_all(project.path().join("skills/upstream")).unwrap();

        let outcome = sync_upstream(project.path(), "v1", repo.path().to_str().unwrap(), NOW)
            .expect("sync should succeed");

        let lock = shared::load_skill_lock_at(project.path()).expect("load written lock");
        assert_eq!(lock.version, 4);
        assert_eq!(lock.dismissed, Some(serde_json::json!({})));
        assert_eq!(lock.skills.len(), 3);
        assert_eq!(outcome.added, vec!["loop-me", "skill-a", "skill-b"]);
        assert!(outcome.removed.is_empty());
        assert!(
            outcome.check_lines.iter().all(|l| l.starts_with("PASS:")),
            "post-sync check must be all PASS, got: {:?}",
            outcome.check_lines
        );
    }

    #[test]
    fn sync_fails_when_allowlisted_skill_missing_at_ref() {
        let repo = TempDir::new().unwrap();
        fs::create_dir_all(repo.path()).unwrap();
        git(repo.path(), &["init", "--quiet"]);
        write_skill(repo.path(), "skills/engineering/skill-a", "# A\n");
        git(repo.path(), &["add", "-A"]);
        git(
            repo.path(),
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
        git(repo.path(), &["tag", "v1"]);

        let project = TempDir::new().unwrap();
        make_project_root(project.path());

        let err =
            sync_upstream(project.path(), "v1", repo.path().to_str().unwrap(), NOW).unwrap_err();
        assert!(
            err.contains("loop-me"),
            "error should name the skill: {}",
            err
        );
        // The failure precedes any tree/lock mutation.
        assert!(project.path().join("skills/upstream/STALE.md").exists());
        let lock = shared::load_skill_lock_at(project.path()).unwrap();
        assert!(lock.skills.iter().any(|s| s.name == "orphan-skill"));
    }

    #[test]
    fn sync_fails_on_unknown_ref() {
        let repo = TempDir::new().unwrap();
        make_fixture_repo(repo.path());
        let project = TempDir::new().unwrap();
        make_project_root(project.path());

        let err = sync_upstream(
            project.path(),
            "no-such-ref",
            repo.path().to_str().unwrap(),
            NOW,
        )
        .unwrap_err();
        assert!(
            err.contains("fetch"),
            "error should name the failed step: {}",
            err
        );
        assert!(project.path().join("skills/upstream/STALE.md").exists());
    }

    // ── Failure atomicity ───────────────────────────────────────────────

    #[test]
    fn sync_rolls_back_when_validation_gate_fails() {
        let repo = TempDir::new().unwrap();
        make_fixture_repo_with_invalid_skill(repo.path());
        let project = TempDir::new().unwrap();
        make_project_root(project.path());

        let upstream = project.path().join("skills/upstream");
        let before_tree = snapshot_dir(&upstream);
        let before_lock = fs::read(project.path().join(".skill-lock.json")).unwrap();

        let err =
            sync_upstream(project.path(), "v1", repo.path().to_str().unwrap(), NOW).unwrap_err();

        assert!(
            err.contains("post-sync validation failed"),
            "error should name the failed gate: {}",
            err
        );
        assert!(
            err.contains("pre-sync state restored"),
            "error should report the rollback: {}",
            err
        );
        assert_eq!(
            snapshot_dir(&upstream),
            before_tree,
            "failed sync must restore the upstream tree byte-identically"
        );
        assert_eq!(
            fs::read(project.path().join(".skill-lock.json")).unwrap(),
            before_lock,
            "failed sync must restore the lock byte-identically"
        );
        assert!(
            !project.path().join(".upstream-pin.json").exists(),
            "a rolled-back sync must not leave a pin file behind"
        );
    }

    #[test]
    fn sync_refuses_dirty_tracked_files() {
        let repo = TempDir::new().unwrap();
        make_fixture_repo(repo.path());
        let project = TempDir::new().unwrap();
        make_project_root(project.path());
        init_project_git(project.path());
        fs::write(
            project.path().join("skills/upstream/STALE.md"),
            "locally edited\n",
        )
        .unwrap();

        let upstream = project.path().join("skills/upstream");
        let before_tree = snapshot_dir(&upstream);
        let err =
            sync_upstream(project.path(), "v1", repo.path().to_str().unwrap(), NOW).unwrap_err();

        assert!(err.contains("refusing to sync"), "{}", err);
        assert!(err.contains("STALE.md"), "{}", err);
        assert_eq!(
            snapshot_dir(&upstream),
            before_tree,
            "a refused sync must not touch the tree"
        );
    }

    #[test]
    fn sync_force_proceeds_and_reports_overwrites() {
        let repo = TempDir::new().unwrap();
        make_fixture_repo(repo.path());
        let project = TempDir::new().unwrap();
        make_project_root(project.path());
        init_project_git(project.path());
        fs::write(
            project.path().join("skills/upstream/STALE.md"),
            "locally edited\n",
        )
        .unwrap();
        fs::write(
            project.path().join("skills/upstream/UNTRACKED.md"),
            "scratch\n",
        )
        .unwrap();

        let outcome = sync_upstream_with_options(
            project.path(),
            "v1",
            repo.path().to_str().unwrap(),
            NOW,
            &SyncOptions { force: true },
        )
        .expect("forced sync should succeed");

        assert!(
            outcome
                .forced_overwrites
                .iter()
                .any(|p| p == "skills/upstream/STALE.md"),
            "forced overwrites: {:?}",
            outcome.forced_overwrites
        );
        assert!(
            outcome
                .forced_overwrites
                .iter()
                .any(|p| p == "skills/upstream/UNTRACKED.md"),
            "forced overwrites: {:?}",
            outcome.forced_overwrites
        );
        assert!(!project.path().join("skills/upstream/STALE.md").exists());
        assert!(!project.path().join("skills/upstream/UNTRACKED.md").exists());
        assert!(outcome.check_lines.iter().all(|l| l.starts_with("PASS:")));
    }

    #[test]
    fn sync_refuses_untracked_files_under_upstream() {
        let repo = TempDir::new().unwrap();
        make_fixture_repo(repo.path());
        let project = TempDir::new().unwrap();
        make_project_root(project.path());
        init_project_git(project.path());
        fs::write(
            project.path().join("skills/upstream/UNTRACKED.md"),
            "scratch\n",
        )
        .unwrap();

        let err =
            sync_upstream(project.path(), "v1", repo.path().to_str().unwrap(), NOW).unwrap_err();

        assert!(err.contains("refusing to sync"), "{}", err);
        assert!(err.contains("UNTRACKED.md"), "{}", err);
        assert!(
            project.path().join("skills/upstream/UNTRACKED.md").exists(),
            "a refused sync must not delete untracked files"
        );
    }
}
