//! Deploy tooling for autopilot-toolkit.
//!
//! Provides dev symlinking, tarball packaging, GitHub release automation,
//! and the shared `sync_path` primitive used by dev and link-principles.
//!
//! Public API:
//! - `sync_path(src, dst, kind)` — generic symlink helper (merged from sync_skill + sync_agent)
//! - `link_principles(src, principles_dir)` — ensure principles symlink
//! - `dev::dev_all(...)` — symlink all skills into agent dirs
//! - `dev::dev_clean(...)` — remove dev symlinks
//! - `pack::pack_command(project_root)` — build tarball
//! - `pack::get_version(project_root)`, `pack::get_repo_slug(project_root)` — git helpers
//! - `release::release_command(project_root)` — pack + push to GitHub Releases

pub mod dev;
pub mod distill;
pub mod pack;
pub mod release;

use std::os::unix::fs::symlink;
use std::path::Path;

use anyhow::Context;

fn warn(msg: &str) {
    eprintln!("WARNING: {}", msg);
}

/// What kind of path is being symlinked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncKind {
    /// Source must be a directory; warn and return Ok if missing.
    Dir,
    /// Source must be a file; error if missing.
    File,
}

/// Create a symlink at `dst` pointing to `src`.
///
/// - Creates parent directories as needed.
/// - Refuses to overwrite real files/directories (non-symlinks).
/// - Skips if an existing symlink already points to the right place.
/// - For `Dir`: warns and returns `Ok(())` if source doesn't exist or isn't a directory.
/// - For `File`: returns an error if source doesn't exist or isn't a file.
pub fn sync_path(src: &Path, dst: &Path, kind: SyncKind) -> Result<(), anyhow::Error> {
    // Validate source existence
    match kind {
        SyncKind::Dir => {
            if !src.is_dir() {
                warn(&format!(
                    "source directory does not exist: {}",
                    src.display()
                ));
                return Ok(());
            }
        }
        SyncKind::File => {
            if !src.is_file() {
                anyhow::bail!("source file does not exist: {}", src.display());
            }
        }
    }

    // Ensure parent directory of dst exists
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("cannot create directory {}", parent.display()))?;
    }

    // If dst exists as a real file/directory (not a symlink), refuse to overwrite
    if dst.exists() && !dst.is_symlink() {
        let kind_str = match kind {
            SyncKind::Dir => "directory",
            SyncKind::File => "file",
        };
        warn(&format!(
            "{} exists as a real {} (not a symlink) — refusing to overwrite",
            dst.display(),
            kind_str
        ));
        anyhow::bail!(
            "real {} conflict at {}",
            kind_str,
            dst.display()
        );
    }

    // If dst is a symlink, inspect its current state
    if dst.is_symlink() {
        let existing = std::fs::read_link(dst)
            .with_context(|| format!("cannot read symlink {}", dst.display()))?;

        // Valid symlink pointing to the correct source — nothing to do
        let matches = existing == src
            && match kind {
                SyncKind::Dir => src.is_dir(),
                SyncKind::File => src.is_file(),
            };
        if matches {
            return Ok(());
        }

        // Broken or pointing to the wrong target — remove it before rebuilding
        std::fs::remove_file(dst)
            .with_context(|| format!("cannot remove symlink {}", dst.display()))?;
    }

    // Create the symlink
    symlink(src, dst).with_context(|| {
        format!(
            "cannot create symlink {} -> {}",
            dst.display(),
            src.display()
        )
    })?;

    Ok(())
}

/// Ensure `~/.agents/principles` is a symlink to `src`.
pub fn link_principles(src: &Path, principles_dir: &Path) -> Result<(), anyhow::Error> {
    sync_path(src, principles_dir, SyncKind::Dir)
}

// ── Coupled skill staging ─────────────────────────────────────────────────

