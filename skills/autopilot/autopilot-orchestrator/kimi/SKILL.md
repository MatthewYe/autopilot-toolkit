---
name: autopilot-orchestrator
description: "Autopilot issue resolution loop: scan → implement → review → retry. Dispatches implementer and reviewer subagents per ready-for-agent issue via the Agent tool, then runs global meta-review. Use when processing autopilot issues from any source."
---

Before anything else, read ~/.agents/principles/karpathy.md. Apply Principle 1 "Think Before Analyzing" variant + Principles 2, 4.

Execute the autopilot orchestrator workflow below.

## 子代理 dispatch 模型（Kimi）

Kimi 没有 `run_skill`。dispatch implementer / reviewer 一律使用 **Agent 工具**：

- dispatch prompt 的**第一段**必须指示子代理先读取其技能正文并严格遵循：
  - implementer：`Read ~/.agents/skills/autopilot-implementer/SKILL.md`
  - reviewer：`Read ~/.agents/skills/autopilot-reviewer/SKILL.md`
- 第二段是任务描述（格式见"执行 implementer" / "处理 implementer 结果"）。Kimi 子代理不继承父会话的技能清单，所以技能正文必须通过 prompt 显式让它读。
- 等待子代理返回，从返回文本中解析 `IMPLEMENTER_REPORT:` / `REVIEWER_REPORT:`。

文件操作使用 Kimi 工具：`Read`（读文件）、`Edit`（改 frontmatter Status 行）、`Write`（写 suggestions.json）、`Glob`（扫描）、`Grep`、`Bash`（gh CLI 等）。

## Issue 来源识别

autopilot 支持两种 issue 来源。根据 `target` 参数或扫描结果判断：

| target 特征 | 来源 | 状态机 | 合约文件 |
|---|---|---|---|
| 包含 `/` 的路径 | 本地 `.scratch/` | frontmatter `Status:` | `AGENT-BRIEF.md` |
| `#N` 或纯数字 `N` | GitHub Issue | labels | issue body（含 AC） |
| 无参数扫描到本地 | 本地 `.scratch/` | frontmatter `Status:` | `AGENT-BRIEF.md` |
| 无参数扫描到 GitHub | GitHub Issue | labels | issue body |

## 前置约定

### 本地 issue 模式

- `target` 使用绝对路径。如传入相对路径，拼接当前工作目录。
- `issue.md` 以 YAML frontmatter 开头，`Status` 字段在 frontmatter 中。
- 更新 Status：用 `Edit` 工具修改 frontmatter 中的 `Status:` 行。
- 追加注释：在 `## Comments` 节末尾加 `- <时间戳> autopilot: <内容>`。无该节则在文件末尾创建。
- 合约文件：同目录下 `AGENT-BRIEF.md`。

### GitHub Issue 模式

> **MCP 优先**：启动时检测 `mcp__github__*` 工具是否可用（检查工具注册表）。如 MCP 可用，优先使用 MCP 工具（`mcp__github__list_issues`、`mcp__github__update_issue`、`mcp__github__add_comment` 等）；如不可用，回退到 `Bash` 运行 `gh issue ...`（需 gh CLI 已安装且已认证）。下文 `gh` CLI 调用均为回退路径，MCP 可用时应优先使用对应 MCP 工具。

- 从 `git remote -v` 自动推断 repo。
- 状态通过 labels 表达：`in-progress`、`resolved`、`needs-info`。
- 追加注释：`gh issue comment <N> --body "..."`（MCP 可用时用 `mcp__github__add_comment`）
- 合约来自 issue body（其中包含 Acceptance Criteria 和 What to build，由 `to-issues` 创建）。
- 读取 issue：`gh issue view <N> --json number,title,body,labels,state`（MCP 可用时用 `mcp__github__get_issue`）

### 共用概念

- `Status: ready-for-agent`（本地 frontmatter）↔ label `ready-for-agent`（GitHub）
- `Status: in-progress` ↔ label `in-progress`
- `Status: resolved` ↔ label `resolved`
- `Status: needs-info` ↔ label `needs-info`

---

## PRD 检测与跳过

