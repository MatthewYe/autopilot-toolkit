//! Skill integrity check — verifies that the git tree hashes in
//! `.skill-lock.json` and `.vendor-lock.json` match the actual skill
//! directories on disk.
//!
//! Migrated from `scripts/check.rs`.  Uses `shared::load_skill_lock_at()` and
//! `shared::load_vendor_lock_at()` for parsing and `shared::SkillLock` /
//! `shared::LockedSkill` as the source-of-truth types.
//!
//! Public API:
//! - `check_skills(project_root)` → `Result<CheckReport, String>`
//! - `check_upstream_pin(project_root)` → `Result<Option<(String, CheckResult)>, String>`

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

// ── Types ──────────────────────────────────────────────────────────────────

/// Outcome of checking a single skill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckResult {
    Pass,
    Fail(String),
    Fix(String),
    Skip(String),
}

/// Aggregate result of a skill-check run.
#[derive(Debug)]
pub struct CheckReport {
    /// Per-skill results in the order they were checked.
    pub results: Vec<(String, CheckResult)>,
    /// Skills whose hash was updated (TODO-recalculated → computed hash).
    pub updated: BTreeMap<String, String>,
    /// Vendor skills whose hash was updated (TODO-recalculated → computed hash).
    pub updated_vendor: BTreeMap<String, String>,
    /// Whether any github-source skill was found.
    pub found_github: bool,
}

/// Compare one skill directory's git tree hash against its lock entry.
///
/// Emits FIX + PASS when the lock holds `TODO-recalculated`, PASS on match,
/// and FAIL on mismatch or hash error.
fn check_tree_hash(
    name: &str,
    folder: &Path,
    expected_hash: &str,
    updated: &mut BTreeMap<String, String>,
    results: &mut Vec<(String, CheckResult)>,
) {
    match git_utils::compute_tree_hash(folder) {
        Err(err) => results.push((name.to_string(), CheckResult::Fail(err))),
        Ok(tree_hash) => {
            if expected_hash == "TODO-recalculated" {
                updated.insert(name.to_string(), tree_hash.clone());
                results.push((name.to_string(), CheckResult::Fix(tree_hash.clone())));
                // FIX is emitted so the update is visible, then PASS so the
                // skill is reported green after the lock is repaired.
                results.push((name.to_string(), CheckResult::Pass));
            } else if tree_hash == expected_hash {
                results.push((name.to_string(), CheckResult::Pass));
            } else {
                results.push((
                    name.to_string(),
                    CheckResult::Fail(format!(
                        "computed: {}, lockfile: {}",
                        tree_hash, expected_hash
                    )),
                ));
            }
        }
    }
}

// ── Core logic ─────────────────────────────────────────────────────────────

/// Check all skills against their lockfile hashes.
///
/// Uses `shared::load_skill_lock_at(project_root)` for `.skill-lock.json` and
/// `shared::load_vendor_lock_at(project_root)` for `.vendor-lock.json`, then
/// computes git tree hashes for each skill directory and compares them
/// against the expected values.
pub fn check_skills(project_root: &Path) -> Result<CheckReport, String> {
    let lock = shared::load_skill_lock_at(project_root)?;

    let mut results: Vec<(String, CheckResult)> = Vec::new();
    let mut updated: BTreeMap<String, String> = BTreeMap::new();
    let mut updated_vendor: BTreeMap<String, String> = BTreeMap::new();
    let mut found_github = false;

    // First pass: github-source skills
    for skill in &lock.skills {
        if skill.source_type != "github" {
            continue;
        }
        found_github = true;

        let folder_rel = match skill.upstream_dir_rel() {
            Some(f) => f,
            None => {
                results.push((
                    skill.name.clone(),
                    CheckResult::Skip(format!("unexpected skillPath format: {}", skill.skill_path)),
                ));
                continue;
            }
        };

        let folder = project_root.join(&folder_rel);
        check_tree_hash(
            &skill.name,
            &folder,
            &skill.skill_folder_hash,
            &mut updated,
            &mut results,
        );
    }

    // Second pass: non-github skills with TODO-recalculated
    for skill in &lock.skills {
        if skill.source_type == "github" {
            continue;
        }
        if skill.skill_folder_hash != "TODO-recalculated" {
            continue;
        }

        let folder_rel = match skill.skill_dir_rel() {
            Some(f) => f,
            None => continue,
        };

        let folder = project_root.join(&folder_rel);

        match git_utils::compute_tree_hash(&folder) {
            Err(err) => {
                results.push((
                    skill.name.clone(),
                    CheckResult::Skip(format!("cannot compute hash: {}", err)),
                ));
            }
            Ok(tree_hash) => {
                updated.insert(skill.name.clone(), tree_hash.clone());
                results.push((skill.name.clone(), CheckResult::Fix(tree_hash)));
            }
        }
    }

    // Third pass: vendor skills from .vendor-lock.json
    let vendor_lock_path = project_root.join(shared::VENDOR_LOCK_FILE);
    if vendor_lock_path.is_file() {
        let vendor_lock = shared::load_vendor_lock_at(project_root)?;
        for skill in &vendor_lock.skills {
            // Every vendor lock entry is integrity-checked; sourceType is
            // provenance metadata, not a reason to skip.
            found_github = true;

            let folder = project_root.join(skill.vendor_dir());
            if !folder.join("PROVENANCE.md").is_file() {
                results.push((
                    skill.name.clone(),
                    CheckResult::Fail(
                        "missing PROVENANCE.md (vendor provenance and license required)"
                            .to_string(),
                    ),
                ));
                continue;
            }
            check_tree_hash(
                &skill.name,
                &folder,
                &skill.skill_folder_hash,
                &mut updated_vendor,
                &mut results,
            );
        }
    }

    Ok(CheckReport {
        results,
        updated,
        updated_vendor,
        found_github,
    })
}

