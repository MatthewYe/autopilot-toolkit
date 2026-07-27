# ADR 0036: Single Discoverable Router for Runtime-Coupled Skills

## Context

The self-contained install placed a fallback `SKILL.md` and each runtime
variant's `SKILL.md` beneath the same shared skill directory. Codex recursively
discovers that directory, so one logical Autopilot skill appeared four times.
Agent-exclusive symlinks did not isolate the variants because the shared tree
was still visible.

## Decision

Package every runtime-coupled skill with exactly one discoverable
`SKILL.md`. That file is a small router. Full instructions are installed as
`runtime/<runtime>/INSTRUCTIONS.md`, with
`runtime/default/INSTRUCTIONS.md` as the fallback.

All runtimes consume the shared router. Bootstrap no longer creates
runtime-specific skill symlinks. It removes legacy toolkit-owned links during
upgrade while preserving unrelated user entries. Codex custom `agent.toml`
files remain symlinked from `runtime/codex/`.

The source layout remains unchanged so each runtime variant can still be
validated as a standalone skill before packaging. The router retains Reasonix
execution metadata (`runAs` and `allowed-tools`) because Reasonix reads it at
discovery time; Codex and Kimi ignore those additional frontmatter keys.

## Consequences

- Recursive discovery yields one entry per logical skill.
- Codex, Kimi, and Reasonix variants remain independently maintainable.
- Pack and development installs share the same transformed layout.
- Runtime selection happens when the router is invoked, adding one small
  instruction-file read.
- ADR 0007's agent-exclusive skill links and ADR 0008's packaged variant layout
  are superseded by this routing model.
