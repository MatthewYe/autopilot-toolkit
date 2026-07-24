---
name: autopilot-orchestrator
description: "Codex autopilot loop: scan -> implement -> review -> retry via spawn agent, then global meta-review."
---

Before anything else, read `~/.agents/principles/karpathy.md`. Apply Principle 1 "Think Before Analyzing" variant plus Principles 2 and 4.

Execute the autopilot orchestrator workflow below. The implementer and reviewer are Codex custom agents, installed as `~/.codex/agents/*.toml`, and already contain their own methodology. Dispatch them directly by name.

## Issue 来源识别

autopilot supports two issue sources. Determine the source from the explicit `target` argument or from scanning:

| target feature | Source | State machine | Contract |
| --- | --- | --- | --- |
| Path containing `/` | Local `.scratch/` | frontmatter `Status:` | `AGENT-BRIEF.md` |
| `#N` or plain number `N` | GitHub Issue | labels | issue body containing AC |
| No target, local match found | Local `.scratch/` | frontmatter `Status:` | `AGENT-BRIEF.md` |
| No target, GitHub match found | GitHub Issue | labels | issue body |

## 前置约定

### Local Issue Mode

- Treat a path target as an issue directory. If relative, resolve it against the current working directory.
- Expect `<target>/issue.md` to start with YAML frontmatter containing `Status:`.
- Update status by editing only the frontmatter `Status:` line.
- Append comments to the end of the `## Comments` section as `- <timestamp> autopilot: <content>`. If the section is absent, create it at the end of the file.
- Use `<target>/AGENT-BRIEF.md` as the contract.

### GitHub Issue Mode

MCP first: if GitHub MCP tools are registered, prefer them for listing issues, reading issues, editing labels, and adding comments. If MCP is unavailable, fall back to `gh issue ...`; require the `gh` CLI to be installed and authenticated.

- Infer the repository from `git remote -v`.
- Express state with labels: `ready-for-agent`, `in-progress`, `resolved`, `needs-info`.
- Read issues with MCP issue read or fallback `gh issue view <N> --json number,title,body,labels,state`.
- Update labels with MCP issue update if available, or fallback `gh issue edit <N> --add-label "<new>" --remove-label "<old>"`.
- Append comments with MCP issue comment if available, or fallback `gh issue comment <N> --body "..."`.
- Use the issue body sections `## What to build` and `## Acceptance Criteria` as the contract.

### Shared State Mapping

- `Status: ready-for-agent` (local) <-> GitHub label `ready-for-agent`
- `Status: in-progress` <-> GitHub label `in-progress`
- `Status: resolved` <-> GitHub label `resolved`
- `Status: needs-info` <-> GitHub label `needs-info`

## PRD 检测与跳过

A PRD describes the whole design and should not be dispatched to an implementer. Concrete child issues carry implementation contracts.

Detect PRDs by either:

- Optional marker: local frontmatter `Type: prd` or GitHub label `prd`.
- Content pattern: body contains `## Problem Statement` and `## Solution`, and does not contain `## What to build` or `## Acceptance Criteria`.

When a PRD is detected, skip it and report:

```text
<id> is a PRD, not directly implementable. Process its child issues instead.
```

Do not change the PRD status during Phase 1.

## 如果指定了 target

### target 是路径（含 `/`）

1. Confirm `<target>/issue.md` exists; if absent, report the error and stop.
2. Confirm `<target>/AGENT-BRIEF.md` exists; if absent, report the error and stop.
3. Read `<target>/issue.md`; continue only when `Status:` is `ready-for-agent` or `in-progress`.
4. If the status is different, report the current status and stop.
5. Run PRD detection. If matched, report the PRD skip message and stop.
6. Update `Status:` to `in-progress`.
7. Set `source = local`, `id = <target>`, and `contract = <target>/AGENT-BRIEF.md` contents.
8. Infer the feature directory from the issue directory, for example `.scratch/auth/issues/01-login/` -> `.scratch/auth/`.
9. Continue to "交叉 Issue Suggestion 匹配".

### target 是 GitHub issue 号（`#N` 或纯数字 `N`）

