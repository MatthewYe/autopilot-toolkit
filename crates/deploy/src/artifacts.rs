//! Release CLI artifact build support, shared by every shipped CLI.
//!
//! Each shipped CLI (`distill`, `director`) is cross-compiled for the release
//! platforms and staged into the tarball by one code path: a [`CliSpec`] names
//! the binary, its crate, the `dist/<name>/` artifact tree and the environment
//! prefix its linker overrides use; the rest is the same walk over
//! [`RELEASE_TARGETS`] for both.

use std::collections::BTreeMap;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Context;

/// One release platform: the harness name, the Rust target triple, and the
/// linker environment the build needs for cross-compiled Linux targets.
#[derive(Debug, Clone, Copy)]
pub struct ReleaseTarget {
    pub platform: &'static str,
    pub rust_target: &'static str,
    pub cargo_linker_env: Option<&'static str>,
}

pub const RELEASE_TARGETS: &[ReleaseTarget] = &[
    ReleaseTarget {
        platform: "darwin-arm64",
        rust_target: "aarch64-apple-darwin",
        cargo_linker_env: None,
    },
    ReleaseTarget {
        platform: "linux-arm64",
        rust_target: "aarch64-unknown-linux-musl",
        cargo_linker_env: Some("CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER"),
    },
    ReleaseTarget {
        platform: "linux-x64",
        rust_target: "x86_64-unknown-linux-musl",
        cargo_linker_env: Some("CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER"),
    },
];

/// Everything that differs between the shipped CLIs.
#[derive(Debug, Clone, Copy)]
pub struct CliSpec {
    /// Binary name, as built by cargo and installed in the tarball.
    pub name: &'static str,
    /// Workspace crate that provides the binary.
    pub crate_name: &'static str,
    /// `dist/<dir>/<platform>/<name>` artifact tree.
    pub dist_subdir: &'static str,
    /// Human label for progress output.
    pub label: &'static str,
    /// Prefix for the CLI's build overrides (`<PREFIX>_CARGO`, `_RUSTC`,
    /// `_LINUX_LINKER`).
    pub env_prefix: &'static str,
    /// Per-target linker override names, e.g. `<PREFIX>_LINUX_ARM64_LINKER`.
    pub linux_arm64_linker_env: &'static str,
    pub linux_x64_linker_env: &'static str,
}

pub const CLIS: &[CliSpec] = &[
    CliSpec {
        name: "distill",
        crate_name: "distill-cli",
        dist_subdir: "distill",
        label: "Distill CLI",
        env_prefix: "DISTILL",
        linux_arm64_linker_env: "DISTILL_LINUX_ARM64_LINKER",
        linux_x64_linker_env: "DISTILL_LINUX_X64_LINKER",
    },
    CliSpec {
        name: "director",
        crate_name: "director-cli",
        dist_subdir: "director",
        label: "Director CLI",
        env_prefix: "DIRECTOR",
        linux_arm64_linker_env: "DIRECTOR_LINUX_ARM64_LINKER",
        linux_x64_linker_env: "DIRECTOR_LINUX_X64_LINKER",
    },
];

impl CliSpec {
    fn linker_override_env(self, target: &ReleaseTarget) -> Option<&'static str> {
        if target.platform == "linux-arm64" {
            Some(self.linux_arm64_linker_env)
        } else if target.platform == "linux-x64" {
            Some(self.linux_x64_linker_env)
        } else {
            None
        }
    }

    fn artifacts_root(self, project_root: &Path) -> PathBuf {
        project_root.join("dist").join(self.dist_subdir)
    }

    fn crate_present(self, project_root: &Path) -> bool {
        project_root
            .join("crates")
            .join(self.crate_name)
            .join("Cargo.toml")
            .is_file()
    }
}

/// Look a CLI spec up by binary name.
pub fn spec(name: &str) -> Result<&'static CliSpec, anyhow::Error> {
    CLIS.iter()
        .find(|spec| spec.name == name)
        .ok_or_else(|| anyhow::anyhow!("unknown CLI '{}'", name))
}

/// Resolve an optional comma-separated platform filter to the targets to
/// build. `None` selects all targets; an unknown platform name is an error
/// listing the valid platforms, and nothing is selected in that case.
pub fn select_targets(
    platform_filter: Option<&str>,
) -> Result<Vec<&'static ReleaseTarget>, anyhow::Error> {
    let Some(filter) = platform_filter else {
        return Ok(RELEASE_TARGETS.iter().collect());
    };
    let valid: Vec<&str> = RELEASE_TARGETS.iter().map(|t| t.platform).collect();
    let mut selected = Vec::new();
    for name in filter.split(',').map(str::trim) {
        match RELEASE_TARGETS.iter().find(|t| t.platform == name) {
            Some(target) => selected.push(target),
            None => anyhow::bail!(
                "unknown platform '{}'. Valid platforms: {}",
                name,
                valid.join(", ")
            ),
        }
    }
    Ok(selected)
}

/// Check every artifact for `spec` exists under `dist/<subdir>/`.
pub fn all_artifacts_present(project_root: &Path, spec: &CliSpec) -> bool {
    let artifacts_root = spec.artifacts_root(project_root);
    RELEASE_TARGETS.iter().all(|target| {
        artifacts_root
            .join(target.platform)
            .join(spec.name)
            .is_file()
    })
}

