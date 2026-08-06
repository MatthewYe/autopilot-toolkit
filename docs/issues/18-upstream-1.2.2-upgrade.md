# 升级 upstream 至 v1.2.2

> **Parent**: 无（维护性升级）
> **Status**: ready-for-agent

## Problem Statement

当前 vendored upstream snapshot（`skills/upstream/`）基于 mattpocock/skills **v1.1.0**。上游已发布 v1.2.2，包含 1 个 breaking rename、3 个新 skill、grilling round-by-round 模式重构、prototype 重设计、Codex dual-harness metadata 等变化。不升级将导致与上游最新 skill 定义脱节。

## Solution

运行 `rust-script scripts/sync-upstream.rs v1.2.2` 执行全量 replacement，然后验证部署链路。

## User Stories

1. As a toolkit maintainer, I want the vendored upstream snapshot to match v1.2.2, so that installed skills reflect the latest upstream definitions.
2. As a toolkit maintainer, I want the `writing-great-skills` → `writing-for-agents` rename to be handled without manual migration, so that no stale skill directory lingers in the SSOT after upgrade.
3. As a toolkit user targeting Codex, I want `writing-for-agents` to be model-invokable (v1.2.2 Codex fix), so that the agent autonomously uses it when editing skills.
4. As a toolkit user, I want the three new upstream skills (`to-questionnaire`, `wizard`, `wait-what`) to be available after upgrade.
5. As a toolkit maintainer, I want all 19+3 toolkit skills to pass frontmatter validation post-sync.
6. As a toolkit maintainer, I want the deploy system to correctly symlink all upgraded skills into `~/.agents/skills/`.

## Implementation Decisions

- **Target tag**: `v1.2.2`（最新 stable，含 Codex 的 `writing-for-agents` model-invokable 修复）
- **Sync 方式**: `scripts/sync-upstream.rs` 执行全量 replacement — 删除旧 `skills/upstream/`，从 tag clone，重新计算所有 `skillFolderHash`，处理孤儿（`writing-great-skills` 移除）和新 skill 发现（`to-questionnaire`, `wizard`, `wait-what`, `writing-for-agents`）
- **BREAKING rename**: `writing-great-skills` → `writing-for-agents`。sync script 自动处理：旧条目 orphan 并从 `.skill-lock.json` 移除，新条目自动发现并添加
- **新增 metadata 文件**: 每个 skill 目录旁的 `agents/openai.yaml`（Codex 调用元数据）、repo 根的 `AGENTS.md` symlink、`.claude-plugin/plugin.json` — 均由 `copy_dir_except_git` 全量复制，与 deploy 系统正交，无需特殊处理
- **3 个新 skill 全部采用**: `to-questionnaire`（productivity）、`wizard`（engineering）、`wait-what`（productivity）— 无运行时依赖，低风险
- **CONTEXT.md 不改动**: 本次升级不修改域模型；decision ticket 等新术语留待后续 autopilot-orchestrator 适配时一并处理
- **grilling round-by-round**: 透明升级 — autopilot-orchestrator 和 autopilot-distill 被动受益

## Testing Decisions

- **测试 seam**: `scripts/sync-upstream.rs` 为唯一主 seam — 验证全量 replacement + orphan 清理 + 新 skill 发现
- **验证 seam**: `validation/run.rs` — 所有 SKILL.md 前端信息格式正确
- **部署 seam**: `deploy.rs dev` — 符号链接重建后验证 `~/.agents/skills/` 中 skill 集合正确
- **测试方式**: 先执行 sync，再跑 validation 和 dev 部署，检查 `~/.agents/skills/` 下目录列表
- **好测试的标准**: 只验证最终状态（lock file 条目正确、部署目录完整），不测试 sync script 内部实现

## Out of Scope

- autopilot-reviewer/orchestrator 适配（如吸收 decision ticket 术语、tdd reference-only 变化）
- CONTEXT.md 域模型更新
- 任何 autopilot skill body 修改
- 上游新增 skill（`wizard` 等）的 toolkit 级集成测试

## Further Notes

执行步骤：

```bash
# 1. 替换 upstream + 更新 .skill-lock.json
rust-script scripts/sync-upstream.rs v1.2.2

# 2. 验证 frontmatter
rust-script validation/run.rs

# 3. 重新部署符号链接
rust-script deploy.rs dev

# 4. 验证部署结果
ls ~/.agents/skills/ | grep -E "writing-for-agents|writing-great-skills|to-questionnaire|wizard|wait-what"
```

预期结果：
- `writing-great-skills` 不在 `~/.agents/skills/` 中
- `writing-for-agents`、`to-questionnaire`、`wizard`、`wait-what` 均存在
- `.skill-lock.json` 中技能数量从 24 变为 27（+3 新 -1 重命名 +1 新名 = 27）
