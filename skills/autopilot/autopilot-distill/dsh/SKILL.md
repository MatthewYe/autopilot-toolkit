---
name: autopilot-distill
description: "Run the Distill requirement-to-issues workflow through the installed toolkit CLI."
---

使用已安装的 Distill CLI：`~/.agents/skills/.autopilot/bin/distill`。

若 `~/.agents/skills/.autopilot/distill.env` 存在，读取它并用其中的 `AUTOPILOT_DISTILL_BIN` 作为可执行文件路径；否则使用上面的稳定路径。

本 skill 可由主 agent 用 `skill` 工具按名加载，也可由用户在聊天输入 `/autopilot-distill` 触发。

## 当前 DSH 会话身份（Current DSH Session Identity）

Distill 是 session-bound 的。在 start、resume、submit-evidence、inspect 或 takeover 之前，必须先为当前 DSH 会话取得**唯一无歧义**的运行时原生身份，并作为 `--session-id "<dsh-session-id>"` 传入。

DSH 会话持久化在 `~/.dsh/sessions/<workspace-key>/<session-id>/`：

- `workspace-key` 由当前工作区的**绝对路径**导出：去掉开头的 `/`，把其余 `/` 全部替换为 `-`，再前后各加 `--`。例：`/Users/you/proj` → `--Users-you-proj--`。
- `<session-id>` 是会话目录名（形如 `session-<uuid>` 或 `<uuid>`），目录内的 trace 文件是 `session.v4.jsonl.zstd`——JSONL 事件流，zstd 压缩。
- 每个事件是一行 JSON，带 `type`、`seq`、`time` 字段；`session` 事件还带 `id`、`cwd`、`delegationDepth` 等顶层字段，其余事件的数据在 `data` 里。

取得身份的步骤：

1. 用 `bash` 运行 `pwd -P` 解析当前工作区的物理路径，按上面的规则得到 `workspace-key`。
2. 用 `glob` 列出 `~/.dsh/sessions/<workspace-key>/*/session*.jsonl*` 定位候选会话 trace。
3. **先采样再分析**：先用 `bash` 运行 `zstd -dc <file> | head -n 5` 解压并查看前几条事件，确认当前 DSH 版本实际写出的字段；不要凭猜测使用字段名。
4. 用 `bash` 配合 `zstd -dc <file> | grep -n <pattern>`（或逐行解析）筛选候选，只保留能证明它是当前会话的：其 `session` 事件的 `cwd` 与当前项目一致，且 trace 中包含本次 `/autopilot-distill` 调用与当前 requirement 文本（或本次调用独有的标记）。不要只按文件修改时间或目录里的"最新一个"判断。
5. 若恰好剩下一个候选，用其会话目录名（或该会话 `session` 事件的 `id`）作为 `--session-id`。
6. 若没有任何候选匹配、匹配到多个、或当前 DSH 会话身份不可用/有歧义，**fail closed**：报告 `Distill cannot start because the current DSH session identity is unavailable or ambiguous.`，不要靠 recency、标题、当前目录本身、会话目录里最新的文件、用户提供的非运行时标识或调用方注入的环境变量来猜测。

不要把用户提供的会话 ID、prompt 里写的 ID 或其他自定义 token 当作 Distill 的 `--session-id`；这个 ID 必须来自 DSH 自己写入的会话 trace。

## Start Or Resume

给定明确文本 requirement 时，从目标项目 worktree 启动或恢复 run：

```bash
"${AUTOPILOT_DISTILL_BIN:-$HOME/.agents/skills/.autopilot/bin/distill}" start --json --runtime dsh --session-id "<dsh-session-id>" --worktree "$PWD" --requirement "<explicit requirement text>"
```

运行时提交的 intake 则传 DSH 捕获的 intake JSON，而不是纯文本：

```bash
"${AUTOPILOT_DISTILL_BIN:-$HOME/.agents/skills/.autopilot/bin/distill}" start --json --runtime dsh --session-id "<dsh-session-id>" --worktree "$PWD" --intake-json '<intake-json>'
```

