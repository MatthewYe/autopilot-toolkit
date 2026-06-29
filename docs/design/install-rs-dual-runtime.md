# install.rs: Dual-Runtime Changes

## Overview

Add `--target` flag to route skills to the correct agent directory. The caller (toolkit-setup) decides per-skill whether to use the shared or agent-exclusive directory.

## New interface

```
install.rs <subcommand> [--target reasonix|codex] [--shared] [args...]

Subcommands (unchanged):
  sync <name> <src>       Ensure <skills-dir>/<name> is a symlink to <src>
  unlink <name>           Remove a toolkit-owned symlink from <skills-dir>
  link-principles <src>   Ensure ~/.agents/principles is a symlink to <src>
  deploy-agent <name> <src>  Copy a .toml custom agent to .codex/agents/<name>.toml

Flags:
  --target reasonix|codex   Select agent directory (default: reasonix)
  --shared                  Override to ~/.agents/skills/ (shared, both agents scan)
```

## Directory routing

| Flag combo | `sync` target | `deploy-agent` target |
|---|---|---|
| (default) | `~/.reasonix/skills/` | N/A (Reasonix has no custom agents) |
| `--target codex` | `~/.codex/skills/` | `.codex/agents/` (project) or `~/.codex/agents/` (user) |
| `--shared` | `~/.agents/skills/` | N/A |

Environment variable overrides: `REASONIX_SKILLS_DIR`, `CODEX_SKILLS_DIR`, `CODEX_AGENTS_DIR`, `AGENTS_SKILLS_DIR` (shared).

## New subcommand: `deploy-agent`

```
install.rs deploy-agent <name> <src> [--target codex] [--user]
```

Copies (not symlinks) the TOML file at `<src>` to `.codex/agents/<name>.toml` (project-local by default, `--user` for `~/.codex/agents/`). Copy instead of symlink because `.codex/` is often gitignored and not tracked alongside the toolkit repo. On subsequent runs, overwrites if content differs.

## Toolkit-setup calling pattern

```
# Runtime-agnostic skills (17)
install.rs sync tdd skills/upstream/skills/engineering/tdd --shared
install.rs sync toolkit-setup skills/autopilot/toolkit-setup --shared
...

# Reasonix-coupled skills (4)
install.rs sync autopilot-orchestrator skills/autopilot/autopilot-orchestrator/reasonix --target reasonix
install.rs sync autopilot-implementer skills/autopilot/autopilot-implementer/reasonix --target reasonix
install.rs sync autopilot-reviewer skills/autopilot/autopilot-reviewer/reasonix --target reasonix
install.rs sync audit-autopilot skills/autopilot/audit-autopilot/reasonix --target reasonix

# Codex-coupled skills (4)
install.rs sync autopilot-orchestrator skills/autopilot/autopilot-orchestrator/codex --target codex
install.rs deploy-agent autopilot-implementer skills/autopilot/autopilot-implementer/agent.toml --target codex
install.rs deploy-agent autopilot-reviewer skills/autopilot/autopilot-reviewer/agent.toml --target codex
install.rs sync audit-autopilot skills/autopilot/audit-autopilot/codex --target codex
```

## Unlink behavior

`unlink` must clean up across all three directories. When `--target` is specified, only clean that target's directory. Without `--target`, clean all three (shared + reasonix + codex) — used by `toolkit-setup` for full teardown.

## Backward compatibility

Default `--target reasonix` preserves current behavior for existing Reasonix-only users. `--shared` maps to the old `~/.agents/skills/` path. No existing call sites break.
