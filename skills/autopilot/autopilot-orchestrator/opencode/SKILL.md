---
name: autopilot-orchestrator
description: OpenCode autopilot loop using toolkit-owned implementer and reviewer agents. Use for AFK implementation of ready local, remote-mirrored, or GitHub issues.
---

Before anything else, read `~/.agents/principles/karpathy.md`. Apply Principle 1 "Think Before Analyzing" variant + Principles 2 and 4.

Execute the autopilot orchestrator workflow below. **Orchestrator MUST include explicit `skill` tool loading instructions in implementer and reviewer dispatch prompts** — see "执行 implementer" and reviewer dispatch sections for the exact preamble format.

## Issue 来源识别

autopilot 支持 GitHub issue 和本地 `.scratch` issue。本地 issue 可以声明远端 tracker，同时保留为本地执行合约。一次运行首次遇到这类 issue 时，读取 canonical source 中的 `../references/tracker-backends.md`，只解析一次项目配置和 backend contract，之后在所有生命周期边界调用其 hooks。若当前 skill 是 OpenCode managed wrapper，先读取同目录 `.autopilot-toolkit-source` 获取 canonical source 路径。

| target 特征 | 来源 | 状态机 | 合约文件 |
|---|---|---|---|
| 包含 `/` 的路径 | 本地 `.scratch/` | frontmatter `status:`/`Status:` | issue 文件或 `AGENT-BRIEF.md` |
| `#N` 或纯数字 `N` | GitHub Issue | labels | issue body（含 AC） |
| 无参数扫描到本地 | 本地 `.scratch/` | frontmatter `status:`/`Status:` | issue 文件或 `AGENT-BRIEF.md` |
| 无参数扫描到 GitHub | GitHub Issue | labels | issue body |

## 前置约定

### 本地 issue 模式

- 接受 Markdown issue 文件，或包含 `issue.md` 的 issue 目录。如传入相对路径，拼接当前工作目录。
- 大小写不敏感地读取 YAML frontmatter 中的 `status:`。
- 同目录存在 `AGENT-BRIEF.md` 时用它作为合约；否则从 issue 文件的 `## What to build` 和 `## Acceptance Criteria` 读取合约。
- 普通本地 issue：直接更新 frontmatter 状态，并在 `## Comments` 追加 `- <时间戳> autopilot: <内容>`；该节不存在时创建。
- 远端 tracker 镜像：通过 `tracker`、`remote-id`、`remote-type` 识别，并使用已解析的 backend lifecycle hooks。不要单独修改镜像状态或重复写评论；backend 同时更新两端。
- backend hook 失败即生命周期屏障失败。开始失败时不得 dispatch，完成失败时不得进入下一 issue；禁止稍后补写。

### GitHub Issue 模式

- 使用 `gh` CLI 操作 issue。从 `git remote -v` 自动推断 repo。
- 状态通过 labels 表达：`in-progress`、`resolved`、`needs-info`。
- 追加注释用 `gh issue comment <N> --body "..."`。
- 合约来自 issue body（其中包含 Acceptance Criteria 和 What to build，由 `to-tickets` 创建）。
- 读取 issue：`gh issue view <N> --json number,title,body,labels,state`。

### 共用概念

- `Status: ready-for-agent`（本地 frontmatter）↔ label `ready-for-agent`（GitHub）
- `Status: in-progress` ↔ label `in-progress`
- `Status: resolved` ↔ label `resolved`
- `Status: needs-info` ↔ label `needs-info`

### AFK Continuation Contract

本 contract 继承 wayfinder 的 HITL/AFK 分类语义：AFK ticket 由 agent 独立驱动，
不依赖人类实时参与。

破坏 AFK 的行为：在不需要产品/范围决策时暂停等待人类输入。

**Principles**

1. **Autonomous Progress** — Agent 自主推进，仅在合约无法覆盖的产品/范围决策时
   请求人类输入。工程歧义通过判断解决，不阻塞流程。

