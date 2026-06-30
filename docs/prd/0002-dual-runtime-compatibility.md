## Problem Statement

autopilot-toolkit currently targets only Reasonix. While 17 of 19 skills are runtime-agnostic (pure methodology instructions usable by any Agent Skills-compliant agent), the 4 autopilot workflow skills (orchestrator, implementer, reviewer, audit-autopilot) are coupled to Reasonix-specific mechanisms: `run_skill` dispatch, `runAs: subagent`, and `complete_step`. Codex has equivalent but structurally different mechanisms: `spawn agent` dispatch, `~/.codex/agents/*.toml` custom agents, and no `runAs` concept in SKILL.md.

The shared `~/.agents/skills/` directory (Agent Skills standard) creates a naming conflict if both variants are installed there. Users need a clean way to install the toolkit for either runtime — or both simultaneously — without cross-contamination.

## Solution

**Per-runtime skill variants + agent-exclusive install directories.**

- 17 runtime-agnostic skills stay as-is in `~/.agents/skills/` (shared, both agents discover them).
- 4 runtime-coupled skills maintain two variants in source: `reasonix/SKILL.md` and `codex/SKILL.md` (or `codex/agent.toml` for implementer/reviewer).
- install.rs gains `--target reasonix|codex` and `--shared` flags. Runtime-coupled skills are installed to agent-exclusive directories (`~/.reasonix/skills/` or `~/.codex/skills/`), eliminating cross-agent conflicts without depending on `compatibility` field filtering.
- Codex implementer and reviewer are deployed as custom agents (`~/.codex/agents/*.toml`), not as skills — matching Codex's native subagent dispatch model.

## Assumptions (to verify before implementation)