/// Copy a directory tree, renaming `SKILL.md` → `INSTRUCTIONS.md`.
pub fn copy_instruction_tree(src: &Path, dst: &Path) -> Result<(), anyhow::Error> {
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
pub fn skill_frontmatter(content: &str) -> Result<&str, anyhow::Error> {
    let stripped = content
        .strip_prefix("---\n")
        .or_else(|| content.strip_prefix("---"))
        .ok_or_else(|| anyhow::anyhow!("SKILL.md has no opening frontmatter delimiter"))?;
    // Bare "---" without newline is malformed — refuse to parse
    if !content.starts_with("---\n") && content.starts_with("---") {
        anyhow::bail!("SKILL.md frontmatter opening delimiter must be followed by a newline");
    }
    if stripped.contains("\n---") {
        Ok(stripped.splitn(2, "\n---").next().unwrap())
    } else {
        anyhow::bail!("SKILL.md has no closing frontmatter delimiter")
    }
}

/// Stage a coupled skill into a runtime-router layout.
///
/// Creates:
/// ```text
/// dst/
/// ├── SKILL.md              ← router (frontmatter + runtime dispatch instructions)
/// └── runtime/
///     ├── default/           ← top-level non-variant files (SKILL.md→INSTRUCTIONS.md)
///     ├── reasonix/          ← reasonix variant subtree
///     ├── codex/             ← codex variant subtree
///     └── kimi/              ← kimi variant subtree
/// ```
pub fn stage_coupled_skill(src: &Path, dst: &Path) -> Result<(), anyhow::Error> {
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
        "---\n{frontmatter}\n---\n\n# Runtime routing\n\n\
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

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix;
    use tempfile::TempDir;

    // ── sync_path Dir variant ──────────────────────────────────────────

    #[test]
    fn sync_path_dir_creates_symlink() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("myskill");
        std::fs::create_dir(&src).unwrap();
        let dst = tmp.path().join("link");

        sync_path(&src, &dst, SyncKind::Dir).unwrap();

        assert!(dst.is_symlink(), "dst should be a symlink");
        assert_eq!(std::fs::read_link(&dst).unwrap(), src);
    }

    #[test]
    fn sync_path_dir_missing_source_warns_and_ok() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("nonexistent");
        let dst = tmp.path().join("link");

        let result = sync_path(&src, &dst, SyncKind::Dir);
        assert!(result.is_ok(), "missing Dir source should return Ok");
        assert!(!dst.exists(), "no symlink should be created");
    }

    #[test]
    fn sync_path_dir_source_is_file_not_dir_warns_and_ok() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("somefile");
        std::fs::write(&src, "hello").unwrap();
        let dst = tmp.path().join("link");

        let result = sync_path(&src, &dst, SyncKind::Dir);
        // Dir variant: if src is not a directory, warns and returns Ok
        assert!(result.is_ok(), "file-as-Dir source should return Ok");
        assert!(!dst.exists(), "no symlink should be created");
    }

    // ── sync_path File variant ─────────────────────────────────────────

    #[test]
    fn sync_path_file_creates_symlink() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("myfile");
        std::fs::write(&src, "content").unwrap();
        let dst = tmp.path().join("link");

        sync_path(&src, &dst, SyncKind::File).unwrap();

        assert!(dst.is_symlink(), "dst should be a symlink");
        assert_eq!(std::fs::read_link(&dst).unwrap(), src);
    }

    #[test]
    fn sync_path_file_missing_source_is_error() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("nonexistent");
        let dst = tmp.path().join("link");

        let result = sync_path(&src, &dst, SyncKind::File);
        assert!(result.is_err(), "missing File source should be error");
        assert!(!dst.exists(), "no symlink should be created");
    }

    #[test]
    fn sync_path_file_source_is_dir_is_error() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("mydir");
        std::fs::create_dir(&src).unwrap();
        let dst = tmp.path().join("link");

        let result = sync_path(&src, &dst, SyncKind::File);
        assert!(result.is_err(), "dir-as-File source should be error");
        assert!(!dst.exists(), "no symlink should be created");
    }

    // ── Existing symlink handling ──────────────────────────────────────

    #[test]
    fn sync_path_skips_valid_symlink() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("myskill");
        std::fs::create_dir(&src).unwrap();
        let dst = tmp.path().join("link");

        // First call creates the symlink
        sync_path(&src, &dst, SyncKind::Dir).unwrap();
        // Record the inode or modification info — second call should be no-op
        let meta_before = std::fs::symlink_metadata(&dst).unwrap();

        // Second call with same args should skip
        sync_path(&src, &dst, SyncKind::Dir).unwrap();

        let meta_after = std::fs::symlink_metadata(&dst).unwrap();
        assert_eq!(meta_before.modified().unwrap(), meta_after.modified().unwrap(),
            "valid symlink should not be touched");
        assert_eq!(std::fs::read_link(&dst).unwrap(), src);
    }

    #[test]
    fn sync_path_overwrites_wrong_target_symlink() {
        let tmp = TempDir::new().unwrap();
        let src_a = tmp.path().join("skill_a");
        let src_b = tmp.path().join("skill_b");
        std::fs::create_dir(&src_a).unwrap();
        std::fs::create_dir(&src_b).unwrap();
        let dst = tmp.path().join("link");

        // Point dst at src_a
        unix::fs::symlink(&src_a, &dst).unwrap();
        assert_eq!(std::fs::read_link(&dst).unwrap(), src_a);

        // Now sync_path to src_b should overwrite
        sync_path(&src_b, &dst, SyncKind::Dir).unwrap();
        assert_eq!(std::fs::read_link(&dst).unwrap(), src_b);
    }

    #[test]
    fn sync_path_overwrites_broken_symlink() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("realskill");
        std::fs::create_dir(&src).unwrap();
        let broken_target = tmp.path().join("gone");
        let dst = tmp.path().join("link");

        // Create a symlink pointing to a nonexistent target
        unix::fs::symlink(&broken_target, &dst).unwrap();
        assert!(dst.is_symlink());

        // sync_path should remove the broken link and create a valid one
        sync_path(&src, &dst, SyncKind::Dir).unwrap();
        assert_eq!(std::fs::read_link(&dst).unwrap(), src);
    }

    // ── Real file/dir conflict ─────────────────────────────────────────

    #[test]
    fn sync_path_refuses_to_overwrite_real_dir() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("myskill");
        std::fs::create_dir(&src).unwrap();
        let dst = tmp.path().join("realdir");
        std::fs::create_dir(&dst).unwrap();

        let result = sync_path(&src, &dst, SyncKind::Dir);
        assert!(result.is_err(), "should refuse to overwrite real directory");
        assert!(dst.is_dir() && !dst.is_symlink(), "real dir should remain untouched");
    }

    #[test]
    fn sync_path_refuses_to_overwrite_real_file() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("myfile");
        std::fs::write(&src, "content").unwrap();
        let dst = tmp.path().join("realfile");
        std::fs::write(&dst, "original").unwrap();

        let result = sync_path(&src, &dst, SyncKind::File);
        assert!(result.is_err(), "should refuse to overwrite real file");
        assert!(dst.is_file() && !dst.is_symlink(), "real file should remain untouched");
    }

    // ── Parent directory creation ──────────────────────────────────────

    #[test]
    fn sync_path_creates_parent_directories() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("myskill");
        std::fs::create_dir(&src).unwrap();
        let dst = tmp.path().join("deep").join("nested").join("link");

        assert!(!dst.parent().unwrap().exists());

        sync_path(&src, &dst, SyncKind::Dir).unwrap();

        assert!(dst.is_symlink());
        assert_eq!(std::fs::read_link(&dst).unwrap(), src);
    }

    // ── skill_frontmatter ───────────────────────────────────────────────

    #[test]
    fn frontmatter_standard() {
        let fm = skill_frontmatter("---\nname: test\ndescription: x\n---\nbody").unwrap();
        assert_eq!(fm, "name: test\ndescription: x");
    }

    #[test]
    fn frontmatter_no_closing_delimiter_is_error() {
        assert!(skill_frontmatter("---\nname: test\n").is_err());
    }

    #[test]
    fn frontmatter_no_opening_delimiter_is_error() {
        assert!(skill_frontmatter("name: test\n---\n").is_err());
    }

    #[test]
    fn frontmatter_bare_dashes_without_newline_is_error() {
        assert!(skill_frontmatter("---name: test\n---\n").is_err());
    }

    #[test]
    fn frontmatter_triple_dash_in_body_not_misparsed() {
        let fm = skill_frontmatter("---\nkey: value\n---\nbody with --- inside\n").unwrap();
        assert_eq!(fm, "key: value");
    }

    // ── stage_coupled_skill ─────────────────────────────────────────────

    #[test]
    fn stage_coupled_skill_creates_router_layout() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("myskill");
        std::fs::create_dir_all(src.join("reasonix")).unwrap();
        std::fs::write(src.join("SKILL.md"), "---\nname: myskill\ndescription: test\n---\nfallback\n").unwrap();
        std::fs::write(src.join("reasonix").join("SKILL.md"), "---\nname: myskill\ndescription: reasonix\n---\nreasonix body\n").unwrap();
        std::fs::create_dir_all(src.join("codex")).unwrap();
        std::fs::write(src.join("codex").join("SKILL.md"), "---\nname: myskill\ndescription: codex\n---\ncodex body\n").unwrap();

        let dst = tmp.path().join("staged");
        stage_coupled_skill(&src, &dst).unwrap();

        // Router SKILL.md at root
        let router = std::fs::read_to_string(dst.join("SKILL.md")).unwrap();
        assert!(router.starts_with("---\n"), "router must start with YAML frontmatter delimiter");
        assert!(router.contains("name: myskill"));
        assert!(router.contains("\n---\n"), "router must have closing frontmatter delimiter");
        assert!(router.contains("Runtime routing"));

        // runtime/reasonix/INSTRUCTIONS.md
        let reasonix_instructions = std::fs::read_to_string(
            dst.join("runtime").join("reasonix").join("INSTRUCTIONS.md"),
        ).unwrap();
        assert!(reasonix_instructions.contains("reasonix body"));

        // runtime/default/INSTRUCTIONS.md (from top-level non-variant files)
        let default_instructions = std::fs::read_to_string(
            dst.join("runtime").join("default").join("INSTRUCTIONS.md"),
        ).unwrap();
        assert!(default_instructions.contains("fallback"));
    }

    #[test]
    fn stage_coupled_skill_no_top_level_skill_md() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("myskill");
        std::fs::create_dir_all(src.join("reasonix")).unwrap();
        std::fs::write(src.join("reasonix").join("SKILL.md"), "---\nname: myskill\n---\nbody\n").unwrap();

        let dst = tmp.path().join("staged");
        stage_coupled_skill(&src, &dst).unwrap();

        // fallback to reasonix variant
        let default_instructions = std::fs::read_to_string(
            dst.join("runtime").join("default").join("INSTRUCTIONS.md"),
        ).unwrap();
        assert!(default_instructions.contains("body"));
    }
}