PRD（Product Requirement Document）描述整体设计方案，不包含可直接实现的 `## Acceptance Criteria` 或 `## What to build`。PRD 不应被 dispatch 给 implementer——具体实现由子 issue 承载。

### 检测信号（双向兼容，零上游依赖）

| 信号 | 本地 markdown | GitHub |
|------|-------------|--------|
| **主检测**（内容模式） | body 含 `## Problem Statement` + `## Solution`，但**不含** `## What to build` 和 `## Acceptance Criteria` | 同左 |
| **加速标记**（可选） | frontmatter `Type: prd` | label `prd` |

内容模式检测覆盖了 `to-issues` 生成的标准 implementable issue（它们必有 `## What to build` + `## Acceptance Criteria`，不会被误判）。标记只是让 orchestrator 跳过内容解析的加速路径，非必须。

### 行为

无论显式指定 target 还是扫描模式，检测到 PRD 后：
- **跳过**，不进入 Phase 1 调度循环
- 回复原因：`"<id> is a PRD, not directly implementable. Process its child issues instead."`
- 不修改 PRD 的状态（保持原有状态，待 Phase 2 处理）

---

## 如果指定了 target

### target 是路径（含 `/`）

1. 确认 `<target>/issue.md` 存在，不存在则报告错误并停止
2. 确认 `<target>/AGENT-BRIEF.md` 存在，不存在则报告错误并停止
3. 读取 `<target>/issue.md`，检查 `Status:` 是否为 `ready-for-agent` 或 `in-progress`
4. 非以上状态 → 回复当前状态并停止
5. **PRD 检测**：检查 frontmatter 中是否有 `Type: prd`，或 body 是否满足 PRD 内容模式（含 `## Problem Statement` + `## Solution`，不含 `## What to build` 和 `## Acceptance Criteria`）。命中 → 回复 `"<target> is a PRD, not directly implementable. Process its child issues instead."` 并停止
6. 更新 Status 为 `in-progress`
7. 设置 `source = "local"`, `id = <target>`
8. 从 `<target>` 推断 feature 目录（取 issue 目录的父级父级，如 `.scratch/auth/issues/01-login/` → `.scratch/auth/`）
9. 设置 `contract = <target>/AGENT-BRIEF.md` 的内容作为合约文本
10. 跳到"交叉 Issue Suggestion 匹配"

### target 是 GitHub issue 号（`#N` 或纯数字 `N`）

提取数字部分为 `issueNumber`：

1. `gh issue view <issueNumber> --json number,title,body,labels,state` 获取 issue 信息
2. 检查 labels 是否含 `ready-for-agent` 或 `in-progress`
3. 非以上标签 → 回复当前状态并停止
4. **PRD 检测**：检查 labels 是否含 `prd`，或 issue body 是否满足 PRD 内容模式（含 `## Problem Statement` + `## Solution`，不含 `## What to build` 和 `## Acceptance Criteria`）。命中 → 回复 `"#<issueNumber> is a PRD, not directly implementable. Process its child issues instead."` 并停止
5. 将 `ready-for-agent` 标签替换为 `in-progress`：`gh issue edit <issueNumber> --add-label "in-progress" --remove-label "ready-for-agent"`
6. 追加评论：`gh issue comment <issueNumber> --body "autopilot: 开始处理"`
7. 从 issue body 提取 Acceptance Criteria 和 What to build 作为合约文本
8. 设置 `source = "github"`, `id = <issueNumber>`, `contract = <解析出的合约文本>`
9. 从 issue title 生成 feature slug（如 `Implement Suggestion matching` → `suggestion-matching` → `.scratch/suggestion-matching/`）
10. 跳到"交叉 Issue Suggestion 匹配"

---

## 否则（无参数）：扫描模式

同时扫描两个来源：

### 本地扫描

1. `Glob` 扫描 `.scratch/*/issues/*.md`
2. 对每个文件，读取前 30 行，检查是否有 `Status: ready-for-agent`
3. 对匹配项，检查是否为 PRD：读取 frontmatter 中 `Type: prd` 字段，或读取 body 检查是否满足 PRD 内容模式（含 `## Problem Statement` + `## Solution`，不含 `## What to build` 和 `## Acceptance Criteria`）。PRD 条目**不纳入调度队列**，单独记录
4. 收集所有非 PRD 的匹配项

