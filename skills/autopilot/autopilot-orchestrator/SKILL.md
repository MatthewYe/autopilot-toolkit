---
name: autopilot-orchestrator
description: Put issue resolution on autopilot — scans local .scratch/ files AND GitHub Issues for ready-for-agent issues, dispatches implementer → reviewer in a retry loop until resolved. After all issues complete, runs global meta-review against ADR/PRD and fixes cross-module issues. Use when processing autopilot issues from any source.
---

Before anything else, read ~/.agents/principles/karpathy.md. Apply Principle 1 "Think Before Analyzing" variant + Principles 2, 4.

Execute the autopilot orchestrator workflow below. The implementer and reviewer subagents have methodology already inlined in their skill bodies — no `skill()` loading preamble is needed in dispatch prompts.

## Issue 来源识别

autopilot 支持两种 issue 来源。根据 `target` 参数或扫描结果判断：

| target 特征 | 来源 | 状态机 | 合约文件 |
|---|---|---|---|
| 包含 `/` 的路径 | 本地 `.scratch/` | frontmatter `Status:` | `AGENT-BRIEF.md` |
| `#N` 或纯数字 `N` | GitHub Issue | labels | issue body（含 AC） |
| 无参数扫描到本地 | 本地 `.scratch/` | frontmatter `Status:` | `AGENT-BRIEF.md` |
| 无参数扫描到 GitHub | GitHub Issue | labels | issue body |

## 前置约定

> **TODO: reasonix file/issue API** — `edit_file` tool, `gh issue edit`, and `gh issue comment` operations below must be adapted to reasonix tool equivalents. The operational logic (what to do, when, and with what content) is preserved.

### 本地 issue 模式

- `target` 使用绝对路径。如传入相对路径，拼接当前工作目录。
- `issue.md` 以 YAML frontmatter 开头，`Status` 字段在 frontmatter 中。
- 更新 Status：用 `edit_file` 工具修改 frontmatter 中的 `Status:` 行。
- 追加注释：在 `## Comments` 节末尾加 `- <时间戳> autopilot: <内容>`。无该节则在文件末尾创建。
- 合约文件：同目录下 `AGENT-BRIEF.md`。

### GitHub Issue 模式

> **MCP 优先**：启动时检测 `mcp__github__*` 工具是否可用（检查工具注册表）。如 MCP 可用，优先使用 MCP 工具（`mcp__github__list_issues`、`mcp__github__update_issue`、`mcp__github__add_comment` 等）；如不可用，回退到 `bash gh issue ...`（需 gh CLI 已安装且已认证）。下文 `gh` CLI 调用均为回退路径，MCP 可用时应优先使用对应 MCP 工具。

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

## 如果指定了 target

### target 是路径（含 `/`）

1. 确认 `<target>/issue.md` 存在，不存在则报告错误并停止
2. 确认 `<target>/AGENT-BRIEF.md` 存在，不存在则报告错误并停止
3. 读取 `<target>/issue.md`，检查 `Status:` 是否为 `ready-for-agent` 或 `in-progress`
4. 非以上状态 → 回复当前状态并停止
5. 更新 Status 为 `in-progress`
6. 设置 `source = "local"`, `id = <target>`
7. 从 `<target>` 推断 feature 目录（取 issue 目录的父级父级，如 `.scratch/auth/issues/01-login/` → `.scratch/auth/`）
8. 设置 `contract = <target>/AGENT-BRIEF.md` 的内容作为合约文本
9. 跳到"交叉 Issue Suggestion 匹配"

### target 是 GitHub issue 号（`#N` 或纯数字 `N`）

提取数字部分为 `issueNumber`：

