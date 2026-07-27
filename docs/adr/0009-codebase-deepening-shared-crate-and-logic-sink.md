# 代码架构深化：共享基础设施 crate + 逻辑下沉

将代码架构中的重复代码消除、模块化拆分、强类型化三项改善统一执行。延续 ADR-0005 的"rust-script 薄入口 + workspace crate 承载业务逻辑"模式，新建 `crates/shared/`、`crates/deploy/`、`crates/validation-runner/`、`crates/skill-check/` 四个 workspace crate。

## 核心决策

### 1. 新建 `crates/shared/` 作为公共基础设施

项目根目录推导逻辑在 6+ 处重复，`.skill-lock.json` 解析在 4 处使用 `serde_json::Value` 手动提取。统一抽取到 `shared` crate，定义强类型 struct（`SkillLock`, `LockedSkill`），其他 crate 通过 Cargo path dependency 引用。

### 2. `deploy.rs` 逻辑下沉到 `crates/deploy/`

`deploy.rs` 当前 743 行，5 个子命令混在一个文件。拆为子模块（`dev.rs`、`pack.rs`、`release.rs`）放入 `crates/deploy/`，`deploy.rs` 入口文件变为薄 CLI 层。核心逻辑享受 `cargo test` 和 IDE 支持。

### 3. `validation/run.rs` 拆分并下沉为 `crates/validation-runner/`

`validation/run.rs` 当前 ~1000 行，三重职责（技能发现、验证调度、报告生成）。拆分后：技能发现逻辑移除（统一使用 `skill-index` crate），验证调度入 `crates/validation-runner/`。入口 `validation/run.rs` 变为薄 CLI 层。

### 4. `scripts/check.rs` 下沉为 `crates/skill-check/`

`check.rs` ~774 行含 15 个测试。下沉为 workspace crate，原文件保留为薄 CLI 入口。`env-check.rs`（150 行纯诊断）保持 rust-script 不变。

### 5. `.skill-lock.json` 强类型化

在 `crates/shared/` 定义 `SkillLock` 和 `LockedSkill` struct，作为唯一解析入口。rust-script 文件通过 `//! ```cargo` path dependency 引用。

### 6. 入口文件保持原位

`deploy.rs`、`validation/run.rs`、`scripts/check.rs` 保持当前路径不变，最小化对外接口变更。

### 7. 严格串行执行

执行顺序：shared → skill-index → validation → deploy → validation-runner → skill-check。每步跑全量测试确保不回归。

## 重构后 Crate 布局

```
crates/
├── shared/             # NEW — 公共基础设施（根目录推导、lock 文件类型）
├── skill-index/        # 现有 — 技能发现/分类，依赖 shared
├── validation/         # 现有 — frontmatter 校验，依赖 shared
├── deploy/             # NEW — deploy 核心逻辑，依赖 shared + skill-index
├── validation-runner/  # NEW — 验证调度 + 报告，依赖 shared + skill-index + validation
└── skill-check/        # NEW — 技能完整性校验，依赖 shared
```

依赖图：`shared` 是所有 crate 的叶子依赖，`skill-index` 和 `validation` 相互独立。

## Considered Options

- **共享逻辑并入 skill-index vs 新建 shared**：选 shared。skill-index 语义是"技能发现"，塞入根目录推导、lock 文件类型会污染职责。保持 skill-index 和 validation 相互独立，shared 作为无争议的基础设施层。
- **deploy 下沉到 crate vs 保持 rust-script 仅拆分文件**：选下沉到 crate。享受 `cargo test`、编译缓存、IDE 支持。rust-script 每次从头编译，对 700+ 行逻辑不友好。
- **validation/run.rs 下沉 vs 仅拆分 rust-script 文件**：选下沉。同上理由，且拆分出的 validate/report 模块更适合 crate 组织。
- **check.rs 下沉 vs 保留**：选下沉，理由同上。env-check.rs 保留（150 行不值得建 crate）。
- **入口文件统一到 bin/ vs 保持原位**：选保持原位。外部接口稳定优先。

## Consequences

- 新增 4 个 workspace crate，`Cargo.toml` workspace members 从 `["crates/*"]` 自动包含
- `deploy.rs`、`validation/run.rs`、`scripts/check.rs` 从"有业务逻辑的 rust-script"退化为"薄 CLI 入口"
- 消除：项目根推导 6+ 处重复、skill 发现 4 处重复、sync_skill/sync_agent 重复、lock 文件 Value 解析 4 处重复
- `cargo test` 覆盖率提升：原 rust-script 内嵌测试转为标准 `#[cfg(test)]`
- 向后兼容：所有 CLI 入口路径和参数不变
