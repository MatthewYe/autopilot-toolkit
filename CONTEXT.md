# Autopilot Toolkit

A skill-pack repo targeting Reasonix and Codex. Ships 19 skills — 13 upstream (from mattpocock/skills, tracked in `.skill-lock.json`) plus 6 autopilot (custom, living in `skills/autopilot/`). 17 skills are runtime-agnostic (work on any Agent Skills-compliant agent); 4 autopilot workflow skills have per-runtime variants due to differing subagent dispatch mechanisms.

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
A skill whose body contains only methodology instructions — no references to runtime-specific tools (`run_skill`, `complete_step`), dispatch mechanisms, or CLI commands. Works on any Agent Skills-compliant agent (Reasonix, Codex, Claude Code, etc.). 17 of 19 toolkit skills fall in this category.
_Avoid_: universal skill, portable skill

**Runtime-coupled skill**:
A skill whose body depends on runtime-specific mechanisms (subagent dispatch, session export, proprietary tools). The 4 autopilot workflow skills (orchestrator, implementer, reviewer, audit-autopilot) are runtime-coupled.
_Avoid_: platform-specific skill, bound skill

**Skill variant**:
A runtime-specific version of a runtime-coupled skill. Same skill identity (name, purpose), different body — the Reasonix variant uses `run_skill` dispatch and `complete_step`; the Codex variant uses `spawn agent` and `.codex/agents/*.toml` custom agents. Each variant is a separate source file: `SKILL.reasonix.md` or `SKILL.codex.md`.
_Avoid_: skill version, skill flavor

**Variant source**:
A file in the source tree that carries a specific runtime variant of a runtime-coupled skill. Named `SKILL.<runtime>.md` (e.g. `SKILL.reasonix.md`, `SKILL.codex.md`) alongside any runtime-agnostic supporting files. The install script selects the matching variant based on `--target`.
_Avoid_: variant file, alternate body

## Install model

**Install target**:
The directory where a skill symlink is deployed. Varies by skill category and runtime target:

| Skill category | Reasonix target | Codex target |
|---|---|---|
| Runtime-agnostic | `~/.agents/skills/<name>/` | `~/.agents/skills/<name>/` |
| Runtime-coupled | `~/.reasonix/skills/<name>/` | `~/.codex/skills/<name>/` |

_Avoid_: skills dir, agents skills

**Agent-exclusive skill directory**:
A skill directory scanned by exactly one agent runtime. `~/.reasonix/skills/` (Reasonix only) and `~/.codex/skills/` (Codex only). Runtime-coupled skill variants are installed here to eliminate cross-agent conflicts without relying on `compatibility` field filtering.
_Avoid_: private skills dir, isolated directory

**Shared skill directory**:
`~/.agents/skills/` — the Agent Skills standard shared location, scanned by both Reasonix and Codex. Runtime-agnostic skills are installed here so both agents can discover them from a single copy.
_Avoid_: common skills dir, public skills dir

**Custom agent** (Codex only):
A `.codex/agents/*.toml` file defining a named subagent with model, sandbox, and instruction configuration. The Codex variants of implementer and reviewer ship TOML files that the install script places under `.codex/agents/` (project-local) or `~/.codex/agents/` (user-global). Not a skill — a Codex-native subagent definition.
_Avoid_: agent config, worker definition

**Symlink target**:
The absolute path a symlink in the install target resolves to. For a correct toolkit install, it must match `<PROJECT_ROOT>/skills/upstream/<path>` or `<PROJECT_ROOT>/skills/autopilot/<name>` (variant selection handled at install time).
_Avoid_: link destination, resolved path

**Same-name conflict**:
A symlink at a toolkit skill's name that resolves to a directory outside the toolkit's own source tree — looks present but belongs to a different project. Applies to any install target directory.
_Avoid_: name collision, shadowing

**Real directory** (vs symlink):
A non-symlink directory at an install target path where a symlink is expected. Indicates manual tampering or a competing install method. install.rs must not silently delete it.
_Avoid_: concrete directory, non-link directory

**Orphaned symlink**:
A symlink in any install target directory whose target points under PROJECT_ROOT but whose name is not in the expected set. Created when a toolkit skill is removed upstream — `install.rs unlink <name>` cleans it up.
_Avoid_: leftover symlink, stale symlink, dangling symlink (means broken target, different thing)

**Operational sync**:
The act of calling `install.rs sync <name> <src>` to bring one skill symlink to its expected state. Skips if already correct, creates if missing, replaces if broken or wrong target, warns and exits non-zero on real-directory conflict. Now accepts `--target reasonix|codex` to select the variant source and install directory.
_Avoid_: install step, link action

**Toolkit setup**:
The end-to-end install-or-update workflow, orchestrated by the `toolkit-setup` skill. Discovers the expected set, diagnoses every skill, computes and executes the minimal set of `sync`/`unlink`/`link-principles` operations for the target runtime, then verifies.
_Avoid_: selfcheck (that's now only the verification step), install flow

## Relationships

- The **expected set** is the union of upstream skills (from `.skill-lock.json`) and autopilot skills (from `skills/autopilot/` scanning)
- An **install target** entry at `<name>` should be a symlink whose **symlink target** matches the toolkit's source directory for that name
- A **same-name conflict** is a symlink at the right name with the wrong symlink target
- A **real directory** at a toolkit skill's name is a conflict of type, not just target
- An **orphaned symlink** is a toolkit symlink whose name is no longer in the expected set — cleaned by `unlink`
- **Toolkit setup** invokes **operational sync** per skill, then verifies via a final diagnostic pass
- Runtime-agnostic skills go to the **shared skill directory**; runtime-coupled skills go to the **agent-exclusive skill directory** for the target runtime
- A **skill variant** is selected at install time from the **variant source** matching `--target`
- Codex **custom agent** TOML files are deployed alongside Codex skill variants for implementer and reviewer

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
