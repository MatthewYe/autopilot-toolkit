#!/usr/bin/env python3
"""Generate the Distill clause ontology, task data and context cards.

The clause inventory is derived from the unmodified stage skills, the Distill
runner contract and the repository ADRs.  It is not a rewrite of those skills:
each clause carries a source anchor, and the adapter (not the skill) renders it.

Run from the experiment root:  python3 scripts/build_artifacts.py
"""
from __future__ import annotations

import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REPO = ROOT.parents[1]
PREFIX = "https://example.org/autopilot/distill#"

# ── clauses ────────────────────────────────────────────────────────────────
# id, axis, stage, check, text, anchor, keywords, conditions, checked_by, enforced
# conditions keys: changeKind, surface, runtime, multiModule, crossRuntime,
# multiSlice, domainDocs, adr, newAdr, externalTracker, wideRefactor,
# userFacing, hasExistingSeam, ambiguity
CLAUSES = [
    # Runner-owned invariants.  These are already enforced in Rust; injecting
    # them into the model's context is noise, so the selection arms exclude them.
    ("R01", "Intake", "Intake", "Runner",
     "The requirement text is captured verbatim and frozen; later stages read the snapshot, not the mutable source.",
     "docs/adr/0010-distill-snapshots-requirement-inputs.md", "requirement snapshot freeze",
     {}, None, "Runner"),
    ("R02", "Intake", "Intake", "Runner",
     "Only the stage's authorized executor runs; stages are never skipped, reordered or run early.",
     "docs/adr/0041-distill-state-machine-owns-workflow-control.md", "stage order authorized executor",
     {}, None, "Runner"),
    ("R03", "Intake", "Intake", "Runner",
     "Every yield to the user reports run_id, stage, revision and next_action.",
     "crates/distill-cli/workflows/distill.v1.json", "run id stage revision next action",
     {}, None, "Runner"),
    ("R04", "Intake", "Intake", "Runner",
     "Only an explicit user instruction authorizes abort, purge or takeover.",
     "docs/adr/0026-distill-separates-user-and-executor-authority.md", "abort purge takeover user authority",
     {}, None, "Runner"),
    ("R05", "Intake", "Issues", "Runner",
     "Publication receipts name the frozen payload hash and are verified offline before the run advances.",
     "docs/adr/0023-distill-freezes-publication-payloads.md", "receipt publication hash confirmed",
     {}, None, "Runner"),
    ("R06", "Intake", "Intake", "Runner",
     "An ambiguous or missing runtime session identity fails closed.",
     "skills/autopilot/autopilot-distill/dsh/SKILL.md", "session identity fail closed ambiguous",
     {}, None, "Runner"),
    ("R07", "Intake", "Intake", "Runner",
     "Context drift blocks the stage unless a reasoned immaterial acknowledgement or a supersession is recorded.",
     "docs/adr/0035-distill-blocks-on-worktree-context-drift.md", "context drift immaterial supersession",
     {}, None, "Runner"),
    ("R08", "Intake", "Issues", "Runner",
     "Local publication happens only through the runner, into .scratch/<feature_slug>/.",
     "skills/autopilot/autopilot-distill/dsh/SKILL.md", "local publication runner scratch",
     {}, None, "Runner"),

    # Clarification: the runner checks field presence and shape; the clauses
    # below are the semantic part the runner cannot check.
    ("C01", "Clarification", "Clarification", "Judgement",
     "The clarified requirement restates the problem, constraints and success condition without silently resolving open decisions.",
     "skills/upstream/skills/productivity/grilling/SKILL.md", "clarified requirement restate constraints success", {}),
    ("C02", "Clarification", "Clarification", "Judgement",
     "Every decision taken during clarification is listed explicitly, with the alternative that was rejected.",
     "skills/autopilot/autopilot-distill/dsh/SKILL.md", "decision alternative rejected explicit", {}),
    ("C03", "Clarification", "Clarification", "Judgement",
     "Every accepted assumption is listed explicitly and is falsifiable.",
     "skills/autopilot/autopilot-distill/dsh/SKILL.md", "assumption falsifiable accepted", {}),
    ("C04", "Clarification", "Clarification", "Judgement",
     "Every material unknown is recorded; none may remain unresolved when clarification completes.",
     "crates/distill-cli/src/main.rs", "material unknown unresolved clarification", {}),
    ("C05", "Clarification", "Clarification", "Gate",
     "Every changed glossary, domain document or ADR is recorded with its worktree-relative path and SHA-256.",
     "crates/distill-cli/src/main.rs", "domain document artifact path sha256 glossary adr",
     {"domainDocs": ["true"]}, "ClarificationShape"),
    ("C06", "Clarification", "Clarification", "Judgement",
     "Each round asks the whole current frontier; questions that depend on an open answer wait for the next round.",
     "skills/upstream/skills/productivity/grilling/SKILL.md", "frontier round question dependency", {}),
    ("C07", "Clarification", "Clarification", "Judgement",
     "Each question is numbered and carries a recommended answer worded so that yes accepts it.",
     "skills/upstream/skills/productivity/grilling/SKILL.md", "numbered question recommended answer", {}),
    ("C08", "Clarification", "Clarification", "Judgement",
     "Facts are looked up by the agent; the user is never asked for something the environment can answer.",
     "skills/upstream/skills/productivity/grilling/SKILL.md", "lookup facts user question environment", {}),
    ("C09", "Clarification", "Clarification", "Human",
     "Decisions belong to the user; do not act until they confirm a shared understanding.",
     "skills/upstream/skills/productivity/grilling/SKILL.md", "user decision shared understanding wait", {}),
    ("C10", "Clarification", "Clarification", "Judgement",
     "Glossary conflicts and fuzzy terms are challenged with a concrete scenario before they enter the spec.",
     "skills/upstream/skills/engineering/domain-modeling/SKILL.md", "glossary conflict fuzzy term scenario",
     {"domainDocs": ["true"]}),
    ("C11", "Clarification", "Clarification", "Judgement",
     "A new ADR is proposed only when the decision is hard to reverse, surprising without context, and a real trade-off.",
     "skills/upstream/skills/engineering/domain-modeling/SKILL.md", "adr hard to reverse trade-off",
     {"newAdr": ["true"]}),
    ("C12", "Clarification", "Clarification", "Judgement",
     "The clarification records what is explicitly out of scope for this requirement.",
     "skills/upstream/skills/engineering/to-spec/SKILL.md", "out of scope boundary excluded", {}),
    ("C13", "Clarification", "Clarification", "Runner",
     "Under DSH, the session identity is proven from the session trace, never guessed from recency or user-supplied ids.",
     "skills/autopilot/autopilot-distill/dsh/SKILL.md", "dsh session trace identity",
     {"runtime": ["Dsh"]}),

    # PRD: structural sections are gate-able; the quality of the prose is not.
    ("P01", "PRD", "Prd", "Gate",
     "Problem Statement describes the user-visible problem and contains no solution.",
     "skills/upstream/skills/engineering/to-spec/SKILL.md", "problem statement user visible no solution",
     {}, "PrdShape"),
    ("P02", "PRD", "Prd", "Gate",
     "Solution states the user-visible outcome and leaves no open design decision.",
     "skills/upstream/skills/engineering/to-spec/SKILL.md", "solution outcome no open decision",
     {}, "PrdShape"),
    ("P03", "PRD", "Prd", "Gate",
     "User Stories are numbered, in As a / I want / so that form, and cover each affected user.",
     "skills/upstream/skills/engineering/to-spec/SKILL.md", "user stories numbered as a i want so that",
     {}, "PrdShape"),
    ("P04", "PRD", "Prd", "Gate",
     "Implementation Decisions name modules, interfaces and contracts; they do not prescribe new source file paths or code.",
     "skills/upstream/skills/engineering/to-spec/SKILL.md", "implementation decisions module interface contract file path",
     {}, "PrdShape"),
    ("P05", "PRD", "Prd", "Gate",
     "Testing Decisions name the single highest seam and what makes a good test at that seam.",
     "skills/upstream/skills/engineering/to-spec/SKILL.md", "testing decisions seam good test",
     {}, "PrdShape"),
    ("P06", "PRD", "Prd", "Gate",
     "Out of Scope lists the excluded behaviour explicitly.",
     "skills/upstream/skills/engineering/to-spec/SKILL.md", "out of scope excluded",
     {}, "PrdShape"),
    ("P07", "PRD", "Prd", "Human",
     "The testing seam is confirmed with the user before the PRD is published.",
     "crates/distill-cli/workflows/distill.v1.json", "testing seam confirmed checkpoint", {}),
    ("P08", "PRD", "Prd", "Judgement",
     "The spec uses glossary vocabulary and does not contradict an accepted ADR without an explicit supersession.",
     "skills/upstream/skills/engineering/domain-modeling/SKILL.md", "glossary vocabulary adr contradiction supersession",
     {"domainDocs": ["true"]}),
    ("P09", "PRD", "Prd", "Judgement",
     "No interview happens at this stage: the spec synthesizes the clarification already captured.",
     "skills/upstream/skills/engineering/to-spec/SKILL.md", "no interview synthesize clarification", {}),
    ("P10", "PRD", "Prd", "Gate",
     "When the change spans runtimes, the spec states the variant plan and the fallback behaviour.",
     "docs/adr/0007-dual-runtime-skill-variants.md", "runtime variant fallback plan",
     {"crossRuntime": ["true"]}, "PrdShape"),
    ("P11", "PRD", "Prd", "Gate",
     "When a public contract changes, the spec states caller impact and the deprecation or migration path.",
     "skills/upstream/skills/engineering/to-spec/SKILL.md", "contract change caller impact migration deprecation",
     {"changeKind": ["ContractChange"]}, "PrdShape"),
    ("P12", "PRD", "Prd", "Gate",
     "When skill sources change, the spec names the repository validation and check commands as verification.",
     "AGENTS.md", "skill validation check frontmatter",
     {"surface": ["Skills"]}, "PrdShape"),
    ("P13", "PRD", "Prd", "Gate",
     "When CI or release changes, the spec states the trigger and the failure behaviour.",
     "AGENTS.md", "ci workflow trigger failure release",
     {"surface": ["Ci"]}, "PrdShape"),

    # Issues: verticality and sizing stay judgement calls; structure and edges
    # are mechanical enough to gate.
    ("T01", "Issues", "Issues", "Judgement",
     "Each issue is a vertical tracer bullet that crosses the layers it changes and is demoable on its own.",
     "skills/upstream/skills/engineering/to-tickets/SKILL.md", "vertical tracer bullet demoable layers", {}),
    ("T02", "Issues", "Issues", "Judgement",
     "Each issue fits one fresh context window and can be implemented without the rest of the set.",
     "skills/upstream/skills/engineering/to-tickets/SKILL.md", "context window size independent implement", {}),
    ("T03", "Issues", "Issues", "Gate",
     "Blocking edges are declared only for genuine prerequisites and form a DAG.",
     "skills/upstream/skills/engineering/to-tickets/SKILL.md", "blocking edges prerequisite dag cycle",
     {"multiSlice": ["true"]}, "IssueSetShape"),
    ("T04", "Issues", "Issues", "Gate",
     "Each issue names its parent PRD or spec.",
     "skills/upstream/skills/engineering/to-tickets/SKILL.md", "parent prd spec reference",
     {}, "IssueSetShape"),
    ("T05", "Issues", "Issues", "Judgement",
     "What to build describes user-observable behaviour, not a layer-by-layer task list.",
     "skills/upstream/skills/engineering/to-tickets/SKILL.md", "what to build observable behaviour layer", {}),
    ("T06", "Issues", "Issues", "Gate",
     "Acceptance criteria are concrete, independently verifiable, and each maps to a test or command.",
     "skills/upstream/skills/engineering/to-tickets/SKILL.md", "acceptance criteria verifiable test command",
     {}, "IssueSetShape"),
    ("T07", "Issues", "Issues", "Judgement",
     "A wide refactor is sliced as expand, migrate, contract rather than one big-bang change.",
     "skills/upstream/skills/engineering/to-tickets/SKILL.md", "wide refactor expand migrate contract",
     {"wideRefactor": ["true"]}),
    ("T08", "Issues", "Issues", "Human",
     "The user approves issue granularity and blocking edges before publication.",
     "crates/distill-cli/workflows/distill.v1.json", "slice breakdown approved checkpoint", {}),
    ("T09", "Issues", "Issues", "Gate",
     "Blockers are published before or together with the issues they block.",
     "skills/upstream/skills/engineering/to-tickets/SKILL.md", "blocker publish order frontier",
     {"multiSlice": ["true"]}, "IssueSetShape"),
    ("T10", "Issues", "Issues", "Judgement",
     "The issue set covers every PRD user story and decision, and invents no scope the PRD did not decide.",
     "skills/upstream/skills/engineering/to-tickets/SKILL.md", "coverage traceability scope prd story", {}),
    ("T11", "Issues", "Issues", "Judgement",
     "Open questions discovered while slicing go back to the PRD instead of being settled inside a ticket.",
     "skills/upstream/skills/engineering/to-tickets/SKILL.md", "open question back to prd", {}),
]