1. `gh issue view <issueNumber> --json number,title,body,labels,state` 获取 issue 信息
2. 检查 labels 是否含 `ready-for-agent` 或 `in-progress`
3. 非以上标签 → 回复当前状态并停止
4. 将 `ready-for-agent` 标签替换为 `in-progress`：`gh issue edit <issueNumber> --add-label "in-progress" --remove-label "ready-for-agent"`
5. 追加评论：`gh issue comment <issueNumber> --body "autopilot: 开始处理"`
6. 从 issue body 提取 Acceptance Criteria 和 What to build 作为合约文本
7. 设置 `source = "github"`, `id = <issueNumber>`, `contract = <解析出的合约文本>`
8. 从 issue title 生成 feature slug（如 `Implement Suggestion matching` → `suggestion-matching` → `.scratch/suggestion-matching/`）
9. 跳到"交叉 Issue Suggestion 匹配"

---

## 否则（无参数）：扫描模式

同时扫描两个来源：

### 本地扫描

1. Glob 扫描 `.scratch/*/issues/*.md`
2. 对每个文件，读取前 30 行，检查是否有 `Status: ready-for-agent`
3. 收集所有匹配项

### GitHub 扫描

4. `gh issue list --label "ready-for-agent" --state open --json number,title --limit 50`
5. 收集所有匹配项

### 选择并报告

6. 合并两个来源的结果。向用户列出所有找到的 issue
7. 选择第一个（按先本地后 GitHub，各自内部按自然序），标注正在处理哪个
8. 如果零个 → 跳到"Phase 2: 全局 meta-review"
9. 根据选中 issue 的来源，走对应的初始化流程

---

## Phase 1: 调度循环

维护 `retry_count = 0`，最多 3 轮（`retry_count` = 0, 1, 2）：
- retry_count = 0: 首次实现
- retry_count = 1: 第 1 次 retry
- retry_count = 2: 第 2 次 retry
- retry_count >= 3: 转为 needs-info

### 更新状态（抽象）

- **local**: `edit_file` 工具修改 `issue.md` 的 `Status:` 行
- **github**: `gh issue edit <N> --add-label "<新>" --remove-label "<旧>"`

### 追加注释（抽象）

- **local**: 在 `issue.md` 的 `## Comments` 节末尾添加条目
- **github**: `gh issue comment <N> --body "<时间戳> autopilot: <内容>"`

### 交叉 Issue Suggestion 匹配

dispatch implementer 前，扫描 `suggestions.json`，匹配 pending suggestions 到当前 issue 的 AGENT-BRIEF：

#### 推断 feature 目录

- **本地模式**：从 issue 路径提取（如 `.scratch/auth/issues/01-login/` → `.scratch/auth/`）
- **GitHub 模式**：从 issue title 生成 feature slug → `.scratch/<feature-slug>/`
- 若无从推断 → 跳过匹配，不传 CROSS_ISSUE_SUGGESTIONS

#### 读取和匹配

1. 检查 `.scratch/<feature>/suggestions.json` 是否存在：
   - 不存在 → 跳过匹配，不传 CROSS_ISSUE_SUGGESTIONS
   - 存在 → 读取，筛选 `status: "pending"` 的条目
2. 对每条 pending suggestion，执行双重匹配（**任一命中即视为匹配**）：
   - **文件路径匹配**：suggestion 的 `files` 数组中任一路径字符串作为子串出现在 AGENT-BRIEF 全文（issue body、AC 文本、文件引用）→ 命中
   - **关键词匹配**：suggestion 的 `keywords` 数组中任一关键词作为子串出现在 AGENT-BRIEF 全文中（**大小写不敏感**）→ 命中
3. 未命中的 suggestions 保持 `pending` 状态，不传递
4. 命中的 suggestions 组装为 `CROSS_ISSUE_SUGGESTIONS` JSON 数组。每条附带完整 reviewer 上下文：
   ```json
   {
     "source_issue": "#N 或 <slug>",
     "round": <N>,
     "content": "<suggestion 正文>",
     "files": ["path/to/file1.ts", ...],
     "keywords": ["keyword1", ...],
      "reviewer_context": "<原 REVIEWER_REPORT 摘录：该 Suggestion 所属 REVIEWER_REPORT 中 Suggestion 条目全文（含 KEYWORKS/FILES 标注）>"
    }
    ```
    **`reviewer_context` 重建**：`suggestions.json` 中存储的是结构化字段（`content`、`files`、`keywords`），不含标注行。组装 `CROSS_ISSUE_SUGGESTIONS` 时，orchestrator 需从独立字段重建 `reviewer_context`（即带 KEYWORDS/FILES 标注行的完整 reviewer report 摘录），格式如：
    ```
    - [ ] <content>
      KEYWORDS: <keywords>
      FILES: <files>
    ```
