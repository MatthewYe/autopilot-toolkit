---
name: autopilot-director
description: "Complete one whole spec as a single PR: a Director drives child tickets through code-run gates and a two-axis review gate, delegating code development to a role-pinned fast-model Worker. State transitions are owned by the director CLI. Use when the user asks to run a spec end to end."
---

执行一次 autopilot-director **Spec run**：驱动一个 spec issue 及其子 ticket 收敛到单个 Spec PR。每一次状态转换都必须经过 `director` CLI；CLI 无法记录的变化等于没有发生。本 skill 可由主 agent 用 `skill` 工具按名加载，也可由用户输入 `/autopilot-director` 触发。

词汇表见 `GLOSSARY.md`（"Autopilot Director"；不存在时回退旧版 `CONTEXT.md`）；背后的决策是 ADR 0046（role-pinned models）、ADR 0047（one Spec PR, one commit per ticket）、ADR 0048（two-axis gate, absolute-zero bar, escalation）。

## 参数

| 参数 | 默认值 | 含义 |
| --- | --- | --- |
| spec issue | 必填 | 要完成的 spec |
| worker model | `deepseek-flash` | Worker 派发时钉定的角色模型 |
| auto-merge | off | off：打开 Spec PR 后停下等人工批准；on：CI 绿且两层 gate 均归零后合并 |
| worktree | 当前检出 | run state 落在其 git-ignored 的 `.director/` 中 |

## 前置条件

1. `director --help` 可用。工具包已安装时，优先读取 `~/.agents/skills/.autopilot/director.env`（`AUTOPILOT_DIRECTOR_BIN`），回退到稳定路径 `~/.agents/skills/.autopilot/bin/director`；在源码检出中用 `cargo build --release -p director-cli` 构建。
2. `gh` 对 spec 所在仓库已认证。
3. 你在目标 worktree 中；绝不绕过 CLI 对 `.director/` 的 git-ignore 强制。

## 子代理 dispatch 模型（DSH）

使用 **`subagent` 工具**派发 Worker 与 reviewer。`subagent` 每次派发都是一个独立上下文的新代理，prompt 必须**完整自包含**（子代理看不到父会话历史，也不保证能访问父会话的 skill 清单）。

- **Worker** —— 每个 ticket 派发一个，派发时钉定角色模型：
  - dispatch prompt 第一段必须指示它加载技能正文并严格遵守：`用 read 工具读取 ~/.agents/skills/autopilot-director/runtime/default/references/worker-contract.md（源码检出中为 references/worker-contract.md），并遵循上游 tdd skill。`
  - 第二段是任务描述：ticket issue 引用、worktree、分支、该 ticket 的 `Seam:` 标注（或你给出的 `Seam(inferred)`），fix 轮次再加上待解决的 findings。
  - **模型钉定**：DSH 的 `subagent` 工具按 prompt 派发，不暴露独立的 model 参数；把 worker model 作为角色标签记录在 `director dispatch begin --ticket <n> --worker <label>` 与 dispatch prompt 声明中，绝不写入任何已提交的 agent 定义。
  - **文件指针兜底**：子代理有写权限。prompt 过长或担心载荷丢失时，先把 brief 写入文件，再发 `read <path> 并端到端执行其中任务`。若派发返回空载荷或缺少 `WORKER_REPORT:`，先用 `director dispatch finish --ticket <n> --outcome failed --reason "no payload"` 记录这次死掉的尝试，再通过一次 follow-up 重派；重派仍失败就自己完成这块工作，并在 run 报告中记录 fallback。不要把整个 run 耗在 spawn 路径上。
- **Reviewer** —— 每个 axis、每一轮各派发一个，均使用默认（lead）模型、各自全新上下文，每个只收到自己那个 axis 的 prompt：气味基线全文粘贴、standards 来源列全、spec 内容引用到位。绝不把 Worker 上下文传给 reviewer。
- 每条派发都在 prompt 第一段显式给出技能或契约文件路径——不要假设子代理能自行发现技能。

## 沙箱与 plan mode（DSH）

- 文件沙箱通常为 `workspace-write`：只能修改当前工作区内的文件。
- `bash` 可能报 `Operation not permitted`；rust-script 改用 `bash scripts/sandboxed-rust-script.sh` 包一层执行。
- 修改 `crates/*` 之后、运行任何 rust-script 套件之前，先执行 `bash scripts/refresh-rs-cache.sh`（rust-script 缓存不感知 path 依赖；`--test` 运行不受影响）。
- plan mode 下无法写文件，`director` 无法推进状态；开始 run 之前先退出 plan mode。

## 循环

### 1. 读取 spec

