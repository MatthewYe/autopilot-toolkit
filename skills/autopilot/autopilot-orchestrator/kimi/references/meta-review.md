# Phase 2: Global Meta-Review

Executed after all Phase 1 issues are resolved (no `ready-for-agent` issues remain). Reviews the entire codebase against ADRs, PRDs, and all issue contracts.

## Purpose

Audit the whole codebase for:
- **Implementation correctness** — does every module satisfy its ACs and PRD global constraints?
- **Cross-module consistency** — pattern drift, duplicate implementations, inconsistent conventions
- **Unplanned changes** — orphan files, undeclared dependencies, stale references

## Execution: parallel review

Orchestrator self-review and reviewer subagent run **in parallel**. Both produce independent reports, then merge.

### 1. Dispatch reviewer subagent

`run_skill(name: "autopilot-reviewer", arguments: "<task>")` — read-only, no edit_file/bash.

Prompt:

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

### 2. Orchestrator self-review

Use grep/glob to cover the same scope as the reviewer subagent:

1. Read all PRDs and relevant ADRs; list every global constraint
2. Verify each constraint with grep/glob scans across the codebase
3. Check AC coverage for every resolved issue against its contract
4. Check cross-module consistency (entry detection, import style, error handling, logging, algorithm choice, file layout)
5. Check unplanned changes (orphan files, undeclared behaviors, side effects, undeleted files)
6. Output structured report: Critical / Important / Suggestion + VERDICT

### 3. Wait for both reports

Both steps 1 and 2 run in parallel. Once both produce independent reports, proceed to merge.

## Report merge

Two independent meta-review reports:
- **Orchestrator self-review** — checked against ADRs, PRDs, and issue contracts
- **Reviewer subagent parallel review** — 4-axis review (Behavior alignment, TDD discipline, Code quality, Plan fidelity)

Merge into `MERGED_META_REPORT` before entering the repair loop:

1. **Union strategy**: Critical and Important issues from either report are included. Suggestion-level items also unioned (deduplicated).
2. **Conflict adjudication**: when reports disagree on the same file/path:
   - **Default to stricter finding**: if uncertain, keep the issue.
   - **Downgrade only on confirmed false positive**: only remove/downgrade when the orchestrator explicitly confirms a false alarm.
   - Record decisions: "冲突裁决：<path> — 采纳 <source> 的结论"
3. **Deduplication**: identical findings (same file + same pattern) in both reports → single entry, marked "双来源一致：<finding>".

`MERGED_META_REPORT` contains:
- Critical entries (merged + deduplicated)
- Important entries (merged + deduplicated)
- Suggestion entries (merged + deduplicated)
- Conflict adjudication records

## Repair loop

Take Critical + Important entries from `MERGED_META_REPORT`. **Orchestrator fixes directly** (no implementer dispatch) — meta issues are usually mechanical:

- **Unify patterns**: inconsistent `isMain` checks → directly edit files to one pattern
- **Delete residue**: orphan files / `__pycache__` / stale references → direct delete/edit
- **Update docs**: SKILL.md / schemas.md / ADR references → direct edit

For design-level questions requiring judgment (e.g. "which of two algorithms to pick?"), add a comment and mark `needs-info`.

### Post-repair verification

1. Run the project's test command — confirm all green
2. Re-run meta-review — confirm 0 Critical + 0 Important
3. **Maximum 2 repair cycles**. After 2 cycles with remaining issues → report residual problems, mark `needs-info`.

### Completion

Report full Phase 1 and Phase 2 results: issue count, total rounds, final statuses, meta-review findings and fixes.

## PRD Resolution

After the meta-review repair loop, resolve PRDs touched in this session:

1. Collect PRDs detected but skipped during scanning (plus any explicitly targeted PRD that was skipped)
2. For each PRD, find child issues: search GitHub for issues with `Parent` links to the PRD; scan local `.scratch/*/issues/*.md` for `Parent:` lines
3. Check if all child issues are `resolved`:
   - All resolved → mark PRD `resolved`: local frontmatter `Status: resolved` + `## Comments` entry; GitHub add `resolved` label + comment `"All child issues resolved + meta-review passed."`
   - Unresolved remain → keep PRD current state, report `"PRD <id> has unresolved child issues: <list>"`
4. Report PRD resolution results
