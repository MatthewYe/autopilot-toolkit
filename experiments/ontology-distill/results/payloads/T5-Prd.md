# Distill context pack: T5 / Prd

## Task facts

- requirement: Extract the duplicated state-directory hygiene and atomic-write substrate from director and distill into one shared crate, removing the drifted atomic-write behaviour while keeping both CLIs working.
- change kinds: ContractChange, Refactor
- surfaces: Cli, State
- runtimes: —
- multi-module: true; multi-slice: false; cross-runtime: false

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

### P11 [PRD]

When a public contract changes, the spec states caller impact and the deprecation or migration path.

## Context pointers (7)

- `docs/adr/0040-distill-run-state-typed-module.md:1-14` — ADR 0040: typed run-state module
  > # Distill run state is a typed module > Amended by [ADR 0050](0050-run-state-persistence-substrate.md) for the storage substrate: the single-consumer rejection applies to the state schema, which stays in-crate. ## Contex
- `docs/adr/0041-distill-state-machine-owns-workflow-control.md:1-14` — ADR 0041: state machine owns workflow control
  > # Distill state machine owns workflow control > **Renumbered**: this ADR was previously `0009-distill-state-machine-owns-workflow-control`. It was renumbered to 0041 to resolve the duplicate number with `0009-codebase-de
- `docs/adr/0009-codebase-deepening-shared-crate-and-logic-sink.md:1-14` — ADR 0009: shared crate and logic sink
  > # 代码架构深化：共享基础设施 crate + 逻辑下沉 将代码架构中的重复代码消除、模块化拆分、强类型化三项改善统一执行。延续 ADR-0005 的\"rust-script 薄入口 + workspace crate 承载业务逻辑\"模式，新建 `crates/shared/`、`crates/deploy/`、`crates/validation-runner/`、`crates/skill-check/` 四个 workspac
- `docs/agents/domain.md:1-35` — Domain docs configuration
  > # Domain Docs How the engineering skills should consume this repo's domain documentation when exploring the codebase. ## Before exploring, read these - **`GLOSSARY.md`** at the repo root; if absent, read legacy **`CONTEX
- `GLOSSARY.md:1-40` — Repository glossary (canonical vocabulary)
  > # Autopilot Toolkit A skill-pack repo targeting Reasonix, Codex, and Kimi Code. Ships 41 skills — 32 upstream (from mattpocock/skills, tracked in `.skill-lock.json`), 1 vendor (third-party, tracked in `.vendor-lock.json`
- `skills/upstream/skills/engineering/to-spec/SKILL.md:1-75` — to-spec: PRD template and testing seam
  > --- name: to-spec description: \"Turn the current conversation into a spec and publish it to the project issue tracker: no interview, just synthesis of what you've already discussed.\" disable-model-invocation: true --- 
- `docs/agents/issue-tracker.md:1-38` — Issue tracker configuration
  > # Issue tracker: GitHub Issues and PRDs for this repo live as GitHub issues on `MatthewYe/autopilot-toolkit`. Use the `gh` CLI for all operations. ## Conventions - **Create an issue**: `gh issue create --title \"...\" --
