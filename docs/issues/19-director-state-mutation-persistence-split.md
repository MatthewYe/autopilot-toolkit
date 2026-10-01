# 19 — Director 状态机：变更与持久化分离

Parent: 架构改进（architecture-review-20261001 候选 #1, Top recommendation）
Status: done
ADR 依据: 执行 ADR 0040/0041 已决定的形状（state machine owns workflow control; transitions are methods with guards internalized），无新 ADR。

## 背景

`crates/director-cli/src/transition.rs`（927 行）的 12 个 `pub(crate)` 变更入口各自接收
`(worktree: &Path, state: &mut RunState, ...)` 并直接调用私有 `write(worktree, state)`
（15 个写点，每次写跑 3 个 `git rev-parse`）。distill-cli 的形状相反：13 个纯 `impl RunState`
方法持有 guard + revision，持久化集中在 `transition::commit` 一个 seam。
AGENTS.md 声称 director "layering mirrors distill-cli"——在 seam 层面这个声明当前为假。

目标：把 director 改为"纯记录方法 + 单一 commit seam"，**行为零变更**。

## 已锁定决策（grilling 2026-10-01，用户全部确认）

| # | 决策 |
|---|---|
| Q1 | 试点先行：`close_round`（17 行，最简单）+ `open_round`（123 行，含升级路径）先定形状，同 ticket 内扫尾其余 10 个入口 |
| Q2 | 方法分挂记录：`TicketState`/`SpecGate`/`ReviewRound`/`RunState` 各自持有自己的 guard；`RunState` 管跨记录编排 + revision |
| Q3 | 搭车：envelope 收敛为单一构造器（14 处内联 `json!` → 1）；`TicketState::open_round`/`SpecGate::open_round` 与 `round_mut` 同构重复归一。**不搭车**：gate refusal 值化、`cap_exhausted` 消费（留后续 ticket） |
| Q4 | 三态 `Outcome`：`Applied` / `Refused(reason)` / `RefusedWithMutation(reason)`。4 条"升级即持久化拒绝"路径成为类型事实 |
| Q5 | envelope **严格逐字段保留现状**（含已知不对称：`close_round`/`record_finding`/`dispose_finding` 缺 layer+ticket；`dispose_finding` 不回显 disposition；spec round-open 的 `"ticket": null`；`dispatch-finish` 双函数共用 command 串）。修复留后续 |
| Q6 | 混合快照验证：8 个无字段断言的命令补 golden envelope 测试；6 个已有断言的靠现有测试 |

## 关键事实（fact-finding 已核实）

### 4 条 RefusedWithMutation 路径（先写盘后 exit 1，有测试钉住，必须保留）

1. `open_round` ticket 层 cap 耗尽：transition.rs:308 设 `Escalated` → 309 write → 310-313 Err
   （测试 `the_third_unresolved_round_escalates_the_ticket`, tests/transitions.rs:355）
2. `open_round` spec 层 cap 耗尽：transition.rs:360 设 `Escalated` → 361 write → 362-365 Err
   （**无测试钉住**——本 ticket 补）
3. `record_worker_report` malformed：603 委托 `fail_open_dispatch`（731-739 mutate+write）→ 604 Err
   （测试 `a_malformed_report_is_a_recorded_dispatch_failure`, dispatch.rs:246；
   `an_envelope_that_contradicts_itself_is_a_recorded_failure`, dispatch.rs:281）
4. `finish_dispatch_failed` escalated：`fail_open_dispatch` write(739) → 667-670 Err
   （测试 `one_same_worker_retry_then_escalation`, dispatch.rs:307；
   `a_failed_finish_records_its_reason_and_keeps_one_retry`, dispatch.rs:422）

### 其他不变量

- revision bump 集中在 `write()`（transition.rs:867-870）；`init_run` 绕过它（revision 保持 0，
  init_state.rs:99 钉住）；clean resume 不写盘（resume.rs:111-126 字节级钉住）