# ── context cards ──────────────────────────────────────────────────────────
# id, title, path, start, end, stage, conditions
CONTEXTS = [
    ("CTX_GRIL", "Grilling: frontier rounds and question format",
     "skills/upstream/skills/productivity/grilling/SKILL.md", 1, 30, ["Clarification"], {}),
    ("CTX_DOMSKILL", "Domain modeling: glossary and ADR discipline",
     "skills/upstream/skills/engineering/domain-modeling/SKILL.md", 1, 50, ["Clarification"],
     {"domainDocs": ["true"]}),
    ("CTX_GLOSSARY", "Repository glossary (canonical vocabulary)",
     "GLOSSARY.md", 1, 40, ["Cross"], {}),
    ("CTX_DOMAIN", "Domain docs configuration",
     "docs/agents/domain.md", 1, 35, ["Clarification", "Prd"], {"domainDocs": ["true"]}),
    ("CTX_TRACKER", "Issue tracker configuration",
     "docs/agents/issue-tracker.md", 1, 38, ["Cross"], {}),
    ("CTX_WORKFLOW", "Distill workflow declaration",
     "crates/distill-cli/workflows/distill.v1.json", 1, 33, ["Clarification"], {}),
    ("CTX_ADR10", "ADR 0010: snapshot the requirement input",
     "docs/adr/0010-distill-snapshots-requirement-inputs.md", 1, 14, ["Clarification"], {}),
    ("CTX_ADR26", "ADR 0026: separate user and executor authority",
     "docs/adr/0026-distill-separates-user-and-executor-authority.md", 1, 14,
     ["Clarification"], {}),
    ("CTX_ADR30", "ADR 0030: run to the next boundary",
     "docs/adr/0030-distill-skills-run-to-the-next-boundary.md", 1, 14, ["Clarification"], {}),
    ("CTX_TOSPEC", "to-spec: PRD template and testing seam",
     "skills/upstream/skills/engineering/to-spec/SKILL.md", 1, 75, ["Prd"], {}),
    ("CTX_ADR7", "ADR 0007: dual-runtime skill variants",
     "docs/adr/0007-dual-runtime-skill-variants.md", 1, 14, ["Prd"], {"crossRuntime": ["true"]}),
    ("CTX_ADR9", "ADR 0009: shared crate and logic sink",
     "docs/adr/0009-codebase-deepening-shared-crate-and-logic-sink.md", 1, 14, ["Prd"],
     {"surface": ["State"]}),
    ("CTX_ADR36", "ADR 0036: single discoverable runtime router",
     "docs/adr/0036-single-discoverable-runtime-router.md", 1, 14, ["Prd"],
     {"crossRuntime": ["true"]}),
    ("CTX_ADR13", "ADR 0013: adapt skills without modifying them",
     "docs/adr/0013-distill-adapts-skills-without-modifying-them.md", 1, 14,
     ["Prd", "Issues"], {"surface": ["Skills"]}),
    ("CTX_ADR40", "ADR 0040: typed run-state module",
     "docs/adr/0040-distill-run-state-typed-module.md", 1, 14, ["Prd", "Issues"],
     {"surface": ["State"]}),
    ("CTX_ADR41", "ADR 0041: state machine owns workflow control",
     "docs/adr/0041-distill-state-machine-owns-workflow-control.md", 1, 14,
     ["Prd", "Issues"], {"surface": ["State"]}),
    ("CTX_ADR44", "ADR 0044: single owner for the Expected set",
     "docs/adr/0044-single-owner-for-the-expected-set.md", 1, 14, ["Prd", "Issues"],
     {"surface": ["Skills"]}),
    ("CTX_ADR45", "ADR 0045: entries resolve their skill files",
     "docs/adr/0045-expected-set-entry-resolves-its-skill-files.md", 1, 14, ["Prd"],
     {"surface": ["Skills"]}),
    ("CTX_ADR52", "ADR 0052: upstream pin and drift check",
     "docs/adr/0052-upstream-pin-and-drift-check.md", 1, 14, ["Prd"], {"surface": ["Ci"]}),
    ("CTX_ADR8", "ADR 0008: self-contained tarball install",
     "docs/adr/0008-self-contained-tarball-install.md", 1, 14, ["Prd"],
     {"surface": ["Install"]}),
    ("CTX_UPSTREAM", "Upstream sync procedure",
     "docs/agents/upstream-sync.md", 1, 40, ["Prd"], {"surface": ["Ci"]}),
    ("CTX_DSHDOC", "DSH runtime contract",
     "docs/agents/runtimes/dsh.md", 1, 60, ["Clarification", "Prd"], {"runtime": ["Dsh"]}),
    ("CTX_PRD4", "PRD 0004: install exemplar",
     "docs/prd/0004-self-contained-tarball-install.md", 1, 30, ["Prd"],
     {"surface": ["Install"]}),
    ("CTX_TOTICK", "to-tickets: vertical slicing and edges",
     "skills/upstream/skills/engineering/to-tickets/SKILL.md", 1, 105, ["Issues"], {}),
    ("CTX_TRIAGE", "Triage labels",
     "docs/agents/triage-labels.md", 1, 15, ["Issues"], {}),
]

