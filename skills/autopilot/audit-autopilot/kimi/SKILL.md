---
name: audit-autopilot
description: Post-hoc audit of autopilot execution fidelity. Analyzes agent session traces to evaluate how faithfully the autopilot workflow executed against its contract, surfacing errors, friction, and drift with traceable evidence anchors. Use when the user wants to audit an autopilot run, analyze session quality, check if autopilot did what it was supposed to, or provides a session ID from an autopilot execution.
---

# Audit Autopilot (Kimi)

Audit an autopilot execution by analyzing its session traces. Kimi 没有 `read_session` 工具——会话 trace 直接以 JSONL 文件落盘，用 `Read`/`Grep`/`Bash` 读取即可。The audit evaluates three layers of fidelity, producing a structured scorecard with evidence anchors back to the raw session data.

## 会话存储布局（Kimi）

- 全局索引：`~/.kimi-code/session_index.jsonl` — 每行 `{"sessionId": "...", "sessionDir": "...", "workDir": "..."}`
- 单会话目录：`<sessionDir>/`，内含 `state.json`（标题、workDir、updatedAt）和 `agents/`
- orchestrator trace：`<sessionDir>/agents/main/wire.jsonl`
- 子代理 trace：`<sessionDir>/agents/agent-N/wire.jsonl`（N 从 0 开始，按 dispatch 顺序递增；每个 `Agent` 工具调用产生一个 `agent-N` 目录）
- `wire.jsonl` 是完整会话记录：metadata、system prompt、用户消息、模型回复、工具调用与结果，每行一个 JSON 事件

## When to use

Run after an autopilot session completes. User provides the orchestrator session ID or lets you discover it from `~/.kimi-code/session_index.jsonl`. Do not use for non-autopilot sessions.

## Workflow

### Step 0: Gather inputs

The session ID may come from the command argument (`/audit-autopilot <session-id>`) or be stated directly in the user's prompt. If already provided, skip asking and proceed.

If not provided, ask the user for:
- **Orchestrator session ID** (required) — the session where autopilot ran. 可接受 `session_<uuid>` 或完整 sessionDir 路径
- **Project directory** (optional, defaults to cwd) — where `.scratch/` issues and contracts live

If the user doesn't know the session ID, help them find it: read `~/.kimi-code/session_index.jsonl`, filter by `workDir` (defaults to the project directory), then by recency (`state.json` 的 `updatedAt`) 或标题（`state.json` 的 `title`，找 autopilot / issue 相关的）。列出候选让用户确认。

If the user has already specified subagent session IDs or contract file paths, use them directly rather than re-discovering them.

### Step 1: Locate and parse session traces

定位 orchestrator 会话目录：在 `session_index.jsonl` 中按 `sessionId` 查 `sessionDir`（用户直接给了路径则跳过）。

Read `<sessionDir>/agents/main/wire.jsonl`（大文件用 `Grep` 按模式检索 + `Read` 分段读，不要一次性全文载入）。Extract key metadata:

- **Issue sources**: Find paths like `.scratch/<feature>/issues/<NN-slug>/` or GitHub issue numbers in the user's initial messages
- **Subagent traces**: 扫描 `Agent` 工具调用事件——每次 dispatch（implementer / reviewer）按顺序对应 `agents/agent-0`、`agents/agent-1`……。Track which subagent mapped to which agent type (implementer / reviewer) and round number。不确定对应关系时，读候选 `agent-N/wire.jsonl` 开头的 dispatch prompt 确认（其中含"先 Read ~/.agents/skills/autopilot-<role>/SKILL.md"字样）
- **Contract files**: From the orchestrator's dispatch prompts, locate `AGENT-BRIEF.md` and `issue.md` paths

For GitHub issues, the contract is embedded in the orchestrator's prompt text — extract it directly.

**If the user already specified subagent traces**, skip the discovery step and read the provided `agent-N/wire.jsonl` files directly.

