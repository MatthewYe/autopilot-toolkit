//! Tarball packaging: `pack` subcommand.
//!
//! Builds a self-contained `autopilot-toolkit.tar.gz` tarball in `dist/`
//! from the source tree and generates `dist/install.sh`.

use std::path::Path;

use super::distill::stage_distill_executables;
use super::stage_coupled_skill;
use skill_index::{classify_skill, SkillType};
use anyhow::Context;

/// Build a self-contained tarball into `dist/`.
pub fn pack_command(project_root: &Path) -> Result<(), anyhow::Error> {
    let version = get_version(project_root)?;
    let dist_dir = project_root.join("dist");
    std::fs::create_dir_all(&dist_dir)
        .with_context(|| format!("cannot create dist directory {}", dist_dir.display()))?;

    // Create staging directory for tarball contents
    let staging = dist_dir.join("staging");
    if staging.exists() {
        std::fs::remove_dir_all(&staging)?;
    }
    std::fs::create_dir_all(&staging)?;

    let skills_staging = staging.join("skills");
    std::fs::create_dir_all(&skills_staging)?;

    let autopilot_staging = staging.join(".autopilot");
    std::fs::create_dir_all(&autopilot_staging)?;

    // ── scan autopilot skills (file copy) ──
    let autopilot_dir = project_root.join("skills").join("autopilot");
    if autopilot_dir.is_dir() {
        for entry in std::fs::read_dir(&autopilot_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let skill_name = entry.file_name();
            let skill_name_str = skill_name.to_string_lossy().to_string();
            let src_dir = entry.path();
            let (skill_type, _variants, _codex_agent) = classify_skill(&src_dir);
            if skill_type == SkillType::Coupled {
                stage_coupled_skill(&src_dir, &skills_staging.join(&skill_name_str))?;
            } else {
                copy_dir_all(&src_dir, &skills_staging.join(&skill_name_str))?;
            }
        }
    }

    // ── scan upstream skills (file copy, uses shared::load_skill_lock for paths) ──
    let lock_path = project_root.join(".skill-lock.json");
    if lock_path.is_file() {
        match shared::load_skill_lock() {
            Ok(lock) => {
                for skill in &lock.skills {
                    let src_parent =
                        std::path::Path::new(&skill.skill_path).parent().unwrap_or(std::path::Path::new(""));
                    let src_dir = project_root
                        .join("skills")
                        .join("upstream")
                        .join(src_parent);
                    if src_dir.is_dir() {
                        copy_dir_all(&src_dir, &skills_staging.join(&skill.name))?;
                    } else {
                        eprintln!(
                            "WARNING: upstream skill '{}' source dir missing ({}), skipping",
                            skill.name,
                            src_dir.display()
                        );
                    }
                }
            }
            Err(e) => {
                eprintln!("WARNING: failed to load .skill-lock.json for upstream copy: {}", e);
            }
        }
    }

    // ── generate manifest.json (via skill-index lib) ──
    let skills = skill_index::discover_skills(project_root)?;
    let manifest = skill_index::generate_manifest(&skills, &version);
    let mut manifest_value = serde_json::to_value(manifest)?;
    let distill_platforms = stage_distill_executables(project_root, &autopilot_staging)?;
    if !distill_platforms.is_empty() {
        manifest_value
            .as_object_mut()
            .context("manifest must be an object")?
            .insert(
                "executables".to_string(),
                serde_json::json!({
                    "distill": {
                        "platforms": distill_platforms,
                    }
                }),
            );
    }
    let manifest_json = serde_json::to_string_pretty(&manifest_value)?;
    std::fs::write(autopilot_staging.join("manifest.json"), &manifest_json)?;

    // ── write .version ──
    std::fs::write(autopilot_staging.join(".version"), &version)?;

    // ── copy .skill-lock.json ──
    if lock_path.is_file() {
        std::fs::copy(&lock_path, autopilot_staging.join(".skill-lock.json"))?;
    }

    // ── generate install.sh from template ──
    let template_path = project_root.join("templates").join("install.sh.in");
    let template_content = std::fs::read_to_string(&template_path)
        .with_context(|| format!("template not found at {}", template_path.display()))?;
    let repo_url = get_repo_slug(project_root)?;
    let install_content = template_content
        .replace("__VERSION__", &version)
        .replace("__REPO_URL__", &format!("https://github.com/{}", repo_url));

    // ── copy bootstrap.sh ──
    let bootstrap_src = project_root.join("bootstrap.sh");
    if bootstrap_src.is_file() {
        std::fs::copy(&bootstrap_src, autopilot_staging.join("bootstrap.sh"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms =
                std::fs::metadata(autopilot_staging.join("bootstrap.sh"))?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(autopilot_staging.join("bootstrap.sh"), perms)?;
        }
    }

    // ── copy uninstall.sh ──
    let uninstall_src = project_root.join("templates").join("uninstall.sh");
    if uninstall_src.is_file() {
        std::fs::copy(&uninstall_src, autopilot_staging.join("uninstall.sh"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms =
                std::fs::metadata(autopilot_staging.join("uninstall.sh"))?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(autopilot_staging.join("uninstall.sh"), perms)?;
        }
    }

    // ── copy principles/ ──
    let principles_src = project_root.join("principles");
    if principles_src.is_dir() {
        copy_dir_all(&principles_src, &staging.join("principles"))?;
    }

    // ── create tarball ──
    let tarball_name = "autopilot-toolkit.tar.gz".to_string();
    let tarball_path = dist_dir.join(&tarball_name);

    let status = std::process::Command::new("tar")
        .args([
            "-czf",
            &tarball_path.to_string_lossy(),
            "-C",
            &staging.to_string_lossy(),
            ".",
        ])
        .status()
        .context("tar command failed — is tar installed?")?;

    if !status.success() {
        anyhow::bail!("tar exited with error");
    }

    // Also save install.sh as standalone file in dist/ for curl | bash
    let install_sh_path = dist_dir.join("install.sh");
    std::fs::write(&install_sh_path, &install_content)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&install_sh_path)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&install_sh_path, perms)?;
    }

    // Clean up staging
    std::fs::remove_dir_all(&staging)?;

    println!("Built: {}", tarball_path.display());
    println!("Install script: {}", install_sh_path.display());
    Ok(())
}

