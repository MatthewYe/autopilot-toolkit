# PRD 0004: Self-Contained Tarball Install

**Status**: ready-for-agent
**Parent**: —
**ADR**: [0008-self-contained-tarball-install](../adr/0008-self-contained-tarball-install.md)

## Problem Statement

autopilot-toolkit currently installs skills as symlinks from agent skills directories into the local source repo. This fails when `~/.agents/skills/` is synced across machines, because the symlink target (absolute path to the repo) differs on each machine. Users must re-run toolkit setup on every machine, and the repo must be cloned locally for skills to function. There is no one-command install — users must clone the repo, navigate to it, and run `rust-script install.rs ...` or invoke the `toolkit-setup` skill.

## Solution

A self-contained tarball distribution model where `curl -sSL <url>/install.sh | bash` is the only command a user ever needs. Skills are deployed as real files into `~/.agents/skills/`, making the directory portable across machines. Agents that require skills in their own directory (Reasonix, Codex) get bootstrap symlinks from their agent-exclusive directory into the SSOT, created automatically during install. Versioning uses git commit hashes for exact-match comparison. Upgrades remove only toolkit-owned directories (per manifest) and re-extract.

## User Stories

1. As a new autopilot user, I want to install all skills with a single curl | bash command, so that I can start using autopilot immediately without cloning a repo or running multiple setup steps.

2. As a user who syncs `~/.agents/skills/` across machines, I want skills to work on every machine without re-installing, so that my toolchain is consistent everywhere.

3. As a user who already has skills installed, I want to upgrade to a new version with the same single command, so that keeping current is effortless.

4. As a Reasonix user, I want autopilot to automatically create the necessary symlinks in `~/.reasonix/skills/`, so that I don't need to know about agent-exclusive directories.

5. As a Codex user, I want autopilot to automatically configure both skills and custom agents, so that the orchestrator can dispatch implementer and reviewer subagents immediately.

6. As a Kimi Code user, I want the install to just work — no extra configuration — since Kimi scans `~/.agents/skills/` directly.

7. As a user who has manually installed other skills in `~/.agents/skills/` (e.g. defuddle, obsidian-bases), I want upgrades to never touch those skills, so that my custom setup is preserved.

8. As a user on a machine without git, I want to install autopilot without any dependency beyond curl, tar, and bash.

9. As a user who wants a specific version, I want to pass `--version <hash>` to pin an older release, so that I can roll back if needed.

10. As a user running the install command again with the same version already installed, I want it to skip the download and just re-verify the bootstrap symlinks, so that it's fast and safe to run repeatedly.

11. As a developer working on autopilot skills, I want a local symlink flow (`install.rs sync`) that bypasses the tarball, so that I can iterate rapidly without building and extracting.

12. As a developer preparing a release, I want `install.rs build` to produce a complete tarball from the source tree, so that the release process is automated and reproducible.

13. As a user on an agent runtime that autopilot hasn't explicitly adapted for, I want each coupled skill to have a usable fallback variant, so that the toolkit degrades gracefully instead of being absent.

14. As a user who wants to understand what was installed, I want to inspect `~/.agents/skills/.autopilot/manifest.json` to see which directories are managed by autopilot.

## Implementation Decisions

### Tarball structure

The tarball `autopilot-toolkit-<git-hash>.tar.gz` contains:
- `skills/` — all agnostic, coupled (with all variant subdirectories and fallback SKILL.md), and upstream skill directories
- `.autopilot/` — bootstrap.sh, manifest.json, .version, .skill-lock.json
- `principles/` — deployed to `~/.agents/principles/`

### install.sh behavior

The user-facing entry script, hardcoding its own version (git hash). Flow:
1. Compare embedded version with `~/.agents/skills/.autopilot/.version` — same → skip download, run bootstrap, exit
2. Download tarball from GitHub Releases for the embedded version (or `--version` override)
3. If existing install: read `manifest.json`, remove all listed directories from `~/.agents/skills/`
4. Extract tarball to `~/.agents/skills/`
5. Deploy `principles/` to `~/.agents/principles/`
6. Auto-detect runtimes: `[ -d ~/.reasonix ]` → `bootstrap.sh --target reasonix`; `[ -d ~/.codex ]` → `bootstrap.sh --target codex`
7. Report installed version and bootstrapped runtimes

### bootstrap.sh behavior

