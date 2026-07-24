# Fallback variants for coupled skills

**Status**: ready-for-agent
**Parent**: [PRD 0004](../prd/0004-self-contained-tarball-install.md)

## What to build

Each of the 4 runtime-coupled skills needs a root-level `SKILL.md` that serves as a vendor-neutral fallback for agents without a native variant. These fallbacks are based on the Kimi variant — the closest to "generic" — with Kimi-specific path references removed.

The fallback must:
- Describe the generic workflow (scan → implement → review → retry for orchestrator, TDD cycle for implementer, four-axis review for reviewer, trace analysis for audit-autopilot)
- Instruct the agent to use its native subagent/spawn/worker mechanism for dispatch
- Contain no references to `~/.kimi-code/sessions/`, Kimi-specific tools, or any runtime-proprietary APIs
- Be included in the tarball at `<name>/SKILL.md` (alongside the subdirectory variants)

## Acceptance criteria

- [ ] `autopilot-orchestrator/SKILL.md` describes the scan → dispatch → review → retry loop in vendor-neutral terms
- [ ] `autopilot-implementer/SKILL.md` describes the TDD implementation cycle in vendor-neutral terms
- [ ] `autopilot-reviewer/SKILL.md` describes the four-axis review in vendor-neutral terms
- [ ] `audit-autopilot/SKILL.md` describes the fidelity audit in vendor-neutral terms
- [ ] No fallback contains Kimi-specific paths (`~/.kimi-code/`) or tool references
- [ ] No fallback contains Reasonix-specific (`run_skill`, `complete_step`) or Codex-specific (`spawn_agent`, `agent.toml`) references
- [ ] Fallbacks are included in the tarball by `install.rs build`
- [ ] Each fallback has valid frontmatter (name + description) passable by validation

## Blocked by

- [#14 Build pipeline](14-build-pipeline.md) (must be built into tarball; can be developed in parallel with #15 and #16)

## Seam

Seam 4 (manifest boundary): test that fallback files are present in tarball and have correct content.