# Context cards that the task itself produced, rather than reusable input.
# They postdate the task (or ship with it) and would leak the accepted answer
# into a replay, so every selection arm drops them.  Decision records that the
# task consumed as input, and general documents from later dates, stay.
POSTHOC = {
    "T6": ["CTX_ADR52", "CTX_UPSTREAM"],
    "T7": ["CTX_DSHDOC"],
    "T8": ["CTX_ADR45"],
}

# First commit date of each card's file, from `git log --follow --format=%aI
# --reverse -- <path>`.  Used by the historical-context metric; selection
# additionally drops the task's own post-hoc decisions (POSTHOC above).
INTRODUCED = {
    "skills/upstream/skills/productivity/grilling/SKILL.md": "2026-07-27",
    "skills/upstream/skills/engineering/domain-modeling/SKILL.md": "2026-09-10",
    "GLOSSARY.md": "2026-10-02",
    "docs/agents/domain.md": "2026-06-23",
    "docs/agents/issue-tracker.md": "2026-06-23",
    "crates/distill-cli/workflows/distill.v1.json": "2026-07-27",
    "docs/adr/0010-distill-snapshots-requirement-inputs.md": "2026-07-27",
    "docs/adr/0026-distill-separates-user-and-executor-authority.md": "2026-07-27",
    "docs/adr/0030-distill-skills-run-to-the-next-boundary.md": "2026-07-27",
    "skills/upstream/skills/engineering/to-spec/SKILL.md": "2026-07-27",
    "docs/adr/0007-dual-runtime-skill-variants.md": "2026-06-30",
    "docs/adr/0009-codebase-deepening-shared-crate-and-logic-sink.md": "2026-07-27",
    "docs/adr/0036-single-discoverable-runtime-router.md": "2026-07-27",
    "docs/adr/0013-distill-adapts-skills-without-modifying-them.md": "2026-07-27",
    "docs/adr/0040-distill-run-state-typed-module.md": "2026-09-03",
    "docs/adr/0041-distill-state-machine-owns-workflow-control.md": "2026-09-03",
    "docs/adr/0044-single-owner-for-the-expected-set.md": "2026-09-11",
    "docs/adr/0045-expected-set-entry-resolves-its-skill-files.md": "2026-09-15",
    "docs/adr/0052-upstream-pin-and-drift-check.md": "2026-11-07",
    "docs/adr/0008-self-contained-tarball-install.md": "2026-07-24",
    "docs/agents/upstream-sync.md": "2026-11-07",
    "docs/agents/runtimes/dsh.md": "2026-10-02",
    "docs/prd/0004-self-contained-tarball-install.md": "2026-07-24",
    "skills/upstream/skills/engineering/to-tickets/SKILL.md": "2026-07-27",
    "docs/agents/triage-labels.md": "2026-06-23",
}