2. **Contract Authority** — AGENT-BRIEF（或等价的 issue body AC）是 AFK 执行的
   唯一权威合约。Agent 不超出合约范围自行扩展，也不在合约未要求时自行降级。

3. **Transparent Completion** — 外部限制（权限、工具链、依赖不可用）必须明确记录。
   已完成但未验证的实现声明为 UNVERIFIED，绝不伪报 DONE。

4. **Authority Boundaries** — Agent 不升级权限、不覆盖父 agent 配置。
   子 agent 继承父 agent 的执行边界和模型选择。

5. **Evidence Integrity** — 验证证据可审计、可复现。代码和环境未变化时已收集的
   证据可复用，变化后失效。

6. **Continuation** — 任务完成或阻塞后，Agent 保留上下文并继续推进范围内下一个
   可执行目标。

**Execution Specification**

- 优先以 AGENT-BRIEF 为权威合约；issue body 作为上下文补充，不覆盖合约条款。
- 权限、网络、缺失工具链等外部失败记录 `BLOCKER_TYPE: external-unavailable`，
  在 `TEST_EVIDENCE` 中写明未执行命令和原因。
- 仅合约无法覆盖的产品/范围歧义记录 `BLOCKER_TYPE: human-decision` 和
  `needs-info`。工程可判断的歧义不升级为 human-decision。
- `TEST_EVIDENCE` 以 `command + WORK_BASE + worktree fingerprint` 缓存；
  工作树未变化时复用，代码变化后失效。
- 诊断流程在 AFK 模式下不阻塞等待用户——继续按优先级测试假设。
- 扫描模式延后当前任务后返回 scanning frontier 中下一个 runnable issue；
  显式 target 模式结束本轮。

---

## Recovery Decision Model

失败时基于两个维度决定行为，而非计数：

### 决策矩阵

|              | 有新信息（evidence improved）    | 无新信息（no new evidence） |
|--------------|--------------------------------|---------------------------|
| Agent 可修复  | RETRY                          | 停止（空转）               |
| Agent 不可修复 | UNVERIFIED                     | needs-info / exhausted     |

- **Recoverability**（Agent 可修复？）：工程 bug、测试失败、命名/结构错误 → 可修复；工具链缺失、权限不足、合约歧义、方向性错误 → 不可修复
- **Evidence Progress**（有新信息？）：本轮尝试了不同策略/缩小了问题范围 → 有新信息；同一错误、同一推理路径反复出现 → 无新信息

### Failure Classification

| 失败类型 | 决策 | BLOCKER_TYPE |
|---------|------|-------------|
| Recoverable + 有新信息 | RETRY，继续迭代 | — |
| Recoverable + 无新信息 | 停止，空转 | exhausted |
| Environment（工具链/权限/外部服务不可达） | UNVERIFIED | external-unavailable |
| Authority（合约歧义、产品/范围决策缺失） | needs-info | human-decision |
| Terminal（系统性失败、兜底触发） | 停止 | exhausted |

### 防作弊机制

Agent 不自行判断是否停止。兜底前裁量权仅由 orchestrator 在 cap 触发后行使。

#### Rationalization Table

orchestrator 裁量前对照下表，排除偷懒判断：

| Agent 可能的停止理由 | 必须满足的证据条件 |
|---|---|
| "无进展" | PREV_REVIEW 与当前 REVIEWER_REPORT 的 Critical 列表完全一致 + implementer CHANGED_FILES 与上一轮无变化 |
| "无法修复" | 已尝试 2+ 种不同实现策略 + reviewer 确认均不满足 AC |
| "合约缺失" | reviewer 报告中明确标注 "AC 描述不足以判断正确性" 或 "缺少必要的产品决策" |

#### 空转检测 (Stall Detection)

连续 2 轮满足以下全部条件 → 触发 `BLOCKER_TYPE: exhausted`，记录 reviewer 问题清单并停止：

