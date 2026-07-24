---
name: distill
description: "Run the Distill requirement-to-issues workflow through the installed toolkit CLI."
---

Use the installed Distill CLI at `~/.agents/skills/.autopilot/bin/distill`.

If `~/.agents/skills/.autopilot/distill.env` exists, read it and use `AUTOPILOT_DISTILL_BIN` as the executable path. Otherwise use the stable path above.

Run the CLI from the target project worktree and pass through the user's Distill command arguments. Report any non-zero exit status and stderr without retrying with another executable.