### GitHub 扫描

5. `gh issue list --label "ready-for-agent" --state open --json number,title,labels --limit 50`
6. 过滤掉 labels 含 `prd` 的条目
7. 对剩余条目，用 `gh issue view <N> --json body` 检查 body 是否满足 PRD 内容模式。命中的条目**不纳入调度队列**，单独记录
8. 收集所有非 PRD 的匹配项

### 选择并报告

9. 合并两个来源的非 PRD 结果。列出找到的 implementable issue，同时报告跳过的 PRD 数量（如 "skipped 1 PRD: #12"）
10. 选择第一个（按先本地后 GitHub，各自内部按自然序），标注正在处理哪个
11. 如果零个 implementable issue → 跳到"Phase 2: 全局 meta-review"
12. 根据选中 issue 的来源，走对应的初始化流程

---

## Phase 1: 调度循环

维护 `retry_count = 0`，最多 3 轮（`retry_count` = 0, 1, 2）：
- retry_count = 0: 首次实现
- retry_count = 1: 第 1 次 retry
- retry_count = 2: 第 2 次 retry
- retry_count >= 3: 转为 needs-info

### 更新状态（抽象）

- **local**: `Edit` 工具修改 `issue.md` 的 `Status:` 行
- **github**: `gh issue edit <N> --add-label "<新>" --remove-label "<旧>"`

### 追加注释（抽象）

- **local**: 在 `issue.md` 的 `## Comments` 节末尾添加条目
- **github**: `gh issue comment <N> --body "<时间戳> autopilot: <内容>"`

### 交叉 Issue Suggestion 匹配

dispatch implementer 前，若 `.scratch/<feature>/suggestions.json` 存在且有 `status: "pending"` 条目，按 `references/suggestion-matching.md` 中的算法匹配到当前 issue 的 AGENT-BRIEF。匹配到的条目组装为 `CROSS_ISSUE_SUGGESTIONS` 传入 implementer；否则跳过。

### 执行 implementer

#### 前置：Pre-flight 工具链检测

dispatch implementer 前，检测项目的工具链是否可用：

1. 根据项目类型推断测试命令（Rust → `cargo test`，Node → `npm test`，Python → `pytest` 或 `uv run pytest`）
2. 运行 `which <tool>` 检测工具链是否存在（如 `which cargo`、`which npm`）
3. 不可用时尝试常见安装路径（`~/.cargo/bin/cargo`、`~/.rustup/toolchains/*/bin/cargo`）
4. 设置 `TOOLCHAIN: available` 或 `TOOLCHAIN: unavailable`，传入 implementer 的 dispatch prompt

#### 前置：REFACTORING 模式检测

分析合约内容，检测当前 issue 是否为纯重构任务（非新功能开发）：

1. 扫描合约关键词：`replace`、`consolidate`、`extract`、`delete`、`Remove`、`Replace`、`inline`、`shared function`、`duplicated` → 命中 2+ 且不含 `Add`、`new feature`、`Implement`（作为新增功能时）→ 标记 `REFACTORING: true`
2. 对照 AC：如果所有 AC 描述的是"替换"或"删除"而非"新增功能" → `REFACTORING: true`
3. 设置 `REFACTORING: true|false`，传入 implementer 的 dispatch prompt

使用 **Agent 工具** dispatch implementer subagent。prompt 格式：

```
先 Read ~/.agents/skills/autopilot-implementer/SKILL.md 并严格遵循其中的实施者方法论。

<以下为任务描述>

<根据 retry_count 和模式动态生成>
```

任务描述部分传递：
- **共同的**：`source`, `id`, `contract`（合约内容）, `TOOLCHAIN: <available|unavailable>`, `REFACTORING: <true|false>`，以及：
  - 首次（retry_count = 0）：`ROUND: 0`
  - retry（retry_count >= 1）：`ROUND: <retry_count>` + `PREV_REVIEW: <上一轮 REVIEWER_REPORT 全文>`
  - 如有匹配到的 CROSS_ISSUE_SUGGESTIONS，一并传入