CLI 返回 JSON 时，检查 `run_id`、`stage`、`revision`、`next_action`、`authorized_action`。同会话 resume 返回同一个未完成 run。不同 DSH 会话不得推进它；跨会话拒绝由 runner 通过共享的 `--session-id` 契约强制执行。一个 DSH 会话最多拥有一个未完成 run；如果 runner 报告多个未完成 run 或重复的 session 绑定，停止并报告该状态，不要替它选一个。

## Run To Boundary

一直推进到 runner 给出 terminal、waiting、blocked 或 needs-reconciliation 状态。每次向用户让出控制权时都必须包含 `run_id`、`stage`、`revision`、`next_action`，以及要求的下一条命令或执行者。

只调用 `authorized_action` 指定的执行者。不要跳阶段、不要重排阶段、不要为返回的 `stage` 之外的阶段提交 evidence。

- `next_action` 为 `terminal` → 报告完成报告路径与会话释放状态。
- runner 状态为 `blocked` → 报告 blocked 原因与所需恢复动作；恢复后用同一 DSH 会话身份与返回的 `revision` 继续。
- runner 状态为 `needs-reconciliation` → 不要盲目重试 publication；按 runner 的 reconciliation 指示做，核实外部 tracker 状态，再用返回的 `revision` 继续。

## Authorized Executors

`authorized_action.skill` 指向某个 skill 时，用 **`skill` 工具按名加载**该 skill 并原样执行（DSH 主 agent 可以按名加载 catalog 中的 skill）；若需要确认其内文，也可用 `read` 读取 `~/.agents/skills/<skill-name>/SKILL.md`。不要修改这些 skill 的正文。

若 `authorized_action.skill` 是 `grill-with-docs`，对捕获的 requirement 调用未修改的 `grill-with-docs` skill。该阶段信息足够后，带上保留的 checkpoint 提交完成证据：

```bash
"${AUTOPILOT_DISTILL_BIN:-$HOME/.agents/skills/.autopilot/bin/distill}" submit-evidence --json --worktree "$PWD" --run-id "<run_id>" --session-id "<dsh-session-id>" --expected-revision "<revision>" --stage clarification --evidence '{"checkpoint":"clarification-complete","summary":"<clarification summary>","clarified_requirement":"<complete clarified requirement>","decisions":[],"accepted_assumptions":[],"material_unknowns":[],"domain_document_artifacts":[]}'
```

若 `authorized_action.skill` 是 `to-spec`，调用未修改的 `to-spec` skill。提交精确的已接受 PRD markdown 与 testing-seam checkpoint：

```bash
"${AUTOPILOT_DISTILL_BIN:-$HOME/.agents/skills/.autopilot/bin/distill}" submit-evidence --json --worktree "$PWD" --run-id "<run_id>" --session-id "<dsh-session-id>" --expected-revision "<revision>" --stage prd --evidence '{"checkpoint":"testing-seam-confirmed","summary":"<PRD summary>","feature_slug":"<stable-lowercase-feature-slug>","prd_markdown":"<exact PRD markdown>"}'
```

若 `authorized_action.skill` 是 `to-tickets`，调用未修改的 `to-tickets` skill。提交精确的已接受 implementation issue payload 与 approval checkpoint：

```bash
"${AUTOPILOT_DISTILL_BIN:-$HOME/.agents/skills/.autopilot/bin/distill}" submit-evidence --json --worktree "$PWD" --run-id "<run_id>" --session-id "<dsh-session-id>" --expected-revision "<revision>" --stage issues --evidence '{"checkpoint":"slice-breakdown-approved","summary":"<issue slicing summary>","issues":[{"title":"<issue title>","body":"<exact issue markdown>"}]}'
```

## LOCAL_ISSUE_HANDOFF_CONTRACT

