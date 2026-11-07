# autopilot-toolkit

41 skills for Reasonix, Codex, and Kimi Code — 32 upstream skills, 1 vendored third-party skill, and 8 autopilot workflow skills. Self-contained tarball distribution with one-command install.

The upstream snapshot is pinned to release v1.3.2 `d1478fcb140db0b93c0c8df17bcfd311844749d0`, recorded by the sync in `.upstream-pin.json` and verified by `scripts/check.rs`. It brings the v1.3 line forward with fresh-context sub-agent guidance across the workflow skills, sub-issue support in `/to-tickets`, a reachable-repo gate in `/setup-matt-pocock-skills`, and a rewritten upstream docs set; the shipped skill set is unchanged. The snapshot is tracked by `.skill-lock.json` and synced via `scripts/sync-upstream.rs <ref>`: a release tag, branch, or commit SHA, always explicit; a branch or commit needs `--reason "…"`, and the resolver records the ref, kind, commit, upstream version, and sync time in the pin. Only `loop-me` ships from upstream's `in-progress/` bucket through the script's allowlist; nothing else from that bucket is picked up, and a ref without an allowlisted skill fails the sync instead of dropping it.

Third-party skills live under `skills/vendor/` and are tracked separately by `.vendor-lock.json`, so an upstream sync never touches them.

## Install

```bash
curl -sSL https://github.com/MatthewYe/autopilot-toolkit/releases/latest/download/install.sh | bash
```

Installs all skills to `~/.agents/skills/`, auto-detects your agent runtimes,
and configures runtime support files. DSH (DeepSeek Harness) discovers the same
directory — see [docs/agents/runtimes/dsh.md](docs/agents/runtimes/dsh.md) for
its discovery rules, frontmatter contract, and runtime-router selection.

## Uninstall

```bash
curl -sSL https://github.com/MatthewYe/autopilot-toolkit/releases/latest/download/uninstall.sh | bash
```

Or locally:

```bash
bash ~/.agents/skills/.autopilot/uninstall.sh
```

## Development

Requires [rust-script](https://github.com/fornwall/rust-script):

```bash
brew install rust-script   # or: cargo install rust-script
```

Clone and symlink skills for local iteration:

```bash
git clone git@github.com:MatthewYe/autopilot-toolkit.git
cd autopilot-toolkit
rust-script deploy.rs dev      # symlink all skills from source tree
```

Iterate on skill files, then test changes in your agent. When done:

```bash
rust-script deploy.rs dev-clean   # remove all dev symlinks
```

## Commands

```
deploy.rs                        pack + release (no-args shortcut)
deploy.rs dev                    stage and symlink all skills into the shared dir
deploy.rs dev-clean              remove all dev symlinks
deploy.rs pack                   build tarball into dist/
deploy.rs release                push tarball to GitHub Releases
deploy.rs link-principles <src>  symlink ~/.agents/principles
```

## How it works

Skills are YAML-frontmatter markdown files (`SKILL.md`) consumed directly by
agents. The install tarball deploys them to `~/.agents/skills/` as real files —
no symlinks to a source repo, so cross-machine sync works. Runtime-coupled
skills expose one shared `SKILL.md` router and keep full runtime instructions
under non-discoverable `runtime/*/INSTRUCTIONS.md` files. Codex custom agents
are still linked into `~/.codex/agents/`.

Release versioning uses git commit hashes. Each `deploy.rs` run builds a tarball, creates a lightweight tag, and pushes to GitHub Releases. The install URL never changes — `/latest/download/` always points to the newest release.

Full details: [PRD 0004](docs/prd/0004-self-contained-tarball-install.md), [ADR 0008](docs/adr/0008-self-contained-tarball-install.md).