| # | Assumption | Status | Verification |
|---|-----------|--------|-------------|
| A1 | Codex inline skill can autonomously trigger `spawn agent` (not just via user prompt) | ✅ Verified | Smoke test skill at `~/.codex/skills/a1-spawn-test/` (#42) — Codex successfully spawns agents from within skill body |
| A2 | Codex has a session export mechanism equivalent to `reasonix session export` | ⏳ Deferred | audit-autopilot codex variant (#49) uses `TODO: codex session export — TBD` placeholders; session export mechanism not yet researched |
| A3 | `~/.reasonix/skills/` is a stable, supported Reasonix skill directory | ✅ Verified | Empirically confirmed Reasonix scans `~/.reasonix/skills/` (2025-06-29); directory used for all runtime-coupled Reasonix installs |

## User Stories

1. As a Reasonix user, I want to install autopilot-toolkit with `--target reasonix`, so that all 19 skills are available with optimal subagent isolation for the autopilot workflow.
2. As a Codex user, I want to install autopilot-toolkit with `--target codex`, so that all 19 skills are available with native `spawn agent` dispatch for the autopilot workflow.
3. As a user of both Reasonix and Codex on the same machine, I want runtime-coupled skills installed to agent-exclusive directories, so that Codex doesn't load Reasonix-specific subagent skills and vice versa.
4. As a user of any Agent Skills-compliant agent, I want the 17 runtime-agnostic skills available from the shared `~/.agents/skills/` directory, so that I can use them regardless of which agent I'm running.
5. As a toolkit maintainer, I want a single install.rs script that handles both targets via a flag, so that I don't maintain separate install scripts per platform.
6. As a toolkit maintainer, I want runtime-coupled skill variants stored alongside each other in the source tree (`reasonix/` and `codex/` subdirectories), so that I can see and update both variants together.
7. As a toolkit maintainer, I want Codex implementer and reviewer defined as `~/.codex/agents/*.toml` custom agents, so that they integrate natively with Codex's subagent dispatch without emulating the Reasonix `runAs: subagent` model.
8. As a toolkit maintainer, I want `install.rs unlink` to clean up across all three directories (shared + reasonix + codex), so that `toolkit-setup` can fully tear down a previous install regardless of target.
9. As a toolkit maintainer, I want existing Reasonix-only users unaffected — the default `--target reasonix` preserves backward compatibility.

## Implementation Decisions

### install.rs changes

- New `--target reasonix|codex` flag (default: `reasonix` for backward compat).
- New `--shared` flag routes to `~/.agents/skills/` regardless of `--target`.
- New `deploy-agent <name> <src>` subcommand for Codex custom agent TOML deployment to `~/.codex/agents/`.
- Environment variables for directory overrides: `AGENTS_SKILLS_DIR` (shared), `REASONIX_SKILLS_DIR`, `CODEX_SKILLS_DIR`, `CODEX_AGENTS_DIR`.
- `unlink` without `--target` cleans all three directories; with `--target` cleans only that target's directory.

### Source repo layout

Runtime-agnostic skills unchanged. Runtime-coupled skills gain per-runtime subdirectories:

```
skills/autopilot/<name>/
├── reasonix/SKILL.md    # Reasonix variant (all 4 coupled skills)
└── codex/
    ├── SKILL.md         # Codex variant (orchestrator, audit-autopilot)
    └── agent.toml       # Codex variant (implementer, reviewer — custom agents)
```

install.rs `sync` symlinks the agent directory to the variant subdirectory (e.g., `~/.reasonix/skills/autopilot-orchestrator → .../reasonix/`). This preserves the existing directory-level symlink model.

### Why implementer/reviewer are TOML on Codex

Codex subagents are installed in `~/.codex/agents/*.toml` with `developer_instructions` as the body. They are dispatched by name via `spawn agent`, not via skill loading. Making them skills would add unnecessary indirection. The orchestrator's Codex variant instructs the agent to spawn them directly by name.

### Variant body differences

| Aspect | Reasonix variant | Codex variant |
|--------|-----------------|---------------|
| Dispatch | `run_skill(name: "autopilot-implementer", arguments: "...")` | `spawn agent autopilot-implementer with task: "..."` |
| Subagent definition | `runAs: subagent` in SKILL.md frontmatter | `~/.codex/agents/implementer.toml` file |
| Step sign-off | `complete_step` tool | Not applicable (custom agent reports result directly) |
| Session export | `reasonix session export` | Codex session mechanism (TODO — deferred) |
| Tool allowlist | `allowed-tools` in SKILL.md | `dependencies.tools` in `agents/openai.yaml` (optional) |

### Validation impact

`validation/run.rs` must scan one level deeper for variant subdirectories. Codex variants without SKILL.md (implementer, reviewer) are valid — validation should not require SKILL.md for every skill directory. The `disable-model-invocation` field check should be relaxed for Codex variants (Codex may use this field).

### ADR 0007

Full decision record at `docs/adr/0007-dual-runtime-skill-variants.md`. Records alternatives considered: single-body conditional language, `compatibility` field filtering in shared directory, and single-runtime only.

## Testing Decisions

### What makes a good test

- Test install.rs produces correct symlink structure for each `--target` / `--shared` combination.
- Test symlink targets resolve to correct variant subdirectories.
- Test `deploy-agent` copies TOML to correct `~/.codex/agents/` path.
- Test `unlink` cleans correct directories.
- Test real-directory conflict detection works in agent-exclusive directories too.
- Do NOT test SKILL.md body content correctness (that's per-variant implementation, not install infrastructure).

### Verification seams

| Seam | Method |
|------|--------|
| `sync --target reasonix` | Symlink at `~/.reasonix/skills/<name>` → `<project>/skills/autopilot/<name>/reasonix` |
| `sync --target codex` | Symlink at `~/.codex/skills/<name>` → `<project>/skills/autopilot/<name>/codex` |
| `sync --shared` | Symlink at `~/.agents/skills/<name>` → `<project>/skills/...` (unchanged) |
| `deploy-agent --target codex` | File at `~/.codex/agents/<name>.toml` with content matching source |
| `unlink` (no target) | No symlinks under toolkit project_root in any of the three directories |
| `unlink --target codex` | No symlinks under project_root in `~/.codex/skills/` only |

### Existing test infrastructure

`tests/test_install.sh` already covers `sync`/`unlink` for the shared directory. Extend with `--target` and `--shared` variants following the same assert-style pattern.

## Out of Scope

- Writing the actual Codex variant SKILL.md bodies (body content for orchestrator-codex, implementer-codex, reviewer-codex, audit-autopilot-codex). This PRD covers the install infrastructure and source layout; variant bodies are separate issues.
- Codex `agents/openai.yaml` metadata files — nice-to-have for UI polish, not needed for functional autopilot workflow.
- `compatibility` field in SKILL.md frontmatter — agent-exclusive directories make this unnecessary for conflict avoidance.
- Session export implementation for audit-autopilot (both Reasonix and Codex) — still deferred as in PRD 0001.
- Cross-agent skill discovery testing (verifying Reasonix doesn't see Codex variants and vice versa) — requires both agents installed; smoke test separately.

## Further Notes

- The 17 runtime-agnostic skills do not change at all. Only 4 autopilot skills gain a second variant.
- `toolkit-setup` skill body must be updated to pass `--target` and route skills to correct directories. This is the only runtime-agnostic skill that needs modification.
- The `CODEOWNERS` or maintenance convention should ensure both variants of a skill are updated together — variant drift is the main long-term risk.
- Verification assumptions A1-A3 should be resolved before implementation begins. If A1 fails (Codex can't autonomously spawn agents), the Codex orchestrator variant design changes materially.