当 `docs/agents/issue-tracker.md` 配置为 Local Markdown 时，为 PRD evidence 选择一个稳定的 lowercase `feature_slug`，用 `to-tickets` 起草并批准 vertical slices，但在其 tracker-publication 步骤之前停下。Distill runner 是唯一的本地发布者：它在 `.scratch/<feature_slug>/PRD.md` 创建 PRD，并在 `.scratch/<feature_slug>/issues/` 下一次性创建每个本地 issue。不要在其他任何 `.scratch/` 位置再创建 issue 副本。runner 会拒绝已存在不同内容的目标路径；绝不把这种冲突当作无关紧要的漂移。

`submit-evidence` 之前，确保每个本地 issue 的 `body` 以 agent-ready triage frontmatter 开头：

```markdown
---
Status: ready-for-agent
---
```

包含该 frontmatter 的精确 Markdown 就是冻结后的 issue payload。当配置的 tracker 是 GitHub 时，保留下面的外部发布与 receipt 流程。

每次 `submit-evidence` 响应之后，把返回的 `revision` 作为下一次的 `--expected-revision`。

Clarification 完成是 agent 的声明，不是用户 checkpoint。显式填充每个结构化字段。每个 material unknown 必须包含 `description`、`material`、`resolved`，以及已解决时的 `resolution`；仍有未解决的 material unknown 时不得完成。对 clarification 改动过的每个 glossary、domain document 或 ADR，在 `domain_document_artifacts` 中包含其 worktree-relative `path` 与 SHA-256。

当配置的 tracker 是 GitHub 时，`to-spec` 与 `to-tickets` 负责外部创建。把已确认的 receipt 作为 PRD evidence 或每个 issue 对象的 `external_publication` 传入。receipt 必须包含 `tracker: "github"`、配置的 `repository`、稳定的 `operation_id`（`<run_id>-r<revision>-prd` 或 `<run_id>-r<revision>-issue-<two-digit-index>`）、精确冻结 Markdown payload 的 SHA-256、`status: "confirmed"`、作为 `artifact_id` 的正数 issue 号，以及其规范 `artifact_url`。runner 保持离线，校验该 receipt，绝不回退到其他 tracker。如果响应是 `needs-reconciliation`，停止并遵循 `required_next_action`；不要调用其他 skill 或创建重复 issue。

## Takeover And Recovery

同会话 resume 是默认路径。如果之前的 DSH 会话已搁浅且用户显式授权 takeover，使用共享 runner 的 takeover 命令：

```bash
"${AUTOPILOT_DISTILL_BIN:-$HOME/.agents/skills/.autopilot/bin/distill}" takeover --json --worktree "$PWD" --run-id "<run_id>" --from-session "<old-dsh-session-id>" --to-session "<current-dsh-session-id>" --expected-revision "<revision>" --reason "<user-authorized reason>" --user-authorized
```

保留返回的 revision 并继续 run-to-boundary。不要用 takeover 绕过有歧义的当前 DSH 会话身份。不要直接改动 `.distill/` 状态。

只有在用户显式指示后，agent 才可以调用 `abort`、`purge` 或 `takeover`；必须传 `--user-authorized` 与返回的 expected revision。绝不从失败或用户结束对话中推断出这种授权。

## Sandbox（DSH）

- 文件沙箱通常为 `workspace-write`：只能修改当前工作区内的文件。
- `bash` 可能报 `Operation not permitted`；rust-script 改用 `bash scripts/sandboxed-rust-script.sh` 包一层执行。
- 修改 `crates/*` 之后、运行任何 rust-script 套件之前，先执行 `bash scripts/refresh-rs-cache.sh`。
- plan mode 下无法写文件，Distill 的本地发布阶段无法完成；开始前先退出 plan mode。

## Reporting

每次面向用户的让出都应包含：

- `run_id`
- `stage`
- `revision`
- `next_action`
- `authorized_action` 或 terminal 报告路径
- DSH 会话仍被绑定还是已释放

完成时，报告规范的 JSON 报告路径、渲染时的 Markdown 报告路径、已发布的 PRD 与 issue 引用，以及会话释放状态。
