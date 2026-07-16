#!/usr/bin/env rust-script
//! ```cargo
//! [dependencies]
//! anyhow = "1"
//! serde_json = "1"
//! ```

use anyhow::Context;
use std::collections::HashSet;
use std::env;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

fn warn(msg: &str) {
    eprintln!("WARNING: {}", msg);
}

fn usage() -> ! {
    println!(
        "Usage: install.rs <subcommand> [args...] [--target reasonix|codex|ksana] [--shared] [--agent]"
    );
    println!();
    println!("Subcommands:");
    println!("  sync <name> <src>       Ensure a symlink exists at the appropriate location");
    println!(
        "                           Skills (default): ~/.reasonix/skills/<name> -> <src> (dir)"
    );
    println!("                           --target reasonix: ~/.reasonix/skills/<name>");
    println!("                           --target codex:   ~/.codex/skills/<name>");
    println!("                           --target ksana:   ~/.ksana/skills/<name>");
    println!("                           --shared:         ~/.agents/skills/<name>");
    println!(
        "                           --agent:          ~/.<runtime>/agents/<name>.toml -> <src> (file)"
    );
    println!("                           Requires --target codex|ksana with --agent.");
    println!("  unlink <name>           Remove a toolkit-owned symlink from skills/agents dirs");
    println!("                           Default (no --target): all skill directories");
    println!("                           --target reasonix|codex|ksana: only that target");
    println!("                           --shared: only ~/.agents/skills/");
    println!("                           --agent: ~/.<codex|ksana>/agents/<name>.toml");
    println!("  link-principles <src>   Ensure ~/.agents/principles is a symlink to <src>");
    println!("  setup --target <name>   Install or repair the complete toolkit for one runtime");
    println!("                           Supports --dry-run to report planned changes only.");
    println!("  status --target <name>  Report the complete toolkit state without changing it");
    std::process::exit(1);
}

/// Parse flags from the positional args tail.
/// Returns (positional_args, target_value, shared_flag, agent_flag, dry_run_flag).
fn parse_flags(args: &[String]) -> (Vec<&str>, Option<String>, bool, bool, bool) {
    let mut positional: Vec<&str> = Vec::new();
    let mut target: Option<String> = None;
    let mut shared = false;
    let mut agent = false;
    let mut dry_run = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--target" => {
                i += 1;
                if i < args.len() {
                    target = Some(args[i].clone());
                } else {
                    eprintln!("ERROR: --target requires a value (reasonix, codex or ksana)");
                    usage();
                }
            }
            "--shared" => {
                shared = true;
            }
            "--agent" => {
                agent = true;
            }
            "--dry-run" => {
                dry_run = true;
            }
            other => {
                positional.push(other);
            }
        }
        i += 1;
    }

    (positional, target, shared, agent, dry_run)
}

#[derive(Clone)]
struct SkillEntry {
    name: String,
    source: PathBuf,
    shared: bool,
}

#[derive(Clone)]
struct AgentEntry {
    name: String,
    source: PathBuf,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LinkState {
    Correct,
    Missing,
    WrongTarget,
    Broken,
    RealEntry,
}

impl LinkState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Correct => "correct",
            Self::Missing => "missing",
            Self::WrongTarget => "wrong_target",
            Self::Broken => "broken",
            Self::RealEntry => "real_entry",
        }
    }
}

fn validate_runtime(target: Option<&str>) -> Result<&str, anyhow::Error> {
    match target {
        Some("reasonix" | "codex" | "ksana") => Ok(target.unwrap()),
        Some(other) => anyhow::bail!(
            "unknown --target '{}'. Expected reasonix, codex or ksana",
            other
        ),
        None => anyhow::bail!("setup and status require --target reasonix, codex or ksana"),
    }
}