/// Build the CLI's executables for the selected release platforms.
pub fn artifacts_command(
    project_root: &Path,
    spec: &CliSpec,
    platform_filter: Option<&str>,
) -> Result<(), anyhow::Error> {
    let targets = select_targets(platform_filter)?;
    let artifacts_root = spec.artifacts_root(project_root);
    let cargo = env::var(format!("{}_CARGO", spec.env_prefix)).unwrap_or_else(|_| "cargo".into());
    let rustc = env::var(format!("{}_RUSTC", spec.env_prefix)).ok();
    let linux_linker = env::var(format!("{}_LINUX_LINKER", spec.env_prefix)).ok();
    let derived_linux_linker = if linux_linker.is_none() {
        derive_rust_lld(&rustc)?
    } else {
        None
    };
    for target in targets {
        println!(
            "==> Building {} for {} ({})",
            spec.name, target.platform, target.rust_target
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
                spec.name,
                "--target",
                target.rust_target,
            ])
            .current_dir(project_root);
        if let Some(rustc) = &rustc {
            command.env("RUSTC", rustc);
        }
        if let Some(linker_env) = target.cargo_linker_env {
            let target_linker = spec
                .linker_override_env(target)
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
            .join(spec.name);
        let dest = artifacts_root.join(target.platform).join(spec.name);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(&built, &dest).with_context(|| {
            format!(
                "cannot copy {} artifact {} -> {}",
                spec.label,
                built.display(),
                dest.display()
            )
        })?;
        make_executable(&dest)?;
    }
    Ok(())
}

/// Build every shipped CLI whose crate is present in this checkout.
///
/// `release` uses this when it is not skipping the artifact build, so a
/// checkout that carries only one of the CLIs still releases.
pub fn build_shipped_clis(project_root: &Path) -> Result<(), anyhow::Error> {
    for spec in CLIS {
        if spec.crate_present(project_root) {
            artifacts_command(project_root, spec, None)?;
        }
    }
    Ok(())
}

/// Stage the CLI's executables into the tarball staging directory.
///
/// Returns a map of platform → archive path; empty when the crate is not part
/// of this checkout. Missing artifacts fail loudly: a tarball that silently
/// omits a shipped binary is worse than a failed pack.
pub fn stage_executables(
    project_root: &Path,
    spec: &CliSpec,
    autopilot_staging: &Path,
) -> Result<BTreeMap<String, String>, anyhow::Error> {
    if !spec.crate_present(project_root) {
        return Ok(BTreeMap::new());
    }

    let artifacts_root = spec.artifacts_root(project_root);
    if !all_artifacts_present(project_root, spec) {
        for target in RELEASE_TARGETS {
            let artifact = artifacts_root.join(target.platform).join(spec.name);
            if !artifact.is_file() {
                anyhow::bail!(
                    "missing {} artifact for {} at {}; run `deploy.rs {}-artifacts` before `deploy.rs pack`",
                    spec.label,
                    target.platform,
                    artifact.display(),
                    spec.name
                );
            }
        }
    }

    let mut platforms = BTreeMap::new();
    for target in RELEASE_TARGETS {
        let src = artifacts_root.join(target.platform).join(spec.name);
        if !src.is_file() {
            anyhow::bail!(
                "missing {} artifact for {} at {}",
                spec.label,
                target.platform,
                src.display()
            );
        }
        let rel = format!(
            "bin/{}-artifacts/{}/{}",
            spec.name, target.platform, spec.name
        );
        let dest = autopilot_staging.join(&rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(&src, &dest)?;
        make_executable(&dest)?;
        platforms.insert(target.platform.to_string(), rel);
    }

    Ok(platforms)
}

fn make_executable(path: &Path) -> Result<(), anyhow::Error> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path)?.permissions();
        perms.set_mode(perms.mode() | 0o755);
        std::fs::set_permissions(path, perms)?;
    }
    let _ = path;
    Ok(())
}

/// Derive the rust-lld shipped with the selected toolchain, used as the musl
/// linker when the caller gave no explicit one.
pub(crate) fn derive_rust_lld(rustc: &Option<String>) -> Result<Option<String>, anyhow::Error> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_filter_selects_all_targets() {
        let selected = select_targets(None).unwrap();
        let platforms: Vec<&str> = selected.iter().map(|t| t.platform).collect();
        assert_eq!(platforms, vec!["darwin-arm64", "linux-arm64", "linux-x64"]);
    }

    #[test]
    fn filter_selects_only_matching_platforms() {
        let selected = select_targets(Some("linux-x64,linux-arm64")).unwrap();
        let platforms: Vec<&str> = selected.iter().map(|t| t.platform).collect();
        assert_eq!(platforms, vec!["linux-x64", "linux-arm64"]);
    }

    #[test]
    fn unknown_platform_errors_listing_valid_names() {
        let err = select_targets(Some("linux-x64,windows-x64")).unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("windows-x64"),
            "error should name the unknown platform: {msg}"
        );
        for valid in ["darwin-arm64", "linux-arm64", "linux-x64"] {
            assert!(
                msg.contains(valid),
                "error should list valid platform {valid}: {msg}"
            );
        }
    }

    #[test]
    fn empty_filter_is_an_error() {
        assert!(select_targets(Some("")).is_err());
    }

    #[test]
    fn both_shipped_clis_have_a_spec() {
        let distill = spec("distill").unwrap();
        assert_eq!(distill.crate_name, "distill-cli");
        assert_eq!(distill.env_prefix, "DISTILL");
        let director = spec("director").unwrap();
        assert_eq!(director.crate_name, "director-cli");
        assert_eq!(director.dist_subdir, "director");
        assert_eq!(director.env_prefix, "DIRECTOR");
        assert!(spec("nope").is_err());
    }
}
