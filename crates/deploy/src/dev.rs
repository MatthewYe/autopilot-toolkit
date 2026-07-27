//! Dev symlink management: `dev` and `dev-clean` subcommands.
//!
//! `dev_all` creates runtime-routed symlinks for coupled skills and direct
//! symlinks for agnostic/upstream skills into the agent runtime directories.
//! `dev_clean` removes all symlinks that point back into the project tree.

use std::path::{Path, PathBuf};

use anyhow::Context;
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

// ── Helpers ──────────────────────────────────────────────────────────────

/// Remove a symlink if it points back into the project tree.
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

/// Copy a directory tree, renaming `SKILL.md` → `INSTRUCTIONS.md`.
fn copy_instruction_tree(src: &Path, dst: &Path) -> Result<(), anyhow::Error> {
    if src.is_dir() {
        std::fs::create_dir_all(dst)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            let ty = entry.file_type()?;
            let file_name = entry.file_name();
            let dest_name = if file_name == "SKILL.md" {
                "INSTRUCTIONS.md".into()
            } else {
                file_name
            };
            let dest = dst.join(dest_name);
            if ty.is_dir() {
                copy_instruction_tree(&entry.path(), &dest)?;
            } else {
                std::fs::copy(entry.path(), &dest)?;
            }
        }
    } else if src.is_file() {
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(src, dst)?;
    }
    Ok(())
}

/// Extract the YAML frontmatter from a SKILL.md (text between `---` delimiters).
fn skill_frontmatter(content: &str) -> Result<&str, anyhow::Error> {
    let stripped = content.strip_prefix("---\n").unwrap_or(content);
    stripped
        .splitn(2, "\n---")
        .next()
        .context("SKILL.md has no frontmatter")
}

/// Stage a coupled skill into a runtime-router layout:
///
/// ```
/// dst/
/// ├── SKILL.md              ← router (frontmatter + runtime dispatch instructions)
/// └── runtime/
///     ├── default/           ← top-level non-variant files (SKILL.md→INSTRUCTIONS.md)
///     ├── reasonix/          ← reasonix variant subtree
///     ├── codex/             ← codex variant subtree
///     └── kimi/              ← kimi variant subtree
/// ```
fn stage_coupled_skill(src: &Path, dst: &Path) -> Result<(), anyhow::Error> {
    if dst.exists() {
        std::fs::remove_dir_all(dst)?;
    }
    std::fs::create_dir_all(dst)?;

    let top_level_skill = src.join("SKILL.md");
    let fallback_variant = ["codex", "kimi", "reasonix"]
        .iter()
        .map(|variant| src.join(variant))
        .find(|variant_dir| variant_dir.join("SKILL.md").is_file());
    let reasonix_skill = src.join("reasonix").join("SKILL.md");
    let frontmatter_source = if reasonix_skill.is_file() {
        reasonix_skill
    } else if top_level_skill.is_file() {
        top_level_skill.clone()
    } else {
        fallback_variant
            .as_ref()
            .context("runtime-coupled skill has no SKILL.md source")?
            .join("SKILL.md")
    };
    let default_content = std::fs::read_to_string(frontmatter_source)?;
    let frontmatter = skill_frontmatter(&default_content)?;
    let router = format!(
        "{frontmatter}\n\n# Runtime routing\n\n\
This installed skill has one discoverable entry point so runtimes do not index duplicate skills.\n\n\
1. Identify the current agent runtime from the system context: `codex`, `kimi`, or `reasonix`.\n\
2. Read `runtime/<runtime>/INSTRUCTIONS.md` completely when it exists.\n\
3. Otherwise read `runtime/default/INSTRUCTIONS.md` completely.\n\
4. Follow only the selected instruction file and its relative references. Do not load another runtime's instructions.\n"
    );
    std::fs::write(dst.join("SKILL.md"), router)?;

    let runtime_root = dst.join("runtime");
    let default_dst = runtime_root.join("default");
    std::fs::create_dir_all(&default_dst)?;
    if top_level_skill.is_file() {
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            let name = entry.file_name();
            if ["codex", "kimi", "reasonix"]
                .iter()
                .any(|variant| name == *variant)
            {
                continue;
            }
            let ty = entry.file_type()?;
            let dest_name = if name == "SKILL.md" {
                "INSTRUCTIONS.md".into()
            } else {
                name
            };
            let dest = default_dst.join(dest_name);
            if ty.is_dir() {
                copy_instruction_tree(&entry.path(), &dest)?;
            } else {
                std::fs::copy(entry.path(), &dest)?;
            }
        }
    } else {
        copy_instruction_tree(
            fallback_variant
                .as_ref()
                .context("runtime-coupled skill has no default instruction source")?,
            &default_dst,
        )?;
    }

    for variant in &["codex", "kimi", "reasonix"] {
        let variant_src = src.join(variant);
        if variant_src.is_dir() {
            copy_instruction_tree(&variant_src, &runtime_root.join(variant))?;
        }
    }
    Ok(())
}