/// Write an updated `.skill-lock.json` with new skill folder hashes.
///
/// Loads the typed lock, applies each `skillFolderHash` update via
/// `SkillLock::set_folder_hash` (which leaves `updatedAt` untouched), and
/// writes the byte-stable serialization back. All other fields and the
/// file's formatting are preserved.
pub fn write_updated_lockfile(
    project_root: &Path,
    updated: &BTreeMap<String, String>,
) -> Result<(), String> {
    let mut lock = shared::load_skill_lock_at(project_root)?;
    for (name, hash) in updated {
        lock.set_folder_hash(name, hash)?;
    }
    write_lock_bytes(project_root, shared::SKILL_LOCK_FILE, &lock)
}

/// Write an updated `.vendor-lock.json` with new skill folder hashes.
///
/// Loads the typed lock, applies each `skillFolderHash` update via
/// `SkillLock::set_folder_hash` (which leaves `updatedAt` untouched), and
/// writes the byte-stable serialization back. All other fields and the
/// file's formatting are preserved.
pub fn write_updated_vendor_lockfile(
    project_root: &Path,
    updated: &BTreeMap<String, String>,
) -> Result<(), String> {
    let mut lock = shared::load_vendor_lock_at(project_root)?;
    for (name, hash) in updated {
        lock.set_folder_hash(name, hash)?;
    }
    write_lock_bytes(project_root, shared::VENDOR_LOCK_FILE, &lock)
}

fn write_lock_bytes(
    project_root: &Path,
    file_name: &str,
    lock: &shared::SkillLock,
) -> Result<(), String> {
    let lock_path = project_root.join(file_name);
    fs::write(&lock_path, lock.to_bytes())
        .map_err(|e| format!("cannot write {:?}: {}", lock_path, e))
}

/// Determine if any result is a Fail.
pub fn any_fail(results: &[(String, CheckResult)]) -> bool {
    results
        .iter()
        .any(|(_, r)| matches!(r, CheckResult::Fail(_)))
}

/// Determine if any result is a hard failure for a gate: a `Fail`, or a
/// `Skip` (a check that could not run is not evidence that the tree is
/// good, so it must not pass a gate silently).
pub fn any_not_pass(results: &[(String, CheckResult)]) -> bool {
    results
        .iter()
        .any(|(_, r)| matches!(r, CheckResult::Fail(_) | CheckResult::Skip(_)))
}

/// Format a single check result line, matching the bash script output exactly.
pub fn format_result(name: &str, result: &CheckResult) -> String {
    match result {
        CheckResult::Pass => format!("PASS: {}", name),
        CheckResult::Fail(reason) => {
            if reason.starts_with("computed:") {
                format!("FAIL: {} ({})", name, reason)
            } else {
                format!("FAIL: {} — {}", name, reason)
            }
        }
        CheckResult::Fix(hash) => format!("FIX: {} → {}", name, hash),
        CheckResult::Skip(msg) => {
            if msg.starts_with("cannot compute hash:") {
                format!("WARN: {} — {}", name, msg)
            } else {
                format!("SKIP: {} — {}", name, msg)
            }
        }
    }
}

/// Determine the exit code from results.
///
/// Returns 0 when every check passed, or when there was no github/vendor
/// skill to check at all (nothing to verify is not a failure). Returns 1 for
/// any `Fail` **or** `Skip`: a check that could not run (e.g. an unexpected
/// `skillPath` format) is fail-closed so a broken tree cannot pass the gate
/// by being skipped.
pub fn determine_exit_code(results: &[(String, CheckResult)], found_github: bool) -> i32 {
    if !found_github {
        return 0;
    }
    if any_not_pass(results) {
        1
    } else {
        0
    }
}

// ── Upstream pin ───────────────────────────────────────────────────────────

/// Check name used for the `.upstream-pin.json` result line.
pub const UPSTREAM_PIN_CHECK_NAME: &str = "upstream-pin";

