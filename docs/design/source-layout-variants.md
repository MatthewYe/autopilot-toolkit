# Source Repo Layout: Skill Variants

## Design principle

- **Runtime-agnostic skills**: one directory, one `SKILL.md`. No change from current layout.
- **Runtime-coupled skills**: `reasonix/`, `codex/`, and `ksana/` subdirectories under the skill directory, each containing the variant artifact.

This preserves the existing directory-level symlink model — `install.rs sync <name> <src>` symlinks the agent install directory to the variant subdirectory.

## Layout

```
skills/
├── upstream/                          # 13 skills, no change
│   └── skills/
│       ├── engineering/
│       │   ├── tdd/SKILL.md
│       │   ├── diagnosing-bugs/SKILL.md
│       │   └── ...
│       ├── productivity/
│       │   ├── grilling/SKILL.md
│       │   └── ...
│       └── misc/
│           └── ...
│
└── autopilot/
    │
    ├── autopilot-orchestrator/        # runtime-coupled: all variants are inline skills
    │   ├── reasonix/SKILL.md          # → ~/.reasonix/skills/autopilot-orchestrator/
    │   ├── codex/SKILL.md             # → ~/.codex/skills/autopilot-orchestrator/
    │   ├── ksana/SKILL.md             # → ~/.ksana/skills/autopilot-orchestrator/
    │   └── references/                # shared references (meta-review.md, acceptance-report.md)
    │                                  #   — symlink target is the whole variant dir; reasonix/
    │                                  #     variant gets references/ via sibling symlink
    │
    ├── autopilot-implementer/         # runtime-coupled
    │   ├── reasonix/SKILL.md          # → ~/.reasonix/skills/autopilot-implementer/
    │   ├── codex/agent.toml           # → ~/.codex/agents/autopilot-implementer.toml
    │   └── ksana/agent.toml           # → ~/.ksana/agents/autopilot-implementer.toml
    │                                  #    (custom agent, not a skill — use sync --agent)
    │
    ├── autopilot-reviewer/            # runtime-coupled
    │   ├── reasonix/SKILL.md          # → ~/.reasonix/skills/autopilot-reviewer/
    │   ├── codex/agent.toml           # → ~/.codex/agents/autopilot-reviewer.toml
    │   └── ksana/agent.toml           # → ~/.ksana/agents/autopilot-reviewer.toml
    │
    ├── audit-autopilot/               # runtime-coupled: all variants are inline skills
    │   ├── reasonix/
    │   │   ├── SKILL.md               # → ~/.reasonix/skills/audit-autopilot/
    │   │   └── references/            # reasonix session export references
    │   ├── codex/
    │   │   ├── SKILL.md               # → ~/.codex/skills/audit-autopilot/
    │   │   └── references/            # codex session export references
    │   └── ksana/
    │       ├── SKILL.md               # → ~/.ksana/skills/audit-autopilot/
    │       └── references/            # ksana session export references
    │
    ├── toolkit-setup/                 # runtime-agnostic
    │   └── SKILL.md                   # → ~/.agents/skills/toolkit-setup/
    │
    └── zoom-out/                      # runtime-agnostic
        └── SKILL.md                   # → ~/.agents/skills/zoom-out/
```

## Variant artifact type

