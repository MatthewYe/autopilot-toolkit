# DSH（DeepSeek Harness）支持

本文面向"想让本仓库的 skill 在 DSH 下好用"的读者：skill 作者、变体维护者，以及在 DSH 里跑
autopilot 工作流的用户。

结论基于 **DSH desktop 0.2.0-rc.2** 包内源码（`dsh-skill`、`dsh-skill-filesystem`、
`dsh-tool-skill`）的调研。DSH 升级后请按第 5 节重新验收。

## 结论摘要

- **不需要为 DSH 增加专门的安装目标。** `rust-script deploy.rs dev` 把 skill 链接到
  `~/.agents/skills/`，这正是 DSH 的默认扫描根之一；发布版安装脚本落到同一位置。Reasonix /
  Codex / Kimi 的既有安装方式对 DSH 同样成立。
- 本仓库的 router 布局（变体正文放 `runtime/<runtime>/INSTRUCTIONS.md`）与 DSH "只扫一层、
  不递归"的发现规则天然兼容：一个逻辑 skill 只会被索引一次。
- 上游有 **17 个 skill 是 user-invoked**（`disable-model-invocation: true`），**不会出现在模型
  catalog 里**——这是预期行为，用户仍可用 `/<name>` 触发（见第 2 节清单）。

## 1. 发现机制

### 扫描根（优先级从高到低）

| 优先级 | 扫描根 |
| --- | --- |
| 1 | 项目 `<root>/.dsh/skills` |
| 2 | 项目 `<root>/.agents/skills` |
| 3 | 自定义目录 |
| 4 | `~/.dsh/skills` |
| 5 | `~/.agents/skills` |
| 6 | 内置 skill |

同名 skill 由**高优先级根胜出**。项目内的 `.dsh/skills` 或 `.agents/skills` 可以覆盖用户级安装。

### 发现规则

每个扫描根下**只扫一层**，不递归：

- 目录包：`<root>/<name>/SKILL.md`
- 扁平文件：`<root>/<name>.md`

扫描跟随符号链接——所以 `deploy.rs dev` 的 symlink 安装会被发现。

"不递归"这条对理解本仓库很关键：runtime-coupled skill 安装后，变体正文位于
`runtime/<runtime>/INSTRUCTIONS.md`（第二层），**不会被再次索引**，因此 6 个 workflow skill
各自只贡献一个 catalog 条目。设计动机见
[ADR 0036](../../adr/0036-single-discoverable-runtime-router.md)。

### 热更新

- DSH 有文件 watcher，并在**每个 agent step** 重渲染 skill catalog。
- `deploy.rs dev` 重新链接后，**新会话**即可生效；已运行的会话不保证中途刷新。

### 安装

```bash
rust-script deploy.rs dev      # 链接到 ~/.agents/skills/
```

不需要 `--target dsh` 之类的额外目标。

## 2. frontmatter 契约

| 键 | DSH 行为 | 本仓库校验器 |
| --- | --- | --- |
| `name` | 必需；严格 kebab-case `^[a-z0-9]+(?:-[a-z0-9]+)*$`（1–64 字符）。不合规的 skill 会被整条忽略 | 已同步收紧为同一正则（全变体生效） |
| `description` | 必需；进入 catalog 时归一化空白并**截断到 500 字符** | 必需 |
| `disable-model-invocation: true` | 从模型 catalog 隐藏；用户仍可输入 `/<name>` 触发，内容以 `<skill_content>` 注入 | 允许（dsh 变体白名单内） |
| `user-invocable` | 布尔值，支持 | 允许（dsh 变体白名单内） |
| `whenToUse` | 字符串，支持；**不进模型 catalog** | 允许（dsh 变体白名单内） |
| `disableModelInvocation` / `modelInvocable` | legacy camelCase：**整个 skill 被拒**，等于凭空消失 | 已同步显式报错，并提示 kebab-case 替代键 |
| `allowed-tools`、`runAs`、`argument-hint` 等 | **静默忽略**——写了不报错、也不生效 | dsh 变体中显式报错（写它等于静默失效，应在变体源码里修） |

补充说明：

- 严格 kebab-case 与 legacy 键检查对**所有变体**生效，不只是 `dsh/`；白名单检查只作用于
  `dsh/` 变体。
- 上游 `handoff` 的 `argument-hint` 就属于"DSH 静默忽略"一类：在 Claude Code 里是参数提示，
  在 DSH 下不起作用。这不算错误，但不要指望它产生效果。
- 变体维度只影响校验；DSH 本身对所有变体一视同仁。

### 会被 DSH 隐藏的上游 skill（17 个）

