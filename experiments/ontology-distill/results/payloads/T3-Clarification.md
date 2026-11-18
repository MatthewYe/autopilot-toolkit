# Distill context pack: T3 / Clarification

## Task facts

- requirement: Provide a self-contained tarball install: one curl pipe command, skills deployed as real files so the directory is portable across machines, and upgrades that never touch user-owned skills.
- change kinds: Migration, NewFeature
- surfaces: Docs, Skills, Install
- runtimes: Codex, Kimi, Reasonix
- multi-module: true; multi-slice: true; cross-runtime: true

## Clauses (9)

### C01 [Clarification]

The clarified requirement restates the problem, constraints and success condition without silently resolving open decisions.

### C02 [Clarification]

Every decision taken during clarification is listed explicitly, with the alternative that was rejected.

### C03 [Clarification]

Every accepted assumption is listed explicitly and is falsifiable.

### C04 [Clarification]

Every material unknown is recorded; none may remain unresolved when clarification completes.

### C06 [Clarification]

Each round asks the whole current frontier; questions that depend on an open answer wait for the next round.

### C07 [Clarification]

Each question is numbered and carries a recommended answer worded so that yes accepts it.

### C08 [Clarification]

Facts are looked up by the agent; the user is never asked for something the environment can answer.

### C09 [Clarification]

Decisions belong to the user; do not act until they confirm a shared understanding.

### C12 [Clarification]

The clarification records what is explicitly out of scope for this requirement.

## Context pointers (7)

- `docs/adr/0010-distill-snapshots-requirement-inputs.md:1-14` — ADR 0010: snapshot the requirement input
  > # Distill snapshots requirement inputs Every Distill run captures an immutable snapshot of its original requirement inputs, including user text, uploaded file contents, and fetched link contents with provenance and conte
- `docs/adr/0026-distill-separates-user-and-executor-authority.md:1-14` — ADR 0026: separate user and executor authority
  > # Distill separates user and executor authority Only explicit user instructions may abort, purge, or take over a Distill run. Stage executors may declare waiting, blocked, completed, reconciliation, or a reasoned superse
- `docs/adr/0030-distill-skills-run-to-the-next-boundary.md:1-14` — ADR 0030: run to the next boundary
  > # autopilot-distill skills run to the next boundary Once invoked, an `autopilot-distill` skill variant repeatedly executes the runner's currently authorized action and advances completed stages without requiring another 
- `GLOSSARY.md:1-40` — Repository glossary (canonical vocabulary)
  > # Autopilot Toolkit A skill-pack repo targeting Reasonix, Codex, and Kimi Code. Ships 41 skills — 32 upstream (from mattpocock/skills, tracked in `.skill-lock.json`), 1 vendor (third-party, tracked in `.vendor-lock.json`
- `skills/upstream/skills/productivity/grilling/SKILL.md:1-30` — Grilling: frontier rounds and question format
  > --- name: grilling description: Grill the user relentlessly about a plan, decision, or idea. Use when the user wants to stress-test their thinking, or uses any 'grill' trigger phrases. --- Interview the user relentlessly
- `docs/agents/issue-tracker.md:1-38` — Issue tracker configuration
  > # Issue tracker: GitHub Issues and PRDs for this repo live as GitHub issues on `MatthewYe/autopilot-toolkit`. Use the `gh` CLI for all operations. ## Conventions - **Create an issue**: `gh issue create --title \"...\" --
- `crates/distill-cli/workflows/distill.v1.json:1-33` — Distill workflow declaration
  > { \"version\": \"distill.v1\", \"stages\": [ { \"id\": \"intake\", \"executor\": \"runner\", \"skill\": null, \"checkpoint\": \"explicit-text-captured\", \"next_action\": \"capture-text-requirement\" }, { \"id\": \"clari
