# Distill context pack: T8 / Prd

## Task facts

- requirement: Make an Expected-set entry resolve every skill file it owns, root and per-runtime variants, so failure is reported per file; ship it as an expand, migrate, contract sequence.
- change kinds: ContractChange, Refactor, WideRefactor
- surfaces: Skills, Cli
- runtimes: —
- multi-module: true; multi-slice: true; cross-runtime: false

## Clauses (11)

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

### P11 [PRD]

When a public contract changes, the spec states caller impact and the deprecation or migration path.

### P12 [PRD]

When skill sources change, the spec names the repository validation and check commands as verification.

## Context pointers (6)

- `docs/adr/0013-distill-adapts-skills-without-modifying-them.md:1-14` — ADR 0013: adapt skills without modifying them
  > # Distill adapts skills without modifying them Distill-owned stage executor adapters wrap existing natural-language skills with stable input, completion-evidence, and artifact-validation contracts. The vendored `grill-wi
- `docs/adr/0044-single-owner-for-the-expected-set.md:1-14` — ADR 0044: single owner for the Expected set
  > # ADR 0044: Single Owner for the Expected Set > Implements spec [#110](https://github.com/MatthewYe/autopilot-toolkit/issues/110) > (one owner for the Expected set). Builds on ADR 0001 (selfcheck derives the expected > s
- `docs/agents/domain.md:1-35` — Domain docs configuration
  > # Domain Docs How the engineering skills should consume this repo's domain documentation when exploring the codebase. ## Before exploring, read these - **`GLOSSARY.md`** at the repo root; if absent, read legacy **`CONTEX
- `GLOSSARY.md:1-40` — Repository glossary (canonical vocabulary)
  > # Autopilot Toolkit A skill-pack repo targeting Reasonix, Codex, and Kimi Code. Ships 41 skills — 32 upstream (from mattpocock/skills, tracked in `.skill-lock.json`), 1 vendor (third-party, tracked in `.vendor-lock.json`
- `skills/upstream/skills/engineering/to-spec/SKILL.md:1-75` — to-spec: PRD template and testing seam
  > --- name: to-spec description: \"Turn the current conversation into a spec and publish it to the project issue tracker: no interview, just synthesis of what you've already discussed.\" disable-model-invocation: true --- 
- `docs/agents/issue-tracker.md:1-38` — Issue tracker configuration
  > # Issue tracker: GitHub Issues and PRDs for this repo live as GitHub issues on `MatthewYe/autopilot-toolkit`. Use the `gh` CLI for all operations. ## Conventions - **Create an issue**: `gh issue create --title \"...\" --
