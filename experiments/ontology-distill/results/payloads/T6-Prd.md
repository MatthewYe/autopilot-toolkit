# Distill context pack: T6 / Prd

## Task facts

- requirement: Compare the vendored upstream pin with the newest release tag on a weekly schedule, open or update exactly one issue when drift exists, and never sync or merge automatically.
- change kinds: NewFeature
- surfaces: Ci
- runtimes: —
- multi-module: false; multi-slice: false; cross-runtime: false

## Clauses (10)

### P01 [PRD]

Problem Statement describes the user-visible problem and contains no solution.

### P02 [PRD]

Solution states the user-visible outcome and leaves no open design decision.

### P03 [PRD]

User Stories are numbered, in As a / I want / so that form, and cover each affected user.

### P04 [PRD]

Implementation Decisions name modules, interfaces and contracts; they do not prescribe new source file paths or code.

### P05 [PRD]

Testing Decisions name the single highest seam and what makes a good test at that seam.

### P06 [PRD]

Out of Scope lists the excluded behaviour explicitly.

### P07 [PRD]

The testing seam is confirmed with the user before the PRD is published.

### P08 [PRD]

The spec uses glossary vocabulary and does not contradict an accepted ADR without an explicit supersession.

### P09 [PRD]

No interview happens at this stage: the spec synthesizes the clarification already captured.

### P13 [PRD]

When CI or release changes, the spec states the trigger and the failure behaviour.

## Context pointers (6)

- `docs/adr/0052-upstream-pin-and-drift-check.md:1-14` — ADR 0052: upstream pin and drift check
  > # ADR 0052: Record the Upstream Ref as a Machine-Verifiable Pin > Extends [ADR 0037](0037-upstream-sync-full-replacement.md) (the snapshot is > replaced verbatim) and [ADR 0049](0049-typed-lock-write-path-and-upstream-sy
- `docs/agents/domain.md:1-35` — Domain docs configuration
  > # Domain Docs How the engineering skills should consume this repo's domain documentation when exploring the codebase. ## Before exploring, read these - **`GLOSSARY.md`** at the repo root; if absent, read legacy **`CONTEX
- `GLOSSARY.md:1-40` — Repository glossary (canonical vocabulary)
  > # Autopilot Toolkit A skill-pack repo targeting Reasonix, Codex, and Kimi Code. Ships 41 skills — 32 upstream (from mattpocock/skills, tracked in `.skill-lock.json`), 1 vendor (third-party, tracked in `.vendor-lock.json`
- `skills/upstream/skills/engineering/to-spec/SKILL.md:1-75` — to-spec: PRD template and testing seam
  > --- name: to-spec description: \"Turn the current conversation into a spec and publish it to the project issue tracker: no interview, just synthesis of what you've already discussed.\" disable-model-invocation: true --- 
- `docs/agents/issue-tracker.md:1-38` — Issue tracker configuration
  > # Issue tracker: GitHub Issues and PRDs for this repo live as GitHub issues on `MatthewYe/autopilot-toolkit`. Use the `gh` CLI for all operations. ## Conventions - **Create an issue**: `gh issue create --title \"...\" --
- `docs/agents/upstream-sync.md:1-40` — Upstream sync procedure
  > # Upstream Sync Runbook How to replace the vendored snapshot at `skills/upstream/` with a newer [mattpocock/skills](https://github.com/mattpocock/skills) revision. Audience: a maintainer or an agent running a one-shot sy
