# install.rs: Multi-Runtime Changes

## Overview

Add `--target` flag to route skills to the correct agent directory. The caller (toolkit-setup) decides per-skill whether to use the shared or agent-exclusive directory. Supports three runtimes: reasonix, codex, ksana.

## New interface

```
install.rs <subcommand> [--target reasonix|codex|ksana] [--shared] [--agent] [args...]

Subcommands (unchanged):
  sync <name> <src>       Ensure <skills-dir>/<name> is a symlink to <src>
  unlink <name>           Remove a toolkit-owned symlink from <skills-dir>
  link-principles <src>   Ensure ~/.agents/principles is a symlink to <src>

Flags:
  --target reasonix|codex|ksana   Select agent directory (default: reasonix)
  --shared                        Override to ~/.agents/skills/ (shared, all agents scan)
  --agent                         Deploy a .toml custom agent as a file symlink (requires --target codex|ksana)
```

## Directory routing

| Flag combo | `sync` (skill) target | `sync --agent` target |
|---|---|---|
| (default) | `~/.reasonix/skills/` | N/A (Reasonix has no custom agents) |
| `--target codex` | `~/.codex/skills/` | `~/.codex/agents/` |
| `--target ksana` | `~/.ksana/skills/` | `~/.ksana/agents/` |
| `--shared` | `~/.agents/skills/` | N/A |

Environment variable overrides: `REASONIX_SKILLS_DIR`, `CODEX_SKILLS_DIR`, `CODEX_AGENTS_DIR`, `KSANA_SKILLS_DIR`, `KSANA_AGENTS_DIR`, `AGENTS_SKILLS_DIR` (shared).

## Custom agent deployment: `sync --agent`

```
install.rs sync <name> <src> --target codex|ksana --agent
```

Symlinks (not copies) the TOML file at `<src>` to `~/.<runtime>/agents/<name>.toml`. The runtime is selected by `--target` (codex → `~/.codex/agents/`, ksana → `~/.ksana/agents/`). The agents directory can be overridden via `CODEX_AGENTS_DIR` / `KSANA_AGENTS_DIR` for tests or unusual installs. Behaviour mirrors skill sync: creates if missing, replaces if broken/wrong-target, refuses to overwrite real files. Symlinks ensure source updates take effect immediately without re-running setup.

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
install.rs sync autopilot-implementer skills/autopilot/autopilot-implementer/codex/agent.toml --target codex --agent
install.rs sync autopilot-reviewer skills/autopilot/autopilot-reviewer/codex/agent.toml --target codex --agent
install.rs sync audit-autopilot skills/autopilot/audit-autopilot/codex --target codex

# Ksana-coupled skills (4)
install.rs sync autopilot-orchestrator skills/autopilot/autopilot-orchestrator/ksana --target ksana
install.rs sync autopilot-implementer skills/autopilot/autopilot-implementer/ksana/agent.toml --target ksana --agent
install.rs sync autopilot-reviewer skills/autopilot/autopilot-reviewer/ksana/agent.toml --target ksana --agent
install.rs sync audit-autopilot skills/autopilot/audit-autopilot/ksana --target ksana
```

## Unlink behavior

`unlink` must clean up across all four skill directories. When `--target` is specified, only clean that target's directory. Without `--target`, clean all four (shared + reasonix + codex + ksana) — used by `toolkit-setup` for full teardown. With `--agent` and no `--target`, clean both agents directories (`~/.codex/agents/` and `~/.ksana/agents/`); with `--agent --target codex|ksana`, clean only that runtime's agents directory.

## Backward compatibility

Default `--target reasonix` preserves current behavior for existing Reasonix-only users. `--shared` maps to the old `~/.agents/skills/` path. No existing call sites break.