- **本地模式**：额外传 issue 目录绝对路径
- **GitHub 模式**：额外传 issue body（含 AC）+ `IS_GITHUB: true`

等待 implementer 回复，解析 `IMPLEMENTER_REPORT:`。

**空回复处理：** 如果 implementer 返回空结果（无 `IMPLEMENTER_REPORT:` 标记头），自动重试 1 次（重新 dispatch 相同 prompt）。两次都空 → 更新 Status 为 `needs-info` 并停止。

**解析容错：** 回复中找不到 `IMPLEMENTER_REPORT:` 标记头 → 视为不可解析，更新 Status 为 `needs-info` 附原始回复，停止。

### 首次实现：检查 SELF_REVIEW

retry_count = 0 时，检查报告中有无 `SELF_REVIEW:` 段：

- STATUS: DONE → "无问题" 或 "发现问题 → 已修复" → 通过
- STATUS: UNVERIFIED → 必须包含每条 AC 的验证方式标注（测试运行 / 代码结构分析）。**标注缺失但 STATUS: UNVERIFIED → 通过**（UNVERIFIED 本身已声明验证不全）
- STATUS: DONE 或 UNVERIFIED 但缺失 SELF_REVIEW 段 → 标记为 `needs-info`，停止

Retry 轮次（retry_count >= 1）不检查 SELF_REVIEW。

### 收集 SIBLING_CONTEXT

dispatch reviewer 前，自动收集当前 issue 所属 PRD 下所有已 resolved 的兄弟模块信息：

1. 从当前 issue body 的 `Parent` 链接提取 PRD issue 号
2. `gh issue list --label "resolved" --json number,title` 获取所有已 resolve 的 issue
3. 对于每个已 resolve 的 issue（排除当前 issue 自己），提取其 title 和关键约定（入口模式、测试框架、文件布局）
4. 组装为 `SIBLING_CONTEXT` 字符串，包含："已完成的兄弟模块: #N title — 关键约定: ..."

### 处理 implementer 结果

- **STATUS: DONE** → 使用 **Agent 工具** dispatch reviewer subagent。prompt 格式：

```
先 Read ~/.agents/skills/autopilot-reviewer/SKILL.md 并严格遵循其中的审查者方法论。

<以下为任务描述>
```

任务描述部分传递 `source`, `id`, `contract`, `CHANGED_FILES`, `SIBLING_CONTEXT` + 上一轮 `REVIEWER_REPORT`（如有）
  - **GitHub 模式**：额外传 `IS_GITHUB: true`

- **STATUS: UNVERIFIED** → 使用 **Agent 工具** dispatch reviewer subagent（prompt 格式同上）。任务描述中额外传递 `UNVERIFIED: true` + implementer 的完整 `SELF_REVIEW` 段（含逐 AC 验证方式标注）。reviewer 的审查侧重：
  - 结构正确性（代码逻辑是否符合 AC）
  - 是否所有 AC 都有对应的代码实现
  - VERDICT 可选 `VERIFY_NEEDED`（结构通过但需工具链验证）或 `RETRY`（结构本身有问题）

- **STATUS: BLOCKED 或 NEEDS_CONTEXT** → 更新 Status 为 `needs-info`，追加注释说明原因，**停止**

#### 解析 SUGGESTION_RESOLUTIONS

STATUS: DONE 时，从 `IMPLEMENTER_REPORT` 中解析 `SUGGESTION_RESOLUTIONS:` 段，暂存待 reviewer 确认后执行：

1. 如段内容为 "无" 或不存在 → 无需要处理的跨 issue suggestion，跳过
2. 逐条解析，每行格式：`[resolved|rejected|deferred] 来源 <source_issue> round <N>: <content 摘要> → <处理说明>`
3. 提取字段：
   - `type`：`resolved` / `rejected` / `deferred`
   - `source_issue`：来源 issue 标识（如 `#18`、`01-login`）
   - `round`：reviewer 轮次
   - `summary`：`→` 前的 content 摘要
   - `detail`：`→` 后的处理说明（对 rejected 即拒绝理由）
