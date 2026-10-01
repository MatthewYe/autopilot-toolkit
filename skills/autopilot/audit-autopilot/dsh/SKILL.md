---
name: audit-autopilot
description: "Post-hoc audit of autopilot execution fidelity. Analyzes agent session traces to evaluate how faithfully the autopilot workflow executed against its contract, surfacing errors, friction, and drift with traceable evidence anchors. Use when the user wants to audit an autopilot run, analyze session quality, check if autopilot did what it was supposed to, or provides a session ID from an autopilot execution."
---

# Audit Autopilot (DSH)

Audit an autopilot execution by analyzing its session traces. DSH 没有会话导出/列表工具——会话 trace 直接落盘为 zstd 压缩的 JSONL，用 `bash`（`zstd -dc`）+ `glob`/`grep`/`read` 读取即可。The audit evaluates three layers of fidelity, producing a structured scorecard with evidence anchors back to the raw session data.

本 skill 可由主 agent 用 `skill` 工具按名加载，也可由用户在聊天输入 `/audit-autopilot <session-id>` 触发。

## 会话存储布局（DSH）

- 工作区根：`~/.dsh/sessions/<workspace-key>/`，其中 `workspace-key` 由当前工作区的**绝对路径**导出——去掉开头的 `/`，把其余 `/` 全部替换为 `-`，再前后各加 `--`。例：`/Users/you/proj` → `--Users-you-proj--`。
- 每个会话一个子目录：`<workspace-key>/<session-id>/`，`<session-id>` 形如 `session-<uuid>`（主会话）或 `<uuid>`（子代理会话）。
- 每个会话目录内的 trace 文件：`session.v4.jsonl.zstd` —— JSONL 事件流（每行一个 JSON 事件），磁盘上是 **zstd 压缩**的。`read`/`grep` 工具不能直接读压缩文件：先用 `bash` 运行 `zstd -dc <file>`（需要分段分析时可解压到临时文件，再用 `read`/`grep`）。
- 事件结构：每行带 `type`、`seq`、`time`；`session` 事件还带顶层 `id`、`cwd`、`parentSession`、`origin`、`delegationDepth`，其余事件的数据在 `data` 内。常见 `type`：`session`、`user/message`、`assistant/message`、`system/message`、`tool/call`、`tool/result`、`step/start`、`step/end`、`subagent/catalog`、`session/title`。
- **子代理 trace**：DSH 把每次 `subagent` 派发记录为一个独立的会话目录（与父会话同在 `<workspace-key>/` 下）。父会话的 `subagent/catalog` 事件带 `data.childId`（= 子会话目录名）与 `data.childCreatedAt`；子会话自己的 `session` 事件带 `parentSession`（= 父会话 id）、`origin: "subagent"`、`delegationDepth: 1`。
- 字段可能随 DSH 版本变化：**先采样几条事件确认结构，再据实际字段分析**，不要臆造字段名。

## When to use

Run after an autopilot session completes. User provides the orchestrator session ID or lets you discover it from `~/.dsh/sessions/<workspace-key>/`. Do not use for non-autopilot sessions.

## Workflow

### Step 0: Gather inputs

The session ID may come from the command argument（`/audit-autopilot <session-id>`）or be stated directly in the user's prompt. If already provided, skip asking and proceed.

If not provided, 用 `ask_user_question` 问用户要：

- **Orchestrator session ID**（必需）— autopilot 跑在哪个会话。可接受 `session-<uuid>`、`<uuid>` 或会话目录路径
- **Project directory**（可选，默认 cwd）— `.scratch/` issue 与 contract 所在处

If the user doesn't know the session ID, help them find it: 按当前项目的绝对路径算出 `workspace-key`，`glob` `~/.dsh/sessions/<workspace-key>/*/session*.jsonl*`，对候选会话用 `bash` + `zstd -dc` 读前几行和 `session/title` 事件，按 `session` 事件的 `cwd`（应等于项目目录）、`delegationDepth`（主会话为 0）、标题（找 autopilot / issue 相关的）筛选。列出候选让用户确认（用 `ask_user_question`）。

If the user has already specified subagent session IDs or contract file paths, use them directly rather than re-discovering them.

### Step 1: Locate and parse session traces

定位 orchestrator 会话目录：`glob` `~/.dsh/sessions/<workspace-key>/<session-id>/session*.jsonl*`（用户直接给了路径则跳过）。

