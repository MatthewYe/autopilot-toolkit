#!/usr/bin/env rust-script
//! ```cargo
//! [dependencies]
//! deploy = { path = "crates/deploy" }
//! shared = { path = "crates/shared" }
//! anyhow = "1"
//! ```

use std::env;
use std::path::PathBuf;

fn warn(msg: &str) {
    eprintln!("WARNING: {}", msg);
}

fn usage() -> ! {
    println!("Usage: deploy.rs <subcommand> [args...]");
    println!();
    println!("Subcommands:");
    println!("  dev                     Symlink all skills from source tree into agent dirs");
    println!("  pack                    Build a self-contained tarball into dist/");
    println!("  distill-artifacts       Build Distill CLI executables for release platforms");
    println!("  director-artifacts      Build Director CLI executables for release platforms");
    println!("                          [--platform <csv>] to build only the named platforms");
    println!("  release                 Pack + push to GitHub Releases");
    println!(
        "                          [--skip-cli-build] to reuse the prebuilt dist/distill/ and"
    );
    println!("                          dist/director/ artifacts instead of building them");
    println!("  dev-clean               Remove all dev symlinks from agent dirs");
    println!("  link-principles <src>   Ensure ~/.agents/principles is a symlink to <src>");
    std::process::exit(1);
}

/// Split the `*-artifacts` argument tail into the platform filter and any
/// arguments this command does not understand.
fn parse_platform_args(positional: &[&str]) -> (Option<String>, Vec<String>) {
    let mut platform_filter: Option<String> = None;
    let mut extras: Vec<String> = Vec::new();
    let mut i = 0;
    while i < positional.len() {
        if positional[i] == "--platform" {
            i += 1;
            if i >= positional.len() {
                eprintln!("ERROR: --platform requires a comma-separated value");
                usage();
            }
            platform_filter = Some(positional[i].to_string());
        } else if let Some(value) = positional[i].strip_prefix("--platform=") {
            platform_filter = Some(value.to_string());
        } else {
            extras.push(positional[i].to_string());
        }
        i += 1;
    }
    (platform_filter, extras)
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();

    // Derive PROJECT_ROOT via shared::project_root() (4-step fallback with .skill-lock.json verification)
    let project_root = shared::project_root();

    let home = env::var("HOME").unwrap_or_default();

    // Skills directories (with env var overrides)
    let reasonix_skills_dir = env::var("REASONIX_SKILLS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(&home).join(".reasonix/skills"));
    let codex_skills_dir = env::var("CODEX_SKILLS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(&home).join(".codex/skills"));
    let shared_skills_dir = env::var("AGENTS_SKILLS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(&home).join(".agents/skills"));

    let principles_dir = env::var("AGENTS_PRINCIPLES_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(&home).join(".agents/principles"));

    let codex_agents_dir = env::var("CODEX_AGENTS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(&home).join(".codex/agents"));

    // No subcommand: show usage.
    if args.len() < 2 {
        usage();
    }

    let subcommand = &args[1];
    let rest = &args[2..];
    let positional: Vec<&str> = rest.iter().map(|s| s.as_str()).collect();

    match subcommand.as_str() {
        "pack" => {
            if !positional.is_empty() {
                warn(&format!("ignoring extra arguments: {:?}", positional));
            }
            deploy::pack::pack_command(&project_root)?;
        }
        "distill-artifacts" | "director-artifacts" => {
            let cli = subcommand.trim_end_matches("-artifacts");
            let (platform_filter, extras) = parse_platform_args(&positional);
            if !extras.is_empty() {
                warn(&format!("ignoring extra arguments: {:?}", extras));
            }
            deploy::artifacts::artifacts_command(
                &project_root,
                deploy::artifacts::spec(cli)?,
                platform_filter.as_deref(),
            )?;
        }
        "release" => {
            let mut skip_cli_build = false;
            let mut extras: Vec<&str> = Vec::new();
            for arg in &positional {
                if *arg == "--skip-cli-build" {
                    skip_cli_build = true;
                } else {
                    extras.push(arg);
                }
            }
            if !extras.is_empty() {
                warn(&format!("ignoring extra arguments: {:?}", extras));
            }
            deploy::release::release_command(&project_root, skip_cli_build)?;
        }
        "dev" => {
            if !positional.is_empty() {
                warn(&format!("ignoring extra arguments: {:?}", positional));
            }
            deploy::dev::dev_all(
                &project_root,
                &shared_skills_dir,
                &reasonix_skills_dir,
                &codex_skills_dir,
                &codex_agents_dir,
            )?;
        }
        "dev-clean" => {
            if !positional.is_empty() {
                warn(&format!("ignoring extra arguments: {:?}", positional));
            }
            deploy::dev::dev_clean(
                &project_root,
                &shared_skills_dir,
                &reasonix_skills_dir,
                &codex_skills_dir,
                &codex_agents_dir,
            )?;
        }
        "link-principles" => {
            if positional.len() != 1 {
                eprintln!(
                    "ERROR: link-principles requires exactly one argument (<src>), but received {}",
                    positional.len()
                );
                usage();
            }
            let src = PathBuf::from(positional[0]);
            deploy::link_principles(&src, &principles_dir)?;
        }
        _ => {
            eprintln!(
                "ERROR: unknown subcommand '{}'. Available: dev, dev-clean, pack, distill-artifacts, director-artifacts, release, link-principles",
                subcommand
            );
            usage();
        }
    }

    Ok(())
}
