---
name: autopilot-distill
description: "Run the Distill requirement-to-issues workflow in Reasonix through the installed toolkit CLI."
---

Use the installed Distill CLI at `~/.agents/skills/.autopilot/bin/distill`.

If `~/.agents/skills/.autopilot/distill.env` exists, read it and use `AUTOPILOT_DISTILL_BIN` as the executable path. Otherwise use the stable path above.

Read the current Reasonix session identity from runtime-native request/session metadata. Prefer an explicit current session id from Reasonix metadata or the `list_sessions` tool when it identifies the active session unambiguously.

If metadata does not expose the id, use the Reasonix project session trace as a fail-closed fallback:

1. Resolve the physical worktree path with `pwd -P`.
2. Map it to Reasonix's project directory under `~/.reasonix/projects/` by replacing `/` with `-` in the physical path.
3. Inspect that project's `sessions/*.jsonl` files.
4. Keep only the project session trace that contains the current `/distill` invocation and this requirement text.
5. If exactly one project session trace remains, use that trace basename without `.jsonl` as the current Reasonix session identity.

Use the active Reasonix session id, not a generated UUID, process id, timestamp, latest session file, macOS `SECURITYSESSIONID`, or user-provided label. If no project session trace matches, more than one candidate matches, or the current Reasonix session identity is otherwise unavailable or ambiguous, fail closed and report that Distill cannot start without an unambiguous runtime-native current Reasonix session identity.

For an explicitly supplied text requirement, start or resume the run from the target project worktree:

```bash
"${AUTOPILOT_DISTILL_BIN:-$HOME/.agents/skills/.autopilot/bin/distill}" start --json --runtime reasonix --session-id "<current-reasonix-session-id>" --worktree "$PWD" --requirement "<explicit requirement text>"
```

Pass the current Reasonix session identity as `--session-id`. Report any non-zero exit status and stderr without retrying with another executable. When the CLI returns JSON, inspect `run_id`, `stage`, `revision`, `next_action`, and `authorized_action`.

Run to the next Distill boundary in the same Reasonix session:

- If `next_action` is `terminal`, report the completion report paths and note that the session binding has been released.
- If `next_action` is `blocked` or `needs-reconciliation`, report `run_id`, `stage`, `revision`, `next_action`, and the blocked or reconciliation reason exactly from the runner. Do not invent fallback publication or recovery behavior outside the runner.
- If `authorized_action.skill` is `grill-with-docs`, invoke the unmodified Reasonix skill with `run_skill(name: "grill-with-docs", arguments: "<captured requirement and runner context>")`. When that executor has enough information, preserve its `clarification-complete` checkpoint and submit completion evidence:

```bash
"${AUTOPILOT_DISTILL_BIN:-$HOME/.agents/skills/.autopilot/bin/distill}" submit-evidence --json --worktree "$PWD" --run-id "<run_id>" --session-id "<current-reasonix-session-id>" --expected-revision "<revision>" --stage clarification --evidence '{"checkpoint":"clarification-complete","summary":"<clarification summary>","clarified_requirement":"<complete clarified requirement>","decisions":[],"accepted_assumptions":[],"material_unknowns":[],"domain_document_artifacts":[]}'
```

- If `authorized_action.skill` is `to-spec`, invoke the unmodified Reasonix skill with `run_skill(name: "to-spec", arguments: "<clarified requirement and runner context>")`. Submit the resulting PRD markdown and preserve the required `testing-seam-confirmed` checkpoint:

```bash
"${AUTOPILOT_DISTILL_BIN:-$HOME/.agents/skills/.autopilot/bin/distill}" submit-evidence --json --worktree "$PWD" --run-id "<run_id>" --session-id "<current-reasonix-session-id>" --expected-revision "<revision>" --stage prd --evidence '{"checkpoint":"testing-seam-confirmed","summary":"<PRD summary>","prd_markdown":"<exact PRD markdown>"}'
```

- If `authorized_action.skill` is `to-tickets`, invoke the unmodified Reasonix skill with `run_skill(name: "to-tickets", arguments: "<accepted PRD and runner context>")`. Submit the accepted vertical-slice issue payloads and preserve the `slice-breakdown-approved` checkpoint:

