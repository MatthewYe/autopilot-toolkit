#!/usr/bin/env rust-script
//! ```cargo
//! [dependencies]
//! serde = { version = "1", features = ["derive"] }
//! serde_json = { version = "1", features = ["preserve_order"] }
//! chrono = "0.4"
//! git-utils = { path = "../crates/git-utils" }
//! shared = { path = "../crates/shared" }
//! ```
//!
//! Sync the vendored upstream (skills/upstream/) to a pinned ref of
//! mattpocock/skills. Replaces the entire upstream tree, recomputes all
//! skillFolderHash values, drops orphan entries, and adds new skills.
//!
//! Upstream keeps beta skills in `skills/in-progress/`, outside the plugin,
//! with no stability guarantee. IN_PROGRESS_ALLOWLIST names the ones this
//! toolkit ships anyway; they are tracked in `.skill-lock.json` like any
//! other upstream skill. A ref that does not contain an allowlisted skill
//! fails the sync before the tree or the lock is touched.
//!
//! Usage:
//!   rust-script scripts/sync-upstream.rs <REF>
//!
//! REF is required: a release tag, branch, or commit SHA. There is no
//! default, because a ref that predates an allowlisted skill would orphan it.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::Path;
use std::process::{self, Command};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

const UPSTREAM_REPO: &str = "https://github.com/mattpocock/skills.git";
const PLUGIN_NAME: &str = "mattpocock-skills";

// ── Skill discovery ────────────────────────────────────────────────────────

/// BUCKET_DIRS lists the subdirectories under the upstream repo's skills/
/// that contain shippable skills. Order matters for sorting.
const BUCKET_DIRS: &[&str] = &["engineering", "productivity", "misc"];

/// Beta upstream skills this toolkit ships on purpose.
///
/// Upstream's `in-progress/` bucket is excluded from the plugin and can
/// change or disappear without warning, so nothing is picked up from it
/// implicitly: a skill ships only when its name is listed here. Remove a
/// name once upstream graduates it into a stable bucket (the stable entry
/// wins either way) or drops it (the lock entry becomes an orphan).
const IN_PROGRESS_ALLOWLIST: &[&str] = &["implement-spec", "loop-me", "retro"];

type SkillMap = BTreeMap<String, serde_json::Value>;

/// Build one `.skill-lock.json` entry for a skill directory.
fn skill_entry(dir: &Path, skill_path: &str) -> Result<serde_json::Value, String> {
    let hash = git_utils::compute_tree_hash(dir)?;
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

    let mut obj = serde_json::Map::new();
    obj.insert(
        "source".to_string(),
        serde_json::Value::String("mattpocock/skills".to_string()),
    );
    obj.insert(
        "sourceType".to_string(),
        serde_json::Value::String("github".to_string()),
    );
    obj.insert(
        "sourceUrl".to_string(),
        serde_json::Value::String(UPSTREAM_REPO.to_string()),
    );
    obj.insert(
        "skillPath".to_string(),
        serde_json::Value::String(skill_path.to_string()),
    );
    obj.insert(
        "skillFolderHash".to_string(),
        serde_json::Value::String(hash),
    );
    obj.insert(
        "pluginName".to_string(),
        serde_json::Value::String(PLUGIN_NAME.to_string()),
    );
    obj.insert(
        "installedAt".to_string(),
        serde_json::Value::String(now.clone()),
    );
    obj.insert("updatedAt".to_string(), serde_json::Value::String(now));

    Ok(serde_json::Value::Object(obj))
}

/// Walk the cloned upstream tree to discover all SKILL.md files and build a
/// map of skill name → metadata entry (with computed hash).
fn discover_skills(upstream_root: &Path) -> Result<SkillMap, String> {
    let skills_dir = upstream_root.join("skills");
    if !skills_dir.is_dir() {
        return Err(format!(
            "skills/ not found in upstream at {}",
            upstream_root.display()
        ));
    }

    let mut map: SkillMap = BTreeMap::new();

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
            map.insert(name, skill_entry(&path, &skill_path)?);
        }
    }

    // ── In-progress allowlist ──────────────────────────────────────────
    //
    // A name that graduated into a stable bucket is already in `map`, and the
    // stable entry wins. A name that vanished upstream fails the sync, so an
    // allowlisted skill can never be dropped from the lock by accident.
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
            println!(
                "  in-progress allowlist: {} {}, keeping the stable entry",
                name, state
            );
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
        map.insert(name.to_string(), skill_entry(&dir, &skill_path)?);
        println!("  in-progress allowlist: shipping {}", name);
    }

    Ok(map)
}

