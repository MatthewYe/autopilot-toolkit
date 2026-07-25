---
name: toolkit-setup
description: Install or update autopilot-toolkit skills from the current checkout, repair the shared runtime-router layout, link principles, and verify that every logical skill is discovered once.
---

# Toolkit Setup

Set up the current `autopilot-toolkit` checkout for local development. Run
inside the repository and use its `deploy.rs`; do not recreate installation
logic manually.

## Contract

- All skills are discoverable from `${AGENTS_SKILLS_DIR:-$HOME/.agents/skills}`.
- A runtime-coupled skill has one installed `SKILL.md` router.
- Runtime bodies live at `runtime/<runtime>/INSTRUCTIONS.md` and are not
  independently discoverable.
- Do not create coupled-skill links in `~/.codex/skills/` or
  `~/.reasonix/skills/`.
- Codex custom agents remain links in `${CODEX_AGENTS_DIR:-$HOME/.codex/agents}`.
- Preserve real user files and unrelated symlinks.

## Procedure

1. Resolve `PROJECT_ROOT` to the repository root and confirm these files exist:
   `deploy.rs`, `.skill-lock.json`, and `skills/autopilot/`.
2. Inspect `git status --short`. Report existing changes, but do not discard or
   overwrite them.
3. Run:

   ```bash
   rust-script "$PROJECT_ROOT/deploy.rs" dev
   rust-script "$PROJECT_ROOT/deploy.rs" link-principles "$PROJECT_ROOT/principles"
   ```

4. Derive the expected skill names from `.skill-lock.json` and
   `skills/autopilot/*/`; do not hardcode the list.
5. Verify every expected shared entry is a valid directory or symlink.
6. For every source skill that has any of `codex/`, `kimi/`, or `reasonix/`,
   verify:
   - the installed tree contains exactly one file named `SKILL.md`;
   - `runtime/default/INSTRUCTIONS.md` exists;
   - each source runtime `SKILL.md` has a corresponding installed
     `runtime/<runtime>/INSTRUCTIONS.md`;
   - no same-name entry exists in the Codex or Reasonix exclusive skill
     directory.
7. If `codex/agent.toml` exists, verify the corresponding
   `~/.codex/agents/<skill>.toml` link.
8. Verify `${AGENTS_PRINCIPLES_DIR:-$HOME/.agents/principles}` points to
   `$PROJECT_ROOT/principles`.

If `deploy.rs dev` reports a real-directory or real-file conflict, do not
replace it. Report the exact path and leave verification failed until the user
resolves ownership.

## Cleanup

When explicitly asked to remove development links, run:

```bash
rust-script "$PROJECT_ROOT/deploy.rs" dev-clean
```

This removes only links pointing into the current checkout.

## Report

Return a compact report containing:

```text
TOOLKIT_SETUP_REPORT
Expected: <count>
Actions: <dev/link-principles actions>
Routers: <count passed>, <count failed>
Codex agents: <count passed>, <count failed>
Principles: PASS|FAIL
Warnings: <paths or none>
Result: ALL PASS|FAILED
```
