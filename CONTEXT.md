# Autopilot Toolkit

A skill-pack repo targeting Reasonix, Codex, and Kimi Code. Ships 19 skills — 13 upstream (from mattpocock/skills, tracked in `.skill-lock.json`) plus 6 autopilot (custom, living in `skills/autopilot/`). 15 skills are runtime-agnostic (work on any Agent Skills-compliant agent); 4 autopilot workflow skills have per-runtime variants due to differing subagent dispatch mechanisms.

## Language

**Toolkit skill**:
One of the 19 skills that autopilot-toolkit owns and installs. Always traceable to a source: either a `.skill-lock.json` entry (upstream) or a directory under `skills/autopilot/` (autopilot).
_Avoid_: project skill, owned skill

**Expected set**:
The authoritative list of toolkit skills, derived at runtime by reading `.skill-lock.json` (upstream) and scanning `skills/autopilot/*/SKILL.md` (autopilot). No separate manifest — the sources are the SSOT.
_Avoid_: skill inventory, skill manifest

**Skill source**:
The origin of a toolkit skill — either `upstream` (mattpocock/skills, synced via `.skill-lock.json`) or `autopilot` (local, under `skills/autopilot/`).
_Avoid_: skill type, skill category

**Runtime-agnostic skill**:
A skill whose body contains only methodology instructions — no references to runtime-specific tools (`run_skill`, `complete_step`), dispatch mechanisms, or CLI commands. Works on any Agent Skills-compliant agent (Reasonix, Codex, Kimi Code, Claude Code, etc.). 15 of 19 toolkit skills fall in this category.
_Avoid_: universal skill, portable skill

**Runtime-coupled skill**:
A skill whose body depends on runtime-specific mechanisms (subagent dispatch, session export, proprietary tools). The 4 autopilot workflow skills (orchestrator, implementer, reviewer, audit-autopilot) are runtime-coupled.
_Avoid_: platform-specific skill, bound skill

**Skill variant**:
A runtime-specific version of a runtime-coupled skill. Same skill identity (name, purpose), different body — the Reasonix variant uses `run_skill` dispatch and `complete_step`; the Codex variant uses `spawn agent` and `.codex/agents/*.toml` custom agents; the Kimi variant uses `Agent`-tool dispatch and reads session traces from `~/.kimi-code/sessions/`. Each variant is a separate subdirectory: `<skill>/reasonix/`, `<skill>/codex/`, or `<skill>/kimi/`.
_Avoid_: skill version, skill flavor

**Fallback variant**:
A vendor-neutral `SKILL.md` at the root of a coupled skill directory. Based on the Kimi variant with Kimi-specific path references removed. Agents without a native variant symlink to this fallback. Describes the generic workflow and instructs the agent to adapt its native dispatch mechanism.
_Avoid_: generic variant, default variant, universal variant

## Install model

**SSOT** (single source of truth):
`~/.agents/skills/` — the canonical storage for all autopilot skills after installation. All agnostic skills, all coupled skill variants, and the `.autopilot/` metadata directory live here as real directories (not symlinks to the source repo). Cross-machine sync of `~/.agents/skills/` works because no paths reference the local source tree.
_Avoid_: install root, skill store

**Bootstrap**:
The process of creating symlinks from agent-exclusive directories into the SSOT, enabling agents that don't scan `~/.agents/skills/` to discover their variants. Executed by `bootstrap.sh --target <runtime>` after initial install or on demand. Always idempotent — safe to run repeatedly. Driven purely by filesystem conventions (no hardcoded skill lists): scans `~/.agents/skills/` for `<name>/<runtime>/SKILL.md` and creates the corresponding symlink.
_Avoid_: link step, agent setup

**Bootstrap symlink**:
A symlink in an agent-exclusive directory (`~/.reasonix/skills/<name>/` or `~/.codex/skills/<name>/`) that resolves into `~/.agents/skills/<name>/<runtime>/`. Also covers Codex custom agent symlinks: `~/.codex/agents/<name>.toml` → `~/.agents/skills/<name>/codex/agent.toml`. Unlike the old repo-symlink model, these are disposable — the SSOT holds the real files.
_Avoid_: agent symlink, runtime link