- 持久化只有 `.director/state.json` 一个 artifact（main.rs:21-23 明示无 event log）
- 拒绝路径 stdout 无 envelope（main.rs:45-50，stderr + exit 1）；`gate` 命令例外（JSON + exit 1）
- main.rs 只把返回的 `Value` 交给 `print_json`，从不读字段
- crate 是 bin-only（无 lib target），集成测试只能走进程 seam；新单元测试放 `#[cfg(test)]` inline

### 不可破坏的拒绝纯度钉子（字节级 state.json 相等）

transitions.rs:196/205/214, 243/289-297, 441/456-461, 504/533-545；
dispatch.rs:354/382-388, 478/518-521, 525/542-545, 592/604-607, 690/738-741, 745/786-788；
resume.rs:129/158-162, 111/121-125。

## TDD 执行顺序

1. **表征测试（在现状代码上写，全部应通过）**
   - 8 个 golden envelope 快照：`round-open`(ticket+spec)、`round-close`、`finding-record`、
     `finding-dispose`、`ticket-add`、`report-validate`、`dispatch-begin`、`dispatch-finish`(ok+failed)
     ——凡现有测试未断言字段的命令都钉住完整 envelope
   - 1 个 spec 层 cap 路径测试（驱动 spec_gate.rounds 到 cap，断言 Err + 状态已升级）
2. **试点**：`close_round` + `open_round` 迁移到新形状
   - 记录方法返回 `Outcome`；transition.rs 留薄 commit 层
   - 新方法的 guard 矩阵配纯单元表测试（无 git worktree、无子进程）
3. **扫尾**：其余 10 个入口机械迁移；envelope 收敛；`open_round`/`round_mut` 归一
4. **验证**：`cargo test -p director-cli` 全绿（37 集成 + 全部单元）、clippy、fmt

## 验收标准

- [ ] transition.rs 中 `write(worktree, state)` 调用点 15 → 1（commit seam 唯一持有）
- [ ] 12 个变更的 guard 矩阵有不依赖文件系统的单元测试
- [ ] 所有 envelope 字节级不变（golden 快照 + 现有断言证明）
- [ ] 4 条 `RefusedWithMutation` 路径有类型级表达 + 测试（含新补的 spec 层 cap 测试）
- [ ] 37 个现有集成测试全部原样通过
- [ ] GLOSSARY.md 新增 `Outcome`（三态）词条，说明 `RefusedWithMutation` = "拒绝但事故已落盘"的语义

## 环境注意

- 工作目录：/Users/matthewye/Documents/WorkSpace/autopilot-toolkit
- 验证命令：`cargo test -p director-cli`；如 rust-script 报 Operation not permitted 用
  `bash scripts/sandboxed-rust-script.sh`（本 ticket 主要是 cargo，一般不需要）
- 不要动 `crates/distill-cli/`；不要改任何 schema 字段或磁盘格式

## 完成记录（2026-10-01）

### 什么搬到了哪里

| 之前（transition.rs 927 行） | 之后 |
|---|---|
| 12 个 `pub(crate)` 变更入口，各自 `(worktree, state, ...)` + 内联 guard + 直接 `write()` | `records.rs`：12 个纯记录方法，挂在 `impl RunState`，各自持 guard，返回 `Outcome`（2027 行 = 916 行生产 + 1111 行测试，`#[cfg(test)]` 自 917 行起；含 53 个单元测试） |
| 15 个 `write(worktree, state)` 写点 | `transition.rs`：唯一 `write()` 函数（= `state::write_state` 唯一调用者），只被 `commit` 调用 |
| 14 处内联 `json!` envelope（`gate.rs` 未被触碰：其自有 `to_json` 原样保留） | `records::envelope(command, revision, [(key, value)...])` 单一构造器（envelope 收敛） |
| `write()` 里 `revision += 1` | 记录方法自身 bump；`commit` 以「revision 是否移动」作为写盘信号 |
| `transition.rs` 内的 `round_mut` / `open_round` 同构重复 | `records::round_mut` 一份；`TicketState::open_round` 与 `SpecGate::open_round` 各自保留（读方法，未合并） |

