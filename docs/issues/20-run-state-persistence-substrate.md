# 20 — Run-state persistence substrate：state-store crate

Parent: 架构改进（architecture-review-20261001 候选 #2）
Status: done
ADR: 新建 ADR 0050（随本 ticket 一并落地）；ADR 0040 加 blockquote 修订注记。
前置: ticket 19 已完成（director 写路径已收敛到单一 commit seam）。

## 背景

`crates/director-cli/src/storage.rs` 与 `crates/distill-cli/src/storage.rs` 各自实现了同一套
"项目本地状态目录卫生 + 原子写"基座，归一化相似度：`ensure_*_ignored` 0.982、
`ensure_*_path_safe` 0.993、`ensure_worktree` 0.939。`atomic_write` 已漂移：distill 版用
毫秒后缀临时名 + `create_new(true)` 临时文件（防撞）；director 版用固定临时名 +
`create(true).truncate(true)`（并发写可交错）。两个 adapter 已存在，seam 是真的。

fact-finding 已确认：两个 CLI 无任何"必须零依赖"的文档约束（ADR 0019 只说最终用户不需要
Rust）；构建/发布全程在完整 workspace 原地进行（artifacts.rs:173-183、release.yml:24-32）；
tarball 不含 crate 源码。ADR 0040 拒绝 library crate 的理由是"single consumer"，并明确以
ADR 0009 的 shared crate 为先例对照——现在两个消费者，先例适用。

## 已锁定决策（grilling 2026-10-01，两轮，用户全部确认）

| # | 决策 |
|---|---|
| Q1 | **窄抽**：只收 `ensure_*_ignored` / `ensure_*_path_safe` / `ensure_worktree` / `atomic_write`。不收 `read_state`/`state_path`/`deserialize_state`/`print_json`（read_state 两边有真实行为分歧——missing schema_version 处理、deny_unknown_fields vs flatten、迁移链——抽它需要泛型化，违反"don't merge the schemas"） |
| Q2 | **锁不抽不借**。distill 的锁协议（create_new 锁文件 + `"stale":true` 魔串恢复，跨进程语义）留在 distill。在 GLOSSARY.md 记录"Director 是 Spec run 的唯一写者"为设计决策（现状：无锁、无 --expected-revision，revision 仅用于检测） |
| Q4 | **新建 `crates/state-store/`**。不扩 `crates/shared/`（ADR 0009 定位为 tooling 基础设施层，CLI 是发布产物领域，方向不同）；两个 CLI 各加一条 path dep。workspace glob 自动纳入 test/clippy |
| Q5 | `atomic_write(path, bytes)` **单签名**：distill 的内部实现（毫秒后缀临时名 + 临时文件 `create_new(true)`）+ director 的失败清理（rename 失败删临时文件）。**删除 `create_new` 参数与硬链接分支**——7 个调用点全传 false、零测试、删除测试通过；publication 不可变性由 publication.rs:260-268 内容哈希保证，ADR 0050 必须记录这一点 |
| Q6 | 新 ADR 0050 + ADR 0040 加 blockquote 注记：`> Amended by ADR 0050 for the storage substrate: the single-consumer rejection applies to the state schema, which stays in-crate.`（遵循仓库 9 处既有的 blockquote 惯例） |
| Q7 | **测试随代码迁移**：两边现有卫生函数测试搬进 state-store，补齐威胁矩阵表测试（symlinked .gitignore / 规则缺失 / 规则已存在 / 目录先于规则存在 / symlink 逃逸 / 超大路径 / 临时名冲突 / rename 失败清理）。两个 CLI 的进程级集成测试一律不动 |

## 接口形状（implementer 可微调命名，语义契约不变）

```
crates/state-store/src/lib.rs
  pub fn ensure_ignored(worktree: &Path, dir_name: &str, ignore_rule: &str) -> Result<(), String>
      — ADR 0025 规则：拒绝 symlinked .gitignore；规则缺失时追加；规则生效前目录已存在则拒绝
  pub fn ensure_path_safe(worktree: &Path, dir_name: &str) -> Result<PathBuf, String>
      — 拒绝 symlinked/逃逸/超大路径，返回校验后的目录路径
  pub fn ensure_worktree(worktree: &Path, dir_name: &str) -> Result<(), String>
      — worktree 真实性校验
  pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String>
      — 毫秒后缀临时名 + create_new(true) 临时文件 + fsync + rename + 失败清理
```

distill 的 `atomic_write_json`（3 行便利封装）留在 distill-cli 本地，改为调用共享
`atomic_write`。`state_path`/`run_dir`/`validate_run_id` 不迁移（真实行为分歧）。

## 执行顺序

