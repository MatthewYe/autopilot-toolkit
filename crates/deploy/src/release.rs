//! GitHub release automation: `release` subcommand.
//!
//! Packs a tarball and pushes it to GitHub Releases via the `gh` CLI.

use std::path::Path;

use super::pack::{get_repo_slug, get_version, pack_command};

/// Pack + push to GitHub Releases.
pub fn release_command(project_root: &Path) -> Result<(), anyhow::Error> {
    // Check gh is available
    if !std::process::Command::new("which")
        .arg("gh")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        anyhow::bail!("gh CLI not found — install it from https://cli.github.com");
    }

    // Use short hash as tag — no manual tagging needed
    let hash = get_version(project_root)?;
    let short = &hash[..8.min(hash.len())];
    let tag = format!("v-{}", short);
    let repo_slug = get_repo_slug(project_root)?;

    // Skip if release already exists
    if std::process::Command::new("gh")
        .args(["release", "view", &tag, "-R", &repo_slug])
        .current_dir(project_root)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
    {
        println!("==> Release {} already exists, skipping.", tag);
        return Ok(());
    }

    println!("==> Releasing {} to {}", tag, repo_slug);
    pack_command(project_root)?;

    let tarball = project_root.join("dist").join("autopilot-toolkit.tar.gz");
    let install_script = project_root.join("dist").join("install.sh");
    if !tarball.is_file() {
        anyhow::bail!("tarball not found at {}", tarball.display());
    }

    // Create and push lightweight tag
    for args in &[vec!["tag", "-f", &tag], vec!["push", "origin", &tag]] {
        let s = std::process::Command::new("git")
            .args(args)
            .current_dir(project_root)
            .status()?;
        if !s.success() {
            anyhow::bail!("git {:?} failed", args);
        }
    }

    // Create GitHub Release
    let status = std::process::Command::new("gh")
        .args(["release", "create", &tag,
            tarball.to_str().unwrap(), install_script.to_str().unwrap(),
            project_root.join("templates").join("uninstall.sh").to_str().unwrap(),
            "-R", &repo_slug,
            "--title", &format!("autopilot-toolkit {}", short),
            "--notes", &format!("Commit: {}\n\nInstall:\n```\ncurl -sSL https://github.com/{}/releases/download/{}/install.sh | bash\n```", short, repo_slug, tag),
        ])
        .current_dir(project_root).status()?;
    if !status.success() {
        anyhow::bail!("gh release create failed");
    }

    println!("==> Released {}", tag);
    println!(
        "   curl -sSL https://github.com/{}/releases/latest/download/install.sh | bash",
        repo_slug
    );
    Ok(())
}