# ── tasks ──────────────────────────────────────────────────────────────────
# id, title, requirement, sourceAnchor, stage facts, asOf, target artifact
TASKS = [
    ("T1", "Dual-runtime skill compatibility",
     "Allow one skill asset set to install and run on both Reasonix and Codex, without the two runtime directories polluting each other, while keeping the vendored upstream snapshot trackable.",
     "corpus/specs/0002-dual-runtime-compatibility.md",
     {"changeKinds": ["NewFeature", "ContractChange"], "surfaces": ["Skills", "Install"],
      "runtimes": ["Reasonix", "Codex"], "multiModule": True, "crossRuntime": True,
      "multiSlice": True, "ambiguity": "Medium", "domainDocs": True, "adr": True,
      "newAdr": True, "externalTracker": True, "wideRefactor": False,
      "userFacing": True, "hasExistingSeam": False}, "2026-06-30",
     "docs/prd/0002-dual-runtime-compatibility.md"),
    ("T2", "Kimi Code runtime adaptation",
     "Add Kimi Code as a supported runtime, fix the frontmatter parsing that made skills disappear there, and leave Reasonix and Codex behaviour unchanged.",
     "corpus/specs/0003-kimi-code-adaptation.md",
     {"changeKinds": ["NewFeature", "Bugfix"], "surfaces": ["Skills", "Docs"],
      "runtimes": ["Kimi", "Reasonix", "Codex"], "multiModule": True, "crossRuntime": True,
      "multiSlice": True, "ambiguity": "Low", "domainDocs": True, "adr": False,
      "newAdr": False, "externalTracker": True, "wideRefactor": False,
      "userFacing": True, "hasExistingSeam": False}, "2026-07-24",
     "docs/prd/0003-kimi-code-adaptation.md"),
    ("T3", "Self-contained tarball install",
     "Provide a self-contained tarball install: one curl pipe command, skills deployed as real files so the directory is portable across machines, and upgrades that never touch user-owned skills.",
     "corpus/specs/0004-self-contained-tarball-install.md",
     {"changeKinds": ["NewFeature", "Migration"], "surfaces": ["Install", "Skills", "Docs"],
      "runtimes": ["Reasonix", "Codex", "Kimi"], "multiModule": True, "crossRuntime": True,
      "multiSlice": True, "ambiguity": "Medium", "domainDocs": False, "adr": True,
      "newAdr": False, "externalTracker": True, "wideRefactor": False,
      "userFacing": True, "hasExistingSeam": True}, "2026-07-24",
     "docs/prd/0004-self-contained-tarball-install.md"),
    ("T4", "Director write-path convergence",
     "Make every director-cli state mutation a pure record method behind one commit seam, matching the distill-cli layering, with zero behaviour change.",
     "corpus/specs/19-director-state-mutation-persistence-split.md",
     {"changeKinds": ["Refactor"], "surfaces": ["Cli", "State"],
      "runtimes": [], "multiModule": False, "crossRuntime": False,
      "multiSlice": False, "ambiguity": "Low", "domainDocs": True, "adr": True,
      "newAdr": False, "externalTracker": True, "wideRefactor": False,
      "userFacing": False, "hasExistingSeam": True}, "2026-10-02",
     "docs/issues/19-director-state-mutation-persistence-split.md"),
    ("T5", "Shared run-state substrate",
     "Extract the duplicated state-directory hygiene and atomic-write substrate from director and distill into one shared crate, removing the drifted atomic-write behaviour while keeping both CLIs working.",
     "corpus/specs/20-run-state-persistence-substrate.md",
     {"changeKinds": ["Refactor", "ContractChange"], "surfaces": ["Cli", "State"],
      "runtimes": [], "multiModule": True, "crossRuntime": False,
      "multiSlice": False, "ambiguity": "Low", "domainDocs": True, "adr": True,
      "newAdr": True, "externalTracker": True, "wideRefactor": False,
      "userFacing": False, "hasExistingSeam": True}, "2026-10-02",
     "docs/issues/20-run-state-persistence-substrate.md"),
    ("T6", "Scheduled upstream drift check",
     "Compare the vendored upstream pin with the newest release tag on a weekly schedule, open or update exactly one issue when drift exists, and never sync or merge automatically.",
     "https://github.com/MatthewYe/autopilot-toolkit/issues/151",
     {"changeKinds": ["NewFeature"], "surfaces": ["Ci"],
      "runtimes": [], "multiModule": False, "crossRuntime": False,
      "multiSlice": False, "ambiguity": "Low", "domainDocs": True, "adr": True,
      "newAdr": True, "externalTracker": True, "wideRefactor": False,
      "userFacing": False, "hasExistingSeam": True}, "2026-11-06", ""),
    ("T7", "DSH as a first-class skill runtime",
     "Add DeepSeek Harness as a first-class runtime: DSH variant sources, router selection, variant validation and runtime documentation, without changing the other runtimes.",
     "https://github.com/MatthewYe/autopilot-toolkit/issues/144",
     {"changeKinds": ["NewFeature"], "surfaces": ["Skills", "Docs", "Install"],
      "runtimes": ["Dsh", "Reasonix", "Codex", "Kimi"], "multiModule": True, "crossRuntime": True,
      "multiSlice": True, "ambiguity": "Medium", "domainDocs": True, "adr": True,
      "newAdr": False, "externalTracker": True, "wideRefactor": False,
      "userFacing": True, "hasExistingSeam": False}, "2026-10-01", ""),
    ("T8", "Expected-set entry resolves its skill files",
     "Make an Expected-set entry resolve every skill file it owns, root and per-runtime variants, so failure is reported per file; ship it as an expand, migrate, contract sequence.",
     "https://github.com/MatthewYe/autopilot-toolkit/issues/121",
     {"changeKinds": ["Refactor", "ContractChange", "WideRefactor"], "surfaces": ["Cli", "Skills"],
      "runtimes": [], "multiModule": True, "crossRuntime": False,
      "multiSlice": True, "ambiguity": "Low", "domainDocs": True, "adr": True,
      "newAdr": True, "externalTracker": True, "wideRefactor": True,
      "userFacing": False, "hasExistingSeam": True}, "2026-09-11", ""),
]