// ── Lock file update ───────────────────────────────────────────────────────

/// Merge discovered skills into the existing lock file.
/// - Skill exists in both → update hash, preserve installedAt
/// - Skill only in new → add with defaults
fn merge_lock_file(installed_ats: &BTreeMap<String, String>, new_skills: &SkillMap) -> SkillMap {
    let mut merged: SkillMap = BTreeMap::new();

    for (name, new_entry) in new_skills {
        let mut entry = new_entry.clone();
        // Preserve original installedAt if it exists
        if let Some(old_installed) = installed_ats.get(name) {
            if let Some(obj) = entry.as_object_mut() {
                obj.insert(
                    "installedAt".to_string(),
                    serde_json::Value::String(old_installed.clone()),
                );
            }
        }
        merged.insert(name.clone(), entry);
    }

    merged
}

// ── Main ───────────────────────────────────────────────────────────────────

fn usage() -> ! {
    eprintln!("Usage: rust-script scripts/sync-upstream.rs <REF>");
    eprintln!();
    eprintln!("REF is a required mattpocock/skills release tag, branch, or commit SHA.");
    eprintln!("There is no default: a ref that predates an allowlisted skill would orphan it.");
    process::exit(1);
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let tag = match args.get(1) {
        Some(tag) if !tag.trim().is_empty() => tag.clone(),
        _ => usage(),
    };

    // Derive project root
    let project_root = shared::project_root();

    let upstream_dir = project_root.join("skills").join("upstream");
    let lockfile_path = project_root.join(".skill-lock.json");

    println!("=== Sync upstream to {} ===", tag);
    println!("Project root: {}", project_root.display());

    // ── 1. Fetch upstream at the pinned ref ────────────────────────────
    //
    // `git clone --branch` accepts branches and tags only. Init + fetch +
    // checkout also accepts a commit SHA, which is how the in-progress
    // allowlist is pinned to a main commit that has no release tag yet.
    let n = TEMP_COUNTER.fetch_add(1, Ordering::SeqCst);
    let clone_dir =
        std::env::temp_dir().join(format!("sync-upstream-clone-{}-{}", process::id(), n));
    fs::create_dir_all(&clone_dir).expect("cannot create temp clone dir");

    println!("\nFetching {} ({}):", UPSTREAM_REPO, tag);
    let steps: Vec<Vec<&str>> = vec![
        vec!["init", "--quiet"],
        vec!["remote", "add", "origin", UPSTREAM_REPO],
        vec!["fetch", "--quiet", "--depth", "1", "origin", tag.as_str()],
        vec!["checkout", "--quiet", "FETCH_HEAD"],
    ];
    for step in &steps {
        let output = Command::new("git")
            .args(step)
            .current_dir(&clone_dir)
            .output()
            .expect("git failed to start");
        if !output.status.success() {
            eprintln!(
                "ERROR: git {} failed: {}",
                step.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            );
            let _ = fs::remove_dir_all(&clone_dir);
            process::exit(1);
        }
    }
    let resolved = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&clone_dir)
        .output()
        .expect("git rev-parse failed");
    println!(
        "  fetched to {} (commit {})",
        clone_dir.display(),
        String::from_utf8_lossy(&resolved.stdout).trim()
    );

    // ── 2. Discover skills from cloned upstream ────────────────────────
    println!("\nDiscovering skills in upstream...");
    let new_skills = match discover_skills(&clone_dir) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("ERROR: {}", e);
            let _ = fs::remove_dir_all(&clone_dir);
            process::exit(1);
        }
    };
    println!("  found {} skills:", new_skills.len());
    for (name, entry) in &new_skills {
        let hash = entry["skillFolderHash"].as_str().unwrap_or("?");
        let path = entry["skillPath"].as_str().unwrap_or("?");
        println!("    {:.<36} {}", name, path);
        println!("      {}{}", " ".repeat(36), hash);
    }

    // ── 3. Read existing lock file ─────────────────────────────────────
    let (installed_ats, existing_names, version): (BTreeMap<String, String>, Vec<String>, u64) =
        if lockfile_path.exists() {
            match shared::load_skill_lock_at(&project_root) {
                Ok(lock) => {
                    let ats: BTreeMap<String, String> = lock
                        .skills
                        .iter()
                        .filter_map(|s| s.installed_at.clone().map(|at| (s.name.clone(), at)))
                        .collect();
                    let names: Vec<String> = lock.skills.iter().map(|s| s.name.clone()).collect();
                    let ver = lock.version as u64;
                    (ats, names, ver)
                }
                Err(e) => {
                    eprintln!("WARNING: cannot parse lock file: {}", e);
                    (BTreeMap::new(), vec![], 4u64)
                }
            }
        } else {
            (BTreeMap::new(), vec![], 4u64)
        };

    // ── 4. Merge and detect orphans ────────────────────────────────────
    let merged = merge_lock_file(&installed_ats, &new_skills);

    let orphans: Vec<String> = existing_names
        .iter()
        .filter(|n| !new_skills.contains_key(*n))
        .cloned()
        .collect();

    if !orphans.is_empty() {
        println!("\nOrphan skills (removed from lock file):");
        for name in &orphans {
            println!("  - {}", name);
        }
    }

    let added: Vec<_> = merged
        .keys()
        .filter(|k| !existing_names.contains(k))
        .collect();
    if !added.is_empty() {
        println!("\nNew skills (added to lock file):");
        for name in &added {
            println!("  + {}", name);
        }
    }

    // ── 5. Replace upstream directory ──────────────────────────────────
    println!("\nReplacing skills/upstream/...");
    if upstream_dir.exists() {
        fs::remove_dir_all(&upstream_dir).expect("cannot remove old upstream");
    }
    fs::create_dir_all(&upstream_dir).expect("cannot create upstream dir");

    // Copy everything except .git
    copy_dir_except_git(&clone_dir, &upstream_dir);

    // ── 6. Write updated lock file ─────────────────────────────────────

    let mut lock_json = serde_json::Map::new();
    lock_json.insert(
        "version".to_string(),
        serde_json::Value::Number(version.into()),
    );
    lock_json.insert(
        "skills".to_string(),
        serde_json::Value::Object(merged.iter().map(|(k, v)| (k.clone(), v.clone())).collect()),
    );
    lock_json.insert(
        "dismissed".to_string(),
        serde_json::Value::Object(serde_json::Map::new()),
    );

    let lock_content =
        serde_json::to_string_pretty(&serde_json::Value::Object(lock_json)).unwrap_or_default();
    fs::write(&lockfile_path, lock_content + "\n").expect("cannot write .skill-lock.json");
    println!("  written {}", lockfile_path.display());

    // ── 7. Cleanup temp clone ──────────────────────────────────────────
    let _ = fs::remove_dir_all(&clone_dir);

    // ── 8. Run check.rs to validate ────────────────────────────────────
    println!("\n=== Running check.rs ===");
    let check_path = project_root.join("scripts").join("check.rs");
    if check_path.exists() {
        let status = Command::new("rust-script")
            // `-f`: the cache ignores path-dependency changes (`crates/*`,
            // upstream rust-script#122).
            .arg("-f")
            .arg(&check_path)
            .current_dir(&project_root)
            .status();

        match status {
            Ok(s) if s.success() => {
                println!("\nSync complete — all checks PASS.");
            }
            Ok(s) => {
                eprintln!(
                    "\nERROR: post-sync validation failed (check.rs exit code: {:?}).",
                    s.code()
                );
                std::process::exit(1);
            }
            Err(e) => {
                eprintln!(
                    "\nERROR: post-sync validation failed: could not run check.rs: {}",
                    e
                );
                std::process::exit(1);
            }
        }
    } else {
        eprintln!(
            "\nERROR: post-sync validation failed: {} does not exist.",
            check_path.display()
        );
        std::process::exit(1);
    }
}

// ── File copy helpers ──────────────────────────────────────────────────────

fn copy_dir_except_git(src: &Path, dst: &Path) {
    if !src.is_dir() {
        return;
    }
    fs::create_dir_all(dst).ok();

    for entry in fs::read_dir(src).into_iter().flatten().flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str == ".git" {
            continue;
        }
        let src_path = entry.path();
        let dst_path = dst.join(&*name_str);

        if src_path.is_dir() {
            copy_dir_except_git(&src_path, &dst_path);
        } else {
            fs::copy(&src_path, &dst_path).ok();
        }
    }
}