| Skill | Reasonix artifact | Codex artifact | Ksana artifact |
|-------|------------------|----------------|----------------|
| orchestrator | `reasonix/SKILL.md` (inline skill, uses `run_skill`) | `codex/SKILL.md` (inline skill, instructs agent to `spawn agent <name>`) | `ksana/SKILL.md` (inline skill, instructs agent to call `spawn_agent` tool with `agent_type`/`task_name`/`message`) |
| implementer | `reasonix/SKILL.md` (subagent skill, `runAs: subagent`) | `codex/agent.toml` (custom agent, dispatched by orchestrator via `spawn agent`) | `ksana/agent.toml` (custom agent; `model` omitted — supplied by user's global config) |
| reviewer | `reasonix/SKILL.md` (subagent skill, `runAs: subagent`) | `codex/agent.toml` (custom agent) | `ksana/agent.toml` (custom agent; `model` omitted — supplied by user's global config) |
| audit-autopilot | `reasonix/SKILL.md` (inline skill, references `reasonix session export`) | `codex/SKILL.md` (inline skill, references Codex session mechanism) | `ksana/SKILL.md` (inline skill, references Ksana session mechanism) |

## Why implementer/reviewer are TOML not SKILL.md on Codex/Ksana

Codex and Ksana custom agents (`spawn agent`) are installed in `~/.codex/agents/*.toml` and `~/.ksana/agents/*.toml`, not as skills. The orchestrator variant for each runtime instructs the agent to spawn them by name. They don't need to be discoverable as skills — the orchestrator is their only caller.

The ksana `agent.toml` omits `model` (autopilot-toolkit is provider-agnostic and must not pin a slug), whereas the codex `agent.toml` pins `model = "gpt-5.5"`. ksana's `apply_role_to_config` rebuilds the child config from the layer stack without carrying the parent's runtime-resolved model; because the layer merge is non-destructive, a role that omits `model` inherits the `model` layer from the user's global `~/.ksana/config.toml`. Users must provide a top-level `model = "<slug>"` there (or `-c model=`) — without it, `spawn_agent` fails with `could not resolve the child model for service tier validation` when any `service_tier` candidate is present. The `--model` CLI flag does not work (it is a harness override, not a layer).

## install.rs calls per target

```
# Reasonix
install.rs sync autopilot-orchestrator skills/autopilot/autopilot-orchestrator/reasonix --target reasonix
install.rs sync autopilot-implementer  skills/autopilot/autopilot-implementer/reasonix  --target reasonix
install.rs sync autopilot-reviewer     skills/autopilot/autopilot-reviewer/reasonix     --target reasonix
install.rs sync audit-autopilot        skills/autopilot/audit-autopilot/reasonix        --target reasonix

# Codex
install.rs sync autopilot-orchestrator skills/autopilot/autopilot-orchestrator/codex --target codex
install.rs sync autopilot-implementer  skills/autopilot/autopilot-implementer/codex/agent.toml  --target codex --agent
install.rs sync autopilot-reviewer     skills/autopilot/autopilot-reviewer/codex/agent.toml     --target codex --agent
install.rs sync audit-autopilot        skills/autopilot/audit-autopilot/codex        --target codex

# Ksana
install.rs sync autopilot-orchestrator skills/autopilot/autopilot-orchestrator/ksana --target ksana
install.rs sync autopilot-implementer  skills/autopilot/autopilot-implementer/ksana/agent.toml  --target ksana --agent
install.rs sync autopilot-reviewer     skills/autopilot/autopilot-reviewer/ksana/agent.toml     --target ksana --agent
install.rs sync audit-autopilot        skills/autopilot/audit-autopilot/ksana        --target ksana

# Agnostic (all targets, same calls)
install.rs sync toolkit-setup skills/autopilot/toolkit-setup --shared
install.rs sync zoom-out      skills/autopilot/zoom-out      --shared
install.rs sync tdd           skills/upstream/skills/engineering/tdd --shared
# ... (all remaining upstream skills from .skill-lock.json)
```

## Validation impact

`validation/run.rs` must scan one level deeper for variant subdirectories. A SKILL.md in `skills/autopilot/<name>/reasonix/SKILL.md`, `.../codex/SKILL.md`, or `.../ksana/SKILL.md` is valid. It must also handle skills that have no SKILL.md in the Codex/Ksana variant (implementer, reviewer — they have agent.toml instead).

The OpenCode-specific field check must be relaxed for Codex and Ksana variant SKILL.md files (both runtimes may use these fields). Reasonix and agnostic variants still reject them.