1. PREV_REVIEW 与当前 REVIEWER_REPORT 的 Critical 列表完全一致（条目内容、文件路径均相同）
2. implementer CHANGED_FILES 与上一轮无变化（文件增删改计数一致）
3. implementer 未尝试新的实现策略（SUMMARY 或 SELF_REVIEW 中无明确策略切换描述）

迭代终止规则：
- 任一 reviewer 报告含 Authority 类（合约缺失）Critical/Important → 转 `needs-info`，不继续迭代
- 空转检测触发 → `exhausted`，停止
- 符合决策矩阵 "Recoverable + 有新信息" → 继续迭代
- 符合决策矩阵 "Recoverable + 无新信息" → `exhausted`
- 符合决策矩阵 "Agent 不可修复" → `needs-info` 或 `external-unavailable`

---

## Spec 检测与跳过

Spec 只描述整体设计，不直接派发给 implementer。满足任一条件即视为 spec：

- 本地 frontmatter 含 `type: spec` 或兼容标记 `type: prd`（大小写不敏感），或 GitHub label 含 `spec` 或兼容标记 `prd`。
- 正文同时包含 `## Problem Statement` 和 `## Solution`，且不包含 `## What to build` / `## Acceptance Criteria`。

Phase 1 遇到 spec 时记录为 skipped，报告 `<id> is a spec, not directly implementable. Process its child tickets instead.`，不修改其状态；Phase 2 再按“spec 收尾”处理。

---

## 如果指定了 target

### target 是路径（含 `/`）

1. 解析 `issue_file`：target 本身是 `.md` 时直接使用，否则使用 `<target>/issue.md`；不存在则报告错误并停止
2. 解析 `contract`：同目录存在 `AGENT-BRIEF.md` 时使用它，否则解析 `issue_file` 的合约章节
3. 读取 `issue_file`，大小写不敏感地检查 `status:` 是否为 `ready-for-agent` 或 `in-progress`
4. 非以上状态 → 回复当前状态并停止
5. 运行 spec 检测；命中则记录为 skipped 并停止直接派发
6. 远端 tracker 镜像：运行 backend `before_dispatch` hook，并把返回的远端评论加入 implementer 上下文；普通本地 issue：更新状态并追加开始评论
7. 仅步骤 6 成功后继续；设置 `source = "local"`, `id = <issue_file>`
8. 从 issue 文件推断 feature 目录（如 `.scratch/auth/issues/01-login.md` → `.scratch/auth/`）
9. 设置 `contract = <解析出的合约文本>`
10. 跳到"交叉 Issue Suggestion 匹配"

### target 是 GitHub issue 号（`#N` 或纯数字 `N`）

提取数字部分为 `issueNumber`：

1. `gh issue view <issueNumber> --json number,title,body,labels,state` 获取 issue 信息
2. 检查 labels 是否含 `ready-for-agent` 或 `in-progress`
3. 非以上标签 → 回复当前状态并停止
4. 运行 spec 检测；命中则记录为 skipped 并停止直接派发
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

1. Glob 扫描 `.scratch/*/issues/*.md`
2. 对每个文件，读取前 30 行，大小写不敏感地检查是否有 `status: ready-for-agent`
3. 对候选运行 spec 检测；从派发队列排除 spec，并记录为 skipped
4. 解析精确 `## Blocked by` 段；仅 `None` 或全部引用 sibling 均为 `status: resolved` 的候选进入 frontier，其余报告为 dependency-blocked

### GitHub 扫描

4. `gh issue list --label "ready-for-agent" --state open --json number,title --limit 50`
5. 排除 `spec` 或兼容标记 `prd` label，并对其余候选运行正文 spec 检测；解析 native blockers 或 `## Blocked by` 引用，仅全部 resolved 的候选进入 frontier

### 选择并报告

6. 合并两个来源已过滤的 frontier。分别列出 runnable、dependency-blocked 和 skipped spec
7. 选择第一个（按先本地后 GitHub，各自内部按自然序），标注正在处理哪个
8. 如果 frontier 为空但仍有 dependency-blocked 候选，输出带 `BLOCKER_TYPE: dependency` 的 stalled/deferred 汇总并结束扫描，不进入 Phase 2。只有整个 ready queue 为空时才进入 Phase 2
9. 根据选中 issue 的来源，走对应的初始化流程

