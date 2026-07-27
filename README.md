# autopilot-toolkit

19 skills for Reasonix, Codex, and Kimi Code — 13 upstream engineering/productivity skills plus 6 autopilot workflow skills. Self-contained tarball distribution with one-command install.

Upstream v1.1 renamed `/to-prd` to `/to-spec`, replaced `/to-issues` with `/to-tickets`, and added `/code-review`, `/research`, and `/wayfinder`. The vendored upstream snapshot is tracked by `.skill-lock.json` and synced via `scripts/sync-upstream.rs`.

## Install

```bash
curl -sSL https://github.com/MatthewYe/autopilot-toolkit/releases/latest/download/install.sh | bash
```

Installs all skills to `~/.agents/skills/`, auto-detects your agent runtimes,
and configures runtime support files.

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