1. 新建 crate + 迁移实现 + 威胁矩阵表测试（红→绿）
2. distill-cli 接入：删除 storage.rs 本地副本，改 call sites；全部现有测试原样通过
3. director-cli 接入：同上
4. 文档：ADR 0050、ADR 0040 注记、GLOSSARY.md 新词条（State store；Director 单写者决策）
5. `ci.yml:77` fmt 列表加 `crates/state-store/src/lib.rs` 一行
6. 验证（见下）

## 验收标准

- [ ] 两个 CLI 的 Cargo.toml 各多一条 `state-store = { path = "../state-store" }`，本地副本删除
- [ ] `atomic_write` 全仓库只有一个实现；`create_new` 参数与硬链接分支不存在
- [ ] `cargo test --workspace --no-fail-fast` 全绿（distill/director 现有集成测试零改动）
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` 干净
- [ ] `cargo build --release --bin distill` 与 `--bin director` 在本机成功（发布路径冒烟）
- [ ] state-store 的威胁矩阵表测试覆盖上述 8 类场景
- [ ] ADR 0050 落地，编号接续 0049；0040 注记加入；GLOSSARY.md 更新
- [ ] 行为零变更：distill-cli / director-cli 全部既有测试不修改即通过

## 环境注意

- 工作目录：/Users/matthewye/Documents/WorkSpace/autopilot-toolkit
- 不要动 skills/、tests/（顶层）、docs/adr/ 中除 0040 注记与新增 0050 外的任何文件
- fmt：`cargo fmt -p state-store -p distill-cli -p director-cli`（注意 distill-cli 有预存 fmt
  漂移——只格式化你触碰的文件，若预存漂移导致 --check 失败，记录而不修复）

## 完成记录（2026-10-01）

### 什么搬到了哪里

| 之前 | 之后 |
|---|---|
| distill `storage.rs`（447 行）与 director `storage.rs`（237 行）各一份 `ensure_*_ignored` / `ensure_*_path_safe` / `ensure_worktree` / `atomic_write` | `crates/state-store/src/lib.rs`（502 行 = 154 行实现 + 348 行测试，20 个单元测试）：四个函数一份实现 |
| distill `atomic_write(path, bytes, create_new)` 的 `create_new` 参数 + `fs::hard_link` 不可变分支（7 个调用点全传 false、零测试） | 删除；`atomic_write(path, bytes)` 单签名 = 毫秒后缀临时名 + 临时文件 `create_new(true)` + `write_all` / `sync_all` / `rename` + rename 失败删临时文件 |
| director `atomic_write`：固定临时名 `state.json.tmp` + `create(true).truncate(true)` | 并入上面的共享实现（临时名冲突与并发交错随 drift 一并消失） |
| distill `atomic_write_json(path, value, create_new)` | distill `storage.rs::atomic_write_json(path, value)`（2 参数，3 处调用点同步改） |
| distill `state.rs::ensure_worktree`（is_dir 检查 + path_safe）与 director `storage.rs::ensure_worktree`（逐行同体） | `state_store::ensure_worktree(worktree, dir_name)`；distill 保留 `state::ensure_worktree` 一行适配器（main.rs 10 处调用点不动），director 保留 `storage::ensure_worktree` 一行适配器 |
| `state_path` / `run_dir` / `validate_run_id` / 读路径 / distill 锁协议（`start.lock`、`state.lock`、`"stale":true` 恢复） | 原地不动（真实行为分歧 + 跨进程语义） |

两个 CLI 各保留一层薄适配器，绑定自己的目录名与规则（`.distill` + `/.distill/`、`.director` + `/.director/`），
错误文本由参数插值逐字节复现。

### 接口

```rust
// crates/state-store/src/lib.rs（std only；dev-dependency 仅 tempfile）
pub fn ensure_ignored(worktree: &Path, dir_name: &str, ignore_rule: &str) -> Result<(), String>
pub fn ensure_path_safe(worktree: &Path, dir_name: &str) -> Result<PathBuf, String>
pub fn ensure_worktree(worktree: &Path, dir_name: &str) -> Result<(), String>
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String>
```

### 测试计数

| | 之前 | 之后 |
|---|---|---|
| state-store | — | **20**（单元；威胁矩阵 8 类场景 + 成功路径 / 覆写 / trim 匹配 / ensure_worktree） |
| distill-cli | 116 unit + 64 integration = **180** | 116 + 64 = **180**（零改动） |
| director-cli | 110 unit + 40 integration = **150** | 110 + 40 = **150**（零改动） |
| workspace 合计 | 550 | **570** |

### 验收标准对照

- [x] 两个 CLI 的 Cargo.toml 各多一条 `state-store = { path = "../state-store" }`，本地副本删除（`fn atomic_write` 的实现只剩 state-store 一处；distill 保留 2 参便利封装 `atomic_write_json`）
- [x] `atomic_write` 全仓库只有一个实现；`create_new` 参数与硬链接分支不存在（`grep -rn "hard_link\|immutable file already exists\|create_new:" crates/` 为空）
- [x] `cargo test --workspace --no-fail-fast` 全绿（exit 0，570 passed / 0 failed）
- [x] `cargo clippy --workspace --all-targets -- -D warnings` 干净（exit 0）
- [x] `cargo build --release --bin distill` 与 `--bin director` 成功（target/release/distill 1.6 MB、director 1.0 MB）
- [x] state-store 的威胁矩阵测试覆盖 8 类场景（symlinked .gitignore / 规则缺失 / 规则已存在 / 目录先于规则存在 / symlink 逃逸与非目录 / 超大路径 / 临时名冲突 / rename 失败清理）
- [x] ADR 0050 落地（编号接续 0049）；ADR 0040 顶部 blockquote 注记加入；GLOSSARY.md 新增 **State store** 词条，Autopilot Director 段新增 **Sole writer**
- [x] 行为零变更：两 crate 既有测试文件零改动（`git diff -- '*tests/*'` 为空；director `storage.rs` 测试模块唯一 diff 是新增一行 `use std::fs;`，属允许的 use 路径修正）

### 发现的语义差异与处置（并集核查）

1. **超大路径：两边都没有显式长度检查**（全仓库无 PATH_MAX / 长度守卫）。拒绝来自 OS 的 `ENAMETOOLONG`，经既有 `cannot inspect {dir_name} safely:` 分支浮出。处置：**不新增检查**（新增即改变行为），用 `oversized_worktree_path_fails_closed` 钉住 fail-closed 现状。
2. **`ensure_path_safe` 返回类型**：两边都是 `Result<(), String>`，ticket 形状写 `Result<PathBuf, String>`（"返回校验后的目录路径"）。处置：共享实现返回校验过的 joined path（不是 canonical path），两个适配器 `map(|_| ())` 维持 `Result<(), String>`，调用点零改动。
3. **`ensure_worktree` 位置不同**（distill 在 `state.rs`，director 在 `storage.rs`），函数体逐行相同。处置：统一进 state-store，各自保留同名薄适配器。
4. **director 的 `ensure_director_path_safe` 在模块外无调用者**（只被同模块的 ignored / worktree 调用）；distill 的 `ensure_distill_path_safe` 仍被 `main.rs` 直接调用。处置：director 删除该适配器（保留会在 `-D warnings` 下 dead_code），distill 保留。
5. **`atomic_write` 真实漂移**（临时名 + open flags + rename 失败清理）。处置：按 Q5 合并；director 因此获得临时名防撞，distill 因此获得失败清理——两者均不可观测（无测试断言临时文件名）。
6. **`tmp_path` 兜底文件名**：`"distill"` → `"state"`（仅 `file_name()` 为 None 或非 UTF-8 时生效，实际不可达）。
7. **消失的错误串**：`immutable file already exists: {path}` 与 `cannot create immutable file {}: {err}`（同属 `create_new=true` 死分支；7 个调用点全传 false、零测试）。ADR 0050 已记录；publication 不可变性由 `publication.rs:260-268` 内容哈希保证。

### 验证命令结果

- `cargo test --workspace --no-fail-fast` → exit 0，570 passed / 0 failed
- `cargo clippy --workspace --all-targets -- -D warnings` → exit 0
- `cargo build --release --bin distill` / `--bin director` → 均成功
- `cargo fmt -p state-store --check` → 干净
- CI fmt 步骤原命令（含新增 `crates/state-store/src/lib.rs`）→ exit 0
- 触碰文件的 rustfmt 漂移在改动前后**逐 hunk 内容一致**（distill `main.rs` 12、`state.rs` 12、其余 0；既有漂移未修，也未新增）

### 与 ticket 的偏差

1. **测试迁移方式（Q7）**：用户约束"既有测试不修改即通过"优先于"卫生函数测试搬进 state-store"。director `storage.rs` 的 4 个卫生单测**原地保留**（经同名适配器，测试体逐字节未改）；state-store 另建参数化的威胁矩阵 20 测（含同样 4 个场景 × 两个目录参数）。两个 CLI 的进程级集成测试零改动，覆盖只增不减。
2. **distill `atomic_write_json` 去掉 `create_new` 参数**：单签名 `atomic_write` 已无该分支，保留参数只能凭空造出不可达分支；3 处调用点同步改（`main.rs` / `storage.rs` / `publication.rs` 各一），bin crate 无外部可见性。
3. **director 删除 `ensure_director_path_safe`**：见语义差异 4。
4. **额外一行 AGENTS.md**：Architecture 的 crate 列表补 `crates/state-store/`（ticket 未列；该列表是本仓库的 crate 地图，缺失会立即过时。如判为越界，删除该行即可）。
5. **执行方式**：本 session 无法派发 subagent（depth cap `maxDepth 1`），按 AGENTS.md 的 fallback 规则由单 agent 依 ticket 顺序直接执行（与 ticket 19 相同）。
