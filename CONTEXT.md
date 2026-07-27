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

**Distill**:
An ordered intake workflow that turns a raw requirement into a clarified, archived PRD and a set of sliced implementation issues.
_Avoid_: requirements skill, clarification skill

**Distill run**:
One resumable execution of Distill within a specific target worktree. It is bound to one agent session at a time, and its authoritative state, snapshots, completion evidence, and published-artifact references live under that worktree's `.distill/` directory.
_Avoid_: chat, session, workflow instance

**Session binding**:
The exclusive association between an unfinished Distill run and the agent session driving it. A session may own at most one unfinished run; an explicit takeover may move the binding to another session and records the handoff.
_Avoid_: project binding, implicit resume

**Distill workflow definition**:
A versioned, declarative sequence of Distill stages and their assigned executors. A run snapshots the definition it starts with, so later workflow changes affect new runs without altering in-progress runs.
_Avoid_: hard-coded pipeline, skill chain

**autopilot-distill skill**:
The agent-facing entry point for starting, driving, and resuming Distill runs. It wraps the installed `distill` CLI runner but does not own workflow state, authorize stage transitions, or rename the CLI contract.
_Avoid_: Distill runner, workflow controller, distill skill

**Distill stage**:
One gated phase of a Distill run. Its assigned executor declares completion by submitting valid completion evidence; users provide requirement knowledge but do not decide whether the executor has enough information.
_Avoid_: prompt step, checklist item

**Stage executor adapter**:
A Distill-owned boundary that assigns a skill or other executor to a stage and translates between that executor's natural interaction and the stage's stable input, completion-evidence, and artifact-validation contracts.
_Avoid_: wrapped skill, modified upstream skill

**Requirement input snapshot**:
The immutable content captured when a Distill run begins, including the user's text, uploaded file contents, and fetched link contents together with their provenance. Later stages consume the snapshot rather than re-reading mutable sources.
_Avoid_: requirement reference, source link

**Purged run**:
A Distill run whose reproducible content was explicitly deleted while a tombstone retaining its identity, content hashes, purge time, and published-artifact references remains.
_Avoid_: deleted run, completed run

**Distill storage budget**:
The enforced per-source, per-run, and per-project limits on project-local Distill data. Exceeding a budget blocks new writes without truncating or automatically deleting retained history.
_Avoid_: cache size, retention period

**Superseded run**:
A terminal Distill run replaced by a linked successor after material new information made an already-completed stage obsolete. Its history and published artifacts remain visible rather than being rewritten.
_Avoid_: rewound run, edited run

**Completion evidence**:
A structured declaration from a Distill stage executor that the stage contract has been satisfied. It records any accepted assumptions; no unrecorded unknown may remain if it could materially change the solution direction. The Distill runner validates this evidence before permitting the next stage.
_Avoid_: done message, user approval

**Publication record**:
The durable mapping from a Distill publication operation to the PRD or implementation issue it created in the configured issue tracker. It lets an interrupted run reconcile uncertain results and continue without duplicating artifacts.
_Avoid_: publish response, issue URL list

**Publication payload**:
The immutable local copy of the exact PRD or implementation-issue content submitted to the configured issue tracker. Its hash supports reconciliation and drift detection without authorizing Distill to overwrite later human edits.
_Avoid_: tracker cache, draft

**Completion report**:
The machine-readable canonical summary of a completed Distill run, covering its inputs, clarified requirement, decisions, assumptions, domain-document changes, published artifacts, warnings, versions, session, revision, and storage use.
_Avoid_: final message, Markdown report

**Report renderer**:
A replaceable, non-authoritative completion hook that projects a completion report into a human-readable form. Rendering may be retried or upgraded without changing run completion or the canonical report.
_Avoid_: workflow stage, completion validator

**AGENT-BRIEF**:
The contract document for a single issue: a list of Acceptance Criteria plus metadata (seams, scope boundaries). Generated by the orchestrator from an issue, consumed by the implementer.
_Avoid_: task spec, work order, PRD (PRDs are higher-level)

**Acceptance Criterion (AC)**:
One verifiable requirement within an AGENT-BRIEF. Each AC drives one TDD cycle in the implementer.
_Avoid_: task, checklist item, requirement

**Seam**:
An optional free-text annotation on an AC (`Seam: <boundary>`) that tells the implementer where to write tests (above the seam, caller-perspective) and what to mock (below). Human-authored seams take priority; the orchestrator may supplement with `Seam(inferred)`.
_Avoid_: test boundary, mock point, interface cut