/// Verify `.upstream-pin.json` when the project tracks an upstream snapshot.
///
/// Returns `Ok(None)` when the project has neither a pin file nor a vendored
/// `skills/upstream/package.json`, so synthetic projects without an upstream
/// tree keep their old behavior. Once either exists, verification is
/// enforced: a missing pin is a failure, not a skip.
pub fn check_upstream_pin(project_root: &Path) -> Result<Option<(String, CheckResult)>, String> {
    let pin_path = project_root.join(shared::UPSTREAM_PIN_FILE);
    let upstream_package_json = project_root
        .join("skills")
        .join("upstream")
        .join("package.json");
    if !pin_path.is_file() && !upstream_package_json.is_file() {
        return Ok(None);
    }
    Ok(Some((
        UPSTREAM_PIN_CHECK_NAME.to_string(),
        evaluate_upstream_pin(project_root),
    )))
}

fn evaluate_upstream_pin(project_root: &Path) -> CheckResult {
    match evaluate_upstream_pin_inner(project_root) {
        Ok(()) => CheckResult::Pass,
        Err(message) => CheckResult::Fail(message),
    }
}

fn evaluate_upstream_pin_inner(project_root: &Path) -> Result<(), String> {
    let pin_path = project_root.join(shared::UPSTREAM_PIN_FILE);
    let bytes = fs::read(&pin_path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            format!(
                "missing {} (run scripts/sync-upstream.rs to write it; never hand-edit)",
                shared::UPSTREAM_PIN_FILE
            )
        } else {
            format!("cannot read {}: {}", pin_path.display(), e)
        }
    })?;
    let pin = shared::UpstreamPin::parse(&bytes)?;

    if pin.schema_version != shared::UPSTREAM_PIN_SCHEMA_VERSION {
        return Err(format!(
            "unsupported schemaVersion {} (expected {})",
            pin.schema_version,
            shared::UPSTREAM_PIN_SCHEMA_VERSION
        ));
    }
    if !is_lowercase_hex_40(&pin.resolved_commit) {
        return Err(format!(
            "resolvedCommit must be 40 lowercase hex characters, got {:?}",
            pin.resolved_commit
        ));
    }
    if !is_utc_iso8601(&pin.synced_at) {
        return Err(format!(
            "syncedAt must be an ISO-8601 UTC timestamp ending in Z, got {:?}",
            pin.synced_at
        ));
    }
    if pin.ref_name.trim().is_empty() {
        return Err("`ref` must not be empty".to_string());
    }
    if pin.source.trim().is_empty() {
        return Err("`source` must not be empty".to_string());
    }
    if pin.source_url.trim().is_empty() {
        return Err("`sourceUrl` must not be empty".to_string());
    }
    if pin.ref_kind != shared::UpstreamRefKind::Tag
        && pin
            .reason
            .as_deref()
            .map(str::trim)
            .unwrap_or("")
            .is_empty()
    {
        return Err(format!(
            "refKind {:?} requires a non-empty reason so the exception is auditable",
            pin.ref_kind
        ));
    }

    // README documents the pin for humans; the machine facts must agree with
    // it, so a sync that forgets the prose cannot land unnoticed.
    let readme_path = project_root.join("README.md");
    let readme = fs::read_to_string(&readme_path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            "cannot verify the pin: README.md is missing".to_string()
        } else {
            format!("cannot read {}: {}", readme_path.display(), e)
        }
    })?;
    if !readme.contains(&pin.ref_name) {
        return Err(format!(
            "README.md does not mention the pinned ref {:?} (update the prose after a sync)",
            pin.ref_name
        ));
    }
    if !readme.contains(&pin.resolved_commit) {
        return Err(format!(
            "README.md does not mention the pinned commit {}",
            pin.resolved_commit
        ));
    }

    // A tag pin mirrors a released upstream package.json; a mismatch means
    // the vendored tree and the pin come from different commits.
    if pin.ref_kind == shared::UpstreamRefKind::Tag {
        let package_json = project_root
            .join("skills")
            .join("upstream")
            .join("package.json");
        let bytes = fs::read(&package_json).map_err(|e| {
            format!(
                "cannot compare the tag pin with {}: {}",
                package_json.display(),
                e
            )
        })?;
        let json: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|e| format!("cannot parse {}: {}", package_json.display(), e))?;
        let version = json
            .get("version")
            .and_then(|value| value.as_str())
            .ok_or_else(|| format!("{} has no string `version` field", package_json.display()))?;
        if version != pin.version {
            return Err(format!(
                "pin version {:?} does not match skills/upstream/package.json version {:?}",
                pin.version, version
            ));
        }
    }

    Ok(())
}