### Step 2: Load contracts

**If contract paths were provided by the user**, read them directly.

Otherwise, read the contract documents for every issue involved in the autopilot run:
- `<issue_dir>/AGENT-BRIEF.md` — Acceptance Criteria, Out of scope
- `<issue_dir>/issue.md` — Original problem description, intent

For GitHub issues, extract the AC and scope from the orchestrator's dispatch prompt.

### Step 3: Phase 1 — Lightweight analysis + mandatory spot-checks

Answer the 9 analysis questions (see [references/questions.md](references/questions.md)) using primarily the orchestrator session trace and contract documents. Each question gets one of three scores: **PASS**, **WARN**, or **FAIL**.

For every question, first check the orchestrator-level evidence (reports, verdicts, orchestrator actions). Then **always perform spot-checks** on subagent sessions — even when the orchestrator-level analysis suggests no issue. Spot-check strategy:

- **Layer 1 (Fidelity)**: For each issue, sample 1-2 rounds of implementer traces. `Grep` for test execution tool calls (bash/pytest/vitest/cargo test/etc.) matching the AC descriptions. If none found, this is a signal even if reports claim DONE.
- **Layer 2 (Errors)**: Cross-reference reviewer VERDICT changes across rounds. If reviewer gave RETRY with 3 Criticals in round 0 and MERGE in round 1, spot-check round 1's implementer trace for evidence those Criticals were actually fixed.
- **Layer 3 (Friction & Drift)**: Compare round 0 vs round N implementer traces for scope expansion — are later rounds touching files not in the original AC?

Spot-checks are lightweight: `Grep` for specific patterns (test runs, file edits, tool call sequences) rather than reading the full trace. One spot-check per layer per issue is sufficient.

| Score | Meaning |
|-------|---------|
| PASS | No issue found; evidence supports correct behavior |
| WARN | Suspicious but inconclusive; requires Phase 2 deep-dive |
| FAIL | Clear defect confirmed; evidence anchor provided |

Every WARN and FAIL must include an **evidence anchor**: the trace file (`main` 或 `agent-N`)、行号或事件序号、and a brief excerpt from the trace.

See [references/questions.md](references/questions.md) for the full question list, scoring rubric per question, and evidence requirements.

### Step 4: Phase 2 — Deep-dive

If **any** question scored WARN or FAIL in Phase 1, Phase 2 is mandatory. Otherwise skip to Step 5 (all green — clean audit).

For each flagged question, load the relevant subagent trace(s) in full and perform targeted analysis:

- **WARN → confirm or clear**: Search the full subagent trace for confirming or refuting evidence. Update the score to PASS or FAIL with the new evidence.
- **FAIL → root cause**: Trace the failure backward through the session to find the originating moment (e.g., a skipped test, a misread AC, a premature report). Document the chain of causation.

Phase 2 reads subagent traces selectively — only the traces relevant to the flagged questions, not all traces indiscriminately.

### Step 5: Produce scorecard

Output the audit report using the template from [references/report-template.md](references/report-template.md). The report must include:

1. **Executive summary**: Overall fidelity percentage (PASS count ÷ 9), issue count, round count, verdict summary
2. **Scorecard**: 3×3 table with scores and one-line rationale per question
3. **Findings**: Detailed breakdown of every FAIL and WARN, with evidence anchors, severity, and root cause analysis (from Phase 2)
4. **Recommendations**: Concrete, actionable suggestions for improving either the autopilot configuration (agent prompts, command logic) or the contracts (AGENT-BRIEF clarity, AC specificity)

## Principles

- **Evidence over opinion**: Never claim a defect without citing a specific trace file, line/event, and excerpt
- **Spot-check always**: A clean orchestrator-level report does not guarantee clean subagent behavior
- **Deep-dive selectively**: Don't read every subagent trace in full — follow the signals from Phase 1
- **Report for humans**: The audit is for a developer to read and act on, not for automated pipelines
