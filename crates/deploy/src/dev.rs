//! Dev symlink management: `dev` and `dev-clean` subcommands.
//!
//! `dev_all` creates runtime-routed symlinks for coupled skills and direct
//! symlinks for agnostic/upstream skills into the agent runtime directories.
//! `dev_clean` removes all symlinks that point back into the project tree.

use std::path::Path;

use skill_index::{discover_skills, ResolutionStatus, SkillType};

use super::{stage_coupled_skill, sync_path, SyncKind};

/// Symlink all skills from the source tree into agent runtime directories.
pub fn dev_all(
    project_root: &Path,
    shared_skills_dir: &Path,
    reasonix_skills_dir: &Path,
    codex_skills_dir: &Path,
    codex_agents_dir: &Path,
) -> Result<(), anyhow::Error> {
    println!("==> Syncing all skills from source tree...");

    let entries = discover_skills(project_root)?;

    let mut count = 0u32;
    for entry in &entries {
        if let ResolutionStatus::Missing { reason } = &entry.resolution {
            eprintln!(
                "WARNING: {} skill '{}' source dir missing, skipping: {}",
                entry.source, entry.name, reason
            );
            continue;
        }

        // The summary counter preserves the historical arithmetic: one per
        // entry, plus one more for a coupled entry that ships a codex agent.
        count += sync_expected_entry(
            entry,
            project_root,
            shared_skills_dir,
            reasonix_skills_dir,
            codex_skills_dir,
            codex_agents_dir,
        )?;
        count += 1;
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

/// Provision one resolved Expected-set entry.
///
/// Runtime-coupled skills are staged into the router layout first; agnostic
/// and upstream skills are symlinked directly from the source tree. Returns 1
/// when the entry also ships a Codex `agent.toml` (linked here), 0 otherwise.
fn sync_expected_entry(
    entry: &skill_index::ExpectedSetEntry,
    project_root: &Path,
    shared_skills_dir: &Path,
    reasonix_skills_dir: &Path,
    codex_skills_dir: &Path,
    codex_agents_dir: &Path,
) -> Result<u32, anyhow::Error> {
    let name = &entry.name;
    let src_dir = &entry.source_dir;
    let mut extra = 0;

    if entry.skill_type == SkillType::Coupled {
        let dev_staging = project_root.join("dist").join("dev-skills").join(name);
        stage_coupled_skill(src_dir, &dev_staging)?;
        sync_path(&dev_staging, &shared_skills_dir.join(name), SyncKind::Dir)?;

        remove_project_symlink(&reasonix_skills_dir.join(name), project_root)?;
        remove_project_symlink(&codex_skills_dir.join(name), project_root)?;

        if entry.codex_agent {
            let agent_src = src_dir.join("codex").join("agent.toml");
            sync_path(
                &agent_src,
                &codex_agents_dir.join(format!("{}.toml", name)),
                SyncKind::File,
            )?;
            extra += 1;
        }
    } else {
        sync_path(src_dir, &shared_skills_dir.join(name), SyncKind::Dir)?;
    }

    Ok(extra)
}

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

// ── Tests ────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// All dev output locations, rooted in `tmp` so tests never touch `$HOME`.
    struct Dirs {
        shared: PathBuf,
        reasonix: PathBuf,
        codex: PathBuf,
        codex_agents: PathBuf,
    }

    fn dirs_under(tmp: &Path) -> Dirs {
        Dirs {
            shared: tmp.join("home/.agents/skills"),
            reasonix: tmp.join("home/.reasonix/skills"),
            codex: tmp.join("home/.codex/skills"),
            codex_agents: tmp.join("home/.codex/agents"),
        }
    }

    fn write_skill_md(dir: &Path) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("SKILL.md"), "---\nname: test\n---\n").unwrap();
    }

    fn write_vendor_lock(root: &Path, entries: &[(&str, &str)]) {
        let skills = entries
            .iter()
            .map(|(name, vendor_path)| {
                format!(
                    r#""{name}": {{"sourceType": "github", "skillPath": "plugins/{name}/skills/{name}/SKILL.md", "skillFolderHash": "abc123", "vendorPath": "{vendor_path}"}}"#
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        std::fs::write(
            root.join(".vendor-lock.json"),
            format!(r#"{{"version": 1, "skills": {{{skills}}}}}"#),
        )
        .unwrap();
    }

    fn write_upstream_lock(root: &Path, entries: &[(&str, &str)]) {
        let skills = entries
            .iter()
            .map(|(name, skill_path)| {
                format!(
                    r#""{name}": {{"sourceType": "github", "skillPath": "{skill_path}", "skillFolderHash": "abc123"}}"#
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        std::fs::write(
            root.join(".skill-lock.json"),
            format!(r#"{{"version": 4, "skills": {{{skills}}}}}"#),
        )
        .unwrap();
    }

    /// Fixture project: agnostic and coupled-with-codex-agent autopilot skills,
    /// one resolved and one missing vendor skill, and one resolved and one
    /// missing upstream skill.
    fn write_fixture_project(root: &Path) {
        // Autopilot: agnostic.
        let alpha = root.join("skills/autopilot/alpha");
        write_skill_md(&alpha);

        // Autopilot: coupled (reasonix + codex variant, codex agent.toml).
        let beta = root.join("skills/autopilot/beta");
        write_skill_md(&beta);
        write_skill_md(&beta.join("reasonix"));
        write_skill_md(&beta.join("codex"));
        std::fs::write(beta.join("codex/agent.toml"), "[agent]\nname = \"beta\"\n").unwrap();

        // Vendor: one entry resolves, one points at a missing directory.
        write_skill_md(&root.join("skills/vendor/show-me"));
        write_vendor_lock(
            root,
            &[
                ("show-me", "skills/vendor/show-me"),
                ("ghost", "skills/vendor/ghost"),
            ],
        );

        // Upstream: one entry resolves, one points at a missing directory.
        write_skill_md(&root.join("skills/upstream/skills/engineering/tdd"));
        write_upstream_lock(
            root,
            &[
                ("tdd", "skills/engineering/tdd/SKILL.md"),
                ("cut-off", "skills/engineering/cut-off/SKILL.md"),
            ],
        );
    }

    #[test]
    fn dev_links_resolved_entries_and_warns_on_missing_ones() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("project");
        write_fixture_project(&root);
        let dirs = dirs_under(tmp.path());

        let result = dev_all(
            &root,
            &dirs.shared,
            &dirs.reasonix,
            &dirs.codex,
            &dirs.codex_agents,
        );
        assert!(
            result.is_ok(),
            "dev must stay usable on a partial checkout: {:?}",
            result
        );

        // Agnostic autopilot skill: direct link to the source directory.
        let alpha = dirs.shared.join("alpha");
        assert!(alpha.is_symlink());
        assert_eq!(
            std::fs::read_link(&alpha).unwrap(),
            root.join("skills/autopilot/alpha")
        );

        // Coupled autopilot skill: staged into the router layout, shared dir
        // links to the staged tree (never directly into a runtime skill dir).
        let beta = dirs.shared.join("beta");
        assert!(beta.is_symlink());
        assert_eq!(
            std::fs::read_link(&beta).unwrap(),
            root.join("dist/dev-skills/beta")
        );
        assert!(beta.join("SKILL.md").is_file(), "router SKILL.md missing");
        assert!(beta.join("runtime/codex/INSTRUCTIONS.md").is_file());
        assert!(beta.join("runtime/reasonix/INSTRUCTIONS.md").is_file());
        assert!(!dirs.reasonix.join("beta").exists());
        assert!(!dirs.codex.join("beta").exists());

        // Codex agent linked from the source tree.
        let agent = dirs.codex_agents.join("beta.toml");
        assert!(agent.is_symlink());
        assert_eq!(
            std::fs::read_link(&agent).unwrap(),
            root.join("skills/autopilot/beta/codex/agent.toml")
        );

        // Resolved vendor and upstream skills are linked.
        assert!(dirs.shared.join("show-me").is_symlink());
        assert!(dirs.shared.join("tdd").is_symlink());

        // Failed entries: no links, and the run still succeeds.
        assert!(!dirs.shared.join("ghost").exists());
        assert!(!dirs.shared.join("cut-off").exists());
    }

    #[test]
    fn dev_warns_for_missing_entries_and_warns_on_stale_runtime_links() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("project");
        write_fixture_project(&root);
        let dirs = dirs_under(tmp.path());

        // Simulate a checkout laid out by an older dev: coupled skills linked
        // directly into the runtime skill directories.
        std::fs::create_dir_all(&dirs.reasonix).unwrap();
        std::fs::create_dir_all(&dirs.codex).unwrap();
        std::os::unix::fs::symlink(
            root.join("skills/autopilot/beta"),
            dirs.reasonix.join("beta"),
        )
        .unwrap();
        std::os::unix::fs::symlink(root.join("skills/autopilot/beta"), dirs.codex.join("beta"))
            .unwrap();

        let result = dev_all(
            &root,
            &dirs.shared,
            &dirs.reasonix,
            &dirs.codex,
            &dirs.codex_agents,
        );
        assert!(result.is_ok(), "run should still succeed: {:?}", result);

        assert!(
            !dirs.reasonix.join("beta").exists(),
            "stale reasonix link should be removed"
        );
        assert!(
            !dirs.codex.join("beta").exists(),
            "stale codex link should be removed"
        );
    }

    #[test]
    fn dev_errors_on_a_malformed_lock() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("project");
        write_fixture_project(&root);
        std::fs::write(root.join(".skill-lock.json"), "{ not json").unwrap();
        let dirs = dirs_under(tmp.path());

        let result = dev_all(
            &root,
            &dirs.shared,
            &dirs.reasonix,
            &dirs.codex,
            &dirs.codex_agents,
        );
        let error = result.expect_err("a malformed lock is a hard error");
        assert!(
            error.to_string().contains(".skill-lock.json"),
            "error should name the malformed lock: {error}"
        );
    }
}
