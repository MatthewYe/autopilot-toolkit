# Implementer plan: T125

## Task facts

- source: `github`
- runtime: `canonical`
- round: `first`
- specLinked: `true`
- multiModule: `true`
- changeKinds: skill, test
- risks: behavior-preservation

## Clauses (24)

### I01 [TDD]

No production code without a failing test first.

Report check: `tdd_evidence`  
Source: `SKILL.md Step 2 iron rule`

### I02 [TDD]

Tests assert observable behavior, not internal structure.

Report check: `test_review`  
Source: `SKILL.md TDD`

### I03 [TDD]

Mock only at system boundaries; never mock own types.

Report check: `test_review`  
Source: `SKILL.md TDD`

### I04 [TDD]

Run RED, then GREEN, then REFACTOR; do not skip RED.

Report check: `tdd_evidence`  
Source: `SKILL.md TDD`

### I05 [TDD]

Every acceptance criterion has at least one test.

Report check: `ac_coverage`  
Source: `SKILL.md Step 2`

### I06 [Context]

Read the issue or AGENT-BRIEF before editing.

Report check: `context_read`  
Source: `SKILL.md Step 1`

### I07 [Context]

Read GLOSSARY.md (fallback CONTEXT.md).

Report check: `context_read`  
Source: `SKILL.md Step 1`

### I08 [Context]

Read the linked spec/PRD/ADR before editing.

Report check: `context_read`  
Source: `SKILL.md Step 1`

### I09 [Context]

Check Seam annotations and stay inside the named seam.

Report check: `context_read`  
Source: `SKILL.md Step 1`

### I16 [Diagnosis]

After at most two diagnosis hypotheses, report BLOCKED instead of guessing.

Report check: `diagnosis`  
Source: `SKILL.md diagnosis`

### I17 [SelfReview]

Self-review runs the full test suite.

Report check: `self_review`  
Source: `SKILL.md Step 2.5`

### I18 [SelfReview]

Self-review checks acceptance-criteria coverage.

Report check: `self_review`  
Source: `SKILL.md Step 2.5`

### I19 [SelfReview]

Self-review checks scope creep.

Report check: `self_review`  
Source: `SKILL.md Step 2.5`

### I20 [Report]

CHANGED_FILES lists every contract-referenced file; mark unedited files (pre-existing).

Report check: `changed_files`  
Source: `SKILL.md report`

### I21 [Report]

Report starts IMPLEMENTER_REPORT: and carries ROUND, STATUS, SUGGESTION_RESOLUTIONS, SELF_REVIEW, CHANGED_FILES, SUMMARY.

Report check: `report_schema`  
Source: `SKILL.md report`

### I22 [Report]

DONE only if every acceptance criterion has executed passing evidence.

Report check: `status_evidence`  
Source: `SKILL.md status`

### I25 [Diagnosis]

BLOCKED names the blocking fact after two failed hypotheses.

Report check: `diagnosis`  
Source: `SKILL.md status`

### I26 [Diagnosis]

NEEDS_CONTEXT names the missing fact; do not guess.

Report check: `diagnosis`  
Source: `SKILL.md status`

### I27 [Guardrail]

Do not modify scope or acceptance criteria.

Report check: `guardrail`  
Source: `SKILL.md prohibited`

### I28 [Guardrail]

Do not guess a fix; diagnose before editing.

Report check: `guardrail`  
Source: `SKILL.md prohibited`

### I30 [Context]

Read sibling modules named in CHANGED_FILES.

Report check: `context_read`  
Source: `SKILL.md Step 1`

### I33 [Risk]

For behavior-preservation work, keep existing outputs and list deliberate changes.

Report check: `risk_contract`  
Source: `SKILL.md scope`

### I34 [Guardrail]

Do not add undeclared dependencies.

Report check: `guardrail`  
Source: `SKILL.md prohibited`

### I37 [Spec]

Spec-linked work maps each AC to the spec and records deliberate behavior changes.

Report check: `spec_mapping`  
Source: `orchestrator spec contract`

## Context (7)

- `docs/adr/0045-expected-set-entry-resolves-its-skill-files.md:1-40` — ADR 0045
  > # ADR 0045: An Expected-set Entry Resolves Its Skill Files\n\n> Builds on ADR 0044 (single owner for the Expected set) and ADR 0036 (single\n> discoverable router). Sharpens the te
- `CONTEXT.md:19-21` — CONTEXT.md — Skill file
  > **Skill file**:\nOne source file an Expected-set entry owns and the toolkit can act on: the root `SKILL.md`, a runtime variant's `SKILL.md`, or a runtime variant's `agent.toml`. A 
- `corpus/issue-125.json:1-1` — Issue #125
  > ## Parent\n\n#121 — spec: Expected-set entry resolves its skill files\n\n## What to build\n\nThe toolkit-setup procedure and its integration suite read the enumerator's skill-file 
- `crates/skill-index/src/lib.rs:25-100` — skill-index interface at #125 base
  > \n/// Whether an Expected-set entry's source directory resolved.\n#[derive(Debug, Clone, PartialEq, Eq)]\npub enum ResolutionStatus {\n /// The source directory exists.\n Resolved,
- `corpus/issue-121.json:1-1` — Spec #121
  > ## Problem Statement\n\nThe Expected set was given one owner in #110, but the Expected-set entry it returns only resolves a *directory*. Which files that directory owes the toolkit
- `tests/test_toolkit_setup.rs:60-120` — toolkit-setup suite at #125 base
  > let entries = skill_index::discover_skills(&project_root())\n .expect(\"enumerate the Expected set from the repo tree\");\n\n assert!(\n !entries.is_empty(),\n \"the repo tree must
- `skills/autopilot/toolkit-setup/SKILL.md:50-80` — toolkit-setup procedure at #125 base
  > 7. If `codex/agent.toml` exists, verify the corresponding\n `~/.codex/agents/<skill>.toml` link.\n8. Verify `${AGENTS_PRINCIPLES_DIR:-$HOME/.agents/principles}` points to\n `$PROJE

## Report contract

Start with `IMPLEMENTER_REPORT:` and include `ROUND`, `STATUS`, `SUGGESTION_RESOLUTIONS`, `SELF_REVIEW`, `CHANGED_FILES`, `SUMMARY`.
Every clause above needs a check in the report; a production change needs TDD evidence; DONE requires executed passing evidence for every AC.