Pure filesystem-convention-driven. For `--target <runtime>`:
- Scan `~/.agents/skills/` for directories containing `<runtime>/SKILL.md`
- For each: `ln -sf ~/.agents/skills/<name>/<runtime> ~/.<runtime>/skills/<name>`
- Additionally for codex: if `<name>/codex/agent.toml` exists → `ln -sf ~/.agents/skills/<name>/codex/agent.toml ~/.codex/agents/<name>.toml`
- Remove any stale bootstrap symlinks not matching current SSOT state
- Always idempotent: existing correct symlinks are skipped

### manifest.json

Generated by `install.rs build`. Schema:
```json
{
  "version": "<git-hash>",
  "skills": {
    "<name>": {
      "type": "agnostic|coupled|upstream",
      "variants": ["kimi", "codex", "reasonix"],
      "codex_agent": true|false
    }
  }
}
```
`variants` is present only for coupled skills. `codex_agent` is true only for implementer and reviewer.

### Fallback variant

Each coupled skill gets a root-level `SKILL.md` — the Kimi variant with Kimi-specific path references removed. It describes the generic workflow and instructs the agent to adapt using its native subagent/spawn mechanism. This is the variant symlinked when no native variant exists for the agent's runtime.

### Versioning

Version = full git commit hash (40-char). `install.rs build` reads it from `git rev-parse HEAD`. Comparison is exact string match — no semver ordering. `install.sh` embeds its version; `--version <hash>` overrides.

### install.rs changes

- `install.rs sync` — unchanged, retained for dev
- `install.rs build [--version <hash>]` — new: assembles tarball into `dist/`, auto-generates manifest.json and install.sh from templates
- Build reads `.skill-lock.json` to locate upstream skill directories

### Upstream skill handling

Upstream skills (mattpocock/skills) are snapshotted into the tarball at build time. Their versions are tracked in `.skill-lock.json` (repo) and copied to `.autopilot/.skill-lock.json` (tarball) for future independent update reference. The tarball includes the upstream skill content, not symlinks.

### Principles deployment

The `principles/` directory from the tarball is deployed to `~/.agents/principles/` during install. Unlike skills, this is a single top-level directory deployment, not managed per-skill.

## Testing Decisions

### What makes a good test

Tests verify external behavior at seams, not internal implementation. Each seam is tested independently: build produces a valid tarball with correct structure and metadata; install produces the correct filesystem state; bootstrap creates correct symlinks. Tests use temp directories with env var overrides.

### Test modules

- **`tests/test_build.rs`** — integration test: runs `install.rs build`, verifies tarball contains expected files, manifest.json matches contents, .version matches, install.sh embeds correct version
- **`tests/test_install.sh`** — shell integration test: runs `install.sh` against a local tarball in a temp `~/.agents/skills/`, verifies directory structure, manifest, .version file, no stale files on re-install, user skills preserved on upgrade
- **`tests/test_bootstrap.sh`** — shell integration test: sets up a mock SSOT with coupled and agnostic skills, runs `bootstrap.sh --target reasonix` and `--target codex`, verifies symlinks created, codex agent.toml deployed, idempotent re-run
- **`tests/test_upgrade.sh`** — shell integration test: installs v1, adds a user skill outside manifest, installs v2, verifies user skill untouched, old toolkit dirs gone, new toolkit dirs present

### Prior art

- `tests/test_install.rs` — existing integration tests for `install.rs sync` (tmp dir + env var overrides)
- `tests/test_toolkit_setup.rs` — existing end-to-end tests for the toolkit-setup workflow
- Validation tests in `crates/validation/` — pattern for strict structural assertions

## Out of Scope

- **Remote tarball hosting**: This PRD covers building the tarball and the install/bootstrap scripts. GitHub Releases setup and CI/CD pipeline for automatic publishing on push are separate work.
- **Independent upstream skill updates**: Upstream skills are embedded in the tarball. A mechanism to update them without a full autopilot release is future work.
- **Windows support**: install.sh and bootstrap.sh are bash scripts. No PowerShell equivalent.
- **Uninstall**: No uninstall command. Users manually remove `~/.agents/skills/.autopilot/`, manifest-listed directories, and `~/.codex/agents/autopilot-*.toml`.
- **Migration from old symlink install**: Users on the old model must manually clean up old symlinks before first tarball install. No automatic migration.

## Further Notes

- `install.sh` templates should be kept in the repo (e.g. `templates/install.sh.in`) with `__VERSION__` placeholder, so `install.rs build` can sed-replace and validate.
- `manifest.json` is the SSOT for ownership at install time — if a skill is removed from the toolkit, removing it from the tarball + manifest ensures it's cleaned on next upgrade.
- The bootstrap convention (`<name>/<runtime>/SKILL.md`) should be documented in a developer-facing `docs/bootstrap-convention.md` so future runtime additions follow the same pattern.