fn inspect_link(
    target: &Path,
    source: &Path,
    source_is_file: bool,
) -> Result<LinkState, anyhow::Error> {
    if !target.exists() && !target.is_symlink() {
        return Ok(LinkState::Missing);
    }
    if !target.is_symlink() {
        return Ok(LinkState::RealEntry);
    }
    let actual = std::fs::read_link(target)
        .with_context(|| format!("cannot read symlink {}", target.display()))?;
    if actual != source {
        return Ok(LinkState::WrongTarget);
    }
    let valid = if source_is_file {
        target.is_file()
    } else {
        target.is_dir()
    };
    Ok(if valid {
        LinkState::Correct
    } else {
        LinkState::Broken
    })
}

fn discover_expected(
    root: &Path,
    runtime: &str,
) -> Result<(Vec<SkillEntry>, Vec<AgentEntry>), anyhow::Error> {
    let lock_path = root.join(".skill-lock.json");
    let lock_content = std::fs::read_to_string(&lock_path)
        .with_context(|| format!("cannot read {}", lock_path.display()))?;
    let lock: serde_json::Value = serde_json::from_str(&lock_content)
        .with_context(|| format!("cannot parse {}", lock_path.display()))?;
    let locked_skills = lock
        .get("skills")
        .and_then(serde_json::Value::as_object)
        .context(".skill-lock.json must contain a skills object")?;

    let mut skills = Vec::new();
    for (name, metadata) in locked_skills {
        let skill_path = metadata
            .get("skillPath")
            .and_then(serde_json::Value::as_str)
            .with_context(|| format!("locked skill {} has no skillPath", name))?;
        let source = root.join("skills/upstream").join(skill_path);
        let source = source
            .parent()
            .context("skillPath must have a parent directory")?
            .to_path_buf();
        skills.push(SkillEntry {
            name: name.clone(),
            source,
            shared: true,
        });
    }

    let autopilot_root = root.join("skills/autopilot");
    let mut autopilot_dirs: Vec<PathBuf> = std::fs::read_dir(&autopilot_root)
        .with_context(|| format!("cannot read {}", autopilot_root.display()))?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    autopilot_dirs.sort();

    let mut agents = Vec::new();
    for directory in autopilot_dirs {
        let name = directory
            .file_name()
            .and_then(|name| name.to_str())
            .context("autopilot skill directory has a non-UTF-8 name")?
            .to_owned();
        if directory.join("SKILL.md").is_file() {
            skills.push(SkillEntry {
                name,
                source: directory,
                shared: true,
            });
            continue;
        }
        if !directory.join("reasonix/SKILL.md").is_file() {
            continue;
        }

        let variant = directory.join(runtime);
        if variant.join("SKILL.md").is_file() {
            skills.push(SkillEntry {
                name: name.clone(),
                source: variant.clone(),
                shared: false,
            });
        }
        if matches!(runtime, "codex" | "ksana") {
            let agent = variant.join("agent.toml");
            if agent.is_file() {
                agents.push(AgentEntry {
                    name,
                    source: agent,
                });
            }
        }
    }

    skills.sort_by(|left, right| left.name.cmp(&right.name));
    agents.sort_by(|left, right| left.name.cmp(&right.name));
    Ok((skills, agents))
}

fn toolkit_orphans(
    directory: &Path,
    expected_names: &HashSet<String>,
    project_root: &Path,
) -> Result<Vec<String>, anyhow::Error> {
    if !directory.is_dir() {
        return Ok(Vec::new());
    }
    let mut names = Vec::new();
    for entry in std::fs::read_dir(directory)
        .with_context(|| format!("cannot read {}", directory.display()))?
        .flatten()
    {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if !path.is_symlink() || expected_names.contains(&name) {
            continue;
        }
        let link = std::fs::read_link(&path)
            .with_context(|| format!("cannot read symlink {}", path.display()))?;
        if link.starts_with(project_root) {
            names.push(name);
        }
    }
    names.sort();
    Ok(names)
}

