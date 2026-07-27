//! Dev symlink management: `dev` and `dev-clean` subcommands.
//!
//! `dev_all` creates symlinks for all autopilot and upstream skills into the
//! agent runtime directories.  `dev_clean` removes all symlinks that point
//! back into the project tree.

use std::path::{Path, PathBuf};

use skill_index::{classify_skill, SkillType};

use super::{sync_path, SyncKind};

/// Symlink all skills from the source tree into agent runtime directories.
pub fn dev_all(
    project_root: &Path,
    shared_skills_dir: &Path,
    reasonix_skills_dir: &Path,
    codex_skills_dir: &Path,
    codex_agents_dir: &Path,
) -> Result<(), anyhow::Error> {
    println!("==> Syncing all skills from source tree...");

    // ── Autopilot skills ──
    let autopilot_dir = project_root.join("skills").join("autopilot");
    let mut count = 0u32;
    if autopilot_dir.is_dir() {
        for entry in std::fs::read_dir(&autopilot_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            let src_dir = entry.path();

            let (skill_type, variants, codex_agent) = classify_skill(&src_dir);

            if skill_type == SkillType::Coupled {
                // Coupled skill: symlink variant for each detected runtime
                for variant in &variants {
                    let target_dir = match variant.as_str() {
                        "reasonix" => reasonix_skills_dir,
                        "codex" => codex_skills_dir,
                        "kimi" => shared_skills_dir,
                        _ => continue,
                    };
                    // Only symlink if the runtime directory exists on this machine
                    let runtime_home = runtime_dir_for_variant(variant);
                    if let Some(ref home) = runtime_home {
                        if !home.exists() && variant.as_str() != "kimi" {
                            continue;
                        }
                    }
                    let variant_src = src_dir.join(variant);
                    if variant_src.is_dir() {
                        sync_path(&variant_src, &target_dir.join(&name), SyncKind::Dir)?;
                        count += 1;
                    }
                }
                // Codex agent.toml
                if codex_agent {
                    let agent_src = src_dir.join("codex").join("agent.toml");
                    sync_path(
                        &agent_src,
                        &codex_agents_dir.join(format!("{}.toml", name)),
                        SyncKind::File,
                    )?;
                    count += 1;
                }
            } else {
                // Agnostic skill
                sync_path(&src_dir, &shared_skills_dir.join(&name), SyncKind::Dir)?;
                count += 1;
            }
        }
    }

    // ── Upstream skills ──
    if project_root.join(".skill-lock.json").is_file() {
        match shared::load_skill_lock() {
            Ok(lock) => {
                for skill in &lock.skills {
                    let src_parent = Path::new(&skill.skill_path).parent().unwrap_or(Path::new(""));
                    let src_dir = project_root
                        .join("skills")
                        .join("upstream")
                        .join(src_parent);
                    if src_dir.is_dir() {
                        sync_path(&src_dir, &shared_skills_dir.join(&skill.name), SyncKind::Dir)?;
                        count += 1;
                    } else {
                        eprintln!(
                            "WARNING: upstream skill '{}' source dir missing, skipping",
                            skill.name
                        );
                    }
                }
            }
            Err(e) => {
                eprintln!(
                    "WARNING: failed to load .skill-lock.json for upstream dev: {}",
                    e
                );
            }
        }
    }

    println!("==> Done: {} symlinks created/verified.", count);
    Ok(())
}

/// Remove all dev symlinks that point back into the project tree.
pub fn dev_clean(
    project_root: &Path,
    shared_skills_dir: &Path,
    reasonix_skills_dir: &Path,
    codex_skills_dir: &Path,
    codex_agents_dir: &Path,
) -> Result<(), anyhow::Error> {
    println!("==> Removing all dev symlinks...");
    let mut removed = 0u32;

    for dir in &[shared_skills_dir, reasonix_skills_dir, codex_skills_dir] {
        if !dir.is_dir() {
            continue;
        }
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_symlink() {
                continue;
            }
            if let Ok(target) = std::fs::read_link(&path) {
                if target.starts_with(project_root) {
                    std::fs::remove_file(&path)?;
                    removed += 1;
                }
            }
        }
    }

    // Codex agents
    if codex_agents_dir.is_dir() {
        for entry in std::fs::read_dir(codex_agents_dir)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_symlink() {
                continue;
            }
            if let Ok(target) = std::fs::read_link(&path) {
                if target.starts_with(project_root) {
                    std::fs::remove_file(&path)?;
                    removed += 1;
                }
            }
        }
    }

    println!("==> Done: {} symlinks removed.", removed);
    Ok(())
}

/// Map a runtime variant name to its expected home directory on the local machine.
fn runtime_dir_for_variant(variant: &str) -> Option<PathBuf> {
    match variant {
        "reasonix" => std::env::var("HOME")
            .ok()
            .map(|h| PathBuf::from(h).join(".reasonix")),
        "codex" => std::env::var("HOME")
            .ok()
            .map(|h| PathBuf::from(h).join(".codex")),
        "kimi" => Some(PathBuf::from("/")), // always assume kimi
        _ => None,
    }
}