/// True for exactly 40 lowercase hex characters (a full commit SHA).
fn is_lowercase_hex_40(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// True for an RFC 3339 timestamp that is UTC (`Z` suffix).
fn is_utc_iso8601(value: &str) -> bool {
    value.ends_with('Z') && chrono::DateTime::parse_from_rfc3339(value).is_ok()
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // ── Helpers ─────────────────────────────────────────────────────────

    fn project_root() -> PathBuf {
        shared::project_root()
    }

    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let n = TEMP_COUNTER.fetch_add(1, Ordering::SeqCst);
            let dir =
                std::env::temp_dir().join(format!("check-rs-test-{}-{}", std::process::id(), n));
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

    fn make_temp_dir() -> TempDir {
        TempDir::new()
    }

    fn write_lockfile(dir: &Path, skills: &serde_json::Value) {
        let lock = serde_json::json!({
            "version": 4,
            "skills": skills
        });
        let content = serde_json::to_string_pretty(&lock).unwrap() + "\n";
        fs::write(dir.join(".skill-lock.json"), content).expect("write lockfile");
    }

    fn write_vendor_lockfile(dir: &Path, skills: &serde_json::Value) {
        let lock = serde_json::json!({
            "version": 1,
            "skills": skills
        });
        let content = serde_json::to_string_pretty(&lock).unwrap() + "\n";
        fs::write(dir.join(".vendor-lock.json"), content).expect("write vendor lockfile");
    }

    fn create_skill_dir(base: &Path, rel_path: &str, content: &str) {
        let dir = base.join(rel_path);
        fs::create_dir_all(&dir).expect("create skill dir");
        fs::write(dir.join("SKILL.md"), content).expect("write SKILL.md");
    }

    // ── check_skills (using real lockfile) ───────────────────────────────

    #[test]
    fn check_skills_with_real_lockfile() {
        // This test uses the real project's .skill-lock.json.
        // It verifies that all github skills resolve to existing folders
        // and produce valid hashes.
        let root = project_root();
        let lock_path = root.join(".skill-lock.json");
        assert!(
            lock_path.exists(),
            "REQUIRES .skill-lock.json at {:?} — run from project root",
            root
        );

        let report = check_skills(&root).expect("check_skills should succeed");

        assert!(report.found_github, "should find github skills");
        assert!(!report.results.is_empty(), "should have results");

        // All results should be Pass (since lock should be in sync).
        let mut unexpected: Vec<String> = Vec::new();
        for (name, result) in &report.results {
            match result {
                CheckResult::Pass => {} // expected
                CheckResult::Fail(reason) => {
                    if reason.contains("folder not found") {
                        eprintln!("WARN: folder missing for {} — test env issue?", name);
                    } else {
                        unexpected.push(format!("FAIL {}: {}", name, reason));
                    }
                }
                CheckResult::Fix(hash) => {
                    unexpected.push(format!("FIX {} → {} (stale TODO-recalculated)", name, hash));
                }
                CheckResult::Skip(msg) => {
                    unexpected.push(format!("SKIP {}: {}", name, msg));
                }
            }
        }
        assert!(
            unexpected.is_empty(),
            "Unexpected non-PASS results:\n{}",
            unexpected.join("\n")
        );
    }

    // ── check_skills with synthetic lockfiles ────────────────────────────

    #[test]
    fn check_skills_all_pass_when_hashes_match() {
        let tmp = make_temp_dir();
        let root = tmp.path();

        create_skill_dir(
            root,
            "skills/upstream/skills/engineering/tdd",
            "# TDD Skill\n",
        );

        // Use TODO-recalculated so first check auto-fixes,
        // then read back the fixed hash
        write_lockfile(
            root,
            &serde_json::json!({
                "tdd": {
                    "sourceType": "github",
                    "skillPath": "skills/engineering/tdd/SKILL.md",
                    "skillFolderHash": "TODO-recalculated"
                }
            }),
        );

        // check_skills reads from the passed-in root directly
        let report1 = check_skills(root).expect("check_skills 1");

        // Should have FIX + PASS
        let has_fix = report1
            .results
            .iter()
            .any(|(_, r)| matches!(r, CheckResult::Fix(_)));
        let has_pass = report1
            .results
            .iter()
            .any(|(_, r)| matches!(r, CheckResult::Pass));
        assert!(has_fix, "should have FIX entry");
        assert!(has_pass, "should have PASS entry");

        // Apply the update to the lockfile
        let updated_hash = &report1.updated["tdd"];
        write_lockfile(
            root,
            &serde_json::json!({
                "tdd": {
                    "sourceType": "github",
                    "skillPath": "skills/engineering/tdd/SKILL.md",
                    "skillFolderHash": updated_hash
                }
            }),
        );

        // Second run should be all PASS
        let report2 = check_skills(root).expect("check_skills 2");

        for (name, result) in &report2.results {
            assert!(
                matches!(result, CheckResult::Pass),
                "expected PASS for {}, got {:?}",
                name,
                result
            );
        }
    }

    #[test]
    fn check_skills_fail_when_hash_mismatches() {
        let tmp = make_temp_dir();
        let root = tmp.path();

        create_skill_dir(
            root,
            "skills/upstream/skills/engineering/tdd",
            "# TDD Skill\n",
        );

        write_lockfile(
            root,
            &serde_json::json!({
                "tdd": {
                    "sourceType": "github",
                    "skillPath": "skills/engineering/tdd/SKILL.md",
                    "skillFolderHash": "0000000000000000000000000000000000000000"
                }
            }),
        );

        let report = check_skills(root).expect("check_skills");

        let has_fail = report
            .results
            .iter()
            .any(|(_, r)| matches!(r, CheckResult::Fail(_)));
        assert!(has_fail, "should have FAIL entry for hash mismatch");
    }

    #[test]
    fn check_skills_no_github_skills() {
        let tmp = make_temp_dir();
        let root = tmp.path();

        write_lockfile(
            root,
            &serde_json::json!({
                "my-local": {
                    "sourceType": "local",
                    "skillPath": "skills/autopilot/my-local/SKILL.md",
                    "skillFolderHash": "somehash"
                }
            }),
        );

        let report = check_skills(root).expect("check_skills");

        assert!(!report.found_github, "should not find github skills");
        assert_eq!(determine_exit_code(&report.results, report.found_github), 0);
    }

    #[test]
    fn check_skills_skip_unexpected_skill_path() {
        let tmp = make_temp_dir();
        let root = tmp.path();

        write_lockfile(
            root,
            &serde_json::json!({
                "bad-skill": {
                    "sourceType": "github",
                    "skillPath": "not-a-valid-skill-path",
                    "skillFolderHash": "abc123"
                }
            }),
        );

        let report = check_skills(root).expect("check_skills");

        let has_skip = report
            .results
            .iter()
            .any(|(_, r)| matches!(r, CheckResult::Skip(_)));
        assert!(has_skip, "should have SKIP entry for bad path");
    }

    // ── check_skills with vendor lockfiles ───────────────────────────────

    #[test]
    fn check_vendor_skills_all_pass_when_hashes_match() {
        let tmp = make_temp_dir();
        let root = tmp.path();

        create_skill_dir(root, "skills/vendor/show-me", "# Show Me\n");
        fs::write(
            root.join("skills/vendor/show-me/PROVENANCE.md"),
            "# Provenance\n",
        )
        .unwrap();
        write_lockfile(root, &serde_json::json!({}));

        write_vendor_lockfile(
            root,
            &serde_json::json!({
                "show-me": {
                    "sourceType": "github",
                    "skillPath": "plugins/show-me/skills/show-me/SKILL.md",
                    "skillFolderHash": "TODO-recalculated",
                    "vendorPath": "skills/vendor/show-me"
                }
            }),
        );

        let report1 = check_skills(root).expect("check_skills 1");
        assert!(
            report1.found_github,
            "vendor github entry should set found_github"
        );
        assert!(
            report1
                .results
                .iter()
                .any(|(_, r)| matches!(r, CheckResult::Fix(_))),
            "should have FIX entry"
        );

        let updated_hash = report1.updated_vendor["show-me"].clone();
        write_vendor_lockfile(
            root,
            &serde_json::json!({
                "show-me": {
                    "sourceType": "github",
                    "skillPath": "plugins/show-me/skills/show-me/SKILL.md",
                    "skillFolderHash": updated_hash,
                    "vendorPath": "skills/vendor/show-me"
                }
            }),
        );

        let report2 = check_skills(root).expect("check_skills 2");
        for (name, result) in &report2.results {
            assert!(
                matches!(result, CheckResult::Pass),
                "expected PASS for {}, got {:?}",
                name,
                result
            );
        }
    }

    #[test]
    fn check_vendor_skills_fail_when_hash_mismatches() {
        let tmp = make_temp_dir();
        let root = tmp.path();

        create_skill_dir(root, "skills/vendor/show-me", "# Show Me\n");
        fs::write(
            root.join("skills/vendor/show-me/PROVENANCE.md"),
            "# Provenance\n",
        )
        .unwrap();
        write_lockfile(root, &serde_json::json!({}));
        write_vendor_lockfile(
            root,
            &serde_json::json!({
                "show-me": {
                    "sourceType": "github",
                    "skillPath": "plugins/show-me/skills/show-me/SKILL.md",
                    "skillFolderHash": "0000000000000000000000000000000000000000",
                    "vendorPath": "skills/vendor/show-me"
                }
            }),
        );

        let report = check_skills(root).expect("check_skills");

        let has_fail = report
            .results
            .iter()
            .any(|(_, r)| matches!(r, CheckResult::Fail(_)));
        assert!(has_fail, "should have FAIL entry for vendor hash mismatch");
    }

    #[test]
    fn check_vendor_skills_fall_back_to_conventional_path() {
        let tmp = make_temp_dir();
        let root = tmp.path();

        create_skill_dir(root, "skills/vendor/show-me", "# Show Me\n");
        fs::write(
            root.join("skills/vendor/show-me/PROVENANCE.md"),
            "# Provenance\n",
        )
        .unwrap();
        write_lockfile(root, &serde_json::json!({}));
        write_vendor_lockfile(
            root,
            &serde_json::json!({
                "show-me": {
                    "sourceType": "github",
                    "skillPath": "plugins/show-me/skills/show-me/SKILL.md",
                    "skillFolderHash": "TODO-recalculated"
                }
            }),
        );

        let report = check_skills(root).expect("check_skills");

        assert!(
            report.updated_vendor.contains_key("show-me"),
            "missing vendorPath should fall back to skills/vendor/<name>"
        );
    }

    #[test]
    fn check_vendor_skills_fail_without_provenance() {
        let tmp = make_temp_dir();
        let root = tmp.path();

        create_skill_dir(root, "skills/vendor/show-me", "# Show Me\n");
        write_lockfile(root, &serde_json::json!({}));
        write_vendor_lockfile(
            root,
            &serde_json::json!({
                "show-me": {
                    "sourceType": "github",
                    "skillPath": "plugins/show-me/skills/show-me/SKILL.md",
                    "skillFolderHash": "TODO-recalculated",
                    "vendorPath": "skills/vendor/show-me"
                }
            }),
        );

        let report = check_skills(root).expect("check_skills");

        let has_fail = report.results.iter().any(|(name, result)| {
            name == "show-me"
                && matches!(result, CheckResult::Fail(msg) if msg.contains("PROVENANCE.md"))
        });
        assert!(has_fail, "missing PROVENANCE.md must fail the vendor check");
    }

    #[test]
    fn check_vendor_skills_checks_non_github_source_types() {
        let tmp = make_temp_dir();
        let root = tmp.path();

        create_skill_dir(root, "skills/vendor/show-me", "# Show Me\n");
        fs::write(
            root.join("skills/vendor/show-me/PROVENANCE.md"),
            "# Provenance\n",
        )
        .unwrap();
        write_lockfile(root, &serde_json::json!({}));
        write_vendor_lockfile(
            root,
            &serde_json::json!({
                "show-me": {
                    "sourceType": "local",
                    "skillPath": "plugins/show-me/skills/show-me/SKILL.md",
                    "skillFolderHash": "0000000000000000000000000000000000000000",
                    "vendorPath": "skills/vendor/show-me"
                }
            }),
        );

        let report = check_skills(root).expect("check_skills");
        assert!(
            report.found_github,
            "a vendor entry should be checked regardless of sourceType"
        );
        assert!(
            report.results.iter().any(|(name, result)| {
                name == "show-me" && matches!(result, CheckResult::Fail(_))
            }),
            "non-github vendor entry with mismatched hash must fail"
        );
    }

    // ── write_updated_lockfile / write_updated_vendor_lockfile ─────────

    #[test]
    fn write_updated_lockfile_updates_hash_and_preserves_everything_else() {
        let tmp = make_temp_dir();
        let root = tmp.path();

        let original = r#"{
  "version": 4,
  "skills": {
    "tdd": {
      "source": "mattpocock/skills",
      "sourceType": "github",
      "sourceUrl": "https://github.com/mattpocock/skills.git",
      "skillPath": "skills/engineering/tdd/SKILL.md",
      "skillFolderHash": "TODO-recalculated",
      "pluginName": "mattpocock-skills",
      "installedAt": "2026-01-01T00:00:00.000Z",
      "updatedAt": "2026-01-02T00:00:00.000Z"
    }
  },
  "dismissed": {}
}
"#;
        fs::write(root.join(".skill-lock.json"), original).expect("write lockfile");

        let mut updated = BTreeMap::new();
        updated.insert("tdd".to_string(), "newhash123".to_string());
        write_updated_lockfile(root, &updated).expect("write_updated_lockfile");

        let written = fs::read_to_string(root.join(".skill-lock.json")).expect("read back");
        let expected = original.replace("TODO-recalculated", "newhash123");
        assert_eq!(
            written, expected,
            "only skillFolderHash may change; bytes otherwise identical"
        );
    }

    #[test]
    fn write_updated_lockfile_unknown_skill_is_error() {
        let tmp = make_temp_dir();
        let root = tmp.path();
        write_lockfile(
            root,
            &serde_json::json!({
                "tdd": {
                    "sourceType": "github",
                    "skillPath": "skills/engineering/tdd/SKILL.md",
                    "skillFolderHash": "abc"
                }
            }),
        );

        let mut updated = BTreeMap::new();
        updated.insert("nope".to_string(), "x".to_string());
        let err = write_updated_lockfile(root, &updated).unwrap_err();
        assert!(err.contains("nope"), "error should name the skill: {}", err);
    }

    #[test]
    fn write_updated_vendor_lockfile_keeps_alphabetical_vendor_format() {
        let tmp = make_temp_dir();
        let root = tmp.path();

        let original = r#"{
  "skills": {
    "show-me": {
      "installedAt": "2026-09-09T00:00:00.000Z",
      "license": "MIT",
      "pluginName": "show-me",
      "pluginVersion": "1.0.1",
      "skillFolderHash": "TODO-recalculated",
      "skillPath": "plugins/show-me/skills/show-me/SKILL.md",
      "source": "humanlayer/skills",
      "sourceCommit": "3c2629142c5d437428269b1b722b08c0b87f574d",
      "sourceType": "github",
      "sourceUrl": "https://github.com/humanlayer/skills.git",
      "updatedAt": "2026-09-09T00:00:00.000Z",
      "vendorPath": "skills/vendor/show-me"
    }
  },
  "version": 1
}
"#;
        fs::write(root.join(".vendor-lock.json"), original).expect("write vendor lockfile");

        let mut updated = BTreeMap::new();
        updated.insert("show-me".to_string(), "newhash456".to_string());
        write_updated_vendor_lockfile(root, &updated).expect("write_updated_vendor_lockfile");

        let written = fs::read_to_string(root.join(".vendor-lock.json")).expect("read back");
        let expected = original.replace("TODO-recalculated", "newhash456");
        assert_eq!(
            written, expected,
            "only skillFolderHash may change; vendor alphabetical format preserved"
        );
    }

    // ── any_fail ─────────────────────────────────────────────────────────

    #[test]
    fn any_fail_true_when_fail_present() {
        let results = vec![
            ("a".to_string(), CheckResult::Pass),
            ("b".to_string(), CheckResult::Fail("oops".to_string())),
        ];
        assert!(any_fail(&results));
    }

    #[test]
    fn any_fail_false_when_all_pass() {
        let results = vec![
            ("a".to_string(), CheckResult::Pass),
            ("b".to_string(), CheckResult::Pass),
        ];
        assert!(!any_fail(&results));
    }

    #[test]
    fn any_fail_false_when_empty() {
        let results: Vec<(String, CheckResult)> = vec![];
        assert!(!any_fail(&results));
    }

    // ── format_result ────────────────────────────────────────────────────

    #[test]
    fn format_result_pass() {
        assert_eq!(
            format_result("my-skill", &CheckResult::Pass),
            "PASS: my-skill"
        );
    }

    #[test]
    fn format_result_fail_general_error() {
        assert_eq!(
            format_result(
                "my-skill",
                &CheckResult::Fail("folder not found: /x".to_string())
            ),
            "FAIL: my-skill — folder not found: /x"
        );
    }

    #[test]
    fn format_result_fail_hash_mismatch() {
        assert_eq!(
            format_result(
                "my-skill",
                &CheckResult::Fail("computed: abc123, lockfile: def456".to_string())
            ),
            "FAIL: my-skill (computed: abc123, lockfile: def456)"
        );
    }

    #[test]
    fn format_result_fix() {
        assert_eq!(
            format_result("my-skill", &CheckResult::Fix("abc123".to_string())),
            "FIX: my-skill → abc123"
        );
    }

    #[test]
    fn format_result_warn() {
        assert_eq!(
            format_result(
                "my-skill",
                &CheckResult::Skip("cannot compute hash: git error".to_string())
            ),
            "WARN: my-skill — cannot compute hash: git error"
        );
    }

    #[test]
    fn format_result_skip() {
        assert_eq!(
            format_result(
                "my-skill",
                &CheckResult::Skip("unexpected skillPath format: bad/path".to_string())
            ),
            "SKIP: my-skill — unexpected skillPath format: bad/path"
        );
    }

    // ── determine_exit_code ──────────────────────────────────────────────

    #[test]
    fn exit_code_zero_when_all_pass() {
        let results = vec![
            ("a".to_string(), CheckResult::Pass),
            ("b".to_string(), CheckResult::Pass),
        ];
        assert_eq!(determine_exit_code(&results, true), 0);
    }

    #[test]
    fn exit_code_one_when_any_fail() {
        let results = vec![
            ("a".to_string(), CheckResult::Pass),
            ("b".to_string(), CheckResult::Fail("oops".to_string())),
        ];
        assert_eq!(determine_exit_code(&results, true), 1);
    }

    #[test]
    fn exit_code_one_when_any_skip() {
        // A skipped check means a hash could not be verified; that is not
        // evidence the tree is good, so the gate is fail-closed.
        let results = vec![
            ("a".to_string(), CheckResult::Pass),
            (
                "b".to_string(),
                CheckResult::Skip("unexpected skillPath format: bad/path".to_string()),
            ),
        ];
        assert_eq!(determine_exit_code(&results, true), 1);
    }

    #[test]
    fn exit_code_zero_when_no_github_skills() {
        let results: Vec<(String, CheckResult)> = vec![];
        assert_eq!(determine_exit_code(&results, false), 0);
    }

    #[test]
    fn exit_code_zero_when_empty_results_with_github() {
        let results: Vec<(String, CheckResult)> = vec![];
        assert_eq!(determine_exit_code(&results, true), 0);
    }

    // ── SkillLock deserialization preserves source_type per-skill ────────

    #[test]
    fn deserialize_source_type_per_skill() {
        let json = serde_json::json!({
            "version": 4,
            "skills": {
                "upstream-skill": {
                    "sourceType": "github",
                    "skillPath": "skills/engineering/tdd/SKILL.md",
                    "skillFolderHash": "abc123"
                },
                "local-skill": {
                    "sourceType": "local",
                    "skillPath": "skills/autopilot/local-skill/SKILL.md",
                    "skillFolderHash": "TODO-recalculated"
                }
            }
        });
        let content = serde_json::to_string(&json).unwrap();
        let lock: shared::SkillLock = serde_json::from_str(&content).expect("parse");
        assert_eq!(lock.skills.len(), 2);
        assert_eq!(lock.skills[0].source_type, "local");
        assert_eq!(lock.skills[1].source_type, "github");
    }
    // ── upstream pin ────────────────────────────────────────────────────

    const PIN_COMMIT: &str = "d1478fcb140db0b93c0c8df17bcfd311844749d0";

    fn sample_tag_pin() -> shared::UpstreamPin {
        shared::UpstreamPin {
            schema_version: shared::UPSTREAM_PIN_SCHEMA_VERSION,
            source: "mattpocock/skills".to_string(),
            source_url: "https://github.com/mattpocock/skills.git".to_string(),
            ref_name: "v1.3.2".to_string(),
            ref_kind: shared::UpstreamRefKind::Tag,
            resolved_commit: PIN_COMMIT.to_string(),
            version: "1.3.2".to_string(),
            synced_at: "2026-11-07T10:00:00Z".to_string(),
            reason: None,
        }
    }

    fn write_upstream_package(dir: &Path, version: &str) {
        let path = dir.join("skills/upstream/package.json");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            format!(
                "{{\n  \"name\": \"mattpocock-skills\",\n  \"version\": \"{}\"\n}}\n",
                version
            ),
        )
        .unwrap();
    }

    fn write_readme(dir: &Path, ref_name: &str, commit: &str) {
        fs::write(
            dir.join("README.md"),
            format!(
                "# Fixture\n\nThe upstream snapshot is pinned to release {} `{}`.\n",
                ref_name, commit
            ),
        )
        .unwrap();
    }

    fn write_pin(dir: &Path, pin: &shared::UpstreamPin) {
        fs::write(dir.join(shared::UPSTREAM_PIN_FILE), pin.to_bytes()).unwrap();
    }

    fn pin_result(dir: &Path) -> CheckResult {
        check_upstream_pin(dir)
            .expect("pin check should not hard-error")
            .expect("pin check should be triggered")
            .1
    }

    #[test]
    fn pin_check_skipped_without_upstream_snapshot() {
        let dir = make_temp_dir();
        assert_eq!(check_upstream_pin(dir.path()).unwrap(), None);
    }

    #[test]
    fn pin_check_fails_when_pin_missing() {
        let dir = make_temp_dir();
        write_upstream_package(dir.path(), "1.3.2");
        write_readme(dir.path(), "v1.3.2", PIN_COMMIT);
        match pin_result(dir.path()) {
            CheckResult::Fail(message) => {
                assert!(message.contains("missing .upstream-pin.json"), "{}", message)
            }
            other => panic!("expected Fail, got {:?}", other),
        }
    }

    #[test]
    fn pin_check_passes_for_valid_tag_pin() {
        let dir = make_temp_dir();
        write_upstream_package(dir.path(), "1.3.2");
        write_readme(dir.path(), "v1.3.2", PIN_COMMIT);
        write_pin(dir.path(), &sample_tag_pin());
        assert_eq!(pin_result(dir.path()), CheckResult::Pass);
    }

    #[test]
    fn pin_check_fails_when_readme_omits_the_commit() {
        let dir = make_temp_dir();
        write_upstream_package(dir.path(), "1.3.2");
        fs::write(dir.path().join("README.md"), "pinned to v1.3.2\n").unwrap();
        write_pin(dir.path(), &sample_tag_pin());
        match pin_result(dir.path()) {
            CheckResult::Fail(message) => assert!(
                message.contains("does not mention the pinned commit"),
                "{}",
                message
            ),
            other => panic!("expected Fail, got {:?}", other),
        }
    }

    #[test]
    fn pin_check_fails_when_tag_version_mismatches_the_tree() {
        let dir = make_temp_dir();
        write_upstream_package(dir.path(), "1.3.3");
        write_readme(dir.path(), "v1.3.2", PIN_COMMIT);
        write_pin(dir.path(), &sample_tag_pin());
        match pin_result(dir.path()) {
            CheckResult::Fail(message) => assert!(
                message.contains("does not match skills/upstream/package.json"),
                "{}",
                message
            ),
            other => panic!("expected Fail, got {:?}", other),
        }
    }

    #[test]
    fn pin_check_fails_when_pin_is_malformed() {
        let dir = make_temp_dir();
        write_upstream_package(dir.path(), "1.3.2");
        write_readme(dir.path(), "v1.3.2", PIN_COMMIT);
        fs::write(
            dir.path().join(shared::UPSTREAM_PIN_FILE),
            "{\"schemaVersion\": 1, \"surprise\": true}\n",
        )
        .unwrap();
        match pin_result(dir.path()) {
            CheckResult::Fail(message) => {
                assert!(message.contains("malformed upstream pin"), "{}", message)
            }
            other => panic!("expected Fail, got {:?}", other),
        }
    }

    #[test]
    fn pin_check_requires_reason_for_non_tag_pins() {
        let dir = make_temp_dir();
        write_upstream_package(dir.path(), "1.3.3");
        write_readme(dir.path(), "main", PIN_COMMIT);
        let mut pin = sample_tag_pin();
        pin.ref_name = "main".to_string();
        pin.ref_kind = shared::UpstreamRefKind::Branch;
        pin.version = "1.3.3".to_string();
        write_pin(dir.path(), &pin);
        match pin_result(dir.path()) {
            CheckResult::Fail(message) => assert!(message.contains("requires a non-empty reason"), "{}", message),
            other => panic!("expected Fail, got {:?}", other),
        }

        pin.reason = Some("main carries the 1.3.3 release candidate".to_string());
        write_pin(dir.path(), &pin);
        assert_eq!(pin_result(dir.path()), CheckResult::Pass);
    }
}
