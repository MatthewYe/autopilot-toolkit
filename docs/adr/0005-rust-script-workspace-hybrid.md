# Rust-script + Cargo Workspace 混合架构

将项目 bash 脚本全量迁移到 Rust 时，选取 rust-script（单文件）和 cargo workspace（多 crate）的混合架构。边界：rust-script 只做 CLI 流程串联，共享业务逻辑放在 workspace crate 中。

## 边界原则

1. **Workspace crate** — 当代码需要被多个入口脚本复用（如 `validation/run.rs` 和 `scripts/check.rs` 都要调用 YAML 解析器），抽成 `crates/<name>/` lib crate。所有业务逻辑在此，含内联单元测试（`#[cfg(test)]`）。
2. **rust-script 单文件** — 当代码是独立的 CLI 入口（如 `install.rs`），或仅调用 workspace crate 做流程串联（如 `validation/run.rs`），使用单文件 rust-script。依赖通过 `//! ```cargo` 内联声明，可 path-depend 到 workspace crates。
3. **单元测试内联** — 原 bash `source` 库 + 独立测试文件的模式（如 `validate.sh` + `validate.test.sh`）统一为 `#[cfg(test)] mod tests {}`，消除跨文件代码共享问题。
4. **集成测试也是 rust-script** — 测 CLI 契约的集成测试用 rust-script 单文件（`tests/test_*.rs`），通过 `std::process::Command` 调目标 CLI，手动断言退出码和输出。

## Considered Options

- **全 rust-script**：每个 `.rs` 独立，代码共享靠复制。validation 场景会重复 ~200 行 YAML 解析逻辑。拒绝理由：N>2 个消费者时代码重复不可接受。
- **全 cargo workspace**：放弃单文件 rust-script，全部编译成二进制。拒绝理由：install.rs 等独立入口不需要 workspace 开销，单文件更轻量；首次执行延迟对 CLI 工具不可接受。
- **bash + Rust 长期共存**：保留部分 bash 脚本不转换。拒绝理由：两套语言增加认知负担，且 PoC (#24) 已验证 Rust 方案可行。

## Consequences

- 新增 `Cargo.toml` workspace 根 + `crates/validation/` lib crate
- `validation/run.rs` 和 `scripts/check.rs` 通过 path dependency 引用 validation crate
- **后续应用**: `deploy.rs` 中的 skill discovery / 分类 / manifest 生成逻辑提取为 `crates/skill-index/` lib crate（同模式）。`deploy.rs` 和 `validation/run.rs` 通过 path dependency 引用。同时修复 `SkillVariant` 枚举缺失 `Kimi` 成员的问题——`classify_skill` 统一了 codex/kimi/reasonix 三 variant 的识别。
- 全量转换验证通过后，统一删除 11 个 `.sh` 文件（保留 vendored upstream 脚本）
- rust-script 的 path dependency 缓存策略需验证：修改 workspace crate 后 rust-script 是否会重新编译