---

## Phase 1: 调度循环

每个候选任务先按 `## Blocked by` 计算依赖 **frontier**：只有所有引用 ticket
均为 `status: resolved` 才可执行。调用 implementer 前保存
`WORK_BASE = git rev-parse HEAD`。Reviewer 阶段必须读取
`references/upstream-code-review.md`，并行执行 Standards + Spec 的 upstream
code-review，将 `UPSTREAM_REVIEW_REPORT` 传给主 reviewer。

本地调度统一传入 `issue_file`（规范路径
`.scratch/<feature>/issues/<NN-slug>.md`）和解析出的 `contract`；目录式 issue
只作为旧格式回退。

维护 `retry_count = 0` 用于轮次追踪和 suggestion 匹配。不再有硬性轮次上限——迭代终止由决策矩阵 + 空转检测共同决定：
- retry_count = 0: 首次实现

### 更新状态（抽象）

- **普通 local**: 修改 issue 文件 frontmatter 的 `status:`/`Status:` 行
- **远端 tracker 镜像**: 使用匹配的 backend lifecycle hook；禁止独立修改镜像状态
- **github**: `gh issue edit <N> --add-label "<新>" --remove-label "<旧>"`

远端 tracker 镜像需要进入 `needs-info` 时必须调用 backend `block` hook；阻塞解除后重新派发前调用 `resume`。不得通过直接修改 Markdown 模拟这两个事件。

### 追加注释（抽象）

- **普通 local**: 在 issue 文件的 `## Comments` 节末尾添加 `- <时间戳> autopilot: <内容>`
- **远端 tracker 镜像**: 调用 backend `material_update` hook
- **github**: `gh issue comment <N> --body "<时间戳> autopilot: <内容>"`

### 交叉 Issue Suggestion 匹配

dispatch implementer 前，扫描 `suggestions.json`，匹配 pending suggestions 到当前 issue 的 AGENT-BRIEF：

#### 推断 feature 目录

- **本地模式**：从 flat `issue_file` 提取（如 `.scratch/auth/issues/01-login.md` → `.scratch/auth/`）
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

用 `task` 工具 dispatch `autopilot-implementer` agent（subagent_type: `autopilot-implementer`）。**prompt 必须以 skill 加载指令开头（强制，不可省略）**：

```
**在开始任何操作之前，必须使用 `skill` 工具加载以下技能：**
1. `skill(name: "tdd")` — TDD 方法论（红绿重构循环、测试质量标准、mock 纪律）
2. `skill(name: "diagnosing-bugs")` — 系统性诊断流程（遇到意外错误时使用）
3. `skill(name: "zoom-out")` — 不熟悉代码区域时上探抽象层次

**这是强制步骤，不可跳过。** 未加载技能前不得执行任何其他操作。

---

<以下为任务描述>

<根据 retry_count 和模式动态生成>
```

任务描述部分传递：
- **共同的**：`source`, `id`, `contract`（合约内容）, `TOOLCHAIN: <available|unavailable>`, `REFACTORING: <true|false>`，以及：
  - 本地模式：绝对 `issue_file` 路径（不是 issue 目录）
  - 首次（retry_count = 0）：`ROUND: 0`
  - retry（retry_count >= 1）：`ROUND: <retry_count>` + `PREV_REVIEW: <上一轮 REVIEWER_REPORT 全文>`
  - 如有匹配到的 CROSS_ISSUE_SUGGESTIONS，一并传入
- **本地模式**：额外传 issue 文件绝对路径；如使用目录式 issue，也传目录路径
- **GitHub 模式**：额外传 issue body（含 AC）+ `IS_GITHUB: true`

等待 implementer 回复，解析 `IMPLEMENTER_REPORT:`。