用 `bash` + `zstd -dc` 读 orchestrator trace（大文件用 `zstd -dc <file> | grep -n <pattern>` 先定位，再解压到临时文件用 `read` 分段读，不要一次性全文载入）。Extract key metadata:

- **Issue sources**: Find paths like `.scratch/<feature>/issues/<NN-slug>/` or GitHub issue numbers in `user/message` 事件（`data.content` 的文本块）中
- **Subagent traces**: 扫描父会话的 `subagent/catalog` 事件——每个事件给出 `data.childId`（子会话目录名），按 `data.childCreatedAt` 或 `seq` 排序即派发顺序。每个子会话目录的 `session` 事件带 `parentSession` 与 `origin: "subagent"`，确认归属。 Track which subagent mapped to which agent type (implementer / reviewer) and round number. 不确定时，读候选子会话开头（`user/message` 里的 dispatch prompt）确认——DSH 派发的 prompt 第一段会指示子代理 `用 read 工具读取 ~/.agents/skills/autopilot-<role>/SKILL.md`
- **Contract files**: From the orchestrator's dispatch prompts（`tool/call` 事件的 `data.arguments`，或 `assistant/message` 文本），locate `AGENT-BRIEF.md` and `issue.md` paths

For GitHub issues, the contract is embedded in the orchestrator's prompt text — extract it directly.

**If the user already specified subagent traces**, skip the discovery step and read the provided session directories directly.

### Step 2: Load contracts

**If contract paths were provided by the user**, read them directly.

Otherwise, read the contract documents for every issue involved in the autopilot run:
- `<issue_dir>/AGENT-BRIEF.md` — Acceptance Criteria, Out of scope
- `<issue_dir>/issue.md` — Original problem description, intent

For GitHub issues, extract the AC and scope from the orchestrator's dispatch prompt.

### Step 3: Phase 1 — Lightweight analysis + mandatory spot-checks

Answer the 9 analysis questions (see [references/questions.md](references/questions.md)) using primarily the orchestrator session trace and contract documents. Each question gets one of three scores: **PASS**, **WARN**, or **FAIL**.

For every question, first check the orchestrator-level evidence (reports, verdicts, orchestrator actions). Then **always perform spot-checks** on subagent sessions — even when the orchestrator-level analysis suggests no issue. Spot-check strategy:

- **Layer 1 (Fidelity)**: For each issue, sample 1-2 rounds of implementer traces. 在这些 trace 中定位测试执行调用（`zstd -dc <file> | grep -n`）——DSH 里是 `tool/call` 事件且 `data.name` 为 `bash`、`data.arguments` 含 `cargo test`/`pytest`/`vitest` 等——比对 AC 描述。If none found, this is a signal even if reports claim DONE.
- **Layer 2 (Errors)**: Cross-reference reviewer VERDICT changes across rounds. If reviewer gave RETRY with 3 Criticals in round 0 and MERGE in round 1, spot-check round 1's implementer trace for evidence those Criticals were actually fixed.
- **Layer 3 (Friction & Drift)**: Compare round 0 vs round N implementer traces for scope expansion — are later rounds touching files not in the original AC?（`tool/call` 里的 `edit`/`write` 参数给出文件路径）

Spot-checks are lightweight: 先用 `bash` + `zstd -dc ... | grep -n` 定位特定模式（test runs、file edits、tool call sequences），rather than reading the full trace. One spot-check per layer per issue is sufficient.

| Score | Meaning |
|-------|---------|
| PASS | No issue found; evidence supports correct behavior |
| WARN | Suspicious but inconclusive; requires Phase 2 deep-dive |
| FAIL | Clear defect confirmed; evidence anchor provided |

Every WARN and FAIL must include an **evidence anchor**: 会话 id（主会话或子会话目录名）、`seq`（事件序号）或行号、and a brief excerpt from the trace.

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

- **Evidence over opinion**: Never claim a defect without citing a specific 会话/子会话目录名、`seq` 或行号、and excerpt
- **Spot-check always**: A clean orchestrator-level report does not guarantee clean subagent behavior
- **Deep-dive selectively**: Don't read every subagent trace in full — follow the signals from Phase 1
- **Report for humans**: The audit is for a developer to read and act on, not for automated pipelines
- **只读审计**：审计过程不需要写文件；DSH 文件沙箱通常为 `workspace-write`，但审计者只用 `bash` 做只读检查（`zstd -dc`、`grep`、`git diff`、`git status`），不要修改被审计的工作区