1. Extract the numeric issue number.
2. Read issue number, title, body, labels, and state.
3. Continue only when labels include `ready-for-agent` or `in-progress`.
4. If neither label is present, report the current labels and stop.
5. Run PRD detection. If matched, report `#<N> is a PRD, not directly implementable. Process its child issues instead.` and stop.
6. Replace `ready-for-agent` with `in-progress`.
7. Add comment `autopilot: 开始处理`.
8. Extract `## What to build` and `## Acceptance Criteria` as the contract.
9. Set `source = github`, `id = #<N>`, `contract = <parsed contract>`, and `IS_GITHUB: true`.
10. Generate a feature slug from the issue title and infer `.scratch/<feature-slug>/`.
11. Continue to "交叉 Issue Suggestion 匹配".

## 否则（无参数）：扫描模式

Scan both sources.

### Local Scan

1. Glob `.scratch/*/issues/*.md`.
2. For each file, read the frontmatter and select entries with `Status: ready-for-agent`.
3. Run PRD detection on each candidate. Exclude PRDs from the dispatch queue and record them as skipped.
4. Sort implementable local matches by natural path order.

### GitHub Scan

1. List open issues with label `ready-for-agent`, up to 50.
2. Filter out entries with label `prd`.
3. For remaining entries, read the body and apply content-based PRD detection. Exclude PRDs and record them as skipped.
4. Sort implementable GitHub matches by issue number.

### Select

1. Merge local and GitHub implementable candidates, preferring local candidates first.
2. Report all found implementable issues and any skipped PRDs.
3. If no implementable issue remains, enter "Phase 2: 全局 Meta-Review".
4. Choose the first candidate and run the matching target initialization flow above.

## Phase 1: 调度循环

Maintain `retry_count = 0`. 最多 3 轮:

- `retry_count = 0`: first implementation
- `retry_count = 1`: first retry
- `retry_count = 2`: second retry
- `retry_count >= 3`: mark `needs-info`

### 更新状态（抽象）

- Local: edit `issue.md` frontmatter `Status:`.
- GitHub: update labels through MCP or `gh issue edit`.

### 追加注释（抽象）

- Local: append to `## Comments`.
- GitHub: add an issue comment through MCP or `gh issue comment`.

### 交叉 Issue Suggestion 匹配

Before dispatching the implementer, check whether `.scratch/<feature>/suggestions.json` exists and contains entries with `status: "pending"`.

Use the algorithm in `references/suggestion-matching.md`:

1. Infer the feature directory from the local issue path or GitHub issue title.
2. Read pending suggestions.
3. Match by file-path substring or case-insensitive keyword substring against the current contract text.
4. Pass matched entries as `CROSS_ISSUE_SUGGESTIONS` JSON.
5. If no entries match, omit `CROSS_ISSUE_SUGGESTIONS`.

### Pre-flight Toolchain Detection

Before implementer dispatch:

1. Infer the project test command: Rust -> `cargo test`, Node -> `npm test`, Python -> `pytest` or `uv run pytest`.
2. Check whether the tool exists with `which <tool>`.
3. Try common install paths if needed, such as `~/.cargo/bin/cargo`.
4. Set `TOOLCHAIN: available` or `TOOLCHAIN: unavailable` in the implementer task.

### REFACTORING Mode Detection

Detect whether the issue is a pure refactor:

1. Scan the contract for keywords such as `replace`, `consolidate`, `extract`, `delete`, `Remove`, `Replace`, `inline`, `shared function`, and `duplicated`.
2. Mark `REFACTORING: true` if 2+ refactor keywords appear and the contract does not describe a new feature.
3. Mark `REFACTORING: true` if every AC is about replacement or deletion rather than new behavior.
4. Otherwise set `REFACTORING: false`.

### Execute Implementer

Spawn the Codex implementer custom agent:

```text
spawn agent autopilot-implementer with task: "<task description>"
```

The task description must include:

- `source`
- `id`
- `contract`
- `TOOLCHAIN: available|unavailable`
- `REFACTORING: true|false`
- `ROUND: <retry_count>`
- On retry rounds, `PREV_REVIEW: <previous REVIEWER_REPORT>`
- Matched `CROSS_ISSUE_SUGGESTIONS`, if any
- Local mode: absolute issue directory path
- GitHub mode: issue body and `IS_GITHUB: true`

Wait for the implementer result and parse `IMPLEMENTER_REPORT:`.

Empty result handling:

1. If the result has no `IMPLEMENTER_REPORT:` header, retry the same implementer task once.
2. If the second result is also empty or missing the header, mark `needs-info`, add the raw result to the issue comment, and stop this issue.

Parse tolerance:

- If `IMPLEMENTER_REPORT:` is present but required fields are missing, mark `needs-info`, add the raw result, and stop this issue.

### First-Round SELF_REVIEW Check

On `retry_count = 0`, require a `SELF_REVIEW:` section:

- `STATUS: DONE`: accept if it says either no issues were found or issues were found and fixed.
- `STATUS: UNVERIFIED`: accept if each AC has a verification note, or if the section explicitly states verification is incomplete.
- Missing `SELF_REVIEW:` with `STATUS: DONE` or `STATUS: UNVERIFIED`: mark `needs-info` and stop.

Do not require this check on retry rounds.

### Collect SIBLING_CONTEXT

Before reviewer dispatch, collect already resolved sibling issue context for the same PRD:

1. Extract the PRD parent link from the current issue body if present.
2. List resolved sibling issues.
3. Summarize each sibling as `#N title - key conventions: ...`.
4. Pass this as `SIBLING_CONTEXT`.

### Handle Implementer Status

Parse `STATUS:` from `IMPLEMENTER_REPORT`.

- `STATUS: DONE`: dispatch reviewer normally.
- `STATUS: UNVERIFIED`: dispatch reviewer with `UNVERIFIED: true` and the full `SELF_REVIEW` section. Reviewer should focus on structural correctness and may return `VERIFY_NEEDED`.
- `STATUS: BLOCKED` or `STATUS: NEEDS_CONTEXT`: mark `needs-info`, add the reason, and stop this issue.

### Dispatch Reviewer

Spawn the Codex reviewer custom agent:

```text
spawn agent autopilot-reviewer with task: "<task description>"
```

The reviewer task description must include:

- `source`
- `id`
- `contract`
- `CHANGED_FILES`
- `SIBLING_CONTEXT`
- Previous `REVIEWER_REPORT`, if any
- `UNVERIFIED: true` and full `SELF_REVIEW` when implementer status is `UNVERIFIED`
- GitHub mode: `IS_GITHUB: true`

Wait for the reviewer result and parse `REVIEWER_REPORT:`.

If the result has no `REVIEWER_REPORT:` header, mark `needs-info`, add the raw result, and stop this issue.

### Parse SUGGESTION_RESOLUTIONS

When implementer status is `DONE`, parse a `SUGGESTION_RESOLUTIONS:` section if present:

1. If absent or `无`, skip.
2. Parse lines with format:
   ```text
   [resolved|rejected|deferred] 来源 <source_issue> round <N>: <content summary> -> <detail>
   ```
3. Store each parsed entry as `type`, `source_issue`, `round`, `summary`, and `detail`.
4. Keep the parsed entries in `pending_resolutions` until reviewer returns `MERGE`.

### Extract and Persist Reviewer Suggestions

After every reviewer result, regardless of verdict, parse `## Suggestion` items:

1. Read each `- [ ]` item under `## Suggestion`.
2. Extract `content`, optional `KEYWORDS:`, and optional `FILES:`.
3. If keywords are missing, infer 2-5 representative terms from the content.
4. If files are missing, infer from implementer `CHANGED_FILES`.
5. Write entries to `.scratch/<feature>/suggestions.json`, creating the file as `[]` if absent.
6. Deduplicate by exact `content`.
7. New entry schema:
   ```json
   { "issue": "<issue-slug-or-#N>", "round": <retry_count>, "content": "...", "files": [], "keywords": [], "status": "pending" }
   ```
8. In GitHub mode, add a comment for each new suggestion:
   ```text
   autopilot suggestion [pending]: <content>
   ```

Only propagate `Suggestion` items. Critical and Important findings must be resolved in the current issue.

### Handle Reviewer Verdict

Parse `VERDICT:` from `REVIEWER_REPORT`.

- `MERGE`: mark issue `resolved`, add reviewer conclusion, apply pending suggestion resolution updates, then return to scanning for the next issue.
- `VERIFY_NEEDED`: reviewer considers structure correct but tool verification is incomplete.
  1. Try to run the inferred project test command from the orchestrator environment.
  2. If tests pass, mark `resolved` and comment `Orchestrator verified: all tests pass`.
  3. If tests fail or the toolchain remains unavailable, mark `needs-info` and comment that manual verification is required.
  4. Preserve reviewer suggestions either way.
- `RETRY`: increment `retry_count`, clear `pending_resolutions`, and repeat implementer dispatch with `PREV_REVIEW`.
  - If `retry_count < 3`, continue.
  - If `retry_count >= 3`, mark `needs-info`, comment with the reviewer problem list and max-retry note, then return to scanning.