BOOLEAN_FACTS = ["multiModule", "crossRuntime", "multiSlice", "domainDocs", "adr",
                 "newAdr", "externalTracker", "wideRefactor", "userFacing",
                 "hasExistingSeam"]


def esc(text: str) -> str:
    return (text.replace("\\", "\\\\").replace('"', '\\"')
            .replace("\n", "\\n").replace("\r", "").replace("\t", " "))


def lit(text: str) -> str:
    return f'"{esc(text)}"'


def boollit(value: bool) -> str:
    return f'"{str(value).lower()}"^^xsd:boolean'


def excerpt(path: str, start: int, end: int, limit: int = 360) -> str:
    lines = (REPO / path).read_text(encoding="utf-8").splitlines()
    text = " ".join(" ".join(lines[start - 1:end]).split())
    if len(text) > limit:
        text = text[:limit].rstrip() + " …"
    return text


def conditions_ttl(conditions: dict) -> str:
    parts = []
    for key, values in conditions.items():
        prop = "when" + key[0].upper() + key[1:]
        for value in values:
            if isinstance(value, bool) or value in {"true", "false"}:
                parts.append(f"    d:{prop} {boollit(value)} ;")
            else:
                parts.append(f"    d:{prop} d:{value} ;")
    return "\n".join(parts)


