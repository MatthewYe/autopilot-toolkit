---
name: autopilot-distill
description: "Run the Distill requirement-to-issues workflow through the installed toolkit CLI."
---

Use the installed Distill CLI at `~/.agents/skills/.autopilot/bin/distill`.

If `~/.agents/skills/.autopilot/distill.env` exists, read it and use `AUTOPILOT_DISTILL_BIN` as the executable path. Otherwise use the stable path above.

## LOCAL_ISSUE_HANDOFF_CONTRACT

When `docs/agents/issue-tracker.md` configures Local Markdown, use `to-tickets` to draft and approve the vertical slices, but stop before its tracker-publication step. The Distill runner is the sole local publisher: submit the approved payloads to the runner and let it create each local issue exactly once under `.scratch/distill-tracer/issues/`. Do not create a second issue copy elsewhere under `.scratch/`.

Before `submit-evidence`, ensure every local issue `body` begins with agent-ready triage frontmatter:

```markdown
---
Status: ready-for-agent
---
```

The exact Markdown including this frontmatter is the frozen issue payload. GitHub publication behavior is unchanged.

Run the CLI from the target project worktree and pass through the user's Distill command arguments. Report any non-zero exit status and stderr without retrying with another executable.