- `BLOCKED`: mark `needs-info`, comment with reviewer conclusion, then return to scanning.

Missing or unknown verdict: mark `needs-info`, comment with the raw reviewer result, and stop this issue.

### Update Suggestion 状态

When reviewer verdict is `MERGE`, update matching entries in `.scratch/<feature>/suggestions.json` according to `pending_resolutions`:

1. Match by `issue == source_issue`, numeric `round`, and `summary` appearing as a substring of `content`.
2. If multiple entries match, prefer the one whose `files` overlap most with current `CHANGED_FILES`.
3. If still tied, prefer the longest summary/content match.
4. If ambiguity remains, skip that resolution and report it for human handling.
5. Only update entries whose current `status` is `pending`.
6. Apply status transitions:

| Resolution type | New status | Fields |
| --- | --- | --- |
| `resolved` | `resolved` | `resolved_in_issue: <current issue>` |
| `rejected` | `rejected` | `rejected_reason: <detail>` |
| `deferred` | keep `pending` | `deferred_by: <current issue>` |

In GitHub mode, add comments for resolved and rejected suggestion updates:

```text
autopilot suggestion [resolved|rejected]: <content summary>
```

### Phase 1 Exit

When scanning finds no implementable `ready-for-agent` issues, Phase 1 is complete. Enter Phase 2.

## Phase 2: 全局 Meta-Review

Run Phase 2 after every Phase 1 issue is resolved or moved out of the ready queue.

### Purpose

Audit the whole codebase against:

- All ADRs under `docs/adr/`
- All PRDs under `docs/prd/`
- All resolved issue contracts, from local `AGENT-BRIEF.md` files or GitHub issue bodies

Review dimensions:

1. ADR/PRD global constraints and plan fidelity.
2. Cross-module consistency: entry patterns, import style, error handling, logging, algorithms, and file layout.
3. Unplanned changes: orphan files, undeclared dependencies, stale references, undeleted files, and hidden side effects.
4. AC coverage for every resolved issue.

### Parallel Review

Start two independent reviews:

1. Orchestrator self-review using local searches and file reads.
2. Spawn reviewer for an independent read-only global review:

```text
spawn agent autopilot-reviewer with task: "Perform global meta-review over the whole codebase against ADRs, PRDs, and resolved issue contracts. Report Critical, Important, Suggestion, and VERDICT."
```

Wait for the reviewer result while completing the self-review.

### Merge Reports

Merge the self-review report and reviewer report into `MERGED_META_REPORT`:

1. Include the union of all Critical and Important findings.
2. Include deduplicated Suggestion findings.
3. For disagreements, default to the stricter finding unless the orchestrator confirms a false positive.
4. Record conflict decisions as `冲突裁决: <path> - adopted <source> conclusion`.
5. Mark identical findings as `双来源一致: <finding>`.

### Repair Loop

Fix Critical and Important findings directly from the orchestrator when they are mechanical:

- Unify inconsistent patterns.
- Delete residue or stale files.
- Update docs and references.

For design questions that need human judgment, comment and mark `needs-info`.

After each repair cycle:

1. Run the project test command.
2. Re-run meta-review.
3. Stop after 2 repair cycles. If Critical or Important findings remain, report residual issues and mark `needs-info`.

### PRD Resolution

After meta-review repairs:

1. Collect PRDs skipped during scanning plus explicitly targeted PRDs.
2. Find child issues by `Parent` links in GitHub issue bodies and local issue files.
3. If every child is `resolved`, mark the PRD `resolved` and comment `All child issues resolved + meta-review passed.`
4. If unresolved children remain, keep the PRD current state and report the unresolved list.

## FINAL_ACCEPTANCE_REPORT

After Phase 2 repairs, produce the cross-issue suggestion acceptance report described in `references/acceptance-report.md`:

1. Scan `.scratch/*/suggestions.json`.
2. In GitHub mode, also aggregate comments matching `autopilot suggestion [<status>]: <body>` from processed issues.
3. Group suggestions by `pending`, `rejected`, and `resolved`.
4. Output with header `FINAL_ACCEPTANCE_REPORT:`.
5. Verify that resolved entries have `resolved_in_issue`, rejected entries have `rejected_reason`, pending entries are not incorrectly marked resolved, counts match, and no entry has empty `content`.
