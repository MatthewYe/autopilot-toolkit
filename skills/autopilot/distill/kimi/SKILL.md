---
name: distill
description: "Run the Distill requirement-to-issues workflow in Kimi through the installed toolkit CLI."
---

Use the installed Distill CLI at `~/.agents/skills/.autopilot/bin/distill`.

If `~/.agents/skills/.autopilot/distill.env` exists, read it and use `AUTOPILOT_DISTILL_BIN` as the executable path. Otherwise use the stable path above.

## Current Kimi Session Identity

Distill is session-bound. Before starting, resuming, submitting evidence, inspecting, or taking over a run, obtain one unambiguous runtime-native identity for the current Kimi session and pass it as `--session-id "<kimi-session-id>"`.

Use this order:

1. Inspect the current Kimi runtime trace. A Kimi session has a directory under `~/.kimi-code/sessions/.../session_<uuid>/`; its global index is `~/.kimi-code/session_index.jsonl`, with lines shaped like `{"sessionId":"...","sessionDir":"...","workDir":"..."}`. Each `sessionDir` contains `state.json` with fields such as `workDir`, `lastPrompt`, and `agents.main.homedir`.
2. Match the current invocation to exactly one indexed `sessionDir` by runtime-owned trace evidence: the `state.json` `workDir` must match the current project directory, `agents.main.homedir` must be inside that `sessionDir`, and `lastPrompt` or another Kimi-owned current trace value must match the current invocation prompt or unique invocation marker. Use that entry's `sessionId`.
3. If the current agent homedir or trace path is directly known, match it to exactly one `sessionDir` entry and use that entry's `sessionId`.
4. If no runtime-owned evidence yields exactly one current Kimi session, fail closed. Do not guess from recency, title, current working directory alone, the newest entry in `session_index.jsonl`, a user-provided non-runtime identifier, or a caller-injected environment variable.

Do not use a user-supplied `KIMI_SESSION_ID`, prompt-provided session id, or other custom session token as the Distill `--session-id`. The session id must be the native `session_<uuid>` from Kimi's own `session_index.jsonl`.

Report identity failure as: `Distill cannot start because the current Kimi session identity is unavailable or ambiguous.`

## Start Or Resume

For an explicitly supplied text requirement, start or resume the run from the target project worktree:

```bash
"${AUTOPILOT_DISTILL_BIN:-$HOME/.agents/skills/.autopilot/bin/distill}" start --json --runtime kimi --session-id "<kimi-session-id>" --worktree "$PWD" --requirement "<explicit requirement text>"
```

For runtime-submitted intake, pass the Kimi-captured intake JSON instead of plain text:

```bash
"${AUTOPILOT_DISTILL_BIN:-$HOME/.agents/skills/.autopilot/bin/distill}" start --json --runtime kimi --session-id "<kimi-session-id>" --worktree "$PWD" --intake-json '<intake-json>'
```

When the CLI returns JSON, inspect `run_id`, `stage`, `revision`, `next_action`, and `authorized_action`. A same-session resume returns the same unfinished run. A different Kimi session must not advance it; cross-session rejection is enforced by the runner through the shared `--session-id` contract. A Kimi session may own one unfinished run; if the runner reports more than one unfinished run or a duplicate session binding, stop and report that state without choosing one.

## Run To Boundary

Continue until the runner yields a terminal, waiting, blocked, or needs-reconciliation state. Every yield to the user must include `run_id`, `stage`, `revision`, `next_action`, and the required next command or executor.

Only invoke the executor named in `authorized_action`. Do not skip stages, reorder stages, or submit evidence for a stage other than the returned `stage`.

- If `next_action` is `terminal`, report the completion report paths and session release status.
- If the runner state is `blocked`, report the blocked reason and required recovery action. After recovery, resume with the same Kimi session identity and the returned `revision`.
- If the runner state is `needs-reconciliation`, do not retry publication blindly. Follow the runner's reconciliation instruction, verify the external tracker state, then resume with the returned `revision`.

## Authorized Executors

If `authorized_action.skill` is `grill-with-docs`, invoke the unmodified `grill-with-docs` skill for the captured requirement. When that stage has enough information, submit completion evidence with the preserved checkpoint:

```bash
"${AUTOPILOT_DISTILL_BIN:-$HOME/.agents/skills/.autopilot/bin/distill}" submit-evidence --json --worktree "$PWD" --run-id "<run_id>" --session-id "<kimi-session-id>" --expected-revision "<revision>" --stage clarification --evidence '{"checkpoint":"clarification-complete","summary":"<clarification summary>","clarified_requirement":"<complete clarified requirement>","decisions":[],"accepted_assumptions":[],"material_unknowns":[],"domain_document_artifacts":[]}'
```

If `authorized_action.skill` is `to-prd`, invoke the unmodified `to-prd` skill. Submit the exact accepted PRD markdown and the testing-seam checkpoint:

```bash
"${AUTOPILOT_DISTILL_BIN:-$HOME/.agents/skills/.autopilot/bin/distill}" submit-evidence --json --worktree "$PWD" --run-id "<run_id>" --session-id "<kimi-session-id>" --expected-revision "<revision>" --stage prd --evidence '{"checkpoint":"testing-seam-confirmed","summary":"<PRD summary>","prd_markdown":"<exact PRD markdown>"}'
```

If `authorized_action.skill` is `to-issues`, invoke the unmodified `to-issues` skill. Submit the exact accepted implementation issue payloads and approval checkpoint:

```bash
"${AUTOPILOT_DISTILL_BIN:-$HOME/.agents/skills/.autopilot/bin/distill}" submit-evidence --json --worktree "$PWD" --run-id "<run_id>" --session-id "<kimi-session-id>" --expected-revision "<revision>" --stage issues --evidence '{"checkpoint":"slice-breakdown-approved","summary":"<issue slicing summary>","issues":[{"title":"<issue title>","body":"<exact issue markdown>"}]}'
```

After every `submit-evidence` response, use the returned `revision` as the next `--expected-revision`.

Clarification completion is the agent's declaration, not a user checkpoint. Populate every structured field explicitly. Each material unknown must include `description`, `material`, `resolved`, and, when resolved, `resolution`; do not complete while a material unknown remains unresolved. For every glossary, domain document, or ADR changed by clarification, include its worktree-relative `path` and SHA-256 in `domain_document_artifacts`.

When the configured tracker is GitHub, `to-prd` and `to-issues` perform the external creation. Include the confirmed receipt as `external_publication` on the PRD evidence or each issue object. The receipt must contain `tracker: "github"`, the configured `repository`, the stable `operation_id` (`<run_id>-r<revision>-prd` or `<run_id>-r<revision>-issue-<two-digit-index>`), the SHA-256 of the exact frozen Markdown payload, `status: "confirmed"`, the positive issue number as `artifact_id`, and its canonical `artifact_url`. The runner remains offline, validates this receipt, and never falls back to another tracker. If the response is `needs-reconciliation`, stop and follow `required_next_action`; do not invoke another skill or create a duplicate issue.

## Takeover And Recovery

Same-session resume is the default. If a previous Kimi session is stranded and the user explicitly authorizes explicit takeover, use the shared runner takeover command:

```bash
"${AUTOPILOT_DISTILL_BIN:-$HOME/.agents/skills/.autopilot/bin/distill}" takeover --json --worktree "$PWD" --run-id "<run_id>" --from-session "<old-kimi-session-id>" --to-session "<current-kimi-session-id>" --expected-revision "<revision>" --reason "<user-authorized reason>" --user-authorized
```

Preserve the returned revision and continue run-to-boundary. Do not use takeover to bypass an ambiguous current Kimi session identity. Do not mutate `.distill/` state directly.

Only after an explicit user instruction may the agent call `abort`, `purge`, or `takeover`; pass `--user-authorized` and the returned expected revision. Never infer that authority from a failure or from the user ending a conversation.

## Reporting

Every user-visible yield should include:

- `run_id`
- `stage`
- `revision`
- `next_action`
- `authorized_action` or terminal report paths
- whether the Kimi session is still bound or released

On completion, report the canonical JSON report path, Markdown report path when rendered, published PRD and issue references, and session release.
