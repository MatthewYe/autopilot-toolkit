# ADR 0007: Dual-Runtime Skill Variants with Agent-Exclusive Install Directories

## Context

autopilot-toolkit targets both Reasonix and Codex. While 17 of 19 skills are runtime-agnostic (pure methodology instructions), the 4 autopilot workflow skills (orchestrator, implementer, reviewer, audit-autopilot) depend on runtime-specific mechanisms:

- Reasonix: `run_skill` dispatch, `runAs: subagent` in SKILL.md frontmatter, `complete_step` tool
- Codex: `spawn agent` dispatch, `~/.codex/agents/*.toml` custom agent definitions, no `runAs` concept in SKILL.md

Both agents scan `~/.agents/skills/` for skill discovery (Agent Skills standard). A single SKILL.md body cannot serve both runtimes for runtime-coupled skills — the dispatch mechanisms are fundamentally different.

## Decision

**Use per-runtime skill variants with agent-exclusive install directories.**

1. **Source**: Each runtime-coupled skill maintains two variant sources — `SKILL.reasonix.md` and `SKILL.codex.md` — in its source directory under `skills/autopilot/<name>/`.

2. **Install**: `install.rs` accepts `--target reasonix|codex`. Runtime-agnostic skills (17) go to the shared `~/.agents/skills/`. Runtime-coupled skills (4) go to the agent-exclusive directory:
   - `--target reasonix` → symlink `SKILL.reasonix.md` as `~/.reasonix/skills/<name>/SKILL.md`
   - `--target codex` → symlink `SKILL.codex.md` as `~/.codex/skills/<name>/SKILL.md`

3. **Codex custom agents**: For implementer and reviewer, the Codex install also deploys `~/.codex/agents/autopilot-implementer.toml` and `~/.codex/agents/autopilot-reviewer.toml` — Codex-native subagent definitions the orchestrator references via `spawn agent`.

   `~/.codex/agents` recognition was verified empirically with Codex v0.142.3 on 2026-06-30 by placing a temporary invalid TOML file in that directory and observing Codex report it during interactive startup.

4. **No `compatibility` field dependency**: Agent-exclusive directories eliminate cross-agent visibility conflicts. Reasonix does not scan `~/.codex/skills/`; Codex does not scan `~/.reasonix/skills/` (verified empirically 2025-06-29).

## Alternatives considered

### A. Single body with conditional instructions

Write runtime-neutral dispatch language (e.g., "dispatch to implementer using your platform's subagent mechanism"). Rejected: too ambiguous — agents hallucinate tool names or misinterpret instructions. The mechanisms are not semantically equivalent enough to abstract.

### B. Shared directory with `compatibility` field filtering

Install both variants to `~/.agents/skills/` under different names, rely on `compatibility: requires Reasonix` / `compatibility: requires Codex` for each agent to filter. Rejected: the `compatibility` field's filtering behavior is not guaranteed by the Agent Skills spec and has not been verified across agent versions. Also requires different skill names (`autopilot-implementer` vs `autopilot-implementer-codex`), complicating orchestrator dispatch.

### C. Single-runtime only (Reasonix)

Maintain only Reasonix variants; declare Codex unsupported for autopilot workflow. Rejected: the project's goal is dual-runtime compatibility for the complete skill set, not just the runtime-agnostic subset.

## Consequences

- **Maintenance**: 4 skills × 2 variants = 8 variant files to maintain. Upstream skills and runtime-agnostic autopilot skills (17 total) remain single-source.
- **install.rs complexity**: Must route skills to different directories based on `--target` and skill category. Must also handle Codex custom agent TOML deployment.
- **Variant drift risk**: Changes to workflow logic must be applied to both variants. Mitigation: both variants implement the same workflow phases (scan → implement → review → retry); only the dispatch mechanism differs.
- **Verification**: Install tests must cover both `--target reasonix` and `--target codex` paths.
