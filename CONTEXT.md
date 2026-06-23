# Autopilot Toolkit

A skill-pack repo for Reasonix. Ships 19 skills to `~/.agents/skills/` — 13 upstream (from mattpocock/skills, tracked in `.skill-lock.json`) plus 6 autopilot (custom, living in `skills/autopilot/`).

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

**Install target**:
`~/.agents/skills/<name>/` — the global shared directory where skills are deployed as symlinks. Shared by all projects; a toolkit install is one tenant among many.
_Avoid_: skills dir, agents skills

**Symlink target**:
The absolute path a symlink in the install target resolves to. For a correct toolkit install, it must match `<PROJECT_ROOT>/skills/upstream/<path>` or `<PROJECT_ROOT>/skills/autopilot/<name>`.
_Avoid_: link destination, resolved path

**Same-name conflict**:
A symlink at a toolkit skill's name that resolves to a directory outside the toolkit's own source tree — looks present but belongs to a different project.
_Avoid_: name collision, shadowing

**Real directory** (vs symlink):
A non-symlink directory at `~/.agents/skills/<name>` where a symlink is expected. Indicates manual tampering or a competing install method. install.sh must not silently delete it.
_Avoid_: concrete directory, non-link directory

## Relationships

- The **expected set** is the union of upstream skills (from `.skill-lock.json`) and autopilot skills (from `skills/autopilot/` scanning)
- An **install target** entry at `<name>` should be a symlink whose **symlink target** matches the toolkit's source directory for that name
- A **same-name conflict** is a symlink at the right name with the wrong symlink target
- A **real directory** at a toolkit skill's name is a conflict of type, not just target
