## Problem Statement

autopilot-toolkit does not work on Kimi Code. After installing the toolkit, `autopilot-orchestrator` and `autopilot-reviewer` never appear in Kimi's skill list, so the entire autopilot workflow loop (orchestrator → implementer → reviewer) is unavailable. The user runs multiple agent runtimes (Reasonix, Codex, Kimi Code) and wants the autopilot workflow to work on Kimi Code without breaking the existing Reasonix and Codex support.

The immediate cause of the missing skills: the Reasonix variant frontmatter of `autopilot-orchestrator` and `autopilot-reviewer` is invalid YAML — the `description` values contain unquoted `": "` sequences (e.g. `loop: scan →`, `review: Behavior alignment`), which strict YAML parsers reject. Kimi Code silently skips skills whose frontmatter fails to parse.

Beyond that, the 4 runtime-coupled skills have Reasonix-specific and Codex-specific variant sources, but no Kimi variant: their bodies reference `run_skill()` subagent dispatch, `complete_step`/`todo_write` harness sign-off, Reasonix tool names in `allowed-tools`, and `read_session()` for trace export — none of which exist in Kimi Code.

## Solution

Add Kimi Code as a third supported runtime:

1. Fix the invalid-YAML frontmatter on the two Reasonix variants so strict parsers accept them (pure bug fix, no behavioral change).
2. Create a Kimi variant source for each of the 4 runtime-coupled skills, adapted to Kimi Code mechanisms: `Agent`-tool subagent dispatch, `TodoList` for progress tracking, and direct reading of Kimi session trace files (`wire.jsonl`) for auditing.
3. Install Kimi variants to the shared skill directory `~/.agents/skills/` (Kimi Code's User scope), reusing the install script's existing `--shared` flag — no new install target.
4. Extend the validation runner to cover the new variant sources and to catch invalid-YAML frontmatter so this class of bug cannot recur.
5. Teach the `toolkit-setup` skill to discover and install Kimi variants.
6. Unify the two on-disk repo clones so all runtimes load skills from the single working clone.

## User Stories

1. As a Kimi Code user, I want `autopilot-orchestrator` to appear in my skill list after install, so that I can run the autopilot issue-resolution loop.
2. As a Kimi Code user, I want `autopilot-implementer` and `autopilot-reviewer` available, so that the orchestrator can dispatch implementation and review subagents.
3. As a Kimi Code user, I want the orchestrator to dispatch subagents via Kimi's `Agent` tool, so that the implement/review loop actually executes instead of failing on a nonexistent `run_skill` tool.
4. As a Kimi Code user, I want dispatched subagents to explicitly read their SKILL.md bodies, so that they follow the autopilot methodology even though Kimi subagents do not inherit the parent session's skill list.
5. As a Kimi Code user, I want the implementer's progress sign-off expressed with Kimi's `TodoList`, so that the implementation loop works with the harness instead of against it.
6. As a Kimi Code user, I want `audit-autopilot` to analyze my autopilot runs, so that I can verify execution fidelity after the fact.
7. As a Kimi Code user, I want audit-autopilot to locate session traces via `~/.kimi-code/session_index.jsonl` and per-agent `wire.jsonl` files, so that no special trace-export tool is needed.
8. As a Reasonix user, I want my existing 4 autopilot skills to keep working unchanged, so that the Kimi adaptation does not regress my workflow.
9. As a Codex user, I want my existing variants untouched, so that the Kimi adaptation does not regress my workflow.
10. As a toolkit maintainer, I want the frontmatter validator to reject invalid YAML, so that silently-dropped skills are caught at validation time instead of discovered by users.
11. As a toolkit maintainer, I want the validation runner to cover all variant sources including the new Kimi ones, so that every shipped skill file is validated.
12. As a toolkit maintainer, I want `toolkit-setup` to discover and install Kimi variants, so that installation stays a single orchestrated flow instead of manual symlinking.
13. As a toolkit maintainer, I want the Kimi install destination to reuse the existing `--shared` flag, so that no new install-target machinery is added.
14. As a toolkit maintainer, I want install integration tests covering Kimi variant deployment, so that the new path is regression-tested.
15. As a toolkit maintainer, I want AGENTS.md to describe the repo as it actually is (Rust tooling, current skill counts, three runtimes), so that agents and contributors are not misled.
16. As the repo owner, I want all runtimes to load skills from one canonical clone (`/Users/neil/Dev/autopilot-toolkit`), so that edits propagate to every runtime and the stale `build` clone stops shadowing them.
17. As a Kimi Code user, I want the quoted-description bug fix to leave the parsed description text identical, so that Reasonix's lenient parser and Kimi's strict parser read the same value.
18. As a toolkit maintainer, I want Kimi variants to follow the established per-runtime-copy convention (like `reasonix/` and `codex/`), so that the repo structure stays predictable.

## Implementation Decisions

- **Root-cause fix**: quote the `description` values in the Reasonix variant sources of `autopilot-orchestrator` and `autopilot-reviewer`. This converts invalid YAML to valid YAML with an identical parsed value — safe for Reasonix's lenient parser, fixes Kimi's strict one.
- **Kimi variant sources**: new per-runtime copies under each runtime-coupled skill's directory (a `kimi/` variant source alongside the existing `reasonix/` and `codex/` ones), following the established convention of full independent copies per runtime. The `reasonix/` and `codex/` variant sources are not modified beyond the quoting fix.
- **Frontmatter mapping**: Kimi variant frontmatter carries only `name` and `description`. The Reasonix-only fields `runAs: subagent` and `allowed-tools` (which name Reasonix tools like `read_file`, `multi_edit`, `code_index`) are dropped entirely — Kimi has no such concepts and unknown fields would be misleading.
- **Subagent dispatch**: the Kimi orchestrator variant dispatches implementer and reviewer via Kimi's `Agent` tool. Because Kimi subagents do not inherit the parent's skill list, dispatch prompts instruct the subagent to read its skill body from the installed location under `~/.agents/skills/` before starting work. Tool restrictions previously enforced by `allowed-tools` become prose constraints in the skill body (e.g. "you are a read-only reviewer"), matching what the Codex variants already do.
- **Harness sign-off**: `complete_step`/`todo_write` flows in the implementer become Kimi `TodoList` usage.
- **audit-autopilot trace access**: replaces `read_session(subagent_session_id)` with direct file reads. Session discovery: `~/.kimi-code/session_index.jsonl` maps sessionId → sessionDir → workDir (filter by workDir, prefer most recent `updatedAt`, or take a user-supplied session id/path). Trace layout: `<sessionDir>/agents/main/wire.jsonl` for the orchestrator and `<sessionDir>/agents/agent-N/wire.jsonl` for subagents. The two-phase audit method (orchestrator-level scan → selective subagent deep-dive) is preserved unchanged; scan `main/wire.jsonl` for `Agent` tool calls to enumerate subagents.
- **Install destination**: Kimi variants install to the shared skill directory `~/.agents/skills/<name>` via the existing `--shared` flag of `install.rs`. No `--target kimi` is added; `install.rs` itself is expected to need no changes.
- **Known risk — cross-runtime shadowing**: Kimi variants in the shared skill directory are also visible to Reasonix and Codex, which scan that directory. Correctness relies on each runtime resolving same-name skills in favor of its agent-exclusive skill directory (`~/.reasonix/skills/`, `~/.codex/skills/`) over the shared one. This precedence must be verified on Reasonix during implementation; if it does not hold, revisit the install destination.
- **Clone unification**: verify `/Users/neil/build/autopilot-toolkit` has no uncommitted divergence from `/Users/neil/Dev/autopilot-toolkit`, then re-point the `~/.reasonix/skills/` symlinks from the `build` clone to the `Dev` clone (same variant sources, so Reasonix behavior is unchanged). Deleting the `build` clone is left to the user as a manual step.
- **references handling**: if a Kimi variant needs runtime-specific reference content, it carries its own copy inside the variant source (as the Codex orchestrator variant does); runtime-agnostic reference content may be shared from the skill root.
- **AGENTS.md**: updated to reflect reality — Rust tooling (`install.rs`, `validation/run.rs`, `tests/*.rs`), three supported runtimes, current skill/variant layout.

## Testing Decisions

Good tests here verify external behavior through existing seams, never implementation details: frontmatter either parses and meets the contract or it doesn't; an install command either produces the expected symlink state or it doesn't.

- **Seam 1 — the validation library and runner** (`crates/validation`, `validation/run.rs`): add a strict-YAML frontmatter parse check to the validator, and extend the runner's discovery to include Kimi variant sources. Tested with the existing assert-style validation tests — feed valid and invalid frontmatter fixtures and assert accept/reject, mirroring the current `validate_skill` unit tests. The two fixed Reasonix files and four new Kimi variants serve as the real-world regression cases.
- **Seam 2 — the install CLI integration tests** (`tests/test_install.rs`): add a case in the existing pattern asserting that syncing a Kimi variant source with `--shared` produces a symlink at `~/.agents/skills/<name>` resolving to the Kimi variant source. Uses the same temp-directory env-var overrides (`AGENTS_SKILLS_DIR` etc.) the current tests use — no real home-directory writes in tests.
- **No new seams**: Kimi variant *content* correctness is covered by Seam 1 (frontmatter contract) plus a one-time manual smoke check (start Kimi Code, confirm all 4 skills load). The cross-runtime shadowing risk is verified manually on Reasonix, not by an automated test.

## Out of Scope

- Any changes to the `reasonix/` or `codex/` variant bodies (beyond the two-line quoting fix).
- A `--target kimi` install target or a Kimi-exclusive skill directory (e.g. `~/.kimi/skills`) — the shared directory via `--shared` is sufficient.
- Removing or deprecating Reasonix support; the Reasonix install remains.
- Redesigning the autopilot workflow around Kimi-specific features (cron, goals) — the workflow logic is runtime-agnostic and preserved.
- Deleting the old `/Users/neil/build/autopilot-toolkit` clone — manual step for the user after symlink re-pointing.
- Adapting the 17 runtime-agnostic skills — they already load in Kimi Code from the shared directory.
- Automated testing of skill body prose (methodology content is validated by human review and smoke runs, not unit tests).

## Further Notes

- Environment facts established during design (verified on the user's machine): Kimi Code loads User-scope skills from `~/.agents/skills/` and Extra-scope skills from `~/.reasonix/skills/`; Kimi session traces live under `~/.kimi-code/sessions/wd_<workspace>_<hash>/session_<uuid>/agents/{main,agent-N}/wire.jsonl` with a global index at `~/.kimi-code/session_index.jsonl`. Sessions with dispatched subagents show one `agent-N` directory per subagent.
- The invalid-YAML root cause was confirmed empirically: `yaml.safe_load` fails with "mapping values are not allowed here" on exactly the two skills missing from Kimi's list, and parses the two that load.
- CONTEXT.md's glossary should gain Kimi-related entries (e.g. the Kimi variant's install target in the install-model table) as part of keeping domain docs current.
- Terminology follows the domain glossary: runtime-coupled skill, skill variant, variant source, install target, shared skill directory, agent-exclusive skill directory, toolkit setup.