def build_ontology() -> str:
    lines = [
        "@prefix d: <https://example.org/autopilot/distill#> .",
        "@prefix owl: <http://www.w3.org/2002/07/owl#> .",
        "@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .",
        "@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .",
        "",
        "# Classes",
        "d:DistillClause a owl:Class .",
        "d:ClauseAxis a owl:Class .",
        "d:Stage a owl:Class .",
        "d:CheckKind a owl:Class .",
        "d:ContextCard a owl:Class .",
        "d:Task a owl:Class .",
        "",
        "# Axes",
        *[f"d:Axis{axis} a d:ClauseAxis ; rdfs:label {lit(axis)} ." for axis in
          ["Intake", "Clarification", "PRD", "Issues"]],
        "",
        "# Stages",
        *[f"d:{stage} a d:Stage ; rdfs:label {lit(stage)} ." for stage in
          ["Intake", "Clarification", "Prd", "Issues", "Cross"]],
        "",
        "# Check kinds",
        *[f"d:{kind} a d:CheckKind ; rdfs:label {lit(kind)} ." for kind in
          ["Runner", "Gate", "Human", "Judgement"]],
        "",
        "# Properties",
    ]
    for prop in ["clauseId", "axis", "stage", "text", "defaultCheck", "sourceAnchor",
                 "keyword", "enforcedBy", "checkedBy", "contextId", "title", "path",
                 "startLine", "endLine", "excerpt", "taskId", "requirement",
                 "sourceAnchor", "changeKind", "surface", "runtime", "ambiguity",
                 "whenStage", "whenChangeKind", "whenSurface", "whenRuntime",
                 "whenMultiModule", "whenCrossRuntime", "whenMultiSlice",
                 "whenDomainDocs", "whenAdr", "whenNewAdr", "whenExternalTracker",
                 "whenWideRefactor", "whenUserFacing", "whenHasExistingSeam",
                 "whenAmbiguity", "introducedAt", "targetArtifact", "excludesContext",
                 "dependenciesDeclared"]:
        lines.append(f"d:{prop} a owl:ObjectProperty .")
    lines.append("d:asOf a owl:DatatypeProperty .")
    lines.append("")
    lines.append("# Clauses")

    for clause in CLAUSES:
        cid, axis, stage, check, text, anchor, keywords, conditions = clause[:8]
        checked_by = clause[8] if len(clause) > 8 else None
        enforced = clause[9] if len(clause) > 9 else None
        lines.append(f"d:{cid} a d:DistillClause ;")
        lines.append(f"    d:clauseId {lit(cid)} ;")
        lines.append(f"    d:axis d:Axis{axis} ;")
        lines.append(f"    d:stage d:{stage} ;")
        lines.append(f"    d:text {lit(text)} ;")
        lines.append(f"    d:defaultCheck d:{check} ;")
        lines.append(f"    d:sourceAnchor {lit(anchor)} ;")
        lines.append(f"    d:keyword {lit(keywords)} ;")
        if checked_by:
            lines.append(f"    d:checkedBy d:{checked_by} ;")
        if enforced:
            lines.append(f"    d:enforcedBy d:{enforced} ;")
        cond = conditions_ttl(conditions)
        if cond:
            lines.append(cond)
        lines[-1] = lines[-1].rstrip(" ;") + " ."
        lines.append("")
    return "\n".join(lines)