**空回复处理：** 如果 implementer 返回空结果（无 `IMPLEMENTER_REPORT:` 标记头），自动重试 1 次（重新 dispatch 相同 prompt）。两次都空 → 记录 `BLOCKER_TYPE: external-unavailable`，延后当前 issue，返回扫描模式。

**解析容错：** 回复中找不到 `IMPLEMENTER_REPORT:` 标记头 → 先重试一次格式修复；再次不可解析则记录 `BLOCKER_TYPE: contract` 并附原始回复，延后当前 issue，返回扫描模式。

### 首次实现：检查 SELF_REVIEW

retry_count = 0 时，检查报告中有无 `SELF_REVIEW:` 段：

- STATUS: DONE → "无问题" 或 "发现问题 → 已修复" → 通过
- STATUS: UNVERIFIED → 必须包含每条 AC 的验证方式标注（测试运行 / 代码结构分析）。**标注缺失但 STATUS: UNVERIFIED → 通过**（UNVERIFIED 本身已声明验证不全）
- STATUS: DONE 或 UNVERIFIED 但缺失 SELF_REVIEW 段 → 重试一次格式修复；再次缺失则记录 `BLOCKER_TYPE: contract`，延后并返回扫描模式

Retry 轮次（retry_count >= 1）不检查 SELF_REVIEW。

### 收集 SIBLING_CONTEXT

dispatch reviewer 前，自动收集当前 issue 所属 spec 下所有已 resolved 的兄弟模块信息：

1. 从当前 issue body 的 `Parent` 链接提取 spec issue 号
2. `gh issue list --label "resolved" --json number,title` 获取所有已 resolve 的 issue
3. 对于每个已 resolve 的 issue（排除当前 issue 自己），提取其 title 和关键约定（入口模式、测试框架、文件布局）
4. 组装为 `SIBLING_CONTEXT` 字符串，包含："已完成的兄弟模块: #N title — 关键约定: ..."

### 处理 implementer 结果

- **STATUS: DONE** → dispatch `autopilot-reviewer` agent（subagent_type: `autopilot-reviewer`）。**prompt 必须以 skill 加载指令开头（强制，不可省略）**：

```
**在开始任何操作之前，必须使用 `skill` 工具加载以下技能：**
1. `skill(name: "tdd")` — 测试质量标准和 mock 纪律（用于 TDD 审查维度）

**这是强制步骤，不可跳过。** 未加载技能前不得执行任何其他操作。

---

<以下为任务描述>
```

任务描述部分传递 `source`, `id`, `contract`, `CHANGED_FILES`, `SIBLING_CONTEXT`, `UPSTREAM_REVIEW_REPORT`（含 Standards + Spec）+ 上一轮 `REVIEWER_REPORT`（如有）
  - **GitHub 模式**：额外传 `IS_GITHUB: true`

- **STATUS: UNVERIFIED** → dispatch `autopilot-reviewer` agent（同上 prompt 格式）。任务描述中额外传递 `UNVERIFIED: true` + implementer 的完整 `SELF_REVIEW` 段（含逐 AC 验证方式标注）。reviewer 的审查侧重：
  - 结构正确性（代码逻辑是否符合 AC）
  - 是否所有 AC 都有对应的代码实现
  - VERDICT 可选 `VERIFY_NEEDED`（结构通过但需工具链验证）或 `RETRY`（结构本身有问题）

- **STATUS: NEEDS_CONTEXT** → 记录 `BLOCKER_TYPE: human-decision`，更新为 `needs-info` 并写明缺失决策，延后当前 issue
- **STATUS: BLOCKED** → 外部能力失败记录 `BLOCKER_TYPE: external-unavailable`；诊断耗尽记录 `BLOCKER_TYPE: exhausted`。延后并返回扫描模式

远端 tracker 镜像在 STATUS 为 DONE 或 UNVERIFIED、dispatch reviewer 之前，通过 `material_update` 写一条简洁的实质进展：实现轮次、状态、SUMMARY，以及即将进入 review。不要同步逐工具日志。

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

