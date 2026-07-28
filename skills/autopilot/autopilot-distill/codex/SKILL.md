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

- If `authorized_action.skill` is `to-spec`, invoke the unmodified `to-spec` skill. Submit the resulting PRD markdown and the required testing-seam checkpoint:

```bash
"${AUTOPILOT_DISTILL_BIN:-$HOME/.agents/skills/.autopilot/bin/distill}" submit-evidence --json --worktree "$PWD" --run-id "<run_id>" --session-id "<codex-thread-id>" --expected-revision "<revision>" --stage prd --evidence '{"checkpoint":"testing-seam-confirmed","summary":"<PRD summary>","feature_slug":"<stable-lowercase-feature-slug>","prd_markdown":"<exact PRD markdown>"}'
```

- If `authorized_action.skill` is `to-tickets`, invoke the unmodified `to-tickets` skill. Submit the accepted vertical-slice issue payloads and approval checkpoint:

```bash
"${AUTOPILOT_DISTILL_BIN:-$HOME/.agents/skills/.autopilot/bin/distill}" submit-evidence --json --worktree "$PWD" --run-id "<run_id>" --session-id "<codex-thread-id>" --expected-revision "<revision>" --stage issues --evidence '{"checkpoint":"slice-breakdown-approved","summary":"<issue slicing summary>","issues":[{"title":"<issue title>","body":"<exact issue markdown>","depends_on":[]}]}'
```

## LOCAL_ISSUE_HANDOFF_CONTRACT

When `docs/agents/issue-tracker.md` configures Local Markdown, choose a stable lowercase `feature_slug` for the PRD evidence, use `to-tickets` to draft and approve the vertical slices, but stop before its tracker-publication step. The Distill runner is the sole local publisher: it creates the PRD at `.scratch/<feature_slug>/PRD.md` and each local issue exactly once under `.scratch/<feature_slug>/issues/`. Do not create a second issue copy elsewhere under `.scratch/`. The runner rejects a target path that already contains different content; never acknowledge that collision as immaterial drift.

Before `submit-evidence`, ensure every local issue `body` is one canonical runnable ticket:

```markdown
---
key: 01-stable-ticket-slug
title: Exact issue title
type: issue
status: ready-for-agent
parent: .scratch/<feature_slug>/PRD.md
---

## What to build

...

## Acceptance Criteria

...

## Blocked by

- None — can start immediately.

## Comments
```

Use the exact headings and lower-case frontmatter keys shown above. The `key` must equal the runner's `<two-digit-index>-<title-slug>` filename stem. For blocked tickets, list each blocking issue's exact title as one bullet. The exact Markdown is the frozen issue payload. When the configured tracker is GitHub, keep the external publication and receipt flow below.

Every issue object must include `depends_on`, an explicit array of zero-based indices of earlier issues in the same evidence payload. Use `[]` for an unblocked issue. The structured edges must match the issue body's `Blocked by` section; never rely on the runner to infer dependencies from prose.

After each `submit-evidence` response, inspect the returned `revision`, `next_action`, and `authorized_action` and continue the loop until the runner yields a terminal, waiting, blocked, or needs-reconciliation state. Pass the returned `revision` as `--expected-revision` on the next mutation. The runner authorizes stage order; do not skip a stage or submit evidence for a stage other than the returned `stage`.

Clarification completion is the agent's declaration, not a user checkpoint. Populate every structured field explicitly. Each material unknown must include `description`, `material`, `resolved`, and, when resolved, `resolution`; do not complete while a material unknown remains unresolved. For every glossary, domain document, or ADR changed by clarification, include its worktree-relative `path` and SHA-256 in `domain_document_artifacts`.

When the configured tracker is GitHub, `to-spec` and `to-tickets` perform the external creation. Include the confirmed receipt as `external_publication` on the PRD evidence or each issue object. The receipt must contain `tracker: "github"`, the configured `repository`, the stable `operation_id` (`<run_id>-r<revision>-prd` or `<run_id>-r<revision>-issue-<two-digit-index>`), the SHA-256 of the exact frozen Markdown payload, `status: "confirmed"`, the positive issue number as `artifact_id`, and its canonical `artifact_url`. The runner remains offline, validates this receipt, and never falls back to another tracker. If the response is `needs-reconciliation`, stop and follow `required_next_action`; do not invoke another skill or create a duplicate issue.

If `submit-evidence` fails with context drift after an authorized stage executor output changed the worktree, inspect the drift before retrying. When the drift is only the expected output from the authorized stage executor for the current stage, retry the same `submit-evidence` with a reasoned immaterial `drift_acknowledgment` inside the evidence JSON:

```json
{
  "checkpoint": "<stage checkpoint>",
  "summary": "<stage summary>",
  "drift_acknowledgment": {
    "material": false,
    "reason": "The detected worktree drift is authorized stage executor output for <stage> and does not change the approved requirement."
  }
}
```

For the `prd` stage, include the exact `prd_markdown`; for `issues`, include the exact `issues` array. If the drift contains anything other than authorized stage executor output, do not retry as immaterial; report the blocked state or supersede only when the user explicitly authorizes it.

Only after an explicit user instruction may the agent call `abort`, `purge`, or `takeover`; pass `--user-authorized` and the returned expected revision. Never infer that authority from a failure or from the user ending a conversation.