> 备注：`commit` 内有两处**文本上**的 `write(worktree, state)` 调用（`Applied` 与 `RefusedWithMutation` 两条分支），
> 但两处同属一个 commit 函数；`state::write_state` 仍只有一个调用者（即 `write` 本身）。
> 上表的「15 → 1」是函数级 commit seam 的收敛，不是字面调用点的计数。

`main.rs` 新增 `Outcome` 三态 enum（`Applied` / `Refused` / `RefusedWithMutation`，附手写 `Debug` 供断言读取）；`transition.rs` 剩余 411 行 = 12 个薄入口 + `commit` seam + 4 个 seam 单元测试（含 `Refused` 分支的 `debug_assert!` 不变量钉子）。

### 测试计数

| | 之前 | 之后 |
|---|---|---|
| 单元测试（`src/` inline） | 55 | **110** |
| 集成测试 | 37 | **40** |
| — transitions.rs | 7 | 7（未改） |
| — dispatch.rs | 16 | 16（未改） |
| — init_state.rs | 9 | 9（未改） |
| — resume.rs | 5 | 5（未改） |
| — envelope_contract.rs（新） | — | 3 |
| 合计 | 92 | **150** |

### 验收标准对照

- [x] `write(worktree, state)` 调用点 15 → 1（`grep -c "write_state(" transition.rs` = 1）
- [x] 12 个变更的 guard 矩阵有纯单元测试（`records.rs` tests：不碰文件系统、不起子进程；唯一用 git 的测试在 `transition.rs`，测 commit seam 本身）
- [x] 所有 envelope 字节级不变：23 个 golden 快照（`tests/fixtures/envelopes/`，由 `scripts/capture-envelope-goldens.sh` 从拆分前的二进制抓取）+ 差分复核 `checked=23 differing=0`
- [x] 4 条 `RefusedWithMutation` 路径有类型级表达 + 测试（新增 spec 层 cap 端到端测试 `the_exhausted_spec_round_cap_escalates_the_run_and_persists`）
- [x] 37 个现有集成测试全部原样通过（4 个测试文件 `git diff` 为空）
- [x] GLOSSARY.md 新增 `Outcome`、`RefusedWithMutation`、`Commit seam` 三个词条

### 验证命令结果

- `cargo test -p director-cli` → 110 unit + 40 integration，全绿
- `cargo clippy -p director-cli --all-targets` → 无警告
- `cargo fmt --check -p director-cli` → 干净（`--all` 在 `distill-cli` 等未触碰 crate 上有**既有**格式偏差，本 ticket 不动它们）

### 与 ticket 的偏差

1. **`record_resume` 接收 `&Path`**（第一个参数，仅用于拒绝消息里的 `path.display()`）：`drift()` 的诊断文本只有它能让出。它不读文件系统、不做路径判定——git 指纹由 `transition::resume_run` 传入。若要求绝对零 `&Path`，可把 `WorktreeFingerprint::drift` 提为 pub、在薄层拼错误串，代价是诊断逻辑外泄。
2. **`envelope` 构造器签名**取 `(command: &str, revision: u64, fields)`，而 `revision` 由 `commit` 在写盘后 `restamp`：两者一致（记录方法用的是它 bump 后的值），`restamp` 只重放同一个数字。这样记录方法不必知道「是否/何时」被持久化。
3. **`report.rs` 一行空行删除**：`cargo fmt -p director-cli` 顺带修掉的既有格式偏差（该文件本 ticket 未做逻辑改动）。
4. **集成测试 37 → 40**：新增 `envelope_contract.rs` 的 3 个测试（golden 序列 + stdin seam + spec cap），既有 37 个未动。
5. **执行方式**：本 session 无法派发 subagent（depth cap），ticket 由单 agent 按 TDD 顺序直接执行；步骤 1 的表征测试用「拆前二进制差分」而非「拆后猜测」来固定 golden。
