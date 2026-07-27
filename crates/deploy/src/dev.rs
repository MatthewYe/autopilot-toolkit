//! Dev symlink management: `dev` and `dev-clean` subcommands.
//!
//! `dev_all` creates runtime-routed symlinks for coupled skills and direct
//! symlinks for agnostic/upstream skills into the agent runtime directories.
//! `dev_clean` removes all symlinks that point back into the project tree.

use std::path::Path;

use skill_index::{classify_skill, SkillType};

use super::{stage_coupled_skill, sync_path, SyncKind};

/// Symlink all skills from the source tree into agent runtime directories.
pub fn dev_all(
    project_root: &Path,
    shared_skills_dir: &Path,
    reasonix_skills_dir: &Path,
    codex_skills_dir: &Path,
    codex_agents_dir: &Path,
    opencode_skills_dir: &Path,
    _opencode_agents_dir: &Path,
) -> Result<(), anyhow::Error> {
    println!("==> Syncing all skills from source tree...");

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

            let (skill_type, _variants, codex_agent) = classify_skill(&src_dir);

            if skill_type == SkillType::Coupled {
                let dev_staging = project_root.join("dist").join("dev-skills").join(&name);
                stage_coupled_skill(&src_dir, &dev_staging)?;
                sync_path(&dev_staging, &shared_skills_dir.join(&name), SyncKind::Dir)?;
                remove_project_symlink(&reasonix_skills_dir.join(&name), project_root)?;
                remove_project_symlink(&codex_skills_dir.join(&name), project_root)?;
                remove_project_symlink(&opencode_skills_dir.join(&name), project_root)?;
                count += 1;

                if codex_agent {
                    let agent_src = src_dir.join("codex").join("agent.toml");
                    sync_path(
                        &agent_src,
                        &codex_agents_dir.join(format!("{}.toml", name)),
                        SyncKind::File,
                    )?;
                    count += 1;
                }

                // opencode agent.md → opencode skills (subagent)
                let opencode_agent_src = src_dir.join("opencode").join("agent.md");
                if opencode_agent_src.is_file() {
                    sync_path(
                        &opencode_agent_src,
                        &opencode_skills_dir.join(format!("{}.md", name)),
                        SyncKind::File,
                    )?;
                    count += 1;
                }
            } else {
                sync_path(&src_dir, &shared_skills_dir.join(&name), SyncKind::Dir)?;
                count += 1;
            }
        }
    }

    // ── Upstream skills ──
    let lock_path = project_root.join(".skill-lock.json");
    if lock_path.is_file() {
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
    opencode_skills_dir: &Path,
    _opencode_agents_dir: &Path,
) -> Result<(), anyhow::Error> {
    println!("==> Removing all dev symlinks...");
    let mut removed = 0u32;

    for dir in &[
        shared_skills_dir,
        reasonix_skills_dir,
        codex_skills_dir,
        opencode_skills_dir,
    ] {
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

    for agents_dir in &[codex_agents_dir, _opencode_agents_dir] {
        if !agents_dir.is_dir() {
            continue;
        }
        for entry in std::fs::read_dir(agents_dir)? {
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

// ── Helpers ──────────────────────────────────────────────────────────────

fn remove_project_symlink(path: &Path, project_root: &Path) -> Result<(), anyhow::Error> {
    if !path.is_symlink() {
        return Ok(());
    }
    let target = std::fs::read_link(path)?;
    if target.starts_with(project_root) {
        std::fs::remove_file(path)?;
    }
    Ok(())
}
