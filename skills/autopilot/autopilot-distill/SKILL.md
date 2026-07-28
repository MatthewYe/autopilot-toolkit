---
name: autopilot-distill
description: "Run the Distill requirement-to-issues workflow through the installed toolkit CLI."
---

Use the installed Distill CLI at `~/.agents/skills/.autopilot/bin/distill`.

If `~/.agents/skills/.autopilot/distill.env` exists, read it and use `AUTOPILOT_DISTILL_BIN` as the executable path. Otherwise use the stable path above.

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

Use the exact headings and lower-case frontmatter keys shown above. The `key` must equal the runner's `<two-digit-index>-<title-slug>` filename stem. For blocked tickets, list each blocking issue's exact title as one bullet. The exact Markdown is the frozen issue payload. GitHub publication behavior is unchanged.

Every issue object must also include `depends_on`, an explicit array of zero-based indices of earlier issues in the same evidence payload. Use `[]` for an unblocked issue. The structured edges must match the issue body's `Blocked by` section; never rely on the runner to infer dependencies from prose.

If `submit-evidence` fails with context drift after an authorized stage executor output changed the worktree, inspect the drift before retrying. When the drift is only the expected output from the authorized stage executor for the current stage, retry the same evidence with:

```json
"drift_acknowledgment": {
  "material": false,
  "reason": "The detected drift is authorized stage executor output and does not change the approved requirement."
}
```

If the drift contains anything other than authorized stage executor output, do not acknowledge it as immaterial; stop at the blocked boundary or supersede only with explicit user authority.

Run the CLI from the target project worktree and pass through the user's Distill command arguments. Report any non-zero exit status and stderr without retrying with another executable.