**Agent-exclusive skill directory**:
A skill directory scanned by exactly one agent runtime. `~/.reasonix/skills/` (Reasonix only) and `~/.codex/skills/` (Codex only). Bootstrap symlinks are deployed here to give each runtime its variant without duplicating files in the SSOT. Kimi Code has no such directory — its coupled variants live directly in the shared `~/.agents/skills/`.
_Avoid_: private skills dir, isolated directory

**Shared skill directory**:
`~/.agents/skills/` — the Agent Skills standard shared location, scanned by Reasonix, Codex, and Kimi Code. Serves double duty: stores all autopilot skills as the SSOT, and is natively scanned by agents for skill discovery.
_Avoid_: common skills dir, public skills dir

**Custom agent** (Codex only):
A `~/.codex/agents/*.toml` file defining a named subagent with model, sandbox, and instruction configuration. Deployed during bootstrap when a coupled skill has a `<name>/codex/agent.toml` in the SSOT. Not a skill — a Codex-native subagent definition.
_Avoid_: agent config, worker definition

**Manifest**:
`~/.agents/skills/.autopilot/manifest.json` — the install ownership document. Lists every directory under `~/.agents/skills/` that belongs to the toolkit, plus metadata about each skill (type: agnostic/coupled/upstream, variants, codex_agent flag). Generated by `deploy.rs pack` during tarball assembly. Used by `install.sh` at upgrade time to determine which directories are safe to remove before extracting a new version.
_Avoid_: skill list, inventory, lockfile

**Tarball install**:
The distribution model: a single `.tar.gz` published to GitHub Releases, containing `skills/`, `.autopilot/` (bootstrap.sh, manifest.json, .version, .skill-lock.json), and `principles/`. Installed via `curl -sSL <url>/install.sh | bash`. Version is the git commit hash, embedded in install.sh and recorded in `.version`. Same version → skip download and re-run bootstrap. New version → remove manifest-listed directories, extract tarball, bootstrap all detected runtimes.
_Avoid_: package install, release install

**Operational sync** (dev-only):
`deploy.rs dev <name> <src>` — the local development shortcut that creates a symlink from an agent skills directory directly into the source repo, bypassing the tarball entirely. Retained for rapid iteration during development. Not used in production installs.
_Avoid_: dev link, local install

**Toolkit setup**:
The end-to-end install-or-update workflow. Production path: `curl | bash` → download tarball → extract → bootstrap. Development path: `deploy.rs dev` per skill (symlink-to-repo). The `toolkit-setup` skill orchestrates both paths.
_Avoid_: selfcheck, install flow

## Relationships

- The **SSOT** (`~/.agents/skills/`) is the canonical home for all toolkit skills; agent-exclusive directories hold only **bootstrap symlinks** into it
- **Bootstrap** is driven by filesystem convention: `<name>/<runtime>/SKILL.md` exists → create symlink
- The **fallback variant** (`<name>/SKILL.md`) is used when no native variant exists for the agent's runtime
- The **manifest** defines ownership: only directories it lists are removed during upgrade
- **deploy.rs pack** produces the tarball; **deploy.rs dev** provides the dev fast path
- Upstream skills are tracked in `.skill-lock.json` (source repo) and in `.autopilot/.skill-lock.json` (tarball copy)

## Autopilot Workflow

**AGENT-BRIEF**:
The contract document for a single issue: a list of Acceptance Criteria plus metadata (seams, scope boundaries). Generated by the orchestrator from an issue, consumed by the implementer.
_Avoid_: task spec, work order, PRD (PRDs are higher-level)

**Acceptance Criterion (AC)**:
One verifiable requirement within an AGENT-BRIEF. Each AC drives one TDD cycle in the implementer.
_Avoid_: task, checklist item, requirement

**Seam**:
An optional free-text annotation on an AC (`Seam: <boundary>`) that tells the implementer where to write tests (above the seam, caller-perspective) and what to mock (below). Human-authored seams take priority; the orchestrator may supplement with `Seam(inferred)`.
_Avoid_: test boundary, mock point, interface cut
