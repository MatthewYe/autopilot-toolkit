//! Distill CLI artifact build support.
//!
//! Cross-compiles the Distill CLI binary for release platforms
//! and stages them for inclusion in the release tarball.

use std::collections::BTreeMap;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Context;

#[derive(Clone, Copy)]
pub struct DistillTarget {
    pub platform: &'static str,
    pub rust_target: &'static str,
    pub cargo_linker_env: Option<&'static str>,
    pub linker_override_env: Option<&'static str>,
}

pub const DISTILL_TARGETS: &[DistillTarget] = &[
    DistillTarget {
        platform: "darwin-arm64",
        rust_target: "aarch64-apple-darwin",
        cargo_linker_env: None,
        linker_override_env: None,
    },
    DistillTarget {
        platform: "linux-arm64",
        rust_target: "aarch64-unknown-linux-musl",
        cargo_linker_env: Some("CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER"),
        linker_override_env: Some("DISTILL_LINUX_ARM64_LINKER"),
    },
    DistillTarget {
        platform: "linux-x64",
        rust_target: "x86_64-unknown-linux-musl",
        cargo_linker_env: Some("CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER"),
        linker_override_env: Some("DISTILL_LINUX_X64_LINKER"),
    },
];

/// Check all Distill CLI artifacts exist under `dist/distill/`.
pub fn all_distill_artifacts_present(artifacts_root: &Path) -> bool {
    DISTILL_TARGETS.iter().all(|target| {
        artifacts_root
            .join(target.platform)
            .join("distill")
            .is_file()
    })
}

/// Build Distill CLI executables for all release platforms.
pub fn distill_artifacts_command(project_root: &Path) -> Result<(), anyhow::Error> {
    let artifacts_root = project_root.join("dist").join("distill");
    let cargo = env::var("DISTILL_CARGO").unwrap_or_else(|_| "cargo".to_string());
    let rustc = env::var("DISTILL_RUSTC").ok();
    let linux_linker = env::var("DISTILL_LINUX_LINKER").ok();
    let derived_linux_linker = if linux_linker.is_none() {
        derive_rust_lld(&rustc)?
    } else {
        None
    };
    for target in DISTILL_TARGETS {
        println!(
            "==> Building distill for {} ({})",
            target.platform, target.rust_target
        );
        let status = Command::new("rustup")
            .args(["target", "add", target.rust_target])
            .current_dir(project_root)
            .status()
            .with_context(|| {
                format!(
                    "rustup target add failed to start for {}",
                    target.rust_target
                )
            })?;
        if !status.success() {
            anyhow::bail!("rustup target add failed for {}", target.rust_target);
        }

        let mut command = Command::new(&cargo);
        command
            .args([
                "build",
                "--release",
                "--bin",
                "distill",
                "--target",
                target.rust_target,
            ])
            .current_dir(project_root);
        if let Some(rustc) = &rustc {
            command.env("RUSTC", rustc);
        }
        if let Some(linker_env) = target.cargo_linker_env {
            let target_linker = target
                .linker_override_env
                .and_then(|name| env::var(name).ok())
                .or_else(|| linux_linker.clone())
                .or_else(|| derived_linux_linker.clone());
            if let Some(linker) = target_linker {
                command.env(linker_env, linker);
            }
        }
        let status = command
            .status()
            .with_context(|| format!("cargo build failed to start for {}", target.rust_target))?;
        if !status.success() {
            anyhow::bail!("cargo build failed for {}", target.rust_target);
        }

        let built = project_root
            .join("target")
            .join(target.rust_target)
            .join("release")
            .join("distill");
        let dest = artifacts_root.join(target.platform).join("distill");
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(&built, &dest).with_context(|| {
            format!(
                "cannot copy Distill artifact {} -> {}",
                built.display(),
                dest.display()
            )
        })?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&dest)?.permissions();
            perms.set_mode(perms.mode() | 0o755);
            std::fs::set_permissions(&dest, perms)?;
        }
    }
    Ok(())
}

/// Stage distill executables into the autopilot staging directory for tarball inclusion.
/// Returns a map of platform → relative archive path.
pub fn stage_distill_executables(
    project_root: &Path,
    autopilot_staging: &Path,
) -> Result<BTreeMap<String, String>, anyhow::Error> {
    if !project_root
        .join("crates")
        .join("distill-cli")
        .join("Cargo.toml")
        .is_file()
    {
        return Ok(BTreeMap::new());
    }

    let artifacts_root = project_root.join("dist").join("distill");
    if !all_distill_artifacts_present(&artifacts_root) {
        for target in DISTILL_TARGETS {
            let artifact = artifacts_root.join(target.platform).join("distill");
            if !artifact.is_file() {
                anyhow::bail!(
                    "missing Distill CLI artifact for {} at {}; run `deploy.rs distill-artifacts` before `deploy.rs pack`",
                    target.platform,
                    artifact.display()
                );
            }
        }
    }

    let mut platforms = BTreeMap::new();
    for target in DISTILL_TARGETS {
        let src = artifacts_root.join(target.platform).join("distill");
        if !src.is_file() {
            anyhow::bail!(
                "missing Distill CLI artifact for {} at {}",
                target.platform,
                src.display()
            );
        }
        let rel = format!("bin/distill-artifacts/{}/distill", target.platform);
        let dest = autopilot_staging.join(&rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(&src, &dest)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&dest)?.permissions();
            perms.set_mode(perms.mode() | 0o755);
            std::fs::set_permissions(&dest, perms)?;
        }
        platforms.insert(target.platform.to_string(), rel);
    }

    Ok(platforms)
}

fn derive_rust_lld(rustc: &Option<String>) -> Result<Option<String>, anyhow::Error> {
    let rustc_bin = rustc.as_deref().unwrap_or("rustc");
    let sysroot = Command::new(rustc_bin)
        .args(["--print", "sysroot"])
        .output()
        .with_context(|| format!("{} --print sysroot failed to start", rustc_bin))?;
    if !sysroot.status.success() {
        return Ok(None);
    }
    let sysroot = String::from_utf8(sysroot.stdout)
        .context("rustc sysroot output not valid UTF-8")?
        .trim()
        .to_string();
    if sysroot.is_empty() {
        return Ok(None);
    }

    let version = Command::new(rustc_bin)
        .arg("-vV")
        .output()
        .with_context(|| format!("{} -vV failed to start", rustc_bin))?;
    if !version.status.success() {
        return Ok(None);
    }
    let version = String::from_utf8(version.stdout).context("rustc -vV output not valid UTF-8")?;
    let host = version
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .map(str::trim)
        .filter(|host| !host.is_empty());
    Ok(host.map(|host| {
        Path::new(&sysroot)
            .join("lib")
            .join("rustlib")
            .join(host)
            .join("bin")
            .join("rust-lld")
            .to_string_lossy()
            .to_string()
    }))
}
