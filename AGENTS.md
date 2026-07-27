# autopilot-toolkit

19 skills for Reasonix, Codex, and Kimi Code — 19 upstream engineering/productivity skills from mattpocock/skills plus 6 autopilot workflow skills (orchestrator → implementer → reviewer). Runtime-agnostic skills deploy via symlinks to `~/.agents/skills/`; runtime-coupled skills ship per-runtime variant sources behind one installed router.

## Project

A skill-pack repo. The "code" is SKILL.md files — markdown with YAML frontmatter. Tooling (install, validation, tests) is Rust via `rust-script`. The upstream subtree (`skills/upstream/`) is a vendored snapshot of [mattpocock/skills](https://github.com/mattpocock/skills). The autopilot skills (`skills/autopilot/`) are custom additions for the agent workflow loop.

## Commands

```bash
rust-script deploy.rs dev                     # Symlink all skills into agent dirs
rust-script deploy.rs dev-clean               # Remove dev symlinks
rust-script deploy.rs pack                    # Build tarball into dist/
rust-script deploy.rs distill-artifacts       # Build Distill CLI for release platforms
rust-script deploy.rs release                 # Pack + push to GitHub Releases
rust-script deploy.rs link-principles <src>   # Ensure ~/.agents/principles symlink
rust-script validation/run.rs                 # validate all SKILL.md frontmatter (all variants)
rust-script --test validation/run.rs          # runner unit tests
cargo test                                    # validation library unit tests
rust-script --test tests/test_install.rs      # integration tests for deploy.rs
rust-script --test tests/test_toolkit_setup.rs
rust-script --test tests/test_github_verify.rs
rust-script --test tests/test_check.rs
```

No build step — skills are consumed directly from the source tree by the agent runtime.

## Architecture

```
skills/
├── upstream/          # vendored mattpocock/skills (13 installed, see .skill-lock.json)
│   ├── engineering/   # codebase-design, diagnosing-bugs, domain-modeling, tdd, triage, …
│   ├── productivity/  # grilling, handoff, teach, writing-great-skills, …
│   └── misc/          # git-guardrails-claude-code, scaffold-exercises, …
├── autopilot/         # 6 custom autopilot skills
│   ├── autopilot-orchestrator/   # scans .scratch/ + GitHub Issues for ready work
│   │   ├── reasonix/  # per-runtime variant sources (runtime-coupled skills)
│   │   ├── codex/
│   │   ├── kimi/
│   │   └── references/          # shared reference docs
│   ├── autopilot-implementer/    # TDD-driven implementation agent (same variant layout)
│   ├── autopilot-reviewer/       # four-axis review (behavior, TDD, code, plan)
│   ├── audit-autopilot/          # post-hoc fidelity audit of agent execution
│   ├── toolkit-setup/            # install/update orchestration (agnostic)
│   └── zoom-out/                 # higher-level perspective (agnostic)
deploy.rs             # deploy tool (dev symlink + pack/release) (--target reasonix|codex, --shared → ~/.agents/skills/)
crates/validation/     # frontmatter validation library (strict YAML + field checks)
validation/run.rs      # validation runner — discovers all variant sources
tests/                 # rust-script integration tests
docs/
├── agents/            # issue-tracker, triage-labels, domain config
├── issues/            # local issue docs
├── prd/               # PRD-0001..0003
└── reports/           # smoke-test results
.scratch/              # local-markdown issue tracker (legacy)
```

## Install model

- **Runtime-agnostic skills** (upstream 13 + toolkit-setup + zoom-out) → `~/.agents/skills/` via `--shared`.
- **Runtime-coupled skills** (the 5 workflow skills) retain variant sources per runtime, but pack/dev install one router at `~/.agents/skills/<name>/SKILL.md`. Variant bodies are renamed to `runtime/<runtime>/INSTRUCTIONS.md` so recursive discovery yields one logical skill. Codex `agent.toml` files are still linked into `~/.codex/agents/`.
- `toolkit-setup` orchestrates discovery, diagnosis, minimal sync/unlink, and verification per `--target`.

## Conventions

- **SKILL.md frontmatter** — every skill opens with `---`-delimited YAML that must parse under a **strict** YAML parser (quote values containing `: `). Required: `name` (alphanumeric, 1-64 chars, hyphens/underscores/dots ok), `description`. Reasonix variants may add `runAs` (`inline`|`subagent`) + `allowed-tools` (required when `runAs: subagent`); kimi variants carry only `name` + `description`.
- **Rust scripts** — `rust-script` with a `//! ```cargo` dependency header. Section dividers: `# ── name ──`.
- **Tests** — `#[test]` fns run via `rust-script --test`; integration tests drive `install.rs` through `std::process::Command` with temp-dir env overrides (`AGENTS_SKILLS_DIR` etc.).
- **Issue tracking** — local markdown tracker in `docs/issues/` + PRDs in `docs/prd/` (GitHub Issues configured but `gh` not currently available on this machine).

## Agent skills

### Issue tracker

Local tracker: `docs/issues/` (numbered, `Parent` → PRD in `docs/prd/`). GitHub Issues on `MatthewYe/autopilot-toolkit` is the configured remote tracker (see `docs/agents/issue-tracker.md`), used when `gh` is available.

### Triage labels

Defaults: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context (`CONTEXT.md` + `docs/adr/` at repo root). See `docs/agents/domain.md`.

## Notes