5. 无匹配到任何 suggestion → 不传 CROSS_ISSUE_SUGGESTIONS

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

使用 `run_skill(name: "autopilot-implementer", arguments: "<task 描述>")` dispatch implementer agent。prompt 格式：

```
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

- **STATUS: DONE** → 使用 `run_skill(name: "autopilot-reviewer", arguments: "<task 描述>")` dispatch `reviewer` agent。prompt 格式：

```
<以下为任务描述>
```

任务描述部分传递 `source`, `id`, `contract`, `CHANGED_FILES`, `SIBLING_CONTEXT` + 上一轮 `REVIEWER_REPORT`（如有）
  - **GitHub 模式**：额外传 `IS_GITHUB: true`

- **STATUS: UNVERIFIED** → 使用 `run_skill(name: "autopilot-reviewer", arguments: "<task 描述>")` dispatch `reviewer` agent（prompt 格式同上）。任务描述中额外传递 `UNVERIFIED: true` + implementer 的完整 `SELF_REVIEW` 段（含逐 AC 验证方式标注）。reviewer 的审查侧重：
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
7. **写入文件**：将更新后的数组写回 `.scratch/<feature>/suggestions.json`（`write_file` 工具）
   > **TODO: reasonix file/issue API** — adapt `write_file` tool to reasonix equivalent for persisting `suggestions.json`.
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

当所有 issue 处理完毕（无 ready-for-agent 剩余），执行全局审查。

### 目的

对照 ADR、PRD 和所有 issue 合约，审视整个 codebase 的：
- 实现正确性（所有模块是否符合各自的 AC 和 PRD 全局约束）
- 跨模块一致性（是否有模式漂移、重复实现、约定不一致）
- 计划外变更（是否有孤儿文件、未声明依赖、残留引用）

### 执行方式

Orchestrator 自主审查与 reviewer 子 agent **并行**执行。两者均产出独立报告后，进入「报告合并」统一处理。

#### 1. 派遣 reviewer 子 agent（并行）

使用 `run_skill(name: "autopilot-reviewer", arguments: "<task 描述>")` dispatch `reviewer` agent（只读，无 edit_file/bash 权限）。prompt 格式：

```
你正在执行全局 meta-review。审查范围为整个 codebase，对照以下基准：

**审查基准（读取以下全文）：**
- 所有 ADR（docs/adr/，如存在）
- 所有 PRD（如有）
- 所有已 resolved issue 的合约（AGENT-BRIEF.md 或 GitHub issue body 中的 AC）

**审查维度（适配 reviewer 四维框架到全局 meta-review 上下文）：**

1. **ADR/PRD 全局约束验证**（维度四：计划忠实度）：
   - 逐条检查 ADR 和 PRD 中声明的全局约束（输出格式要求、依赖白名单、运行时约束、目录结构约定等）是否在所有模块中满足
   - 是否存在约束降级（如 PRD 要求 byte-identical 但实现仅做到结构等价）
   - 依赖白名单是否被超出

2. **跨模块一致性**（维度三代码质量 + 维度四工程约定）：
   - 入口检测方式、import 风格（静态/动态）、错误处理模式、日志格式、算法选择、文件布局是否一致
   - 是否存在模式漂移（不同模块用不同方式解决同一问题）
   - 是否有重复实现

