# ADR 0008: Tri-Runtime Skill Variants — Adding Ksana

## Context

ADR-0007 established a dual-runtime model (Reasonix + Codex) for the 4 runtime-coupled autopilot skills: each maintains `reasonix/` and `codex/` variant sources, installed into agent-exclusive directories (`~/.reasonix/skills/`, `~/.codex/skills/`) plus Codex custom agents under `~/.codex/agents/`.

A third runtime, **ksana**, now needs support. Ksana is structurally identical to Codex:

- Dispatches subagents via `spawn agent <name>`.
- Discovers custom agents from `~/.ksana/agents/*.toml`.
- Scans `~/.ksana/skills/` (agent-exclusive) and the shared `~/.agents/skills/`.

The codex variant sources hardcode two runtime-specific values that prevent direct reuse:

- `autopilot-orchestrator/codex/SKILL.md` references `~/.codex/agents/*.toml`.
- The codex `agent.toml` files pin `model = "gpt-5.5"`.

## Decision

**Extend the dual-runtime model to a tri-runtime model with a dedicated `ksana/` variant directory per coupled skill — not by aliasing the codex source.**

1. **Source**: Each of the 4 coupled skills gains a `ksana/` subdirectory alongside `reasonix/` and `codex/`:
   - `autopilot-orchestrator/ksana/SKILL.md` (+ `references/`)
   - `audit-autopilot/ksana/SKILL.md` (+ `references/`)
   - `autopilot-implementer/ksana/agent.toml`
   - `autopilot-reviewer/ksana/agent.toml`

2. **Install**: `install.rs` accepts `--target reasonix|codex|ksana`. Coupled ksana skills symlink to `~/.ksana/skills/`; implementer/reviewer deploy as `~/.ksana/agents/<name>.toml` via `sync --target ksana --agent`. Agnostic skills remain in the shared `~/.agents/skills/`.

3. **agent.toml omits `model` — the user's global config supplies it.** The ksana `agent.toml` files deliberately do **not** pin `model`: autopilot-toolkit is provider-agnostic and must not assume a slug. ksana's `apply_role_to_config` rebuilds the child config from the layer stack and does **not** carry the parent's runtime-resolved model into the child (`reload_overrides` preserves only `model_provider`/`service_tier`). The layer-stack merge is non-destructive, so a role that omits `model` inherits the `model` layer from the user's global `~/.ksana/config.toml` (or `-c model=`). The user is responsible for providing a top-level `model = "<slug>"` there; without it, `spawn_agent` fails with `could not resolve the child model for service tier validation` when any `service_tier` candidate is present. (The `--model` CLI flag does **not** work here — it is a harness override that does not enter the layer stack and is lost on child rebuild.) This matches ksana's own built-in roles (e.g. `builtins/awaiter.toml` omits `model`, relying on the global config). The earlier draft of this ADR that pinned `model = "eden/glm-5.2"` was wrong and was reversed after this surfaced.

4. **Validation**: `SkillVariant` gains a `Ksana` arm. The OpenCode-field check (Check 3) is relaxed for both `Codex` and `Ksana`, since both are Codex-aligned runtimes.

5. **Unlink-all** now cleans four directories: `~/.reasonix/skills/`, `~/.codex/skills/`, `~/.ksana/skills/`, `~/.agents/skills/` (plus both agents dirs when `--agent` is set without `--target`).

## Ksana vs Codex dispatch divergence

ksana is codex-aligned in **install layout** but not in **dispatch**. The ksana variant sources reflect this:

- **Custom agents = roles.** ksana auto-discovers `~/.ksana/agents/*.toml` at startup; the file's `name` field is the role name (filename does not fall back). `name`/`description`/`developer_instructions` are required.
- **Dispatch tool differs.** ksana uses the `spawn_agent` tool with `{ task_name, agent_type, message }` — not codex's `spawn agent <name> with task:` prose. `agent_type` matches the role `name`. The orchestrator `SKILL.md` uses the tool-call form accordingly.
- **agent.toml fields.** `sandbox_mode` (kebab-case), `developer_instructions` are valid `ConfigToml` fields. `model` is intentionally omitted — ksana's child-role rebuild does not inherit the parent's runtime model, so the `model` layer must come from the user's global config (see Decision §3).

## Alternatives considered

### A. Alias ksana to the codex source

Point ksana at the existing `codex/` files and only add a `--target ksana` route. Rejected: the codex source hardcodes `~/.codex/agents` (would misdirect ksana users) and `model = "gpt-5.5"` (ksana should use its own default). Generalizing the codex source to runtime-neutral language would couple two runtimes' evolution; any future codex-vs-ksana divergence would then require re-splitting.

### B. Generalize codex source to runtime-neutral language, share between codex and ksana

Rewrite the codex orchestrator SKILL.md and agent.toml to say "your runtime's agents directory" and drop the model pin, then share one source. Rejected for the same drift reason as A: it sacrifices the codex source's specificity (the `~/.codex/agents` path is a verified, concrete fact per ADR-0007) to serve two runtimes, and forces codex to lose its pinned model.

### C. Compatibility-field filtering

Install one variant under `~/.agents/skills/` and rely on a `compatibility` field. Rejected for the same reasons as ADR-0007 alternative B: unverified filtering behavior, name collisions, orchestrator dispatch complexity.

## Consequences

- **Maintenance**: 4 skills × 3 variants = 12 variant files. Upstream and agnostic autopilot skills (the other 17) remain single-source.
- **Drift risk**: Workflow-logic changes must be applied to three variant bodies (reasonix, codex, ksana). Mitigation: all three implement the same workflow phases; only dispatch mechanism and (for ksana) model omission differ. The codex and ksana variants are near-identical, so a diff-driven review catches drift.
- **install.rs**: Routes to a fourth skills directory and a second agents directory. The `--agent` flag now resolves its target directory from `--target` (codex or ksana) rather than being hardwired to codex.
- **Verification**: Install and toolkit-setup tests cover `--target ksana` end-to-end (skill sync + agent deploy + unlink).
- **Pre-existing fix**: This work also corrected `test_toolkit_setup.rs`, which called a `deploy-agent` subcommand that never existed in `install.rs` (the real interface is `sync --target <t> --agent`); the codex target test was failing on `main` and is now fixed alongside the ksana additions.
