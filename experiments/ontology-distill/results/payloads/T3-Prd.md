# Distill context pack: T3 / Prd

## Task facts

- requirement: Provide a self-contained tarball install: one curl pipe command, skills deployed as real files so the directory is portable across machines, and upgrades that never touch user-owned skills.
- change kinds: Migration, NewFeature
- surfaces: Docs, Skills, Install
- runtimes: Codex, Kimi, Reasonix
- multi-module: true; multi-slice: true; cross-runtime: true

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

### P09 [PRD]

No interview happens at this stage: the spec synthesizes the clarification already captured.

### P10 [PRD]

When the change spans runtimes, the spec states the variant plan and the fallback behaviour.

### P12 [PRD]

When skill sources change, the spec names the repository validation and check commands as verification.

## Context pointers (10)

- `docs/adr/0013-distill-adapts-skills-without-modifying-them.md:1-14` — ADR 0013: adapt skills without modifying them
  > # Distill adapts skills without modifying them Distill-owned stage executor adapters wrap existing natural-language skills with stable input, completion-evidence, and artifact-validation contracts. The vendored `grill-wi
- `docs/adr/0036-single-discoverable-runtime-router.md:1-14` — ADR 0036: single discoverable runtime router
  > # ADR 0036: Single Discoverable Router for Runtime-Coupled Skills ## Context The self-contained install placed a fallback `SKILL.md` and each runtime variant's `SKILL.md` beneath the same shared skill directory. Codex re
- `docs/adr/0044-single-owner-for-the-expected-set.md:1-14` — ADR 0044: single owner for the Expected set
  > # ADR 0044: Single Owner for the Expected Set > Implements spec [#110](https://github.com/MatthewYe/autopilot-toolkit/issues/110) > (one owner for the Expected set). Builds on ADR 0001 (selfcheck derives the expected > s
- `docs/adr/0045-expected-set-entry-resolves-its-skill-files.md:1-14` — ADR 0045: entries resolve their skill files
  > # ADR 0045: An Expected-set Entry Resolves Its Skill Files > Builds on ADR 0044 (single owner for the Expected set) and ADR 0036 (single > discoverable router). Sharpens the term recorded as **Skill file** in > `CONTEXT.
- `docs/adr/0007-dual-runtime-skill-variants.md:1-14` — ADR 0007: dual-runtime skill variants
  > # ADR 0007: Dual-Runtime Skill Variants with Agent-Exclusive Install Directories > Superseded for installation and discovery by > [ADR 0036](0036-single-discoverable-runtime-router.md). Per-runtime source > variants rema
- `docs/adr/0008-self-contained-tarball-install.md:1-14` — ADR 0008: self-contained tarball install
  > # ADR 0008: Self-Contained Tarball Install with Bootstrap Symlinks > Superseded for runtime-coupled skill layout and bootstrap behavior by > [ADR 0036](0036-single-discoverable-runtime-router.md). The self-contained > ta
- `GLOSSARY.md:1-40` — Repository glossary (canonical vocabulary)
  > # Autopilot Toolkit A skill-pack repo targeting Reasonix, Codex, and Kimi Code. Ships 41 skills — 32 upstream (from mattpocock/skills, tracked in `.skill-lock.json`), 1 vendor (third-party, tracked in `.vendor-lock.json`
- `docs/prd/0004-self-contained-tarball-install.md:1-30` — PRD 0004: install exemplar
  > # PRD 0004: Self-Contained Tarball Install **Status**: ready-for-agent **Parent**: — **ADR**: [0008-self-contained-tarball-install](../adr/0008-self-contained-tarball-install.md) ## Problem Statement autopilot-toolkit cu
- `skills/upstream/skills/engineering/to-spec/SKILL.md:1-75` — to-spec: PRD template and testing seam
  > --- name: to-spec description: \"Turn the current conversation into a spec and publish it to the project issue tracker: no interview, just synthesis of what you've already discussed.\" disable-model-invocation: true --- 
- `docs/agents/issue-tracker.md:1-38` — Issue tracker configuration
  > # Issue tracker: GitHub Issues and PRDs for this repo live as GitHub issues on `MatthewYe/autopilot-toolkit`. Use the `gh` CLI for all operations. ## Conventions - **Create an issue**: `gh issue create --title \"...\" --
