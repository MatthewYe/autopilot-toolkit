#!/usr/bin/env rust-script
//! ```cargo
//! [dependencies]
//! anyhow = "1"
//! ```

use anyhow::Context;
use std::env;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

fn warn(msg: &str) {
    eprintln!("WARNING: {}", msg);
}

fn usage() -> ! {
    println!(
        "Usage: install.rs <subcommand> [args...] [--target reasonix|codex] [--shared] [--agent]"
    );
    println!();
    println!("Subcommands:");
    println!("  sync <name> <src>       Ensure a symlink exists at the appropriate location");
    println!(
        "                           Skills (default): ~/.reasonix/skills/<name> -> <src> (dir)"
    );
    println!("                           --target reasonix: ~/.reasonix/skills/<name>");
    println!("                           --target codex:   ~/.codex/skills/<name>");
    println!("                           --shared:         ~/.agents/skills/<name>");
    println!(
        "                           --agent:          ~/.codex/agents/<name>.toml -> <src> (file)"
    );
    println!("                           Requires --target codex with --agent.");
    println!("  unlink <name>           Remove a toolkit-owned symlink from skills/agents dirs");
    println!("                           Default (no --target): all skill directories");
    println!("                           --target reasonix|codex: only that target");
    println!("                           --shared: only ~/.agents/skills/");
    println!("                           --agent: ~/.codex/agents/<name>.toml");
    println!("  link-principles <src>   Ensure ~/.agents/principles is a symlink to <src>");
    std::process::exit(1);
}

/// Parse flags (--target, --shared, --agent) from the positional args tail.
/// Returns (positional_args, target_value, shared_flag, agent_flag).
fn parse_flags(args: &[String]) -> (Vec<&str>, Option<String>, bool, bool) {
    let mut positional: Vec<&str> = Vec::new();
    let mut target: Option<String> = None;
    let mut shared = false;
    let mut agent = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--target" => {
                i += 1;
                if i < args.len() {
                    target = Some(args[i].clone());
                } else {
                    eprintln!("ERROR: --target requires a value (reasonix or codex)");
                    usage();
                }
            }
            "--shared" => {
                shared = true;
            }
            "--agent" => {
                agent = true;
            }
            other => {
                positional.push(other);
            }
        }
        i += 1;
    }

    (positional, target, shared, agent)
}

fn sync_skill(name: &str, src: &Path, skills_dir: &Path) -> Result<(), anyhow::Error> {
    let target = skills_dir.join(name);

    // Ensure the skills directory exists
    std::fs::create_dir_all(skills_dir)
        .with_context(|| format!("cannot create directory {}", skills_dir.display()))?;

    // If target exists as a real file/directory (not a symlink), refuse to overwrite
    if target.exists() && !target.is_symlink() {
        warn(&format!(
            "{} exists as a real directory (not a symlink) — refusing to overwrite",
            target.display()
        ));
        anyhow::bail!("real directory conflict at {}", target.display());
    }

    // If target is a symlink, inspect its current state
    if target.is_symlink() {
        let existing = std::fs::read_link(&target)
            .with_context(|| format!("cannot read symlink {}", target.display()))?;

        // Valid symlink pointing to the correct source — nothing to do
        if existing == src && src.is_dir() {
            return Ok(());
        }

        // Broken or pointing to the wrong target — remove it before rebuilding
        std::fs::remove_file(&target)
            .with_context(|| format!("cannot remove symlink {}", target.display()))?;
    }

    // Source directory must exist
    if !src.is_dir() {
        warn(&format!(
            "source directory does not exist: {}",
            src.display()
        ));
        return Ok(());
    }

    // Create the symlink
    symlink(src, &target).with_context(|| {
        format!(
            "cannot create symlink {} -> {}",
            target.display(),
            src.display()
        )
    })?;

    Ok(())
}

fn unlink_skill(name: &str, skills_dir: &Path, project_root: &Path) -> Result<(), anyhow::Error> {
    let target = skills_dir.join(name);

    // Only operate on symlinks
    if !target.is_symlink() {
        return Ok(());
    }

    // Read symlink target
    let link_target = std::fs::read_link(&target)
        .with_context(|| format!("cannot read symlink {}", target.display()))?;

    // Remove only if the symlink target is under PROJECT_ROOT
    // Matches install.sh: case "$link_target" in "$PROJECT_ROOT"|"$PROJECT_ROOT/"*)
    if link_target.starts_with(project_root) {
        std::fs::remove_file(&target)
            .with_context(|| format!("cannot remove symlink {}", target.display()))?;
    }

    Ok(())
}

fn link_principles(src: &Path, principles_dir: &Path) -> Result<(), anyhow::Error> {
    let target = principles_dir;

    // If target exists as a real file/directory (not a symlink), refuse to overwrite
    if target.exists() && !target.is_symlink() {
        warn(&format!(
            "{} exists as a real directory (not a symlink) — refusing to overwrite",
            target.display()
        ));
        anyhow::bail!("real directory conflict at {}", target.display());
    }

    // If target is a symlink, inspect its current state
    if target.is_symlink() {
        let existing = std::fs::read_link(target)
            .with_context(|| format!("cannot read symlink {}", target.display()))?;

        // Valid symlink pointing to the correct source — nothing to do
        if existing == src && src.is_dir() {
            return Ok(());
        }

        // Broken or pointing to the wrong target — remove it before rebuilding
        std::fs::remove_file(target)
            .with_context(|| format!("cannot remove symlink {}", target.display()))?;
    }

    // Source directory must exist
    if !src.is_dir() {
        warn(&format!(
            "source directory does not exist: {}",
            src.display()
        ));
        return Ok(());
    }

    // Ensure parent directory exists (e.g. ~/.agents/)
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("cannot create directory {}", parent.display()))?;
    }

    // Create the symlink
    symlink(src, target).with_context(|| {
        format!(
            "cannot create symlink {} -> {}",
            target.display(),
            src.display()
        )
    })?;

    Ok(())
}

