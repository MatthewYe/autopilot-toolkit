## Parent

Parent PRD: `docs/prd/0003-kimi-code-adaptation.md`

## What to build

Create the Kimi variant source for `audit-autopilot`, then bring the repo's documentation in line with the now-three-runtime reality.

End-to-end behavior:

1. `audit-autopilot` gains a `kimi/` variant source. Its Reasonix `read_session(subagent_session_id)` mechanism is replaced with direct file reads, preserving the two-phase audit method (orchestrator-level scan → selective subagent deep-dive):
   - Session discovery via `~/.kimi-code/session_index.jsonl` (maps sessionId → sessionDir → workDir): filter by workDir, prefer most recent, or accept a user-supplied session id/path.
   - Orchestrator trace at `<sessionDir>/agents/main/wire.jsonl`; scan it for `Agent` tool calls to enumerate dispatched subagents.
   - Subagent traces at `<sessionDir>/agents/agent-N/wire.jsonl`, read selectively per the existing spot-check strategy.
2. The variant is installed and smoke-verified the same way as the main-loop variants: passes validation, installs via `toolkit-setup` + `--shared`, and completes one real audit of an actual Kimi autopilot session.
3. Documentation updates reflecting the final state:
   - `AGENTS.md`: current Rust tooling commands (`install.rs`, `validation/run.rs`, `tests/*.rs`), three supported runtimes, accurate skill counts and variant layout.
   - `CONTEXT.md` glossary: Kimi entries in the install-model table (Kimi variant → shared skill directory) and any new terms the variant work introduced.

## Acceptance criteria

- [ ] `kimi/` variant source for audit-autopilot exists and passes the validation runner
- [ ] No `read_session` or Reasonix-specific mechanisms remain in the Kimi variant body; trace discovery uses `session_index.jsonl` + `wire.jsonl` reads
- [ ] Two-phase audit method (broad scan → selective deep-dive) is preserved in the Kimi variant
- [ ] Smoke run: audit-autopilot completes a real audit of an actual Kimi autopilot session (from issue #12's smoke run or a fresh one) and produces a fidelity report
- [ ] `AGENTS.md` accurately describes Rust tooling, three runtimes, and current skill/variant layout
- [ ] `CONTEXT.md` install-model table includes the Kimi row; new domain terms are defined

## Blocked by

- #12-kimi-variant-main-loop (reuses its install wiring and smoke pattern; docs describe its final state)
