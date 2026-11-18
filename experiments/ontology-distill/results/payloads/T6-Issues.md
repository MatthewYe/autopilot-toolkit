# Distill context pack: T6 / Issues

## Task facts

- requirement: Compare the vendored upstream pin with the newest release tag on a weekly schedule, open or update exactly one issue when drift exists, and never sync or merge automatically.
- change kinds: NewFeature
- surfaces: Ci
- runtimes: —
- multi-module: false; multi-slice: false; cross-runtime: false

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

## Context pointers (4)

- `GLOSSARY.md:1-40` — Repository glossary (canonical vocabulary)
  > # Autopilot Toolkit A skill-pack repo targeting Reasonix, Codex, and Kimi Code. Ships 41 skills — 32 upstream (from mattpocock/skills, tracked in `.skill-lock.json`), 1 vendor (third-party, tracked in `.vendor-lock.json`
- `skills/upstream/skills/engineering/to-tickets/SKILL.md:1-105` — to-tickets: vertical slicing and edges
  > --- name: to-tickets description: Break a plan, spec, or the current conversation into a set of tracer-bullet tickets, each declaring its blocking edges, published to the configured tracker (edges as text in one file per
- `docs/agents/issue-tracker.md:1-38` — Issue tracker configuration
  > # Issue tracker: GitHub Issues and PRDs for this repo live as GitHub issues on `MatthewYe/autopilot-toolkit`. Use the `gh` CLI for all operations. ## Conventions - **Create an issue**: `gh issue create --title \"...\" --
- `docs/agents/triage-labels.md:1-15` — Triage labels
  > # Triage Labels The skills speak in terms of five canonical triage roles. This file maps those roles to the actual label strings used in this repo's issue tracker. | Label in mattpocock/skills | Label in our tracker | Me