fn sync_agent(name: &str, src: &Path, codex_agents_dir: &Path) -> Result<(), anyhow::Error> {
    // Source file must exist
    if !src.is_file() {
        anyhow::bail!("agent source file does not exist: {}", src.display());
    }

    // Ensure agents directory exists
    std::fs::create_dir_all(codex_agents_dir)
        .with_context(|| format!("cannot create directory {}", codex_agents_dir.display()))?;

    let target = codex_agents_dir.join(format!("{}.toml", name));

    // If target exists as a real file (not a symlink), refuse to overwrite
    if target.exists() && !target.is_symlink() {
        warn(&format!(
            "{} exists as a real file (not a symlink) — refusing to overwrite",
            target.display()
        ));
        anyhow::bail!("real file conflict at {}", target.display());
    }

    // If target is a symlink, inspect its current state
    if target.is_symlink() {
        let existing = std::fs::read_link(&target)
            .with_context(|| format!("cannot read symlink {}", target.display()))?;

        // Valid symlink pointing to the correct source — nothing to do
        if existing == src && src.is_file() {
            return Ok(());
        }

        // Broken or pointing to the wrong target — remove it before rebuilding
        std::fs::remove_file(&target)
            .with_context(|| format!("cannot remove symlink {}", target.display()))?;
    }

    // Create the file symlink
    symlink(src, &target).with_context(|| {
        format!(
            "cannot create symlink {} -> {}",
            target.display(),
            src.display()
        )
    })?;

    Ok(())
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        usage();
    }

    // Derive PROJECT_ROOT from script path (equivalent to bash's $(cd "$(dirname "$0")" && pwd))
    let script_path = PathBuf::from(&args[0]);
    let project_root = env::var("PROJECT_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            script_path
                .canonicalize()
                .unwrap_or_else(|_| script_path.clone())
                .parent()
                .unwrap_or(Path::new("."))
                .to_path_buf()
        });

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

    let subcommand = &args[1];
    let rest = &args[2..];

    // Parse flags from the positional tail
    let (positional, target_flag, shared_flag, agent_flag) = parse_flags(rest);

    // Resolve the target skills directory for sync
    let resolve_skills_dir = || -> PathBuf {
        if shared_flag {
            if target_flag.is_some() {
                warn("--shared overrides --target (shared directory takes precedence)");
            }
            shared_skills_dir.clone()
        } else if let Some(ref t) = target_flag {
            match t.as_str() {
                "reasonix" => reasonix_skills_dir.clone(),
                "codex" => codex_skills_dir.clone(),
                other => {
                    eprintln!(
                        "ERROR: unknown --target '{}'. Expected reasonix or codex",
                        other
                    );
                    usage();
                }
            }
        } else {
            reasonix_skills_dir.clone()
        }
    };

    // All three directories for unlink-all
    let all_skills_dirs = || -> Vec<PathBuf> {
        vec![
            reasonix_skills_dir.clone(),
            codex_skills_dir.clone(),
            shared_skills_dir.clone(),
        ]
    };

    match subcommand.as_str() {
        "sync" => {
            if positional.len() != 2 {
                eprintln!(
                    "ERROR: sync requires exactly two arguments (<name> <src>), but received {}",
                    positional.len()
                );
                usage();
            }
            let name = positional[0];
            let src = PathBuf::from(positional[1]);

            if agent_flag {
                // Agent mode: file symlink in ~/.codex/agents/
                if target_flag.as_deref() != Some("codex") {
                    anyhow::bail!("--agent requires --target codex");
                }
                sync_agent(name, &src, &codex_agents_dir)?;
            } else {
                // Skill mode: directory symlink
                let skills_dir = resolve_skills_dir();
                sync_skill(name, &src, &skills_dir)?;
            }
        }
        "unlink" => {
            if positional.len() != 1 {
                eprintln!(
                    "ERROR: unlink requires exactly one argument (<name>), but received {}",
                    positional.len()
                );
                usage();
            }
            let name = positional[0];

            if agent_flag {
                // Agent mode: remove symlink from ~/.codex/agents/
                let target = codex_agents_dir.join(format!("{}.toml", name));
                if target.is_symlink() {
                    let link_target = std::fs::read_link(&target)
                        .with_context(|| format!("cannot read symlink {}", target.display()))?;
                    if link_target.starts_with(&project_root) {
                        std::fs::remove_file(&target).with_context(|| {
                            format!("cannot remove symlink {}", target.display())
                        })?;
                    }
                }
            } else if target_flag.is_some() || shared_flag {
                // Targeted unlink: clean only the specified directory
                let skills_dir = resolve_skills_dir();
                unlink_skill(name, &skills_dir, &project_root)?;
            } else {
                // Unlink from all three directories
                for dir in &all_skills_dirs() {
                    unlink_skill(name, dir, &project_root)?;
                }
            }
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
            link_principles(&src, &principles_dir)?;
        }
        _ => {
            eprintln!(
                "ERROR: unknown subcommand '{}'. Available: sync, unlink, link-principles",
                subcommand
            );
            usage();
        }
    }

    Ok(())
}