4. 暂存为 `pending_resolutions` 列表，在 reviewer 返回 MERGE 后统一执行状态更新

### 处理 reviewer 结果

解析 `REVIEWER_REPORT:`，看 VERDICT。reviewer 任务失败或找不到 `VERDICT:` → 视为 BLOCKED，更新 Status 为 `needs-info` 并停止。

**解析容错：** 找不到 `REVIEWER_REPORT:` 标记头 → 视为不可解析，更新 Status 为 `needs-info` 附原始回复，停止。

#### 提取 Suggestion 并持久化

解析完 REVIEWER_REPORT 后，无论 VERDICT 如何，提取 `## Suggestion` 节的所有条目并写入 `suggestions.json`：

1. **解析条目**：逐条解析 `## Suggestion` 下的每个 `- [ ]` 项：
   - `content`：`- [ ] ` 后的正文文本（不含 KEYWORDS/FILES 标注行）
   - `keywords`：`KEYWORDS:` 行（逗号分隔，可选）→ 解析为数组
   - `files`：`FILES:` 行（逗号分隔，可选）→ 解析为数组
2. **兜底提取**（仅当对应标注缺失时）：
   - **关键词兜底**：从 `content` 文本中提取 2-5 个最有代表性的术语（优先提取技术术语、模块名、模式名）
   - **文件路径兜底**：从当前 issue 的 implementer 报告 `CHANGED_FILES` 中提取，去重
3. **推断 feature 目录**：
   - 本地模式（`source = "local"`）：从 issue 路径提取，如 `.scratch/auth/issues/01-login/` → `.scratch/auth/`
   - GitHub 模式（`source = "github"`）：从 issue title 生成 feature slug，创建 `.scratch/<feature-slug>/`
4. **读取现有文件**：检查 `.scratch/<feature>/suggestions.json` 是否存在，存在则读取，不存在则初始化为空数组 `[]`
5. **去重**：按 `content` 字段比较，已存在相同 `content` 的条目不重复写入
6. **追加新条目**：每个新条目格式为：
   ```json
   { "issue": "<issue-slug>", "round": <N>, "content": "...", "files": [...], "keywords": [...], "status": "pending" }
   ```
   - `issue`：本地模式用目录名（如 `01-login`），GitHub 模式用 `#<N>`
   - `round`：当前 `retry_count`
7. **写入文件**：将更新后的数组写回 `.scratch/<feature>/suggestions.json`（`Write` 工具）
8. **GitHub Issue 评论同步**（仅 `source = "github"` 时执行）：
   - 对每条**新增**的 suggestion（去重跳过的不写），追加 issue comment：
     ```
     gh issue comment <N> --body "autopilot suggestion [pending]: <content>"
     ```
   - 格式：`autopilot suggestion [<status>]: <正文>`
9. **报告**：向用户报告提取结果 — "从 reviewer 提取了 N 条 Suggestion（M 条新增，K 条去重跳过）"；如有 GitHub comment 同步，注明已写入 N 条 comment

**注意**：仅提取 `## Suggestion` 级别条目。Critical 和 Important 必须在当前 issue 内解决，不传播。

---

VERDICT 分支：

- **MERGE** → 更新 Status 为 `resolved`，追加 reviewer 结论。进入"Update Suggestion 状态"步骤，完成后**返回扫描模式处理下一个 issue**
- **VERIFY_NEEDED** → 审查通过（结构正确）但 implementer 工具链不可用，无法实际验证。处理流程：
  1. 尝试运行项目的测试命令（如 `cargo test`、`npm test`、`pytest`）。如工具链在 orchestrator 环境可用 → 运行验证
  2. 验证通过 → 更新 Status 为 `resolved`，追加 "Orchestrator verified: all tests pass"
  3. 验证失败或工具链仍不可用 → 更新 Status 为 `needs-info`，追加 reviewer 结论 + "Toolchain unavailable — requires manual verification"
  4. 所有情况下保留 reviewer 报告和 Suggestion 提取
