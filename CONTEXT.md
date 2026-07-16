# Autopilot Toolkit

A skill-pack repo targeting Reasonix, Codex, and Ksana. Ships 25 named toolkit capabilities — 19 upstream (from mattpocock/skills, tracked in `.skill-lock.json`) plus 6 autopilot (custom, living in `skills/autopilot/`). 21 are runtime-agnostic skills; 4 autopilot workflow capabilities have per-runtime variants due to differing subagent dispatch mechanisms.

## Language

**Toolkit skill**:
One of the 25 named capabilities that autopilot-toolkit owns and installs. Always traceable to a source: either a `.skill-lock.json` entry (upstream) or a directory under `skills/autopilot/` (autopilot).
_Avoid_: project skill, owned skill

**Expected set**:
The authoritative list of toolkit capabilities, derived at runtime by reading `.skill-lock.json` (upstream) and scanning `skills/autopilot/*/` for an agnostic `SKILL.md` or a runtime-coupled `reasonix/SKILL.md`. No separate manifest — the sources are the SSOT.
_Avoid_: skill inventory, skill manifest

**Skill source**:
The origin of a toolkit skill — either `upstream` (mattpocock/skills, synced via `.skill-lock.json`) or `autopilot` (local, under `skills/autopilot/`).
_Avoid_: skill type, skill category

**Runtime-agnostic skill**:
A skill whose body contains only methodology instructions — no references to runtime-specific tools (`run_skill`, `complete_step`), dispatch mechanisms, or CLI commands. Works on any Agent Skills-compliant agent (Reasonix, Codex, Claude Code, etc.). 21 of 25 named toolkit capabilities fall in this category.
_Avoid_: universal skill, portable skill

**Runtime-coupled skill**:
A skill whose body depends on runtime-specific mechanisms (subagent dispatch, session export, proprietary tools). The 4 autopilot workflow skills (orchestrator, implementer, reviewer, audit-autopilot) are runtime-coupled.
_Avoid_: platform-specific skill, bound skill

**Skill variant**:
A runtime-specific version of a runtime-coupled skill. Same skill identity (name, purpose), different body — the Reasonix variant uses `run_skill` dispatch and `complete_step`; the Codex variant uses `spawn agent <name>` and `.codex/agents/*.toml` custom agents; the Ksana variant uses the `spawn_agent` tool (`agent_type`/`task_name`/`message` params) and `~/.ksana/agents/*.toml` custom agent **roles** (with `model` omitted — supplied by the user's global config). Each variant lives in a per-runtime subdirectory: `reasonix/SKILL.md`, `codex/SKILL.md` (or `codex/agent.toml`), or `ksana/SKILL.md` (or `ksana/agent.toml`).
_Avoid_: skill version, skill flavor

**Variant source**:
A directory in the source tree that carries a specific runtime variant of a runtime-coupled skill. Named `<runtime>/` (e.g. `reasonix/`, `codex/`, `ksana/`) under the skill directory, containing the variant `SKILL.md` or `agent.toml` plus any runtime-specific supporting files. The install script selects the matching variant based on `--target`.
_Avoid_: variant file, alternate body

## Install model

**Install target**:
The directory where a skill symlink is deployed. Varies by skill category and runtime target:

| Skill category | Reasonix target | Codex target | Ksana target |
|---|---|---|---|
| Runtime-agnostic | `~/.agents/skills/<name>/` | `~/.agents/skills/<name>/` | `~/.agents/skills/<name>/` |
| Runtime-coupled | `~/.reasonix/skills/<name>/` | `~/.codex/skills/<name>/` | `~/.ksana/skills/<name>/` |

_Avoid_: skills dir, agents skills

**Agent-exclusive skill directory**:
A skill directory scanned by exactly one agent runtime. `~/.reasonix/skills/` (Reasonix only), `~/.codex/skills/` (Codex only), and `~/.ksana/skills/` (Ksana only). Runtime-coupled skill variants are installed here to eliminate cross-agent conflicts without relying on `compatibility` field filtering.
_Avoid_: private skills dir, isolated directory

**Shared skill directory**:
`~/.agents/skills/` — the Agent Skills standard shared location, scanned by Reasonix, Codex, and Ksana. Runtime-agnostic skills are installed here so all agents can discover them from a single copy.
_Avoid_: common skills dir, public skills dir

**Custom agent** (Codex/Ksana):
A `~/.codex/agents/*.toml` or `~/.ksana/agents/*.toml` file defining a named subagent with sandbox, model, and instruction configuration. The Codex and Ksana variants of implementer and reviewer ship TOML files that the install script symlinks under `~/.codex/agents/` or `~/.ksana/agents/` (user-global). Not a skill — a runtime-native subagent definition. On ksana the `agent.toml` omits `model` (autopilot-toolkit is provider-agnostic); the user's global `~/.ksana/config.toml` must supply a top-level `model = "<slug>"` layer, because spawning a child role rebuilds its config from the layer stack without inheriting the parent's runtime model.
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
- Codex and Ksana **custom agent** TOML files are deployed alongside their respective skill variants for implementer and reviewer

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
