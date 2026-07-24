# ADR 0008: Self-Contained Tarball Install with Bootstrap Symlinks

## Context

The current install model creates symlinks from agent skills directories (`~/.agents/skills/`, `~/.codex/skills/`, `~/.reasonix/skills/`) to the source repo (`/Users/xlchen/Dev/autopilot-toolkit/skills/`). This has two problems:

1. **Cross-machine sync**: `~/.agents/skills/` is synced across machines (e.g. via iCloud or syncthing), but symlinks break because the source repo lives at different absolute paths on different machines.
2. **Repo dependency**: Skills cannot function without the source repo present on disk. This makes the toolkit harder to distribute and prevents standalone `curl | bash` installation.

These problems were not visible in ADR 0007, which focused on resolving runtime-specific variant conflicts, not on the install artifact format.

## Decision

**Replace symlink-to-repo install with self-contained tarball deployment, plus bootstrap symlinks for agent-exclusive directories.**

### Core architecture

1. **`~/.agents/skills/` becomes the SSOT** (single source of truth). All autopilot skills — agnostic, coupled variants, and fallback — are deployed here as real directories. The source repo is no longer involved at runtime.

2. **Agent-exclusive directories become symlink views** into the SSOT. `~/.reasonix/skills/<name>` and `~/.codex/skills/<name>` are symlinks pointing to `~/.agents/skills/<name>/<runtime>/`. This preserves ADR 0007's variant isolation without duplicating files.

3. **Tarball distribution** via GitHub Releases. A single tarball contains all skills, a bootstrap script, a manifest, and version metadata. Install is `curl | bash`.

4. **`install.rs build`** produces the tarball from the source tree. `install.rs sync` retains its local-dev symlink flow for rapid iteration.

### Install flow

```
curl -sSL <url>/install.sh | bash
  -> reads embedded version, downloads tarball from GitHub Releases
  -> reads manifest.json, removes old toolkit directories from ~/.agents/skills/
  -> extracts tarball: skills/ -> ~/.agents/skills/, .autopilot/ -> ~/.agents/skills/.autopilot/
  -> for each detected runtime (~/.reasonix/, ~/.codex/):
      bootstrap.sh --target <runtime>
        -> scans ~/.agents/skills/ for <name>/<runtime>/SKILL.md
        -> creates symlink <runtime-dir>/skills/<name> -> ~/.agents/skills/<name>/<runtime>
        -> detects <name>/codex/agent.toml -> symlinks ~/.codex/agents/<name>.toml
  -> deploys principles/ -> ~/.agents/principles/
```

### Tarball structure

```
autopilot-toolkit-<git-hash>.tar.gz
+-- skills/
|   +-- autopilot-orchestrator/
|   |   +-- SKILL.md              # fallback (Kimi-based, vendor-neutral)
|   |   +-- codex/SKILL.md
|   |   +-- kimi/SKILL.md
|   |   +-- reasonix/SKILL.md
|   +-- autopilot-implementer/
|   |   +-- SKILL.md
|   |   +-- codex/agent.toml      # Codex custom agent (no SKILL.md for implementer in codex)
|   |   +-- kimi/SKILL.md
|   |   +-- reasonix/SKILL.md
|   +-- autopilot-reviewer/
|   |   +-- SKILL.md
|   |   +-- codex/agent.toml
|   |   +-- kimi/SKILL.md
|   |   +-- reasonix/SKILL.md
|   +-- audit-autopilot/
|   |   +-- SKILL.md
|   |   +-- codex/SKILL.md
|   |   +-- kimi/SKILL.md
|   |   +-- reasonix/SKILL.md
|   +-- toolkit-setup/SKILL.md    # agnostic
|   +-- zoom-out/SKILL.md         # agnostic
|   +-- tdd/SKILL.md              # upstream
|   +-- ... (remaining upstream skills)
+-- .autopilot/
|   +-- install.sh                # user-facing entry point (embeds version)
|   +-- bootstrap.sh              # per-runtime symlink creation
|   +-- manifest.json             # ownership: all toolkit directory names
|   +-- .version                  # git hash
|   +-- .skill-lock.json          # upstream version tracking
+-- principles/                   # deployed to ~/.agents/principles/
```

### Versioning

Version = git commit hash. `install.sh` embeds its own version string; comparison is exact-match (no semver ordering). `--version <hash>` lets users pin or roll back.

### Bootstrap behavior

`bootstrap.sh --target <runtime>` is purely file-system-driven:

- Discovers coupled skills: any `<name>/<runtime>/SKILL.md` under `~/.agents/skills/` -> creates symlink in agent-exclusive directory
- Discovers Codex agents: any `<name>/codex/agent.toml` -> symlinks to `~/.codex/agents/<name>.toml`
- Kimi requires no bootstrap (its coupled variants are already in `~/.agents/skills/`)
- Agnostic skills are already in `~/.agents/skills/` — no bootstrap needed

`bootstrap.sh` is always idempotent and safe to run repeatedly.

### Fallback variant

Each coupled skill has a root-level `SKILL.md` — a vendor-neutral fallback based on the Kimi variant, stripped of Kimi-specific path references. Agents without a native variant symlink to this fallback. It describes the generic workflow and instructs the agent to adapt its native dispatch mechanism.

### Upgrade

`install.sh` reads `manifest.json` to determine which directories in `~/.agents/skills/` belong to the toolkit, removes them, then extracts the new tarball. User-installed skills (not in manifest) are never touched. Same version -> skip download, re-run bootstrap.

### `install.rs` role

- `install.rs sync` — retained for local development (symlink to source repo, no tarball needed)
- `install.rs build --version <hash>` — new subcommand: assembles tarball into `dist/`, auto-generates `manifest.json` and `install.sh` from templates, reads `.skill-lock.json` for upstream snapshots

## Alternatives considered

### A. Copy skills but keep repo-symlink bootstrap

Copy agnostic skills to `~/.agents/skills/` but keep `~/.reasonix/skills/` symlinks pointing to the repo. Rejected: still breaks cross-machine sync for coupled skills; half the problem remains.

### B. Git-clone based install

`git clone` the entire repo to a standard location and symlink from there. Rejected: requires git, large download, couples install to one repo layout, no clean versioning.

## Consequences

- **install.rs** gains `build` subcommand; gains tarball assembly logic and template rendering
- **toolkit-setup skill** must be rewritten for the new install model (no longer symlink-diagnosing)
- **Upstream skill updates** now require a new tarball release (upstream snapshots are embedded)
- **Cross-machine sync works**: `~/.agents/skills/` is portable; only bootstrap symlinks are local and disposable
- **Distribution simplified**: single `curl | bash` with no prerequisites beyond curl, tar, bash
- **Fallback variant** ensures the toolkit degrades gracefully on unknown agents
