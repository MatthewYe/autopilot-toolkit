#!/usr/bin/env rust-script
//! ```cargo
//! [dependencies]
//! serde = { version = "1", features = ["derive"] }
//! serde_json = { version = "1", features = ["preserve_order"] }
//! chrono = "0.4"
//! ```
//!
//! Sync the vendored upstream (skills/upstream/) to a tagged release of
//! mattpocock/skills. Replaces the entire upstream tree, recomputes all
//! skillFolderHash values, drops orphan entries, and adds new skills.
//!
//! Usage:
//!   rust-script scripts/sync-upstream.rs [TAG]
//!
//! Default tag: v1.1.0

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{self, Command};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

const UPSTREAM_REPO: &str = "https://github.com/mattpocock/skills.git";
const PLUGIN_NAME: &str = "mattpocock-skills";

// ── Git helpers ────────────────────────────────────────────────────────────

fn compute_tree_hash(folder: &Path) -> Result<String, String> {
    if !folder.is_dir() {
        return Err(format!("folder not found: {}", folder.display()));
    }
    let n = TEMP_COUNTER.fetch_add(1, Ordering::SeqCst);
    let tmp = std::env::temp_dir().join(format!("sync-upstream-hash-{}-{}", process::id(), n));
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

// ── Skill discovery ────────────────────────────────────────────────────────

/// BUCKET_DIRS lists the subdirectories under the upstream repo's skills/
/// that contain shippable skills. Order matters for sorting.
const BUCKET_DIRS: &[&str] = &["engineering", "productivity", "misc"];

type SkillMap = BTreeMap<String, serde_json::Value>;

/// Walk the cloned upstream tree to discover all SKILL.md files and build a
/// map of skill name → metadata entry (with computed hash).
fn discover_skills(upstream_root: &Path) -> Result<SkillMap, String> {
    let skills_dir = upstream_root.join("skills");
    if !skills_dir.is_dir() {
        return Err(format!("skills/ not found in upstream at {}", upstream_root.display()));
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

            let hash = compute_tree_hash(&path)?;
            let skill_path = format!("skills/{}/{}/SKILL.md", bucket, name);
            let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

            let mut obj = serde_json::Map::new();
            obj.insert("source".to_string(), serde_json::Value::String("mattpocock/skills".to_string()));
            obj.insert("sourceType".to_string(), serde_json::Value::String("github".to_string()));
            obj.insert(
                "sourceUrl".to_string(),
                serde_json::Value::String(UPSTREAM_REPO.to_string()),
            );
            obj.insert("skillPath".to_string(), serde_json::Value::String(skill_path));
            obj.insert("skillFolderHash".to_string(), serde_json::Value::String(hash));
            obj.insert("pluginName".to_string(), serde_json::Value::String(PLUGIN_NAME.to_string()));
            obj.insert("installedAt".to_string(), serde_json::Value::String(now.clone()));
            obj.insert("updatedAt".to_string(), serde_json::Value::String(now));

            map.insert(name, serde_json::Value::Object(obj));
        }
    }

    Ok(map)
}

// ── Lock file update ───────────────────────────────────────────────────────

/// Merge discovered skills into the existing lock file.
/// - Skill exists in both → update hash, preserve installedAt
/// - Skill only in new → add with defaults
/// - Skill only in old → detected as orphan (returned separately)
fn merge_lock_file(
    old_skills: &SkillMap,
    new_skills: &SkillMap,
) -> (SkillMap, Vec<String>) {
    let mut merged: SkillMap = BTreeMap::new();
    let mut orphans: Vec<String> = Vec::new();

    // Keep existing skills that still exist upstream (update hash)
    // and note skills that are gone (orphans).
    for (name, old_entry) in old_skills {
        if let Some(new_entry) = new_skills.get(name) {
            let mut entry = new_entry.clone();
            // Preserve original installedAt if it exists
            if let Some(old_installed) = old_entry
                .get("installedAt")
                .and_then(|v| v.as_str())
            {
                if let Some(obj) = entry.as_object_mut() {
                    obj.insert(
                        "installedAt".to_string(),
                        serde_json::Value::String(old_installed.to_string()),
                    );
                }
            }
            merged.insert(name.clone(), entry);
        } else {
            orphans.push(name.clone());
        }
    }

    // Add new skills not previously in the lock file.
    for (name, entry) in new_skills {
        if !merged.contains_key(name) {
            merged.insert(name.clone(), entry.clone());
        }
    }

    (merged, orphans)
}

// ── Main ───────────────────────────────────────────────────────────────────

fn main() {
    let args: Vec<String> = env::args().collect();
    let tag = if args.len() > 1 {
        args[1].clone()
    } else {
        "v1.1.0".to_string()
    };

    // Derive project root
    let project_root = env::var("PROJECT_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let script_path = PathBuf::from(&args[0]);
            script_path
                .canonicalize()
                .unwrap_or_else(|_| script_path.clone())
                .parent()
                .and_then(|p| p.parent())
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        });

    let upstream_dir = project_root.join("skills").join("upstream");
    let lockfile_path = project_root.join(".skill-lock.json");

    println!("=== Sync upstream to {} ===", tag);
    println!("Project root: {}", project_root.display());

    // ── 1. Clone upstream at tag ───────────────────────────────────────
    let n = TEMP_COUNTER.fetch_add(1, Ordering::SeqCst);
    let clone_dir = std::env::temp_dir()
        .join(format!("sync-upstream-clone-{}-{}", process::id(), n));

    println!("\nCloning {} ({}):", UPSTREAM_REPO, tag);
    let clone_output = Command::new("git")
        .args(["clone", "--quiet", "--branch", &tag, "--depth", "1"])
        .arg(UPSTREAM_REPO)
        .arg(&clone_dir)
        .output()
        .expect("git clone failed");

    if !clone_output.status.success() {
        let stderr = String::from_utf8_lossy(&clone_output.stderr);
        eprintln!("ERROR: git clone failed: {}", stderr);
        let _ = fs::remove_dir_all(&clone_dir);
        process::exit(1);
    }
    println!("  cloned to {}", clone_dir.display());

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
    let old_skills: SkillMap = if lockfile_path.exists() {
        let content = fs::read_to_string(&lockfile_path)
            .expect("cannot read .skill-lock.json");
        let data: serde_json::Value =
            serde_json::from_str(&content).expect("cannot parse .skill-lock.json");
        data.get("skills")
            .and_then(|s| s.as_object())
            .map(|obj| {
                obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
            })
            .unwrap_or_default()
    } else {
        BTreeMap::new()
    };

    // ── 4. Merge and detect orphans ────────────────────────────────────
    let (merged, orphans) = merge_lock_file(&old_skills, &new_skills);

    if !orphans.is_empty() {
        println!("\nOrphan skills (removed from lock file):");
        for name in &orphans {
            println!("  - {}", name);
        }
    }

    let added: Vec<_> = merged
        .keys()
        .filter(|k| !old_skills.contains_key(*k))
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
    let version = if lockfile_path.exists() {
        let content = fs::read_to_string(&lockfile_path).unwrap_or_default();
        let data: serde_json::Value = serde_json::from_str(&content).unwrap_or_default();
        data.get("version")
            .and_then(|v| v.as_u64())
            .unwrap_or(4)
    } else {
        4u64
    };

    let mut lock_json = serde_json::Map::new();
    lock_json.insert("version".to_string(), serde_json::Value::Number(version.into()));
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
                eprintln!("\nERROR: post-sync validation failed: could not run check.rs: {}", e);
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
