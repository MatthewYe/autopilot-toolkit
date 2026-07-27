---
name: autopilot-distill
description: "Run the Distill requirement-to-issues workflow through the installed toolkit CLI."
---

Use the installed Distill CLI at `~/.agents/skills/.autopilot/bin/distill`.

If `~/.agents/skills/.autopilot/distill.env` exists, read it and use `AUTOPILOT_DISTILL_BIN` as the executable path. Otherwise use the stable path above.

## LOCAL_ISSUE_HANDOFF_CONTRACT

When `docs/agents/issue-tracker.md` configures Local Markdown, choose a stable lowercase `feature_slug` for the PRD evidence, use `to-issues` to draft and approve the vertical slices, but stop before its tracker-publication step. The Distill runner is the sole local publisher: it creates the PRD at `.scratch/<feature_slug>/PRD.md` and each local issue exactly once under `.scratch/<feature_slug>/issues/`. Do not create a second issue copy elsewhere under `.scratch/`. The runner rejects a target path that already contains different content; never acknowledge that collision as immaterial drift.

Before `submit-evidence`, ensure every local issue `body` begins with agent-ready triage frontmatter:

```markdown
---
Status: ready-for-agent
---
```

The exact Markdown including this frontmatter is the frozen issue payload. GitHub publication behavior is unchanged.

Run the CLI from the target project worktree and pass through the user's Distill command arguments. Report any non-zero exit status and stderr without retrying with another executable.
