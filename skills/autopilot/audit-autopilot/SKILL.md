---
name: audit-autopilot
description: "Post-hoc audit of autopilot execution fidelity. Analyzes agent session traces to evaluate how faithfully the autopilot workflow executed against its contract, surfacing errors, friction, and drift with traceable evidence anchors. Use when the user wants to audit an autopilot run, analyze session quality, check if autopilot did what it was supposed to, or provides a session ID from an autopilot execution."
---

# Audit Autopilot

Audit an autopilot execution by analyzing its session traces. Session traces are stored by your agent runtime — consult your runtime's documentation for the specific storage layout. Typically traces are JSONL files (one JSON event per line) recording system prompts, user messages, model responses, and tool calls with results. The audit evaluates three layers of fidelity, producing a structured scorecard with evidence anchors back to the raw session data.

## Session storage layout

Your runtime stores session data in a runtime-specific directory. You need to locate:
- **Global index**: a registry mapping session IDs to their storage directories (e.g. a `session_index.jsonl` file, a database, or runtime API).
- **Per-session directory**: contains the orchestrator trace and sub-agent traces.
  - Orchestrator trace: the main session recording (typically `main/wire.jsonl` or similar).
  - Sub-agent traces: each sub-agent dispatch produces a numbered directory (e.g. `agent-0/`, `agent-1/`) containing its own `wire.jsonl`.

Use your runtime's file-reading and search tools to inspect these trace files.

## When to use

Run after an autopilot session completes. The user provides the orchestrator session ID or lets you discover it from the runtime's session registry. Do not use for non-autopilot sessions.

## Workflow

### Step 0: Gather inputs

The session ID may come from a command argument or be stated directly in the user's prompt. If already provided, skip asking and proceed.

If not provided, ask the user for:
- **Orchestrator session ID** (required) — the session where autopilot ran. Accept a session identifier or full session directory path.
- **Project directory** (optional, defaults to cwd) — where `.scratch/` issues and contracts live.

If the user does not know the session ID, help them find it: read the runtime's session index, filter by the project directory's workDir, then by recency or title (look for autopilot / issue-related sessions). List candidates for the user to confirm.

If the user has already specified subagent session IDs or contract file paths, use them directly rather than re-discovering them.

### Step 1: Locate and parse session traces

Locate the orchestrator session directory from the runtime's session index (or use the user-provided path directly).

Read the orchestrator trace file. For large files, use search to filter by pattern and read sections selectively rather than loading the entire file at once. Extract key metadata:

- **Issue sources**: find paths like `.scratch/<feature>/issues/<NN-slug>/` or GitHub issue numbers in the user's initial messages.
- **Subagent traces**: scan for sub-agent dispatch events — each dispatch (implementer / reviewer) maps in order to agent-0, agent-1, etc. Track which subagent maps to which agent type (implementer / reviewer) and round number. When uncertain, read the opening of candidate `agent-N/wire.jsonl` files to confirm (check for dispatch prompt mentioning the role name).
- **Contract files**: from the orchestrator's dispatch prompts, locate `AGENT-BRIEF.md` and `issue.md` paths.

For GitHub issues, the contract is embedded in the orchestrator's prompt text — extract it directly.

**If the user already specified subagent traces**, skip the discovery step and read the provided trace files directly.

### Step 2: Load contracts

**If contract paths were provided by the user**, read them directly.

Otherwise, read the contract documents for every issue involved in the autopilot run:
- `<issue_dir>/AGENT-BRIEF.md` — Acceptance Criteria, Out of scope.
- `<issue_dir>/issue.md` — Original problem description, intent.

For GitHub issues, extract the AC and scope from the orchestrator's dispatch prompt.

### Step 3: Phase 1 — Lightweight analysis + mandatory spot-checks

Answer the 9 analysis questions (see [references/questions.md](references/questions.md)) using primarily the orchestrator session trace and contract documents. Each question gets one of three scores: **PASS**, **WARN**, or **FAIL**.

For every question, first check the orchestrator-level evidence (reports, verdicts, orchestrator actions). Then **always perform spot-checks** on subagent sessions — even when the orchestrator-level analysis suggests no issue. Spot-check strategy:

- **Layer 1 (Fidelity)**: For each issue, sample 1-2 rounds of implementer traces. Search for test execution tool calls (pytest, vitest, cargo test, etc.) matching the AC descriptions. If none found, this is a signal even if reports claim DONE.
- **Layer 2 (Errors)**: Cross-reference reviewer VERDICT changes across rounds. If reviewer gave RETRY with 3 Criticals in round 0 and MERGE in round 1, spot-check round 1's implementer trace for evidence those Criticals were actually fixed.
- **Layer 3 (Friction and Drift)**: Compare round 0 vs round N implementer traces for scope expansion — are later rounds touching files not in the original AC?

Spot-checks are lightweight: search for specific patterns (test runs, file edits, tool call sequences) rather than reading the full trace. One spot-check per layer per issue is sufficient.

| score | meaning |
|---|---|
| PASS | No issue found; evidence supports correct behavior. |
| WARN | Suspicious but inconclusive; requires Phase 2 deep-dive. |
| FAIL | Clear defect confirmed; evidence anchor provided. |

Every WARN and FAIL must include an **evidence anchor**: the trace file (main or agent-N), line number or event index, and a brief excerpt from the trace.

See [references/questions.md](references/questions.md) for the full question list, scoring rubric per question, and evidence requirements.

### Step 4: Phase 2 — Deep-dive

If **any** question scored WARN or FAIL in Phase 1, Phase 2 is mandatory. Otherwise skip to Step 5 (all green — clean audit).

For each flagged question, load the relevant subagent trace(s) in full and perform targeted analysis:

- **WARN → confirm or clear**: search the full subagent trace for confirming or refuting evidence. Update the score to PASS or FAIL with the new evidence.
- **FAIL → root cause**: trace the failure backward through the session to find the originating moment (e.g. a skipped test, a misread AC, a premature report). Document the chain of causation.

Phase 2 reads subagent traces selectively — only the traces relevant to the flagged questions, not all traces indiscriminately.

### Step 5: Produce scorecard

Output the audit report using the template from [references/report-template.md](references/report-template.md). The report must include:

1. **Executive summary**: Overall fidelity percentage (PASS count divided by 9), issue count, round count, verdict summary.
2. **Scorecard**: 3x3 table with scores and one-line rationale per question.
3. **Findings**: Detailed breakdown of every FAIL and WARN, with evidence anchors, severity, and root cause analysis (from Phase 2).
4. **Recommendations**: Concrete, actionable suggestions for improving either the autopilot configuration (agent prompts, command logic) or the contracts (AGENT-BRIEF clarity, AC specificity).

## Principles

- **Evidence over opinion**: never claim a defect without citing a specific trace file, line/event, and excerpt.
- **Spot-check always**: a clean orchestrator-level report does not guarantee clean subagent behavior.
- **Deep-dive selectively**: do not read every subagent trace in full — follow the signals from Phase 1.
- **Report for humans**: the audit is for a developer to read and act on, not for automated pipelines.