fn report_verification(
    skills: &[SkillEntry],
    agents: &[AgentEntry],
    shared_dir: &Path,
    target_dir: &Path,
    agents_dir: Option<&Path>,
    principles_dir: &Path,
    principles_source: &Path,
) -> Result<bool, anyhow::Error> {
    let mut passed = true;
    for skill in skills {
        let destination = if skill.shared { shared_dir } else { target_dir }.join(&skill.name);
        let state = inspect_link(&destination, &skill.source, false)?;
        let ok = state == LinkState::Correct;
        println!(
            "[{}] {} ({})",
            if ok { "PASS" } else { "FAIL" },
            skill.name,
            state.as_str()
        );
        passed &= ok;
    }
    for agent in agents {
        let destination = agents_dir
            .context("agent entries require an agents directory")?
            .join(format!("{}.toml", agent.name));
        let state = inspect_link(&destination, &agent.source, true)?;
        let ok = state == LinkState::Correct;
        println!(
            "[{}] {}.toml ({})",
            if ok { "PASS" } else { "FAIL" },
            agent.name,
            state.as_str()
        );
        passed &= ok;
    }
    let state = inspect_link(principles_dir, principles_source, false)?;
    let ok = state == LinkState::Correct;
    println!(
        "[{}] principles ({})",
        if ok { "PASS" } else { "FAIL" },
        state.as_str()
    );
    passed &= ok;
    Ok(passed)
}