```bash
"${AUTOPILOT_DISTILL_BIN:-$HOME/.agents/skills/.autopilot/bin/distill}" submit-evidence --json --worktree "$PWD" --run-id "<run_id>" --session-id "<current-reasonix-session-id>" --expected-revision "<revision>" --stage issues --evidence '{"checkpoint":"slice-breakdown-approved","summary":"<issue slicing summary>","issues":[{"title":"<issue title>","body":"<exact issue markdown>"}]}'
```

## LOCAL_ISSUE_HANDOFF_CONTRACT

When `docs/agents/issue-tracker.md` configures Local Markdown, use `to-tickets` to draft and approve the vertical slices, but stop before its tracker-publication step. The Distill runner is the sole local publisher: submit the approved payloads to the runner and let it create each local issue exactly once under `.scratch/distill-tracer/issues/`. Do not create a second issue copy elsewhere under `.scratch/`.

Before `submit-evidence`, ensure every local issue `body` begins with agent-ready triage frontmatter:

```markdown
---
Status: ready-for-agent
---
```

The exact Markdown including this frontmatter is the frozen issue payload. When the configured tracker is GitHub, keep the external publication and receipt flow below.

After each `submit-evidence` response, inspect the returned `run_id`, `stage`, `revision`, `next_action`, and `authorized_action`, then continue the loop until the runner yields a terminal, waiting, blocked, or needs-reconciliation state. Pass the returned `revision` as `--expected-revision` on the next mutation. The runner authorizes stage order; do not skip a stage or submit evidence for a stage other than the returned `stage`.

Clarification completion is the agent's declaration, not a user checkpoint. Populate every structured field explicitly. Each material unknown must include `description`, `material`, `resolved`, and, when resolved, `resolution`; do not complete while a material unknown remains unresolved. For every glossary, domain document, or ADR changed by clarification, include its worktree-relative `path` and SHA-256 in `domain_document_artifacts`.

When the configured tracker is GitHub, `to-spec` and `to-tickets` perform the external creation. Include the confirmed receipt as `external_publication` on the PRD evidence or each issue object. The receipt must contain `tracker: "github"`, the configured `repository`, the stable `operation_id` (`<run_id>-r<revision>-prd` or `<run_id>-r<revision>-issue-<two-digit-index>`), the SHA-256 of the exact frozen Markdown payload, `status: "confirmed"`, the positive issue number as `artifact_id`, and its canonical `artifact_url`. The runner remains offline, validates this receipt, and never falls back to another tracker. If the response is `needs-reconciliation`, stop and follow `required_next_action`; do not invoke another skill or create a duplicate issue.

If `submit-evidence` fails with context drift after an authorized stage executor output changed the worktree, inspect the drift before retrying. When the drift is only the expected output from the authorized executor for the current stage, retry the same `submit-evidence` with a reasoned immaterial `drift_acknowledgment` inside the evidence JSON:

```json
{
  "checkpoint": "<stage checkpoint>",
  "summary": "<stage summary>",
  "drift_acknowledgment": {
    "material": false,
    "reason": "The detected worktree drift is the authorized stage executor output for <stage> and does not change the approved requirement."
  }
}
```

For the `prd` stage, include the exact `prd_markdown` together with this `drift_acknowledgment`. For the `issues` stage, include the exact `issues` array together with this `drift_acknowledgment`. If the drift contains anything other than authorized stage executor output, do not retry as immaterial; report the runner's blocked state or supersede only when the user explicitly authorizes it.

Same-session resume, cross-session rejection, one-unfinished-run enforcement, interruption recovery, explicit takeover, blocked conditions, recovery, publication, reporting, and session release are shared runner behavior. Preserve that contract by always passing the current Reasonix session id and expected revision back to the runner.

For explicit user-authorized takeover, call the runner's takeover command and use only the user's authorization as authority:

```bash
"${AUTOPILOT_DISTILL_BIN:-$HOME/.agents/skills/.autopilot/bin/distill}" takeover --json --worktree "$PWD" --run-id "<run_id>" --from-session "<previous-reasonix-session-id>" --to-session "<current-reasonix-session-id>" --expected-revision "<revision>" --reason "<user-authorized reason>" --user-authorized
```

Only after an explicit user instruction may the agent call `abort`, `purge`, or `takeover`; pass `--user-authorized` and the returned expected revision. Never infer that authority from a failure or from the user ending a conversation.