`gh issue view <spec> --json number,title,body`；用 `gh issue list --state open --json number,title,body` 列出子 ticket（子 ticket 带 `Parent` 段）。`Blocked by` 段定义任务图及其 frontier；v1 按该 frontier 串行跑 ticket。

### 2. 打开或恢复 run

- 全新：`director init --worktree <wt> --spec-issue <N> --slug <slug>`（slug 取自 `dsh/spec-<N>-<slug>` 分支名），然后 `director run transition --worktree <wt> --to running`。
- 恢复：`director resume --worktree <wt>`；drift 错误是硬停——先查清什么变了，仅在人类同意后才带 `--accept-drift` 重跑，否则放弃并报告。
- 每个子 ticket 只登记一次：`director ticket add --worktree <wt> --ticket <n> --title <title> [--blocked-by <m>]...`。

### 3. 单 ticket 循环

1. `director ticket transition --ticket <n> --to implementing`。
2. `director dispatch begin --ticket <n> --worker <label>`（attempt 在 Worker 存在之前就被记录）。
3. 按上述 dispatch 模型派发 Worker，等待返回；然后校验 envelope：`director report validate --ticket <n>`（stdin）或 `--file <path>`，并 `director dispatch finish --ticket <n> --outcome ok|failed [--reason <text>]`。envelope 畸形、报告 `blocked`、或没有 commit：都算失败的 dispatch。状态机只允许一次同一 Worker 重试，之后自行升级。
4. **自己**跑仓库的测试 gate（`cargo test`、ticket 指定的套件）。Worker 自报不是证据。gate 失败就把同一个 Worker 打回 `--to implementing`。
5. `director ticket transition --ticket <n> --to gating`，然后跑审查轮次：
   - `director round open --ticket <n>`；
   - 用 `subagent` 派发两个全新 axis reviewer（Standards 轴还额外携带本仓库的两条审查规则：每个 guard/predicate 要指出其唯一实现位置，并确认合约依赖的边界状态（unset、open、empty、zero）在决策点有测试；每个重命名要把所有 usage string、help text、doc comment 和文档化清单计数一并迁移）；
   - 记录每条 finding：`director finding record --ticket <n> --round <k> --axis <standards|spec> --id <id> --hash <hash> --summary <text>`；
   - `director round close --ticket <n> --round <k>`；
   - 裁定到绝对零：每条 finding 要么由 Worker 修复，要么由你写出理由驳回——`director finding dispose ... --fixed <commit>` 或 `director finding dispose ... --rejected "<书面理由>"`；
   - `director gate --ticket <n>` 必须退出 0。否则 `director ticket transition --ticket <n> --to fixing`，把 findings 交回同一个 Worker，开下一轮。每层最多三轮：第四次 `round open` 会被拒绝并把该层升级。

### 4. 票据边界提交

归零后，把该 ticket 的 WIP commit 压成一个引用 ticket 的 commit（`feat: <title> (ticket #N)`），然后 `director ticket transition --ticket <n> --to done`。

### 5. 聚合 gate 与 Spec PR

1. 所有 ticket `done` → `director run transition --to spec-gating`。
2. 对聚合 diff（`--spec`）跑同样的 two-axis gate，同样的绝对零门槛、同样的三轮上限。
3. 到这时才 push 分支；打开唯一的 Spec PR，正文用 `Closes #…` 列出每个 ticket；然后 `director run transition --to pr-open`。
4. 默认（auto-merge off）：停下，把 PR 交给人工批准。auto-merge on：等 CI 绿且 `director gate` 归零，用 **merge commit** 合并（禁止 squash-merge，它会把边界 commit 压平），然后 `director run transition --to done`。
5. 确认每个 ticket issue 已关闭；报告本次 run。

## 升级

升级不是可以靠重试绕开的失败。当某一层升级（轮次上限耗尽、dispatch 预算耗尽、或两次无法解析的报告），停止 run 并报告：各轮 finding 的演化（每轮发现了什么、修了什么、你以什么理由驳回了什么）、Worker 的自报、你的诊断、以及你需要人类做的决定（追加轮次、直接实现、或放弃该 ticket）。只有人类显式决定后 run 才恢复，且必须走合法边：`escalated → reviewing` 换取又一轮上限的轮次；`escalated → implementing` 意味着你直接实现它。

## 边界

- 每次 run 只有一个 Spec PR；绝不按 ticket 开 PR，绝不 squash-merge。
- 绝不断言 gate 结果：`director gate` 是唯一事实来源。
- 不修改 `autopilot-orchestrator` 和 `autopilot-reviewer`；两个工作流要能并排比较。
- Worker 从不审查；reviewer 从不实现；你从不把 CLI 能自己核对的判断下放给子代理。
