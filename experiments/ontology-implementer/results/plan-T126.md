# Implementer plan: T126

## Task facts

- source: `github`
- runtime: `canonical`
- round: `first`
- specLinked: `true`
- multiModule: `true`
- changeKinds: rust-crate
- risks: api-contract, behavior-preservation

## Clauses (26)

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

### I29 [Quality]

Run the type-check/lint for changed Rust code.

Report check: `typecheck`  
Source: `SKILL.md Step 2`

### I30 [Context]

Read sibling modules named in CHANGED_FILES.

Report check: `context_read`  
Source: `SKILL.md Step 1`

### I32 [Risk]

For API-contract changes, update every caller and doc.

Report check: `risk_contract`  
Source: `SKILL.md scope`

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

## Context (12)

- `docs/adr/0045-expected-set-entry-resolves-its-skill-files.md:1-40` — ADR 0045
  > # ADR 0045: An Expected-set Entry Resolves Its Skill Files\n\n> Builds on ADR 0044 (single owner for the Expected set) and ADR 0036 (single\n> discoverable router). Sharpens the te
- `crates/deploy/src/pack.rs:181-220` — deploy dev/pack strictness
  > /// Refuse to pack while the Expected set contains failed entries (GLOSSARY.md).\n///\n/// `pack` is strict where `dev` is lenient: a tarball that silently omits a\n/// locked skil
- `GLOSSARY.md:14-26` — GLOSSARY — Skill file
  > \n**Expected-set entry**:\nOne skill belonging to the Expected set: its identity, its Skill source, its location in the source tree, and whether that location was resolved. An entr
- `corpus/issue-126.json:1-1` — Issue #126
  > ## Parent\n\n#121 — spec: Expected-set entry resolves its skill files\n\n## What to build\n\nThe entry-level resolution status retires. Every consumer derives failure from the skil
- `corpus/pr-127.diff:1-1` — PR #127 deliberate changes
  > diff --git a/CONTEXT.md b/CONTEXT.md\nindex 0317b4d..ef1139d 100644\n--- a/CONTEXT.md\n+++ b/CONTEXT.md\n@@ -16,6 +16,10 @@ _Avoid_: skill inventory, skill manifest\n One skill bel
- `crates/skill-index/src/lib.rs:25-100` — skill-index interface
  > \n/// Whether a Skill file (or a source directory) resolved.\n#[derive(Debug, Clone, PartialEq, Eq)]\npub enum ResolutionStatus {\n /// The file exists.\n Resolved,\n /// The faile
- `corpus/issue-121.json:1-1` — Spec #121
  > ## Problem Statement\n\nThe Expected set was given one owner in #110, but the Expected-set entry it returns only resolves a *directory*. Which files that directory owes the toolkit
- `tests/test_toolkit_setup.rs:1-60` — workspace test list
  > #!/usr/bin/env rust-script\n//! ```cargo\n//! [dependencies]\n//! skill-index = { path = \"../crates/skill-index\" }\n//! ```\n//!\n//! Integration tests for the toolkit-setup flow
- `crates/deploy/src/pack.rs:342-370` — deploy tests
  > // ── pack_command: Expected-set failure semantics ───────────────────\n\n /// Write a minimal packable project: one autopilot skill plus the\n /// install template. The fixture is
- `crates/skill-index/src/lib.rs:769-950` — skill-index enumerator tests
  > fn entry_skill_files_carry_kind_variant_and_resolution() {\n let tmp = tempfile::tempdir().unwrap();\n let root = tmp.path();\n\n // Agnostic autopilot skill: exactly one skill fil
- `crates/validation-runner/src/lib.rs:1225-1465` — validation pipeline tests
  > fn run_validation_fails_when_expected_entry_directory_is_missing() {\n let tmp = tempfile::tempdir().unwrap();\n let root = tmp.path();\n write_valid_autopilot_skill(root);\n write
- `crates/validation-runner/src/lib.rs:40-95` — validation-runner targets
  > /// One validation target: a Skill file the enumerator resolved, together with\n/// the Expected-set entry that owns it.\n///\n/// The entry is owned here so a report row carries i

## Report contract

Start with `IMPLEMENTER_REPORT:` and include `ROUND`, `STATUS`, `SUGGESTION_RESOLUTIONS`, `SELF_REVIEW`, `CHANGED_FILES`, `SUMMARY`.
Every clause above needs a check in the report; a production change needs TDD evidence; DONE requires executed passing evidence for every AC.
