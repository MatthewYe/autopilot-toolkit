# Fresh install: install.sh + bootstrap.sh

**Status**: ready-for-agent
**Parent**: [PRD 0004](../prd/0004-self-contained-tarball-install.md)

## What to build

The two runtime scripts that turn a tarball into a working autopilot installation.

**install.sh** — user-facing entry point. Downloads the tarball, extracts skills to `~/.agents/skills/`, deploys principles to `~/.agents/principles/`, auto-detects installed runtimes and calls `bootstrap.sh` for each.

**bootstrap.sh --target <runtime>** — symlink factory. Scans `~/.agents/skills/` for coupled skill variant directories, creates symlinks from agent-exclusive directories into the SSOT. For Codex: also symlinks `agent.toml` files to `~/.codex/agents/`.

## Acceptance criteria

- [ ] `install.sh` accepts `--version <hash>` to override the embedded version
- [ ] `install.sh` downloads tarball from GitHub Releases (or local path for testing)
- [ ] `install.sh` extracts `skills/` to `~/.agents/skills/`, `.autopilot/` to `~/.agents/skills/.autopilot/`
- [ ] `install.sh` deploys `principles/` to `~/.agents/principles/`
- [ ] `install.sh` auto-detects runtimes: `~/.reasonix/` → `bootstrap.sh --target reasonix`, `~/.codex/` → `bootstrap.sh --target codex`
- [ ] `bootstrap.sh --target reasonix` creates symlinks for all `<name>/reasonix/SKILL.md` at `~/.reasonix/skills/<name>`
- [ ] `bootstrap.sh --target codex` creates symlinks for all `<name>/codex/SKILL.md` at `~/.codex/skills/<name>`
- [ ] `bootstrap.sh --target codex` creates symlinks for all `<name>/codex/agent.toml` at `~/.codex/agents/<name>.toml`
- [ ] `bootstrap.sh` is idempotent: running twice produces same result
- [ ] `bootstrap.sh` removes stale symlinks no longer matching SSOT state
- [ ] Kimi requires no bootstrap (its variants are natively in `~/.agents/skills/`)
- [ ] Integration test: fresh install in temp dir → verify full directory tree and symlinks

## Blocked by

- [#14 Build pipeline](14-build-pipeline.md)

## Seams

Seam 2 (install boundary): test filesystem state after install.sh runs.
Seam 3 (bootstrap boundary): test symlink state after bootstrap.sh runs.
