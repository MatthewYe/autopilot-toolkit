# Distill context pack: T5 / Issues

## Task facts

- requirement: Extract the duplicated state-directory hygiene and atomic-write substrate from director and distill into one shared crate, removing the drifted atomic-write behaviour while keeping both CLIs working.
- change kinds: ContractChange, Refactor
- surfaces: Cli, State
- runtimes: —
- multi-module: true; multi-slice: false; cross-runtime: false

## Clauses (8)

### T01 [Issues]

Each issue is a vertical tracer bullet that crosses the layers it changes and is demoable on its own.

### T02 [Issues]

Each issue fits one fresh context window and can be implemented without the rest of the set.

### T04 [Issues]

Each issue names its parent PRD or spec.

### T05 [Issues]

What to build describes user-observable behaviour, not a layer-by-layer task list.

### T06 [Issues]

Acceptance criteria are concrete, independently verifiable, and each maps to a test or command.

### T08 [Issues]

The user approves issue granularity and blocking edges before publication.

### T10 [Issues]

The issue set covers every PRD user story and decision, and invents no scope the PRD did not decide.

### T11 [Issues]

Open questions discovered while slicing go back to the PRD instead of being settled inside a ticket.

## Context pointers (6)

- `docs/adr/0040-distill-run-state-typed-module.md:1-14` — ADR 0040: typed run-state module
  > # Distill run state is a typed module > Amended by [ADR 0050](0050-run-state-persistence-substrate.md) for the storage substrate: the single-consumer rejection applies to the state schema, which stays in-crate. ## Contex
- `docs/adr/0041-distill-state-machine-owns-workflow-control.md:1-14` — ADR 0041: state machine owns workflow control
  > # Distill state machine owns workflow control > **Renumbered**: this ADR was previously `0009-distill-state-machine-owns-workflow-control`. It was renumbered to 0041 to resolve the duplicate number with `0009-codebase-de
- `GLOSSARY.md:1-40` — Repository glossary (canonical vocabulary)
  > # Autopilot Toolkit A skill-pack repo targeting Reasonix, Codex, and Kimi Code. Ships 41 skills — 32 upstream (from mattpocock/skills, tracked in `.skill-lock.json`), 1 vendor (third-party, tracked in `.vendor-lock.json`
- `skills/upstream/skills/engineering/to-tickets/SKILL.md:1-105` — to-tickets: vertical slicing and edges
  > --- name: to-tickets description: Break a plan, spec, or the current conversation into a set of tracer-bullet tickets, each declaring its blocking edges, published to the configured tracker (edges as text in one file per
- `docs/agents/issue-tracker.md:1-38` — Issue tracker configuration
  > # Issue tracker: GitHub Issues and PRDs for this repo live as GitHub issues on `MatthewYe/autopilot-toolkit`. Use the `gh` CLI for all operations. ## Conventions - **Create an issue**: `gh issue create --title \"...\" --
- `docs/agents/triage-labels.md:1-15` — Triage labels
  > # Triage Labels The skills speak in terms of five canonical triage roles. This file maps those roles to the actual label strings used in this repo's issue tracker. | Label in mattpocock/skills | Label in our tracker | Me
