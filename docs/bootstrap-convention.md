# Bootstrap Convention

How `bootstrap.sh` discovers and links runtime-coupled skills from the SSOT into agent-exclusive directories.

## Convention

A skill is runtime-coupled if it has a subdirectory named after a runtime containing a `SKILL.md`:

```
~/.agents/skills/<name>/<runtime>/SKILL.md
```

Examples:
- `autopilot-orchestrator/reasonix/SKILL.md` — Reasonix variant
- `autopilot-orchestrator/codex/SKILL.md` — Codex variant
- `autopilot-orchestrator/kimi/SKILL.md` — Kimi variant

`bootstrap.sh` scans `~/.agents/skills/` for this pattern. For each match, it creates a symlink:

```
~/.<runtime>/skills/<name> -> ~/.agents/skills/<name>/<runtime>
```

## Codex custom agents

Additionally, if a coupled skill has `<name>/codex/agent.toml`, bootstrap creates:

```
~/.codex/agents/<name>.toml -> ~/.agents/skills/<name>/codex/agent.toml
```

## Adding a new runtime

1. Create `<name>/<new-runtime>/SKILL.md` for each coupled skill
2. Add the target directory logic to `bootstrap.sh` (mirror the Reasonix/Codex pattern)
3. No other changes needed — the filesystem convention drives discovery

## Fallback

Agents without a native variant symlink to the root-level `<name>/SKILL.md` — a vendor-neutral fallback describing the generic workflow. The fallback instructs the agent to adapt using its native subagent mechanism.