fn setup_toolkit(
    project_root: &Path,
    runtime: &str,
    dry_run: bool,
    shared_dir: &Path,
    target_dir: &Path,
    agents_dir: Option<&Path>,
    principles_dir: &Path,
) -> Result<(), anyhow::Error> {
    let (skills, agents) = discover_expected(project_root, runtime)?;
    let principles_source = project_root.join("principles");
    let mut conflicts = Vec::new();

    for skill in &skills {
        if !skill.source.is_dir() {
            conflicts.push(format!(
                "missing source for {}: {}",
                skill.name,
                skill.source.display()
            ));
            continue;
        }
        let directory = if skill.shared { shared_dir } else { target_dir };
        if inspect_link(&directory.join(&skill.name), &skill.source, false)? == LinkState::RealEntry
        {
            conflicts.push(format!(
                "real directory conflict for {}",
                directory.join(&skill.name).display()
            ));
        }
    }
    for agent in &agents {
        if !agent.source.is_file() {
            conflicts.push(format!(
                "missing source for {}: {}",
                agent.name,
                agent.source.display()
            ));
            continue;
        }
        let directory = agents_dir.context("agent entries require an agents directory")?;
        if inspect_link(
            &directory.join(format!("{}.toml", agent.name)),
            &agent.source,
            true,
        )? == LinkState::RealEntry
        {
            conflicts.push(format!("real file conflict for {}", agent.name));
        }
    }
    if !principles_source.is_dir() {
        conflicts.push(format!(
            "missing principles source: {}",
            principles_source.display()
        ));
    } else if inspect_link(principles_dir, &principles_source, false)? == LinkState::RealEntry {
        conflicts.push(format!(
            "real directory conflict for {}",
            principles_dir.display()
        ));
    }
    if !conflicts.is_empty() {
        anyhow::bail!("setup preflight failed:\n- {}", conflicts.join("\n- "));
    }

    let shared_names: HashSet<String> = skills
        .iter()
        .filter(|skill| skill.shared)
        .map(|skill| skill.name.clone())
        .collect();
    let target_names: HashSet<String> = skills
        .iter()
        .filter(|skill| !skill.shared)
        .map(|skill| skill.name.clone())
        .collect();
    let shared_orphans = toolkit_orphans(shared_dir, &shared_names, project_root)?;
    let target_orphans = toolkit_orphans(target_dir, &target_names, project_root)?;

    println!("TOOLKIT_SETUP_REPORT:");
    println!("Target: {}", runtime);
    println!(
        "Expected: {} skills, {} custom agents",
        skills.len(),
        agents.len()
    );
    if dry_run {
        println!("Mode: dry-run");
    }

    for skill in &skills {
        let directory = if skill.shared { shared_dir } else { target_dir };
        let state = inspect_link(&directory.join(&skill.name), &skill.source, false)?;
        if state != LinkState::Correct {
            println!(
                "{} {} -> {} ({})",
                if dry_run { "WOULD SYNC" } else { "SYNC" },
                skill.name,
                skill.source.display(),
                if skill.shared { "shared" } else { runtime }
            );
            if !dry_run {
                sync_skill(&skill.name, &skill.source, directory)?;
            }
        }
    }
    for agent in &agents {
        let directory = agents_dir.context("agent entries require an agents directory")?;
        let state = inspect_link(
            &directory.join(format!("{}.toml", agent.name)),
            &agent.source,
            true,
        )?;
        if state != LinkState::Correct {
            println!(
                "{} {} -> {} (--target {} --agent)",
                if dry_run { "WOULD SYNC" } else { "SYNC" },
                agent.name,
                agent.source.display(),
                runtime
            );
            if !dry_run {
                sync_agent(&agent.name, &agent.source, directory)?;
            }
        }
    }
    for name in shared_orphans {
        println!(
            "{} {} (shared orphan)",
            if dry_run { "WOULD UNLINK" } else { "UNLINK" },
            name
        );
        if !dry_run {
            unlink_skill(&name, shared_dir, project_root)?;
        }
    }
    for name in target_orphans {
        println!(
            "{} {} ({} orphan)",
            if dry_run { "WOULD UNLINK" } else { "UNLINK" },
            name,
            runtime
        );
        if !dry_run {
            unlink_skill(&name, target_dir, project_root)?;
        }
    }
    if inspect_link(principles_dir, &principles_source, false)? != LinkState::Correct {
        println!(
            "{} principles -> {}",
            if dry_run { "WOULD LINK" } else { "LINK" },
            principles_source.display()
        );
        if !dry_run {
            link_principles(&principles_source, principles_dir)?;
        }
    }

    if dry_run {
        return Ok(());
    }
    let passed = report_verification(
        &skills,
        &agents,
        shared_dir,
        target_dir,
        agents_dir,
        principles_dir,
        &principles_source,
    )?;
    if !passed {
        anyhow::bail!("toolkit setup did not reach a clean state");
    }
    println!("ALL PASS");
    Ok(())
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
    let ksana_skills_dir = env::var("KSANA_SKILLS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(&home).join(".ksana/skills"));
    let shared_skills_dir = env::var("AGENTS_SKILLS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(&home).join(".agents/skills"));

    let principles_dir = env::var("AGENTS_PRINCIPLES_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(&home).join(".agents/principles"));

    let codex_agents_dir = env::var("CODEX_AGENTS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(&home).join(".codex/agents"));
    let ksana_agents_dir = env::var("KSANA_AGENTS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(&home).join(".ksana/agents"));

    let subcommand = &args[1];
    let rest = &args[2..];

    // Parse flags from the positional tail
    let (positional, target_flag, shared_flag, agent_flag, dry_run) = parse_flags(rest);

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
                "ksana" => ksana_skills_dir.clone(),
                other => {
                    eprintln!(
                        "ERROR: unknown --target '{}'. Expected reasonix, codex or ksana",
                        other
                    );
                    usage();
                }
            }
        } else {
            reasonix_skills_dir.clone()
        }
    };

    // All skill directories for unlink-all (reasonix, codex, ksana, shared)
    let all_skills_dirs = || -> Vec<PathBuf> {
        vec![
            reasonix_skills_dir.clone(),
            codex_skills_dir.clone(),
            ksana_skills_dir.clone(),
            shared_skills_dir.clone(),
        ]
    };

    match subcommand.as_str() {
        "setup" => {
            if !positional.is_empty() || shared_flag || agent_flag {
                anyhow::bail!("setup accepts only --target <runtime> and optional --dry-run");
            }
            let runtime = validate_runtime(target_flag.as_deref())?;
            let target_dir = match runtime {
                "reasonix" => &reasonix_skills_dir,
                "codex" => &codex_skills_dir,
                "ksana" => &ksana_skills_dir,
                _ => unreachable!("validate_runtime returned an unsupported runtime"),
            };
            let agents_dir = match runtime {
                "codex" => Some(codex_agents_dir.as_path()),
                "ksana" => Some(ksana_agents_dir.as_path()),
                _ => None,
            };
            setup_toolkit(
                &project_root,
                runtime,
                dry_run,
                &shared_skills_dir,
                target_dir,
                agents_dir,
                &principles_dir,
            )?;
        }
        "status" => {
            if !positional.is_empty() || shared_flag || agent_flag || dry_run {
                anyhow::bail!("status accepts only --target <runtime>");
            }
            let runtime = validate_runtime(target_flag.as_deref())?;
            let target_dir = match runtime {
                "reasonix" => &reasonix_skills_dir,
                "codex" => &codex_skills_dir,
                "ksana" => &ksana_skills_dir,
                _ => unreachable!("validate_runtime returned an unsupported runtime"),
            };
            let agents_dir = match runtime {
                "codex" => Some(codex_agents_dir.as_path()),
                "ksana" => Some(ksana_agents_dir.as_path()),
                _ => None,
            };
            let (skills, agents) = discover_expected(&project_root, runtime)?;
            println!("TOOLKIT_STATUS_REPORT:");
            println!("Target: {}", runtime);
            let passed = report_verification(
                &skills,
                &agents,
                &shared_skills_dir,
                target_dir,
                agents_dir,
                &principles_dir,
                &project_root.join("principles"),
            )?;
            if !passed {
                anyhow::bail!("toolkit status found damaged or missing entries");
            }
            println!("ALL PASS");
        }
        "sync" => {
            if dry_run {
                anyhow::bail!("--dry-run is only supported by setup");
            }
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
                // Agent mode: file symlink in the target runtime's agents dir.
                // Only codex and ksana have a custom-agents concept.
                let agents_dir = match target_flag.as_deref() {
                    Some("codex") => &codex_agents_dir,
                    Some("ksana") => &ksana_agents_dir,
                    _ => anyhow::bail!("--agent requires --target codex or ksana"),
                };
                sync_agent(name, &src, agents_dir)?;
            } else {
                // Skill mode: directory symlink
                let skills_dir = resolve_skills_dir();
                sync_skill(name, &src, &skills_dir)?;
            }
        }
        "unlink" => {
            if dry_run {
                anyhow::bail!("--dry-run is only supported by setup");
            }
            if positional.len() != 1 {
                eprintln!(
                    "ERROR: unlink requires exactly one argument (<name>), but received {}",
                    positional.len()
                );
                usage();
            }
            let name = positional[0];

            if agent_flag {
                // Agent mode: remove symlink from the target runtime's agents dir.
                // When --target is absent, clean both codex and ksana agents dirs.
                let agent_dirs: Vec<PathBuf> = match target_flag.as_deref() {
                    Some("codex") => vec![codex_agents_dir.clone()],
                    Some("ksana") => vec![ksana_agents_dir.clone()],
                    Some(other) => {
                        eprintln!(
                            "ERROR: --agent with --target '{}' is not supported. Expected codex or ksana",
                            other
                        );
                        usage();
                    }
                    None => vec![codex_agents_dir.clone(), ksana_agents_dir.clone()],
                };
                for agents_dir in &agent_dirs {
                    let target = agents_dir.join(format!("{}.toml", name));
                    if target.is_symlink() {
                        let link_target = std::fs::read_link(&target)
                            .with_context(|| format!("cannot read symlink {}", target.display()))?;
                        if link_target.starts_with(&project_root) {
                            std::fs::remove_file(&target).with_context(|| {
                                format!("cannot remove symlink {}", target.display())
                            })?;
                        }
                    }
                }
            } else if target_flag.is_some() || shared_flag {
                // Targeted unlink: clean only the specified directory
                let skills_dir = resolve_skills_dir();
                unlink_skill(name, &skills_dir, &project_root)?;
            } else {
                // Unlink from all skill directories (reasonix, codex, ksana, shared)
                for dir in &all_skills_dirs() {
                    unlink_skill(name, dir, &project_root)?;
                }
            }
        }
        "link-principles" => {
            if dry_run {
                anyhow::bail!("--dry-run is only supported by setup");
            }
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
                "ERROR: unknown subcommand '{}'. Available: setup, status, sync, unlink, link-principles",
                subcommand
            );
            usage();
        }
    }

    Ok(())
}
