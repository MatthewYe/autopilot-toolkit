//! Skill integrity check — verifies that the git tree hashes in
//! `.skill-lock.json` and `.vendor-lock.json` match the actual skill
//! directories on disk.
//!
//! Migrated from `scripts/check.rs`.  Uses `shared::load_skill_lock()` and
//! `shared::load_vendor_lock()` for parsing and `shared::SkillLock` /
//! `shared::LockedSkill` as the source-of-truth types.
//!
//! Public API:
//! - `check_skills(project_root)` → `Result<CheckReport, String>`

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
/// Uses `shared::load_skill_lock()` for `.skill-lock.json` and
/// `shared::load_vendor_lock()` for `.vendor-lock.json`, then computes git
/// tree hashes for each skill directory and compares them against the
/// expected values.
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
/// Reads the current lock file, updates the `skillFolderHash` fields
/// listed in `updated`, and writes the result back.  Preserves all
/// other fields and formatting.
pub fn write_updated_lockfile(
    project_root: &Path,
    updated: &BTreeMap<String, String>,
) -> Result<(), String> {
    write_updated_hashes(&project_root.join(shared::SKILL_LOCK_FILE), updated)
}

/// Write an updated `.vendor-lock.json` with new skill folder hashes.
///
/// Reads the current lock file, updates the `skillFolderHash` fields
/// listed in `updated`, and writes the result back.  Preserves all
/// other fields and formatting.
pub fn write_updated_vendor_lockfile(
    project_root: &Path,
    updated: &BTreeMap<String, String>,
) -> Result<(), String> {
    write_updated_hashes(&project_root.join(shared::VENDOR_LOCK_FILE), updated)
}

fn write_updated_hashes(
    lock_path: &Path,
    updated: &BTreeMap<String, String>,
) -> Result<(), String> {
    let content =
        fs::read_to_string(lock_path).map_err(|e| format!("cannot read {:?}: {}", lock_path, e))?;

    let mut data: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| format!("invalid JSON: {}", e))?;

    if let Some(skills) = data.get_mut("skills").and_then(|s| s.as_object_mut()) {
        for (name, new_hash) in updated {
            if let Some(skill) = skills.get_mut(name) {
                if let Some(obj) = skill.as_object_mut() {
                    obj.insert(
                        "skillFolderHash".to_string(),
                        serde_json::Value::String(new_hash.clone()),
                    );
                }
            }
        }
    }

    let updated_json = serde_json::to_string_pretty(&data).unwrap_or_default();
    fs::write(lock_path, updated_json + "\n")
        .map_err(|e| format!("cannot write {:?}: {}", lock_path, e))?;

    Ok(())
}

/// Determine if any result is a Fail.
pub fn any_fail(results: &[(String, CheckResult)]) -> bool {
    results
        .iter()
        .any(|(_, r)| matches!(r, CheckResult::Fail(_)))
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
/// Returns 0 when all pass (or no github skills), 1 when any FAIL.
pub fn determine_exit_code(results: &[(String, CheckResult)], found_github: bool) -> i32 {
    if !found_github {
        return 0;
    }
    if any_fail(results) {
        1
    } else {
        0
    }
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
}