- **RETRY** → `retry_count += 1`，清空 `pending_resolutions = []`（上一轮 resolutions 在 retry 后失效，新轮次 implementer 需重新声明）
  - `retry_count < 3`：返回"执行 implementer"（传递 PREV_REVIEW）
  - `retry_count >= 3`：更新 Status 为 `needs-info`，追加 reviewer 问题清单 + 说明已达最大重试次数，**返回扫描模式处理下一个 issue**
- **BLOCKED** → 更新 Status 为 `needs-info`，追加 reviewer 结论，**返回扫描模式处理下一个 issue**

#### Update Suggestion 状态

VERDICT: MERGE 时，根据 `pending_resolutions` 更新 `suggestions.json` 中对应条目的状态：

1. **定位条目**：在 `suggestions.json` 中按 `issue`（匹配 `source_issue`）、`round` 和 `content` 三级匹配对应 suggestion 条目：
   - 一级：`issue` 字段匹配 `source_issue`（字符串全等）
   - 二级：`round` 字段匹配 `round`（数字全等）
   - 三级：`summary`（`→` 前的 content 摘要）作为子串出现在条目的 `content` 字段中（子串匹配，大小写敏感）
   - 无匹配条目（implementer 声明了但 suggestions.json 中找不到）→ 跳过该条
   - **多命中歧义消解**（三级命中 2+ 条）：执行四级匹配打破平局——
     1. 计算每条候选 entry 的 `files` 与当前 issue 的 implementer `CHANGED_FILES` 的交集，取交集最多者
     2. 仍平局：取 `summary` 在 `content` 中匹配长度最长者（最精确匹配）
     3. 仍平局（极少见，如相同 content、相同 files）：跳过该条并报告歧义 — "Suggestion resolution ambiguous: `summary` 命中 N 条内容相近的 entry（source_issue + round），无法自动消歧，请人工处理"
2. **状态校验**：定位到条目后，检查其 `status`：
   - `status === "pending"` → 继续步骤 3（正常处理）
   - `status !== "pending"`（如 `resolved`/`rejected`）→ **跳过该条**并报告异常 — "Skipping suggestion resolution: matched entry already has status `<status>` (expected pending). Possible multi-hit mis-match or duplicate resolution."
3. 根据 `type` 执行状态转换：

   | type | 操作 | 字段更新 |
   |------|------|---------|
   | `resolved` | 标记为已解决 | `status: "resolved"`, `resolved_in_issue`: 当前 issue 的 slug（本地模式用目录名，GitHub 模式用 `#<N>`） |
   | `rejected` | 标记为已拒绝 | `status: "rejected"`, `rejected_reason`: `detail` 字段内容（即 `→` 后的处理说明） |
   | `deferred` | 保持 pending + 备注 | `status` 仍为 `"pending"`, `deferred_by`: 当前 issue slug |

4. **写回文件**：将更新后的数组写回 `.scratch/<feature>/suggestions.json`
5. **GitHub Issue 评论同步**（仅 `source = "github"` 时执行）：
   - 对 `resolved` 和 `rejected` 类型，追加 issue comment：
     ```
     gh issue comment <N> --body "autopilot suggestion [resolved|rejected]: <content 摘要>"
     ```
   - `deferred` 不需要额外 issue comment（状态未变，且 initial pending comment 已存在）
   - 注：如 processed issue 与 source issue 是同一个 GitHub issue，在同一 issue 下追加 comment

6. **报告**：汇总更新结果 — "处理了 N 条 suggestion（M resolved, K rejected, J deferred）"；如有 GitHub comment 同步，注明已写入 N 条

### Phase 1 退出条件

当扫描模式返回零个 ready-for-agent issue 时，Phase 1 完成。进入 Phase 2。

---

## Phase 2: 全局 Meta-Review

当所有 Phase 1 issue 处理完毕（无 ready-for-agent 剩余），执行 `references/meta-review.md` 中的全局审查流程：并行派遣 reviewer 子 agent（Agent 工具，prompt 第一段指示其先 Read `~/.agents/skills/autopilot-reviewer/SKILL.md`）+ orchestrator 自主审查，合并报告，修复 Critical/Important 问题，解析 PRD。

### FINAL_ACCEPTANCE_REPORT

meta-review 修复完成后，按 `references/acceptance-report.md` 产出跨 issue Suggestion 验收报告。