/// Recursively copy a directory tree.
fn copy_dir_all(src: &Path, dst: &Path) -> Result<(), anyhow::Error> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dest = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dest)?;
        } else {
            std::fs::copy(entry.path(), &dest)?;
        }
    }
    Ok(())
}

/// Get the current git commit hash.
pub fn get_version(project_root: &Path) -> Result<String, anyhow::Error> {
    let output = std::process::Command::new("git")
        .args(["-C", &project_root.to_string_lossy(), "rev-parse", "HEAD"])
        .output()
        .context("git rev-parse HEAD failed — are you in a git repository?")?;
    if !output.status.success() {
        anyhow::bail!("git rev-parse HEAD exited with error");
    }
    Ok(String::from_utf8(output.stdout)
        .context("git output not valid UTF-8")?
        .trim()
        .to_string())
}

/// Extract the GitHub `owner/repo` slug from the origin remote.
pub fn get_repo_slug(project_root: &Path) -> Result<String, anyhow::Error> {
    let output = std::process::Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(project_root)
        .output()
        .context("git remote get-url failed")?;

    if !output.status.success() {
        return Ok("unknown/autopilot-toolkit".to_string());
    }

    let remote_url = String::from_utf8(output.stdout)
        .context("invalid UTF-8")?
        .trim()
        .to_string();

    if remote_url.is_empty() {
        return Ok("unknown/autopilot-toolkit".to_string());
    }

    if let Some(cap) = remote_url.strip_prefix("https://github.com/") {
        Ok(cap.trim_end_matches(".git").to_string())
    } else if let Some(cap) = remote_url.strip_prefix("git@github.com:") {
        Ok(cap.trim_end_matches(".git").to_string())
    } else {
        anyhow::bail!("cannot parse GitHub repo from remote: {}", remote_url)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn project_root() -> std::path::PathBuf {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        // crates/deploy -> crates -> workspace root
        manifest.parent().unwrap().parent().unwrap().to_path_buf()
    }

    #[test]
    fn get_version_returns_hex_hash() {
        let root = project_root();
        let v = get_version(&root).expect("get_version should succeed in git repo");
        assert!(!v.is_empty(), "version should not be empty");
        assert!(
            v.chars().all(|c| c.is_ascii_hexdigit()),
            "version should be a hex hash, got: {}",
            v
        );
        assert_eq!(v.len(), 40, "git hash should be 40 hex chars");
    }

    #[test]
    fn get_repo_slug_contains_slash() {
        let root = project_root();
        let slug = get_repo_slug(&root).expect("get_repo_slug should succeed");
        assert!(
            slug.contains('/'),
            "repo slug should be owner/repo format, got: {}",
            slug
        );
        assert!(!slug.ends_with(".git"), "slug should strip .git suffix");
    }

    #[test]
    fn get_repo_slug_strips_dot_git() {
        let root = project_root();
        let slug = get_repo_slug(&root).unwrap();
        // https://github.com/owner/repo.git → owner/repo
        // git@github.com:owner/repo.git → owner/repo
        assert!(!slug.ends_with(".git"));
    }
}
