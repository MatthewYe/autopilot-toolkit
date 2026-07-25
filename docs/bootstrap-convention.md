# Bootstrap Convention

Runtime-coupled skills use one discoverable router in the shared skills
directory:

```text
~/.agents/skills/<name>/
├── SKILL.md
└── runtime/
    ├── default/INSTRUCTIONS.md
    ├── codex/INSTRUCTIONS.md
    ├── kimi/INSTRUCTIONS.md
    └── reasonix/INSTRUCTIONS.md
```

Only the router is named `SKILL.md`. This prevents runtimes that recursively
scan `~/.agents/skills/` from indexing the default and runtime-specific
instructions as separate copies of the same skill. The router selects the
current runtime's `INSTRUCTIONS.md`, falling back to `runtime/default/`.

Source files remain under `<name>/<runtime>/SKILL.md`; `deploy.rs pack` and
`deploy.rs dev` transform them into the installed router layout.

## Bootstrap behavior

Codex and Reasonix discover the shared router directly, so `bootstrap.sh` does
not create runtime-specific skill symlinks. During an upgrade it removes only
legacy skill symlinks whose target is inside `~/.agents/skills/`; unrelated
user directories and symlinks are preserved.

Codex custom agents remain runtime-specific:

```text
~/.codex/agents/<name>.toml
  -> ~/.agents/skills/<name>/runtime/codex/agent.toml
```

Kimi needs no bootstrap step.

## Adding a runtime

1. Add `<name>/<new-runtime>/SKILL.md` in the source tree.
2. Add the runtime identifier to the variant lists and router text in
   `deploy.rs`.
3. Add runtime-specific bootstrap behavior only if it has artifacts other than
   skill instructions.
