## Problem Statement

opencode-toolbox 当前将技能分散在多个来源（upstream git subtree、本地 toolbox skills、`~/.agents/skills/`），并与 opencode CLI 深度耦合（plugin 机制、slash command、agent 权限配置）。用户需要在 reasonix/codex 环境中复用这套技能资产，同时剥离对 opencode 的依赖。

## Solution

创建独立项目 `autopilot-toolkit`，将精选技能（上游 14 个 + autopilot 4 个）重构为纯 SKILL.md 文件，安装到 `~/.agents/skills/` 目录下。项目不依赖任何 CLI，通过 `install.sh` 符号链接部署。上游技能通过 git subtree 保持可追踪更新。

## User Stories

1. As a reasonix/codex user, I want to install autopilot-toolkit skills into `~/.agents/skills/`, so that reasonix can discover and invoke them
2. As a reasonix/codex user, I want the autopilot workflow (orchestrator → implementer → reviewer) to work as pure skills, so that I can use autopilot without opencode
3. As a reasonix/codex user, I want to audit autopilot execution fidelity, so that I can evaluate how faithfully the workflow executed against its contract
4. As a toolkit maintainer, I want to sync upstream skills (mattpocock/skills) via git subtree, so that I can pull updates without manual copying
5. As a toolkit maintainer, I want a clean separation between upstream skills and autopilot-specific skills, so that I can manage them independently
6. As a reasonix/codex user, I want the skills to have minimal frontmatter (name, description, runAs), so that they are compatible with the reasonix skill format
7. As a user, I want the install script to create symlinks (not copies), so that updates to the toolkit are immediately reflected in `~/.agents/skills/`
8. As a reasonix/codex user, I want the TDD, diagnose, and zoom-out skills available as supporting skills for the autopilot implementer workflow
9. As a user, I want the grill-with-docs skill available, so that I can stress-test plans against domain models
10. As a user, I want the caveman productivity skill available, so that I can reduce token usage when needed

## Implementation Decisions

### Source inventories

**Upstream (14 skills)** — tracked from `mattpocock/skills` via git subtree, listed in `.skill-lock.json`:

| Category | Skills |
|----------|--------|
| Engineering | diagnose, grill-with-docs, improve-codebase-architecture, prototype, setup-matt-pocock-skills, tdd, to-issues, to-prd, triage |
| Productivity | caveman, grill-me, handoff, teach, write-a-skill |

**Autopilot (5 skills)** — converted from opencode agents/commands → reasonix-compatible skills:

| Skill | Source | runAs |
|-------|--------|-------|
| autopilot-orchestrator | `commands/autopilot.md` | inline |
| autopilot-implementer | `agents/implementer.md` | subagent |
| autopilot-reviewer | `agents/reviewer.md` | subagent |
| audit-autopilot | `skills/audit-autopilot/` | inline |
| toolkit-selfcheck | new | inline |

### Directory structure

```
autopilot-toolkit/
├── README.md
├── install.sh
├── .skill-lock.json
├── .gitignore
├── docs/
│   └── prd/
│       └── 0001-autopilot-toolkit.md
├── skills/
│   ├── upstream/           # 14 skills from mattpocock/skills (git subtree)
│   └── autopilot/          # 4 autopilot-specific skills
│       ├── autopilot-orchestrator/
│       │   └── SKILL.md
│       ├── autopilot-implementer/
│       │   └── SKILL.md
│       ├── autopilot-reviewer/
│       │   └── SKILL.md
│       └── audit-autopilot/
│           ├── SKILL.md
│           └── references/
```

### Frontmatter conversion

Remove opencode-specific fields (`compatibility`, `mode`, `disable-model-invocation`, `permission`) from all SKILL.md files. Replace agent config frontmatter with reasonix-compatible fields:

- `name` — skill identifier, all lowercase, hyphens allowed
- `description` — one-liner for skill index
- `runAs` — `inline` (default) or `subagent` (for skills that spawn child loops)
- `allowed-tools` — comma-separated tool list for subagent skills (tool names to be finalized against reasonix tool registry)