解析 `REVIEWER_REPORT:`，看 VERDICT。reviewer 任务失败或找不到 `VERDICT:` → 自动重试一次；再次失败则记录 `BLOCKER_TYPE: external-unavailable`，延后并返回扫描模式。

**解析容错：** 找不到 `REVIEWER_REPORT:` 标记头 → 自动重试一次；再次不可解析则记录 `BLOCKER_TYPE: external-unavailable` 并附原始回复，延后并返回扫描模式。

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
   - 本地模式（`source = "local"`）：从 flat `issue_file` 提取，如 `.scratch/auth/issues/01-login.md` → `.scratch/auth/`
   - GitHub 模式（`source = "github"`）：从 issue title 生成 feature slug，创建 `.scratch/<feature-slug>/`
4. **读取现有文件**：检查 `.scratch/<feature>/suggestions.json` 是否存在，存在则读取，不存在则初始化为空数组 `[]`
5. **去重**：按 `content` 字段比较，已存在相同 `content` 的条目不重复写入
6. **追加新条目**：每个新条目格式为：
   ```json
   { "issue": "<issue-slug>", "round": <N>, "content": "...", "files": [...], "keywords": [...], "status": "pending" }
   ```
   - `issue`：本地模式用文件 stem（如 `01-login`），GitHub 模式用 `#<N>`
   - `round`：当前 `retry_count`
7. **写入文件**：将更新后的数组写回 `.scratch/<feature>/suggestions.json`（`write` 工具）
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

- **MERGE** → 获取或复用 `TEST_EVIDENCE`。真实代码测试失败进入 RETRY；执行能力不可用则记录 `BLOCKER_TYPE: external-unavailable` 和 `UNVERIFIED` 证据，保持未完成并返回扫描模式，绝不请示提权。远端 tracker 镜像验证通过后，调用 backend `complete` hook；普通 local/GitHub 更新 `resolved`。
- **VERIFY_NEEDED** → 审查通过（结构正确）但 implementer 工具链不可用，无法实际验证。处理流程：
  1. 尝试运行项目的测试命令（如 `cargo test`、`npm test`、`pytest`）。如工具链在 orchestrator 环境可用 → 运行验证
  2. 验证通过 → 远端 tracker 镜像通过同一个 backend `complete` 屏障完成，并以测试命令作为 evidence；其他来源更新 Status 为 `resolved`，追加 "Orchestrator verified: all tests pass"
  3. 真实代码测试失败 → 进入 RETRY；工具链仍不可用 → 记录 `BLOCKER_TYPE: external-unavailable` 和 `UNVERIFIED` 证据，延后当前 issue 并返回扫描模式
  4. 所有情况下保留 reviewer 报告和 Suggestion 提取
- **RETRY** → 远端 tracker 镜像先通过 `material_update` 写一条简洁的 reviewer retry 摘要；然后 `retry_count += 1`，清空 `pending_resolutions = []`（上一轮 resolutions 在 retry 后失效，新轮次 implementer 需重新声明）。

  遵循 `## Recovery Decision Model` + 空转检测：
  - 合约缺失类 Critical/Important → 转 `needs-info`
  - 空转检测触发或决策矩阵判 "无新信息" → 记录 `BLOCKER_TYPE: exhausted`，追加 reviewer 问题清单，延后并返回扫描模式
  - 决策矩阵判 "有新信息 + 可修复" → 返回"执行 implementer"（传递 PREV_REVIEW）
- **BLOCKED** → 分类 reviewer 结论；只有 human decision 或 missing contract 转 `needs-info`，否则记录 `BLOCKER_TYPE: contract|exhausted`，延后并返回扫描模式

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
   | `resolved` | 标记为已解决 | `status: "resolved"`, `resolved_in_issue`: 当前 issue 的 slug（本地模式用文件 stem，GitHub 模式用 `#<N>`） |
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