3. **计划外变更检测**（维度四：孤儿文件、未声明行为）：
   - 是否存在孤儿文件：不在任何合约中声明的新文件
   - 合约要求删除但尚未删除的文件
   - 合约未声明的新行为（悄悄加的 UX 优化、额外校验、额外日志）
   - 未在合约中声明的副作用（自动创建目录、修改全局配置、静默改写其他模块文件）

4. **AC 覆盖率**（维度一：行为对齐的全局化）：
   - 对照所有 resolved issue 合约，逐条检查 AC 是否有对应实现

输出格式与标准 reviewer 一致：以 `REVIEWER_REPORT:` 开头，分 Critical / Important / Suggestion 三级 + VERDICT（MERGE / RETRY / BLOCKED）。
```

#### 2. Orchestrator 自主审查（并行）

Orchestrator 自身用 grep/glob 工具执行审查，覆盖与 reviewer 子 agent 相同的范围：

1. 读取 PRD 全文和所有相关 ADR（包含 ADR 0003、ADR 0004 等），列出每条全局约束
2. 逐条检查：用 grep/glob 扫描 codebase，验证约束满足
3. 对照 issue 合约，检查每个 resolved issue 的 AC 覆盖率
4. 检查跨模块一致性（入口检测方式、import 风格、错误处理、日志格式、算法选择、文件布局）
5. 检查计划外变更（孤儿文件、未声明新行为、副作用、未删除文件）
6. 输出结构化报告：Critical / Important / Suggestion + VERDICT

#### 3. 等待两份报告

上述 1、2 两步并行执行。两者均完成后（均产出独立报告），进入下方「报告合并」流程。

### 报告合并

`执行方式` 产生两份独立的 meta-review 报告：
- **orchestrator 自主审查报告** — 对照 ADR、PRD 和 issue 合约逐条检查
- **reviewer 子 agent 并行审查报告** — 4 轴审查（Behavior alignment、TDD discipline、Code quality、Plan fidelity）

进入修复循环前，将两份报告合并为一份 `MERGED_META_REPORT`：

1. **Union 策略**：两份报告中 Critical 和 Important 级别的问题取其并集——任一份报告标记的问题均纳入修复范围。Suggestion 级别条目同样取并集（去重后）。

2. **冲突裁决**：当两份报告对同一文件/路径有不同结论时（如一方标记为问题，另一方认为正常），orchestrator 手动核实并裁定：
   - **默认采纳更严格结论**：无法确认是否为误报时，默认采纳更严格的发现（标记为问题）。
   - **确认误报后降级**：仅当 orchestrator 明确确认某发现为误报（false positive）时，方可将该条目从修复范围移除或降级为 Suggestion。
   - 裁决过程记录到合并报告中，注明"冲突裁决：\<路径\> — 采纳 \<来源\> 的结论"

3. **去重**：完全相同的发现（同一文件 + 同一问题模式）在两份报告中均出现时，合并为单一条目，标注"双来源一致：<发现描述>"。

合并后产出 `MERGED_META_REPORT`，包含：
- Critical 条目（合并去重后）
- Important 条目（合并去重后）
- Suggestion 条目（合并去重后）
- 冲突裁决记录

### 修复循环

从合并报告（`MERGED_META_REPORT`）中取 Critical + Important 条目，由 **orchestrator 直接修复**（不 dispatch implementer），因为 meta 问题通常是机械性的：

- **统一模式**：isMain 不一致 → 直接 edit 文件统一为一种模式
- **删除残留**：孤儿文件 / __pycache__ / 残留引用 → 直接 delete/edit
- **更新文档**：SKILL.md / schemas.md / ADR 引用 → 直接 edit

遇到需要判断的设计级问题（如"两种算法选哪个"），追加 comment 标记为 needs-info。

### 修复后验证

修复完成后：
1. 运行项目的测试命令确认测试全绿
2. 重新执行 meta-review，确认 0 Critical + 0 Important
3. 最多 **2 轮**修复循环。2 轮后仍有问题 → 报告残余问题，标记 needs-info

### 完成后

向用户报告 Phase 1 和 Phase 2 的完整结果：处理了多少 issue、总轮次、最终状态、meta-review 发现和修复了哪些问题。

### FINAL_ACCEPTANCE_REPORT

meta-review 完成后，产出跨 issue Suggestion 验收报告，供人类签收。

#### 1. 聚合 Suggestions

扫描所有 feature 目录的 `suggestions.json`，汇总所有条目：

- 用 `glob` 扫描 `.scratch/*/suggestions.json`，读取每个文件
- 将每个条目合并到统一列表中，保留来源 feature 信息

**GitHub Issue 模式附加聚合**：

当 Phase 1 处理过 GitHub issue 时，从 issue comments 中提取 suggestions，与本地 `suggestions.json` 合并：

1. 对每个处理过的 GitHub issue，用 `gh issue view <N> --json comments` 读取所有 comments
2. 筛选格式为 `autopilot suggestion [<status>]: <正文>` 的 comments
3. 对每条提取：`status`（从 `[<status>]` 块）、`content`（`:` 后的正文）、`source_issue`（`#<N>`）
4. 与本地 `suggestions.json` 条目按 `content` 去重合并（本地优先：本地已有相同 content 的条目保留本地版本及完整字段）

#### 2. 分组统计

按 `status` 字段分组：

| 分组 | 内容 | 来源 |
|------|------|------|
| **Pending** | `status: "pending"` 的所有条目 | 列出 `content`、`source_issue`、`keywords`；如有 `deferred_by`，注明 |
| **Rejected** | `status: "rejected"` 的所有条目 | 列出 `content`、`source_issue`、`rejected_reason` |
| **Resolved** | `status: "resolved"` 的所有条目 | 列出 `content`、`resolved_in_issue`、原 `source_issue` |

#### 3. 输出 FINAL_ACCEPTANCE_REPORT

以 `FINAL_ACCEPTANCE_REPORT:` 为标记头输出结构化报告：

```
FINAL_ACCEPTANCE_REPORT:

## Pending（需处理）
- <content>
  - 来源: <source_issue>
  - 关键词: <keywords>
  - [deferred by: <issue-slug>]
...（如无 pending，写 "无"）

## Rejected（已拒绝）
- <content>
  - 来源: <source_issue>
  - 理由: <rejected_reason>
...（如无 rejected，写 "无"）

## Resolved（已解决）
- <content>
  - 来源: <source_issue>
  - 由 <resolved_in_issue> 处理
...（如无 resolved，写 "无"）
```

#### 4. 边界处理

- `suggestions.json` 不存在（glob 无结果）→ 报告 "No suggestions.json found. Skipping acceptance report."（**不影响 meta-review 流程**）
- 存在但无 pending → 报告 "All suggestions resolved. Ready for sign-off."
- 有 pending → 报告 "The following suggestions require human attention:" + 逐条列出 + 建议人工判断处理方向（落实为后续 issue 或标记 rejected）
- 仅 GitHub issue comments 中有 suggestions 而本地无 `suggestions.json` → 以 comments 聚合结果为准，仍输出完整报告

#### 5. Self-Verification

FINAL_ACCEPTANCE_REPORT 输出后，orchestrator 执行以下快速自检：

- [ ] `suggestions.json` 中的每条 `status: "resolved"` 条目均有 `resolved_in_issue` 字段
- [ ] `suggestions.json` 中的每条 `status: "rejected"` 条目均有 `rejected_reason` 字段
- [ ] 无 `status: "pending"` 条目被意外标记为 `resolved_in_issue`（仅 resolved 应有此字段）
- [ ] FINAL_ACCEPTANCE_REPORT 的 Pending / Rejected / Resolved 三组条目数之和 = `suggestions.json` 总条目数（去重后）
- [ ] 无空 `content` 字段的条目
- [ ] 发现异常 → 记录到报告末尾的 `## Self-Verification Issues` 节，人工跟进