def build_contexts() -> str:
    lines = [
        "@prefix d: <https://example.org/autopilot/distill#> .",
        "@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .",
        "",
        "# Context cards: path + line range + a short excerpt.  The excerpt makes",
        "# the card self-describing; the pointer is what the adapter renders.",
    ]
    for cid, title, path, start, end, stage, conditions in CONTEXTS:
        lines.append(f"d:{cid} a d:ContextCard ;")
        lines.append(f"    d:contextId {lit(cid)} ;")
        lines.append(f"    d:title {lit(title)} ;")
        lines.append(f"    d:path {lit(path)} ;")
        lines.append(f"    d:startLine {start} ;")
        lines.append(f"    d:endLine {end} ;")
        lines.append(f"    d:excerpt {lit(excerpt(path, start, end))} ;")
        lines.append(f"    d:introducedAt {lit(INTRODUCED[path])} ;")
        for stage_name in stage:
            lines.append(f"    d:stage d:{stage_name} ;")
        cond = conditions_ttl(conditions)
        if cond:
            lines.append(cond)
        lines[-1] = lines[-1].rstrip(" ;") + " ."
        lines.append("")
    return "\n".join(lines)


def build_tasks() -> str:
    lines = [
        "@prefix d: <https://example.org/autopilot/distill#> .",
        "@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .",
        "",
        "# Requirement tasks in the experiment.  Facts are taken from the",
        "# requirement text and the resulting merged artifacts, not from the",
        "# ontology's own conditions.",
    ]
    for task_id, title, requirement, anchor, facts, as_of, target in TASKS:
        lines.append(f"d:{task_id} a d:Task ;")
        lines.append(f"    d:taskId {lit(task_id)} ;")
        lines.append(f"    d:title {lit(title)} ;")
        lines.append(f"    d:requirement {lit(requirement)} ;")
        lines.append(f"    d:sourceAnchor {lit(anchor)} ;")
        lines.append(f"    d:asOf {lit(as_of)} ;")
        if target:
            lines.append(f"    d:targetArtifact {lit(target)} ;")
        for stage_name in ("Clarification", "Prd", "Issues"):
            lines.append(f"    d:stage d:{stage_name} ;")
        for key in ("changeKinds", "surfaces", "runtimes"):
            prop = {"changeKinds": "changeKind", "surfaces": "surface",
                    "runtimes": "runtime"}[key]
            # Explicit sentinel so an empty set stays "known empty" after
            # loading; an absent triple would mean "not collected", and the
            # conservative matcher would then inject every conditional clause.
            for value in facts[key] or ["None"]:
                lines.append(f"    d:{prop} d:{value} ;")
        lines.append(f"    d:ambiguity d:{facts['ambiguity']} ;")
        for key in BOOLEAN_FACTS:
            lines.append(f"    d:{key} {boollit(facts[key])} ;")
        for context_id in POSTHOC.get(task_id, []):
            lines.append(f"    d:excludesContext d:{context_id} ;")
        lines[-1] = lines[-1].rstrip(" ;") + " ."
        lines.append("")
    return "\n".join(lines)


def main() -> None:
    (ROOT / "ontology" / "distill.ttl").write_text(build_ontology(), encoding="utf-8")
    (ROOT / "data" / "contexts.ttl").write_text(build_contexts(), encoding="utf-8")
    (ROOT / "data" / "tasks.ttl").write_text(build_tasks(), encoding="utf-8")
    print("wrote ontology/distill.ttl, data/contexts.ttl, data/tasks.ttl")


if __name__ == "__main__":
    main()
