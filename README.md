# autopilot-toolkit

25 named toolkit capabilities for Reasonix, Codex, and Ksana — 19 selected upstream engineering/productivity skills plus 6 autopilot workflows. Runtime-agnostic skills deploy via symlinks to the shared `~/.agents/skills/`; the 4 runtime-coupled workflow capabilities ship per-runtime variants installed to each runtime's agent-exclusive directory (`~/.reasonix/skills/`, `~/.codex/skills/`, `~/.ksana/skills/`).

## Prerequisites

The Rust-based installer (`install.rs`) requires [rust-script](https://github.com/fornwall/rust-script):

```bash
brew install rust-script
# or: cargo install rust-script
```

## Quickstart

For a first-time Codex installation:

```bash
git clone git@github.com:matthewye/autopilot-toolkit.git && cd autopilot-toolkit
rust-script install.rs setup --target codex
rust-script install.rs status --target codex
```

`setup` is the deterministic bootstrap command: it discovers the selected skill set, repairs missing or incorrect toolkit symlinks, installs Codex custom agents, removes toolkit-owned orphans, links principles, and verifies the result. It requires an explicit target so a Codex installation cannot silently install the Reasonix variants.

After bootstrap, `/toolkit-setup --target codex` remains the conversational install/update entry point. It delegates to the same setup behavior. Use `rust-script install.rs setup --target codex --dry-run` to preview changes without modifying anything.

The upstream `skills/upstream/README.md` Quickstart is intentionally **not** this project's install path: `npx skills@latest add mattpocock/skills` installs the whole upstream package, while this repository vendors and validates a selected snapshot tracked by `.skill-lock.json`.

## Configure a consumer repository

Toolkit installation is global; issue tracking and domain-document conventions belong to each code repository. In every consumer repository, run:

```text
/setup-matt-pocock-skills
```

That guided setup records the issue tracker, triage-label mapping, and `CONTEXT.md`/ADR layout consumed by `/to-issues`, `/triage`, `/tdd`, and related skills. This toolkit repository is already configured under `docs/agents/`.

## Commands

Manage skill symlinks with `install.rs`:

```bash
./install.rs sync <name> <src>       # symlink ~/.agents/skills/<name> → <src> (shared)
./install.rs sync <name> <src> --target codex|ksana --agent  # deploy a custom agent .toml
./install.rs unlink <name>           # remove a toolkit-owned symlink
./install.rs link-principles <src>   # symlink ~/.agents/principles → <src>
./install.rs setup --target <runtime> [--dry-run]  # full deterministic setup
./install.rs status --target <runtime>             # read-only verification
```

`--target reasonix|codex|ksana` selects the agent-exclusive install directory for runtime-coupled skills; `--shared` (default for agnostic skills) uses `~/.agents/skills/`. See `skills/autopilot/toolkit-setup/SKILL.md` for the full setup flow.

### Ksana prerequisite: a resolvable child `model`

Ksana's `spawn_agent` rebuilds the child config from the layer stack and does **not** carry the parent's runtime model into the child. The ksana `agent.toml` roles ship without a `model` (autopilot-toolkit is provider-agnostic), so the `model` layer must come from your global `~/.ksana/config.toml`. Ensure it has a top-level `model = "<your-provider slug>"` (or start ksana with `-c model="<slug>"`). `--model` is a harness override that does **not** enter the layer stack and will not propagate to spawned children. Without this layer, `spawn_agent` fails with `could not resolve the child model for service tier validation`. See `docs/adr/0008-tri-runtime-ksana.md` §3.

## Updating

Pull the latest changes and re-run toolkit setup to sync skills:

```bash
cd autopilot-toolkit && git pull && /toolkit-setup --target codex
```

Full skill inventory and project details: [`docs/prd/0001-autopilot-toolkit.md`](docs/prd/0001-autopilot-toolkit.md).
