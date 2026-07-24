## Parent

Parent PRD: `docs/prd/0003-kimi-code-adaptation.md`

## What to build

Create the Kimi variant sources for the autopilot main loop — orchestrator, implementer, reviewer — and wire them into validation, installation, and a live smoke test, so the full scan → implement → review → retry loop runs on Kimi Code.

End-to-end behavior:

1. Each of the 3 runtime-coupled skills gains a `kimi/` variant source (full independent copy per the established per-runtime convention), adapted to Kimi mechanisms:
   - Frontmatter carries only `name` and `description`; Reasonix-only `runAs`/`allowed-tools` fields are dropped.
   - Subagent dispatch uses Kimi's `Agent` tool; dispatch prompts instruct the subagent to read its installed skill body from `~/.agents/skills/` first, since Kimi subagents do not inherit the parent's skill list.
   - Implementer progress sign-off uses Kimi's `TodoList` instead of `complete_step`/`todo_write`.
   - Tool restrictions formerly enforced by `allowed-tools` become prose constraints in the body (e.g. read-only reviewer), matching the Codex variants' approach.
   - Runtime-specific reference content is copied into the variant source; runtime-agnostic content may be shared from the skill root.
2. `toolkit-setup` learns to discover Kimi variant sources and install them to the shared skill directory `~/.agents/skills/<name>` via the existing `--shared` flag. `install.rs` itself is expected to need no changes.
3. An install integration test (existing `tests/test_install.rs` pattern, temp-dir env overrides) asserts a Kimi variant syncs to `~/.agents/skills/<name>` resolving to the Kimi variant source.
4. Smoke verification in a real Kimi Code session: all 3 skills load, and the orchestrator successfully dispatches implementer and reviewer on a trivial issue.
5. Cross-runtime shadowing check: with the Kimi variant present in the shared skill directory, confirm Reasonix still resolves its own variant from its agent-exclusive skill directory.

The `reasonix/` and `codex/` variant bodies are not modified.

## Acceptance criteria

- [ ] `kimi/` variant sources exist for orchestrator, implementer, and reviewer, and pass the validation runner
- [ ] No `runAs`, `allowed-tools`, `run_skill`, `complete_step`, or Reasonix tool names remain in the Kimi variant bodies
- [ ] `toolkit-setup` discovers the Kimi variants and installs them to `~/.agents/skills/` via `--shared` with no changes to `install.rs`
- [ ] New install integration test passes alongside the existing suite
- [ ] Kimi Code smoke run: all 3 skills appear in the skill list; orchestrator dispatches implementer and reviewer end-to-end on a trivial issue
- [ ] Reasonix smoke check: still loads its own 4 variants, unaffected by the same-name Kimi variants in the shared directory
- [ ] Codex variants and install path unchanged

## Blocked by

- #10-frontmatter-yaml-validation (validation must cover the new variant sources)
- #11-unify-repo-clones (install symlinks must point at the canonical clone)
