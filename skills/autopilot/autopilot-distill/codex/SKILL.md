---
name: autopilot-distill
description: "Run the Distill requirement-to-issues workflow in Codex through the installed toolkit CLI."
---

Use the installed Distill CLI at `~/.agents/skills/.autopilot/bin/distill`.

If `~/.agents/skills/.autopilot/distill.env` exists, read it and use `AUTOPILOT_DISTILL_BIN` as the executable path. Otherwise use the stable path above.

Read the current Codex thread identity from runtime request metadata. If the current thread identity is unavailable or ambiguous, stop and report that Distill cannot start without a runtime-native Codex thread identity.

For an explicitly supplied text requirement, start or resume the run from the target project worktree:

```bash
"${AUTOPILOT_DISTILL_BIN:-$HOME/.agents/skills/.autopilot/bin/distill}" start --json --runtime codex --session-id "<codex-thread-id>" --worktree "$PWD" --requirement "<explicit requirement text>"
```

Pass the Codex thread identity as `--session-id`. Report any non-zero exit status and stderr without retrying with another executable. When the CLI returns JSON, inspect `run_id`, `stage`, `revision`, `next_action`, and `authorized_action`.

Run to the next Distill boundary in the same Codex thread:

- If `next_action` is `terminal`, report the completion report paths.
- If `authorized_action.skill` is `grill-with-docs`, invoke the unmodified `grill-with-docs` skill for the captured requirement. When that stage has enough information, submit completion evidence:

```bash
"${AUTOPILOT_DISTILL_BIN:-$HOME/.agents/skills/.autopilot/bin/distill}" submit-evidence --json --worktree "$PWD" --run-id "<run_id>" --session-id "<codex-thread-id>" --expected-revision "<revision>" --stage clarification --evidence '{"checkpoint":"clarification-complete","summary":"<clarification summary>","clarified_requirement":"<complete clarified requirement>","decisions":[],"accepted_assumptions":[],"material_unknowns":[],"domain_document_artifacts":[]}'
```

- If `authorized_action.skill` is `to-prd`, invoke the unmodified `to-prd` skill. Submit the resulting PRD markdown and the required testing-seam checkpoint:

```bash
"${AUTOPILOT_DISTILL_BIN:-$HOME/.agents/skills/.autopilot/bin/distill}" submit-evidence --json --worktree "$PWD" --run-id "<run_id>" --session-id "<codex-thread-id>" --expected-revision "<revision>" --stage prd --evidence '{"checkpoint":"testing-seam-confirmed","summary":"<PRD summary>","prd_markdown":"<exact PRD markdown>"}'
```

- If `authorized_action.skill` is `to-issues`, invoke the unmodified `to-issues` skill. Submit the accepted vertical-slice issue payloads and approval checkpoint:

```bash
"${AUTOPILOT_DISTILL_BIN:-$HOME/.agents/skills/.autopilot/bin/distill}" submit-evidence --json --worktree "$PWD" --run-id "<run_id>" --session-id "<codex-thread-id>" --expected-revision "<revision>" --stage issues --evidence '{"checkpoint":"slice-breakdown-approved","summary":"<issue slicing summary>","issues":[{"title":"<issue title>","body":"<exact issue markdown>"}]}'
```

## LOCAL_ISSUE_HANDOFF_CONTRACT

When `docs/agents/issue-tracker.md` configures Local Markdown, use `to-issues` to draft and approve the vertical slices, but stop before its tracker-publication step. The Distill runner is the sole local publisher: submit the approved payloads to the runner and let it create each local issue exactly once under `.scratch/distill-tracer/issues/`. Do not create a second issue copy elsewhere under `.scratch/`.

Before `submit-evidence`, ensure every local issue `body` begins with agent-ready triage frontmatter:

```markdown
---
Status: ready-for-agent
---
```

The exact Markdown including this frontmatter is the frozen issue payload. When the configured tracker is GitHub, keep the external publication and receipt flow below.

After each `submit-evidence` response, inspect the returned `revision`, `next_action`, and `authorized_action` and continue the loop until the runner yields a terminal, waiting, blocked, or needs-reconciliation state. Pass the returned `revision` as `--expected-revision` on the next mutation. The runner authorizes stage order; do not skip a stage or submit evidence for a stage other than the returned `stage`.

Clarification completion is the agent's declaration, not a user checkpoint. Populate every structured field explicitly. Each material unknown must include `description`, `material`, `resolved`, and, when resolved, `resolution`; do not complete while a material unknown remains unresolved. For every glossary, domain document, or ADR changed by clarification, include its worktree-relative `path` and SHA-256 in `domain_document_artifacts`.

When the configured tracker is GitHub, `to-prd` and `to-issues` perform the external creation. Include the confirmed receipt as `external_publication` on the PRD evidence or each issue object. The receipt must contain `tracker: "github"`, the configured `repository`, the stable `operation_id` (`<run_id>-r<revision>-prd` or `<run_id>-r<revision>-issue-<two-digit-index>`), the SHA-256 of the exact frozen Markdown payload, `status: "confirmed"`, the positive issue number as `artifact_id`, and its canonical `artifact_url`. The runner remains offline, validates this receipt, and never falls back to another tracker. If the response is `needs-reconciliation`, stop and follow `required_next_action`; do not invoke another skill or create a duplicate issue.

Only after an explicit user instruction may the agent call `abort`, `purge`, or `takeover`; pass `--user-authorized` and the returned expected revision. Never infer that authority from a failure or from the user ending a conversation.
