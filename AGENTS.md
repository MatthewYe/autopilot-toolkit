# autopilot-toolkit

25 named toolkit capabilities for Reasonix, Codex, and Ksana — 19 upstream engineering/productivity skills from mattpocock/skills plus 6 autopilot workflow capabilities (orchestrator → implementer → reviewer). Runtime-agnostic skills deploy via symlinks to the shared `~/.agents/skills/`; the 4 runtime-coupled workflow capabilities ship per-runtime variants (`reasonix/`, `codex/`, `ksana/`) deployed to each runtime's agent-exclusive directory.

## Project

A skill-pack repo for Reasonix, Codex, and Ksana. The "code" is SKILL.md files — markdown with YAML frontmatter (`name`, `description`, optional `runAs`/`allowed-tools`). There is no runtime language; scaffolding and validation are rust-script. The upstream subtree (`skills/upstream/`) is a vendored snapshot of [mattpocock/skills](https://github.com/mattpocock/skills). The autopilot skills (`skills/autopilot/`) are custom additions for the agent workflow loop.

## Commands

```bash
rust-script install.rs sync <name> <src> [--target reasonix|codex|ksana] [--shared|--agent] # atomic symlink sync
rust-script install.rs setup --target reasonix|codex|ksana [--dry-run] # complete deterministic install/update
rust-script install.rs status --target reasonix|codex|ksana # read-only installation verification
rust-script tests/test_install.rs        # integration tests for install.rs
rust-script validation/run.rs            # validate all SKILL.md frontmatter files (all variants)
cargo test -p validation                 # unit tests for the validation library
```

No build step — skills are consumed directly from the source tree by the agent runtime.

## Architecture

```
skills/
├── upstream/          # vendored mattpocock/skills (19 installed, see .skill-lock.json)
│   ├── engineering/   # codebase-design, diagnosing-bugs, domain-modeling, tdd, triage, …
│   ├── productivity/  # grilling, handoff, teach, writing-great-skills, …
│   └── misc/          # git-guardrails-claude-code, scaffold-exercises, …
├── autopilot/         # 6 custom autopilot skills
│   ├── autopilot-orchestrator/   # scans .scratch/ + GitHub Issues for ready work (reasonix/codex/ksana variants)
│   ├── autopilot-implementer/    # TDD-driven implementation agent (reasonix SKILL.md; codex/ksana agent.toml)
│   ├── autopilot-reviewer/       # four-axis review (behavior, TDD, code, plan) (reasonix SKILL.md; codex/ksana agent.toml)
│   ├── audit-autopilot/          # post-hoc fidelity audit of agent execution (reasonix/codex/ksana variants)
│   ├── toolkit-setup/            # install/update orchestration (runtime-agnostic)
│   └── zoom-out/                 # higher-level perspective (runtime-agnostic)
install.rs             # symlink deployment to ~/.agents/skills/ + per-runtime agent-exclusive dirs
.skill-lock.json       # upstream skill manifest (name, path, hashes)
validation/            # frontmatter validation library + runner
tests/                 # integration tests for install.rs
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
