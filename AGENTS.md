# autopilot-toolkit

19 Reasonix skills — 13 upstream engineering/productivity skills from mattpocock/skills plus 6 autopilot workflow skills (orchestrator → implementer → reviewer). Deployed via symlinks to `~/.agents/skills/`.

## Project

A skill-pack repo for Reasonix. The "code" is SKILL.md files — markdown with YAML frontmatter (`name`, `description`, optional `runAs`/`allowed-tools`). There is no runtime language; scaffolding and validation are bash. The upstream subtree (`skills/upstream/`) is a vendored snapshot of [mattpocock/skills](https://github.com/mattpocock/skills). The autopilot skills (`skills/autopilot/`) are custom additions for the agent workflow loop.

## Commands

```bash
bash install.sh                  # deploy symlinks to ~/.agents/skills/
bash tests/test_install.sh        # integration tests for install.sh
bash validation/run.sh            # validate all 19 SKILL.md frontmatter files
bash validation/validate.test.sh  # unit tests for the validation library
```

No build step — skills are consumed directly from the source tree by the agent runtime.

## Architecture

```
skills/
├── upstream/          # vendored mattpocock/skills (14 installed, see .skill-lock.json)
│   ├── engineering/   # codebase-design, diagnosing-bugs, domain-modeling, tdd, triage, …
│   ├── productivity/  # grilling, handoff, teach, writing-great-skills, …
│   └── misc/          # git-guardrails-claude-code, scaffold-exercises, …
├── autopilot/         # 5 custom autopilot skills
│   ├── autopilot-orchestrator/   # scans .scratch/ + GitHub Issues for ready work
│   ├── autopilot-implementer/    # TDD-driven implementation agent
│   ├── autopilot-reviewer/       # four-axis review (behavior, TDD, code, plan)
│   ├── audit-autopilot/          # post-hoc fidelity audit of agent execution
│   └── toolkit-selfcheck/        # installation integrity check
install.sh             # symlink deployment to ~/.agents/skills/
.skill-lock.json       # upstream skill manifest (name, path, hashes)
validation/            # frontmatter validation library + runner
tests/                 # integration tests for install.sh
docs/
├── agents/            # issue-tracker, triage-labels, domain config
├── issues/            # archived issue docs
├── prd/               # PRD-0001
└── reports/           # smoke-test results
.scratch/              # local-markdown issue tracker (legacy)
```

## Conventions

- **SKILL.md frontmatter** — every skill opens with `---`-delimited YAML. Required: `name` (alphanumeric, 1-64 chars, hyphens/underscores/dots ok), `description` (≤120 chars). Optional: `runAs` (`inline`|`subagent`), `allowed-tools` (required when `runAs: subagent`).
- **Bash scripts** — `#!/usr/bin/env bash`, `set -euo pipefail`. Section dividers: `# ── name ──`. Variable naming: `UPPER_CASE` for constants, `lower_case` for locals.
- **Tests** — assert-style: `assert "description" "condition"` and `assert_eq "desc" "expected" "actual"` with PASS/FAIL counters. Source `validation/validate.sh` for the `validate_skill` library.
- **Issue tracking** — primary: GitHub Issues on `matthewye/autopilot-toolkit` (see docs/agents/). Legacy local-markdown tracker under `.scratch/` is historical.

## Agent skills

### Issue tracker

GitHub Issues on `matthewye/autopilot-toolkit`; external PRs are a triage surface. See `docs/agents/issue-tracker.md`.

### Triage labels

Defaults: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context (`CONTEXT.md` + `docs/adr/` at repo root). See `docs/agents/domain.md`.

## Notes
