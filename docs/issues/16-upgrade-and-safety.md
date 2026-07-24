# Upgrade and safety: version check, manifest cleanup, user skill preservation

**Status**: ready-for-agent
**Parent**: [PRD 0004](../prd/0004-self-contained-tarball-install.md)

## What to build

Extend `install.sh` with upgrade-awareness. When run on a system that already has autopilot installed, it must:
- Compare the embedded version with `~/.agents/skills/.autopilot/.version` — if same, skip download and only re-run bootstrap
- Read `manifest.json` to determine which directories in `~/.agents/skills/` belong to the toolkit
- Remove only manifest-listed directories before extracting the new version
- Never touch directories not in the manifest (user-installed skills survive)

Also extend `bootstrap.sh` to clean up stale symlinks that no longer correspond to any skill in the current SSOT.

## Acceptance criteria

- [ ] Same version detected → "already up to date" message, skip download, re-run bootstrap, exit 0
- [ ] New version: all manifest-listed directories removed from `~/.agents/skills/` before extract
- [ ] User-added skills (e.g. `defuddle/`, `obsidian-bases/`) survive upgrade untouched
- [ ] `.version` file updated to new version after successful install
- [ ] `install.sh` with `--version <hash>` pins to a specific release (not just latest)
- [ ] `bootstrap.sh` removes symlinks in agent-exclusive directories that point into the SSOT but whose source no longer exists
- [ ] Integration test: install v1, add user skill, install v2 → user skill present, v1-only dirs gone, v2 dirs present
- [ ] Integration test: install same version twice → second run skips download, runs bootstrap, reports up to date

## Blocked by

- [#15 Fresh install](15-fresh-install.md)

## Seams

Seam 2 (install boundary): test filesystem state after upgrade.
Seam 3 (bootstrap boundary): test stale symlink cleanup.
