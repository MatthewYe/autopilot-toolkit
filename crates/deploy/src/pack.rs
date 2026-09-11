//! Tarball packaging: `pack` subcommand.
//!
//! Builds a self-contained `autopilot-toolkit.tar.gz` tarball in `dist/`
//! from the source tree and generates `dist/install.sh`. Tarball contents
//! and `manifest.json` come from one Expected-set enumeration, so the
//! shipped skill directories and the ownership manifest cannot disagree.

use std::path::Path;

use super::distill::stage_distill_executables;
use super::stage_coupled_skill;
use anyhow::Context;
use skill_index::{discover_skills, ExpectedSetEntry, ResolutionStatus, SkillType};

/// Build a self-contained tarball into `dist/`.
///
/// A lock entry whose source directory is missing fails the pack (unlike
/// `dev`, which warns and skips): the tarball would otherwise silently
/// omit an expected skill.
pub fn pack_command(project_root: &Path) -> Result<(), anyhow::Error> {
    // ── one Expected-set enumeration drives staging and manifest ──
    let entries = discover_skills(project_root)?;
    reject_failed_entries(&entries)?;

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

    // ── stage every expected skill (file copy) ──
    for entry in &entries {
        stage_entry_skill(entry, &skills_staging.join(&entry.name))?;
    }

    // ── generate manifest.json from the same enumeration ──
    let manifest = skill_index::generate_manifest(&entries, &version);
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
    let lock_path = project_root.join(".skill-lock.json");
    if lock_path.is_file() {
        std::fs::copy(&lock_path, autopilot_staging.join(".skill-lock.json"))?;
    }

    // ── copy .vendor-lock.json ──
    let vendor_lock_path = project_root.join(shared::VENDOR_LOCK_FILE);
    if vendor_lock_path.is_file() {
        std::fs::copy(
            &vendor_lock_path,
            autopilot_staging.join(".vendor-lock.json"),
        )?;
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

/// Refuse to pack while the Expected set contains failed entries (CONTEXT.md).
///
/// `pack` is strict where `dev` is lenient: a tarball that silently omits a
/// locked skill would ship a manifest that disagrees with the sources, so the
/// pack fails and names every failed entry instead.
fn reject_failed_entries(entries: &[ExpectedSetEntry]) -> Result<(), anyhow::Error> {
    let failed: Vec<String> = entries
        .iter()
        .filter_map(|entry| match &entry.resolution {
            ResolutionStatus::Missing { reason } => Some(format!(
                "'{}' ({} skill at {}: {})",
                entry.name,
                entry.source,
                entry.source_dir.display(),
                reason
            )),
            ResolutionStatus::Resolved => None,
        })
        .collect();
    if failed.is_empty() {
        return Ok(());
    }
    anyhow::bail!(
        "cannot pack: expected skill(s) missing:\n  {}",
        failed.join("\n  ")
    )
}

/// Stage one resolved Expected-set entry into the tarball.
///
/// Autopilot and vendor skills keep their per-source staging behavior: a
/// coupled skill gets the runtime-router layout, an agnostic one is copied
/// as-is. Upstream skills are agnostic by policy and are copied directly,
/// exactly as the tarball shipped them before.
fn stage_entry_skill(entry: &ExpectedSetEntry, dst: &Path) -> Result<(), anyhow::Error> {
    // Upstream entries cannot reach this arm as coupled: the enumerator
    // constructs them as `SkillType::Agnostic` (agnostic by policy), so no
    // source check is needed here.
    let coupled = entry.skill_type == SkillType::Coupled;
    if coupled {
        stage_coupled_skill(&entry.source_dir, dst)
    } else {
        copy_dir_all(&entry.source_dir, dst)
    }
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

    // ── pack_command: Expected-set failure semantics ───────────────────

    /// Write a minimal packable project: one autopilot skill plus the
    /// install template. The fixture is a git repository so `get_version`
    /// works regardless of where pack checks for unresolved entries.
    fn write_minimal_project(root: &std::path::Path) {
        std::fs::create_dir_all(root.join("skills/autopilot/alpha")).unwrap();
        std::fs::write(
            root.join("skills/autopilot/alpha/SKILL.md"),
            "---\nname: alpha\ndescription: test\n---\n",
        )
        .unwrap();
        std::fs::create_dir_all(root.join("templates")).unwrap();
        std::fs::write(
            root.join("templates/install.sh.in"),
            "#!/bin/bash\nVERSION=\"__VERSION__\"\n",
        )
        .unwrap();
        for args in [
            vec!["init", "-q"],
            vec!["config", "user.email", "test@test.com"],
            vec!["config", "user.name", "Test"],
            vec!["commit", "-q", "--allow-empty", "-m", "init"],
        ] {
            let status = std::process::Command::new("git")
                .args(&args)
                .current_dir(root)
                .status()
                .expect("git should run");
            assert!(status.success(), "git {:?} should succeed", args);
        }
    }

    #[test]
    fn pack_fails_on_missing_vendor_entry() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_minimal_project(root);
        std::fs::write(
            root.join(".vendor-lock.json"),
            r#"{"version":1,"skills":{"missing-vendor":{"skillPath":"plugins/missing-vendor/skills/missing-vendor/SKILL.md","skillFolderHash":"deadbeef","vendorPath":"skills/vendor/missing-vendor"}}}"#,
        )
        .unwrap();

        let err = pack_command(root).expect_err("pack must fail on a failed entry");
        let msg = format!("{err:?}");
        assert!(
            msg.contains("missing-vendor"),
            "error must name the missing skill: {msg}"
        );
        assert!(
            !root.join("dist/autopilot-toolkit.tar.gz").exists(),
            "pack must not produce a tarball when an expected skill is missing"
        );
    }

    #[test]
    fn pack_fails_on_malformed_upstream_lock() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_minimal_project(root);
        std::fs::write(root.join(".skill-lock.json"), "{ not valid json").unwrap();

        let err = pack_command(root).expect_err("pack must fail on a malformed lock");
        let msg = format!("{err:?}");
        assert!(
            msg.contains(".skill-lock.json"),
            "error must name the malformed lock file: {msg}"
        );
    }

    #[test]
    fn pack_manifest_names_match_staged_directories() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_minimal_project(root);

        std::fs::create_dir_all(root.join("skills/vendor/gamma")).unwrap();
        std::fs::write(
            root.join("skills/vendor/gamma/SKILL.md"),
            "---\nname: gamma\ndescription: test\n---\n",
        )
        .unwrap();
        std::fs::write(
            root.join(".vendor-lock.json"),
            r#"{"version":1,"skills":{"gamma":{"skillPath":"plugins/gamma/skills/gamma/SKILL.md","skillFolderHash":"abc","vendorPath":"skills/vendor/gamma"}}}"#,
        )
        .unwrap();

        std::fs::create_dir_all(root.join("skills/upstream/skills/engineering/beta")).unwrap();
        std::fs::write(
            root.join("skills/upstream/skills/engineering/beta/SKILL.md"),
            "---\nname: beta\ndescription: test\n---\n",
        )
        .unwrap();
        std::fs::write(
            root.join(".skill-lock.json"),
            r#"{"version":4,"skills":{"beta":{"skillPath":"skills/engineering/beta/SKILL.md","skillFolderHash":"def"}}}"#,
        )
        .unwrap();

        pack_command(root).expect("pack should succeed on a fully resolved fixture");

        let extracted = extract_pack(root);
        let staged_names = staged_skill_names(&extracted);
        assert_eq!(staged_names, vec!["alpha", "beta", "gamma"]);
        assert_eq!(
            manifest_skill_names(&extracted),
            staged_names,
            "manifest names must equal staged skill directories"
        );
    }

    #[test]
    fn pack_succeeds_without_lock_files() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_minimal_project(root);

        pack_command(root).expect("pack should succeed without any lock file");

        let extracted = extract_pack(root);
        assert_eq!(
            manifest_skill_names(&extracted),
            vec!["alpha"],
            "a missing lock must not invent upstream skills"
        );
        assert!(
            !extracted.join(".autopilot/.skill-lock.json").exists(),
            "no lock file should be staged when the project has none"
        );
    }

    /// Extract the tarball `pack_command` produced into `<root>/extracted`.
    fn extract_pack(root: &std::path::Path) -> std::path::PathBuf {
        let extracted = root.join("extracted");
        std::fs::create_dir_all(&extracted).unwrap();
        let status = std::process::Command::new("tar")
            .args([
                "-xzf",
                &root.join("dist/autopilot-toolkit.tar.gz").to_string_lossy(),
                "-C",
                &extracted.to_string_lossy(),
            ])
            .status()
            .expect("tar should run");
        assert!(status.success(), "tar extract should succeed");
        extracted
    }

    /// Sorted skill names from the extracted `manifest.json`.
    fn manifest_skill_names(extracted: &std::path::Path) -> Vec<String> {
        let manifest: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(extracted.join(".autopilot/manifest.json")).unwrap(),
        )
        .unwrap();
        let mut names: Vec<String> = manifest["skills"]
            .as_object()
            .expect("manifest.skills should be an object")
            .keys()
            .cloned()
            .collect();
        names.sort();
        names
    }

    /// Sorted directory names staged under the extracted `skills/`.
    fn staged_skill_names(extracted: &std::path::Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(extracted.join("skills"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        names.sort();
        names
    }
}