以下上游 skill 的 frontmatter 带 `disable-model-invocation: true`，因此在 DSH 的
`available_skills` catalog 中**看不到**，属预期行为；用户可用 `/<name>` 触发：

`ask-matt`、`grill-me`、`grill-with-docs`、`handoff`、`implement`、`implement-spec`、
`improve-codebase-architecture`、`loop-me`、`retro`、`setup-matt-pocock-skills`、`teach`、
`to-questionnaire`、`to-spec`、`to-tickets`、`triage`、`wait-what`、`wayfinder`。

其余 15 个上游 skill（`code-review`、`codebase-design`、`diagnosing-bugs`、`domain-modeling`、
`git-guardrails-claude-code`、`grilling`、`migrate-to-shoehorn`、`pr`、`prototype`、`research`、
`scaffold-exercises`、`setup-pre-commit`、`tdd`、`wizard`、`writing-for-agents`）由模型主动调用。

以实际 frontmatter 为准，可用下面命令核对**已安装**的隐藏 skill：

```bash
grep -l '^disable-model-invocation: true' ~/.agents/skills/*/SKILL.md
```

注意：上游源码树里还有几个**未安装**的 `in-progress/` skill（`claude-handoff`、
`setup-ts-deep-modules`、`writing-beats` / `writing-fragments` / `writing-shape`）同样带这个键；
直接统计 `skills/upstream/` 会多算，所以按安装目录核对。

## 3. runtime 变体与 router

6 个 runtime-coupled skill（`autopilot-orchestrator`、`autopilot-implementer`、
`autopilot-reviewer`、`autopilot-director`、`autopilot-distill`、`audit-autopilot`）安装为
router；安装后的布局：

```text
~/.agents/skills/<name>/
├── SKILL.md                      # router：唯一可发现的入口
└── runtime/
    ├── dsh/INSTRUCTIONS.md       # DSH 变体
    ├── reasonix/INSTRUCTIONS.md
    ├── codex/INSTRUCTIONS.md
    ├── kimi/INSTRUCTIONS.md
    └── default/INSTRUCTIONS.md   # 没有 dsh 变体时的回退
```

router 的选择逻辑：从 system context 识别当前运行时 → 读 `runtime/dsh/INSTRUCTIONS.md`
（存在时）→ 否则读 `runtime/default/INSTRUCTIONS.md`。变体源文件位于
`skills/autopilot/<name>/dsh/SKILL.md`（打包时重命名为 `INSTRUCTIONS.md`）。

给变体作者的注意点：

- `dsh/` 变体的 frontmatter 只能用第 2 节白名单里的键；`runAs` / `allowed-tools` 会被校验器拒绝。
- DSH 的 `subagent` 派发的子代理**看不到父会话上下文**，也不保证能读到父会话的技能清单。
  因此派发 prompt 要自包含，并让子代理按**路径**读取技能正文
  （`read ~/.agents/skills/<name>/SKILL.md`，由 router 引导到正确变体），不要假设子代理能自行发现 skill。

### DSH 工具名（变体作者参考）

`read`、`edit`、`write`、`bash`、`grep`、`glob`、`todo_write`、`subagent`、`skill`、
`ask_user_question`、`web_fetch`、`present`、`workflow` 等。

## 4. sandbox 与审批

- DSH 文件沙箱通常为 `workspace-write`：只能写工作区（以及部分平台临时目录）。
- `bash` 在沙箱中可能报 `Operation not permitted`。此时按 [AGENTS.md](../../../AGENTS.md)
  的约定，用 `bash scripts/sandboxed-rust-script.sh` 包一层再跑 `rust-script`。
- 自动化循环类 skill（`autopilot-orchestrator` 等）在 **plan mode** 或**逐步审批**策略下会被
  打断。跑循环前先退出 plan mode，并配置合适的审批策略（例如放行工作区写操作），否则循环会在
  每个审批点停下。

## 5. 验收清单

在新 DSH 会话中执行：

1. **catalog 唯一性** —— 查看会话 system context 的 `available_skills`，每个逻辑 skill 恰好
   出现一次（runtime 变体不会重复出现；隐藏 skill 不出现）。
2. **router 命中** —— 加载 `autopilot-orchestrator`（让模型调用 `skill` 工具），确认它读到的是
   `runtime/dsh/INSTRUCTIONS.md`，而不是 `runtime/default/INSTRUCTIONS.md`。
3. **隐藏 skill 的用户手势** —— 输入 `/handoff`，确认内容以 `<skill_content>` 注入并可执行。

## 相关文档

- [ADR 0036：单一可发现的 runtime router](../../adr/0036-single-discoverable-runtime-router.md)
- [README：Install / How it works](../../../README.md)
- [AGENTS.md：Install model](../../../AGENTS.md)
