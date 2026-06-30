# Source Repo Layout: Skill Variants

## Design principle

- **Runtime-agnostic skills**: one directory, one `SKILL.md`. No change from current layout.
- **Runtime-coupled skills**: `reasonix/` and `codex/` subdirectories under the skill directory, each containing the variant artifact.

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
    ├── autopilot-orchestrator/        # runtime-coupled: both variants are inline skills
    │   ├── reasonix/SKILL.md          # → ~/.reasonix/skills/autopilot-orchestrator/
    │   └── codex/SKILL.md             # → ~/.codex/skills/autopilot-orchestrator/
    │   └── references/                # shared references (meta-review.md, acceptance-report.md)
    │                                  #   — symlink target is the whole variant dir; reasonix/
    │                                  #     variant gets references/ via sibling symlink
    │
    ├── autopilot-implementer/         # runtime-coupled
    │   ├── reasonix/SKILL.md          # → ~/.reasonix/skills/autopilot-implementer/
    │   └── codex/agent.toml           # → ~/.codex/agents/autopilot-implementer.toml
    │                                  #    (custom agent, not a skill — use deploy-agent)
    │
    ├── autopilot-reviewer/            # runtime-coupled
    │   ├── reasonix/SKILL.md          # → ~/.reasonix/skills/autopilot-reviewer/
    │   └── codex/agent.toml           # → ~/.codex/agents/autopilot-reviewer.toml
    │
    ├── audit-autopilot/               # runtime-coupled: both variants are inline skills
    │   ├── reasonix/
    │   │   ├── SKILL.md               # → ~/.reasonix/skills/audit-autopilot/
    │   │   └── references/            # reasonix session export references
    │   └── codex/
    │       ├── SKILL.md               # → ~/.codex/skills/audit-autopilot/
    │       └── references/            # codex session export references
    │
    ├── toolkit-setup/                 # runtime-agnostic
    │   └── SKILL.md                   # → ~/.agents/skills/toolkit-setup/
    │
    └── zoom-out/                      # runtime-agnostic
        └── SKILL.md                   # → ~/.agents/skills/zoom-out/
```

## Variant artifact type

| Skill | Reasonix artifact | Codex artifact |
|-------|------------------|----------------|
| orchestrator | `reasonix/SKILL.md` (inline skill, uses `run_skill`) | `codex/SKILL.md` (inline skill, instructs agent to `spawn agent`) |
| implementer | `reasonix/SKILL.md` (subagent skill, `runAs: subagent`) | `codex/agent.toml` (custom agent, dispatched by orchestrator via `spawn agent`) |
| reviewer | `reasonix/SKILL.md` (subagent skill, `runAs: subagent`) | `codex/agent.toml` (custom agent) |
| audit-autopilot | `reasonix/SKILL.md` (inline skill, references `reasonix session export`) | `codex/SKILL.md` (inline skill, references Codex session mechanism) |

## Why implementer/reviewer are TOML not SKILL.md on Codex

Codex custom agents (`spawn agent`) are installed in `~/.codex/agents/*.toml`, not as skills. The Codex orchestrator variant instructs the agent to spawn them by name. They don't need to be discoverable as skills — the orchestrator is their only caller.

## install.rs calls per target

```
# Reasonix
install.rs sync autopilot-orchestrator skills/autopilot/autopilot-orchestrator/reasonix --target reasonix
install.rs sync autopilot-implementer  skills/autopilot/autopilot-implementer/reasonix  --target reasonix
install.rs sync autopilot-reviewer     skills/autopilot/autopilot-reviewer/reasonix     --target reasonix
install.rs sync audit-autopilot        skills/autopilot/audit-autopilot/reasonix        --target reasonix

# Codex
install.rs sync autopilot-orchestrator skills/autopilot/autopilot-orchestrator/codex --target codex
install.rs deploy-agent autopilot-implementer skills/autopilot/autopilot-implementer/codex/agent.toml --target codex
install.rs deploy-agent autopilot-reviewer    skills/autopilot/autopilot-reviewer/codex/agent.toml    --target codex
install.rs sync audit-autopilot        skills/autopilot/audit-autopilot/codex        --target codex

# Agnostic (both targets, same calls)
install.rs sync toolkit-setup skills/autopilot/toolkit-setup --shared
install.rs sync zoom-out      skills/autopilot/zoom-out      --shared
install.rs sync tdd           skills/upstream/skills/engineering/tdd --shared
# ... (all 13 upstream)
```

## Validation impact

`validation/run.rs` must scan one level deeper for variant subdirectories. A SKILL.md in `skills/autopilot/<name>/reasonix/SKILL.md` is valid. It must also handle skills that have no SKILL.md in the Codex variant (implementer, reviewer — they have agent.toml instead).

The `disable-model-invocation` field check must be relaxed for Codex variant SKILL.md files (Codex may use this field). Consider per-variant validation rules or simply validate the `name` + `description` fields universally.