### Agent → Skill conversion strategy

1. **autopilot-orchestrator**: Convert from slash command to inline skill. Remove shell-expansion syntax (`$1`, `$2`). Keep the orchestration workflow logic. Remove `task` tool dispatch instructions — reasonix uses `run_skill` with `runAs: subagent` instead.

2. **autopilot-implementer**: Convert from opencode subagent to reasonix subagent skill. Set `runAs: subagent`. Remove `permission` block (reasonix uses `allowed-tools`). Remove agent-specific `ROUND`/`PREV_REVIEW`/`TOOLCHAIN` variables (those are injected by the orchestrator at invoke time).

3. **autopilot-reviewer**: Same conversion as implementer. Read-only subagent with `runAs: subagent`.

4. **audit-autopilot**: Remove `compatibility: opencode`. References to `opencode export` and `opencode session list` CLI commands are replaced with reasonix equivalents (session export mechanism TBD — mark as placeholder). Keep the audit methodology intact.

### Git subtree tracking

Maintain `.skill-lock.json` pointing to `mattpocock/skills` with per-skill hashes and install dates. The `install.sh` script adds the upstream subtree if not already present. Upstream updates are pulled via `git subtree pull --prefix skills/upstream mattpocock-skills main`.

### Unchanged items

- The original `opencode-toolbox` project remains untouched — this is a new, separate project
- All skill body content and logic is preserved; only frontmatter and opencode-specific references are adapted
- The `.skill-lock.json` structure is maintained for upstream tracking

## Testing Decisions

### What makes a good test

- Test the install script produces correct symlink structure
- Test all SKILL.md files parse with valid frontmatter (no opencode-specific keys)
- Test reasonix skill discovery can find and index all 18 skills
- Test upstream git subtree is correctly configured and pullable
- Test that excluded items (skill-creator, backtest, quant, quant-scheduled, proxy-subscription-parser, rust-artisan series, surge-cli) are NOT present

### Verification seams

| Seam | Method |
|------|--------|
| Directory structure | `ls skills/upstream/` + `ls skills/autopilot/` — count and name match |
| Git tracking | `git remote` shows mattpocock-skills subtree; `.skill-lock.json` present and valid |
| Install | `bash install.sh` creates symlinks in `~/.agents/skills/` pointing to project dirs |
| Frontmatter | All SKILL.md files have `name` and `description`; no `compatibility`/`mode`/`disable-model-invocation`/`permission` keys |
| reasonix compatibility | `name` fields match `^[a-zA-Z0-9][a-zA-Z0-9._-]{0,63}$`; `runAs` is `inline` or `subagent` |

## Out of Scope

- **skill-creator** and its subagents — excluded entirely
- **backtest**, **quant**, **quant-scheduled** — excluded entirely
- **proxy-subscription-parser** — excluded (utility, not autopilot)
- **rust-artisan**, **rust-artisan-v2**, **rust-coder** — excluded (reference-only, not autopilot)
- **surge-cli** — excluded (utility, not autopilot)
- **argus** agent — excluded (image analysis, not autopilot core)
- **python-uv** — excluded (opencode config skill)
- Full reasonix format adaptation for subagent tool lists — placeholder values; final tool names TBD against reasonix registry
- Session export and trace analysis in audit-autopilot — reasonix equivalents TBD; defer to follow-up
- Package publication (npm) — not needed for `~/.agents/` deployment

## Further Notes

- The project name `autopilot-toolkit` reflects its scope: upstream skills + autopilot workflow skills, delivered as a toolkit
- The install script is idempotent — re-running it updates broken symlinks without duplicating
- Upstream skill bodies are preserved verbatim (git subtree); only the wrapping frontmatter may need adjustment if upstream adds opencode-specific fields in future pulls
- Conversion of autopilot agent skills to reasonix format is structural in this round; detailed tool name mapping and session export mechanism are deferred