对照 ADR、spec 和所有 issue 合约，审视整个 codebase 的：
- 实现正确性（所有模块是否符合各自的 AC 和 spec 全局约束）
- 跨模块一致性（是否有模式漂移、重复实现、约定不一致）
- 计划外变更（是否有孤儿文件、未声明依赖、残留引用）

### 执行方式

Orchestrator 自主审查与 reviewer 子 agent **并行**执行。两者均产出独立报告后，进入「报告合并」统一处理。

#### 1. 派遣 reviewer 子 agent（并行）

用 `task` 工具 dispatch `autopilot-reviewer` agent（`subagent_type: "autopilot-reviewer"`，只读，无 edit/bash 权限）。**prompt 必须以 skill 加载指令开头（强制，不可省略）**：

```
**在开始任何操作之前，必须使用 `skill` 工具加载以下技能：**
1. `skill(name: "tdd")` — 测试质量标准和 mock 纪律

**这是强制步骤，不可跳过。** 未加载技能前不得执行任何文件读取或审查操作。

---

你正在执行全局 meta-review。审查范围为整个 codebase，对照以下基准：

**审查基准（读取以下全文）：**
- 所有 ADR（docs/adr/）
- 所有 spec（如有）
- 所有已 resolved issue 的合约（AGENT-BRIEF.md 或 GitHub issue body 中的 AC）

**审查维度（适配 reviewer 五维框架到全局 meta-review 上下文）：**

1. **ADR/spec 全局约束验证**（维度四：计划忠实度）：
   - 逐条检查 ADR 和 spec 中声明的全局约束（输出格式要求、依赖白名单、运行时约束、目录结构约定等）是否在所有模块中满足
   - 是否存在约束降级（如 spec 要求 byte-identical 但实现仅做到结构等价）
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

1. 读取 spec 全文和所有相关 ADR（包含 ADR 0003、ADR 0004 等），列出每条全局约束
2. 逐条检查：用 grep/glob 扫描 codebase，验证约束满足
3. 对照 issue 合约，检查每个 resolved issue 的 AC 覆盖率
4. 检查跨模块一致性（入口检测方式、import 风格、错误处理、日志格式、算法选择、文件布局）
5. 检查计划外变更（孤儿文件、未声明新行为、副作用、未删除文件）
6. 输出结构化报告：Critical / Important / Suggestion + VERDICT

#### 3. 等待两份报告

上述 1、2 两步并行执行。两者均完成后（均产出独立报告），进入下方「报告合并」流程。

### 报告合并

`执行方式` 产生两份独立的 meta-review 报告：
- **orchestrator 自主审查报告** — 对照 ADR、spec 和 issue 合约逐条检查
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
1. 运行 Phase 1 已推断的项目测试命令（如 `cargo test`、`npm test`、`pytest`），确认测试全绿
2. 重新执行 meta-review，确认 0 Critical + 0 Important
3. 最多 **2 轮**修复循环。2 轮后仍有问题 → 以 `BLOCKER_TYPE: exhausted` 报告残余问题；仅明确 human decision 时标记 needs-info

### 完成后

向用户报告 Phase 1 和 Phase 2 的完整结果：处理了多少 issue、总轮次、最终状态、meta-review 发现和修复了哪些问题。

### spec 收尾

1. 收集扫描时跳过的 spec 和显式指定的 spec，再查找其全部子 ticket。
2. 仅当全部子 ticket 为 `resolved` 且 meta-review 通过时继续。
3. 远端 tracker 镜像调用 backend `close_phase` hook；该 hook 负责解析阶段文档和缺失元数据，不得退化成普通本地写入，也不得推进根 spec。
4. 不得仅因代码和测试完成就调用 `release_phase`。阶段真实上线后才释放阶段；总需求收尾必须另行调用 backend `verify_root` hook 检查全部阶段，不能由单个阶段隐式触发。
5. 普通本地和 GitHub 来源保留原行为：全部子项完成后按原 tracker 规则把 spec 标记为 `resolved` 并追加总结。存在未完成子项时保持父项现状并报告列表。

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
