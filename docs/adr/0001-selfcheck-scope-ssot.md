# Selfcheck scope: toolkit-only, SSOT-driven

The toolkit-selfcheck previously inspected the entire `~/.agents/skills/` directory — counting all entries, checking all symlinks, validating all frontmatter — and compared against hardcoded lists of expected and excluded skill names. This caused false failures whenever other projects installed skills into the shared directory, and the hardcoded lists drifted out of sync with `.skill-lock.json` and `skills/autopilot/`.

We decided the selfcheck must only verify the toolkit skills, deriving the **expected set** at runtime from the two single sources of truth: `.skill-lock.json` (upstream skills) and `skills/autopilot/*/SKILL.md` scanning (autopilot skills). The excluded-items check is removed. Symlink integrity checks now include full target-path verification against `PROJECT_ROOT` to catch same-name conflicts from other projects.
