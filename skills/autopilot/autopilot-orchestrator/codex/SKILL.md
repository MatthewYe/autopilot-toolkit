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
| Path containing `/` | Local `.scratch/` | frontmatter `status:` | flat `issue_file` body |
| `#N` or plain number `N` | GitHub Issue | labels | issue body containing AC |
| No target, local match found | Local `.scratch/` | frontmatter `status:` | flat `issue_file` body |
| No target, GitHub match found | GitHub Issue | labels | issue body |

## 前置约定

### Local Issue Mode

- Canonical issues are flat `.scratch/<feature>/issues/<NN-slug>.md` files. Carry the selected path as `issue_file`.
- Require lower-case `key`, `title`, `type`, `status`, and `parent` frontmatter plus exact `## What to build`, `## Acceptance Criteria`, `## Blocked by`, and `## Comments` headings.
- Update state by editing only the lower-case `status:` line in `issue_file`.
- Extract the contract from `## What to build` and `## Acceptance Criteria` in the same file, and append comments to its existing `## Comments` section.
- Legacy issue directories containing `issue.md` plus `AGENT-BRIEF.md` remain compatibility inputs. Detect them explicitly, use `AGENT-BRIEF.md` as their contract, and preserve lifecycle writes in legacy `issue.md`; never require that directory shape for canonical flat files.

### GitHub Issue Mode

MCP first: if GitHub MCP tools are registered, prefer them for listing issues, reading issues, editing labels, and adding comments. If MCP is unavailable, fall back to `gh issue ...`; require the `gh` CLI to be installed and authenticated.

- Infer the repository from `git remote -v`.
- Express state with labels: `ready-for-agent`, `in-progress`, `resolved`, `needs-info`.
- Read issues with MCP issue read or fallback `gh issue view <N> --json number,title,body,labels,state`.
- Update labels with MCP issue update if available, or fallback `gh issue edit <N> --add-label "<new>" --remove-label "<old>"`.
- Append comments with MCP issue comment if available, or fallback `gh issue comment <N> --body "..."`.
- Use the issue body sections `## What to build` and `## Acceptance Criteria` as the contract.

### Shared State Mapping

- `status: ready-for-agent` (canonical local) <-> GitHub label `ready-for-agent`
- `status: in-progress` <-> GitHub label `in-progress`
- `status: resolved` <-> GitHub label `resolved`
- `status: needs-info` <-> GitHub label `needs-info`

### AFK Continuation Contract

This contract inherits HITL/AFK taxonomy from wayfinder: AFK tickets are independently
driven by the agent without real-time human involvement.

AFK-breaking behavior: pausing for human input when no product/scope decision is needed.

**Principles**

1. **Autonomous Progress** — Agent drives itself, requesting human input only when the
   contract cannot cover product/scope decisions. Engineering ambiguity is resolved by
   judgment and does not block progress.

2. **Contract Authority** — AGENT-BRIEF (or equivalent issue body AC) is the sole
   authoritative contract for AFK execution. Agent does not extend beyond the contract
   scope or downgrade implementations not required by the contract.

3. **Transparent Completion** — External limitations (permissions, toolchain, unavailable
   dependencies) must be explicitly recorded. Completed but unverified implementations
   are declared UNVERIFIED, never falsely claimed as DONE.

4. **Authority Boundaries** — Agent does not escalate permissions or override parent
   agent configurations. Child agents inherit the parent agent's execution boundaries
   and model selections.

5. **Evidence Integrity** — Verification evidence is auditable and reproducible. Collected
   evidence is reusable when code and environment are unchanged, invalidated upon change.

6. **Continuation** — After task completion or blocking, the agent retains context and
   continues to the next runnable objective within scope.

**Execution Specification**

- Prioritize AGENT-BRIEF as the authoritative contract; issue body supplements context
  but does not override contract terms.
- External failures (permissions, network, missing toolchain) record `BLOCKER_TYPE:
  external-unavailable`, with the unexecuted command and reason in `TEST_EVIDENCE`.
- Only product/scope ambiguity the contract cannot cover records `BLOCKER_TYPE:
  human-decision` and `needs-info`. Engineering-judgment ambiguity does not escalate
  to human-decision.
- `TEST_EVIDENCE` is cached by `command + WORK_BASE + worktree fingerprint`; reused
  when the worktree is unchanged, invalidated after code changes.
- Diagnostic workflows in AFK mode do not block waiting for the user — continue testing
  hypotheses by priority.
- After deferral in scan mode, return to the next runnable issue on the scanning frontier;
  explicit target mode ends the current round.

## Spec 检测与跳过

A spec describes the whole design and should not be dispatched to an implementer. Concrete child tickets carry implementation contracts.

Detect specs by either:

- Optional marker: local frontmatter `Type: spec` or legacy `Type: prd`, or GitHub label `spec` or legacy `prd`.
- Content pattern: body contains `## Problem Statement` and `## Solution`, and does not contain `## What to build` or `## Acceptance Criteria`.

When a spec is detected, skip it and report:

```text
<id> is a spec, not directly implementable. Process its child tickets instead.
```

Do not change the spec status during Phase 1.

## 如果指定了 target

### target 是路径（含 `/`）

1. Resolve `<target>` against the current worktree.
2. If it is a `.md` file, treat it as canonical `issue_file`: validate the required frontmatter and headings, continue only for `status: ready-for-agent` or `status: in-progress`, run spec detection, edit only `status:` to `in-progress`, extract the contract from the same file, and derive the feature directory from `.scratch/<feature>/issues/<file>.md`.
3. Otherwise, if it is a legacy issue directory, require `issue.md` plus `AGENT-BRIEF.md`, use the brief as the contract without changing the directory shape, and continue through the legacy compatibility path.
4. Otherwise report that the target is neither a canonical issue_file nor a supported legacy issue directory.
5. Set `source = local`, `id = issue_file` for canonical input (or the directory for legacy input), then continue to suggestion matching.

### target 是 GitHub issue 号（`#N` 或纯数字 `N`）

1. Extract the numeric issue number.
2. Read issue number, title, body, labels, and state.
3. Continue only when labels include `ready-for-agent` or `in-progress`.
4. If neither label is present, report the current labels and stop.
5. Run spec detection. If matched, report `#<N> is a spec, not directly implementable. Process its child tickets instead.` and stop.
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
2. For each file, validate canonical frontmatter, select entries with `status: ready-for-agent`, and carry the path as `issue_file`.
3. Run spec detection on each candidate. Exclude specs from the dispatch queue and record them as skipped.
4. Sort implementable local matches by natural path order.

#### LOCAL_ISSUE_DEDUP_CONTRACT

Before sorting or dispatching local candidates, group files with identical full Markdown bytes (a SHA-256 comparison is sufficient) and retain one candidate per group. If an identical group contains both `.scratch/distill-tracer/issues/` and another local issue path, prefer the non-tracer path; otherwise keep the first path in natural order. Report every suppressed duplicate path. A sole issue under `.scratch/distill-tracer/issues/` remains implementable and must not be excluded.

### GitHub Scan

1. List open issues with label `ready-for-agent`, up to 50.
2. Filter out entries with label `spec` or legacy `prd`.
3. For remaining entries, read the body and apply content-based spec detection. Exclude specs and record them as skipped.
4. Sort implementable GitHub matches by issue number.

### Select

1. Merge local and GitHub implementable candidates, preferring local candidates first.
2. Report all found implementable issues and any skipped specs.
3. If no implementable issue remains, enter "Phase 2: 全局 Meta-Review".
4. Choose the first candidate and run the matching target initialization flow above.

## Recovery Decision Model

Decide behavior based on two dimensions, not a count:

### Decision Matrix

|              | New evidence (improved)    | No new evidence          |
|--------------|----------------------------|--------------------------|
| Agent-recoverable | RETRY                   | Stop (stall)             |
| Agent-unrecoverable | UNVERIFIED            | needs-info / exhausted   |

- **Recoverability**: engineering bugs, test failures, naming/structure errors → recoverable; missing toolchain, insufficient permissions, contract ambiguity, directional errors → unrecoverable
- **Evidence Progress**: this round tried a different strategy / narrowed problem scope → new evidence; same error, same reasoning path repeats → no new evidence

### Failure Classification

| Failure type | Decision | BLOCKER_TYPE |
|---|---|---|
| Recoverable + new evidence | RETRY, continue | — |
| Recoverable + no new evidence | Stop, stall | exhausted |
| Environment (toolchain/permissions/external unreachable) | UNVERIFIED | external-unavailable |
| Authority (contract ambiguity, missing product/scope decision) | needs-info | human-decision |
| Terminal (systematic failure, cap triggered) | Stop | exhausted |

### Anti-Cheat Mechanism

Agent does not self-judge whether to stop. Pre-cap adjudication authority is exercised only by the orchestrator after cap is triggered.

#### Rationalization Table

Orchestrator checks this table before adjudicating:

| Agent's possible stop reason | Required evidence |
|---|---|
| "No progress" | PREV_REVIEW Critical list identical to current REVIEWER_REPORT + implementer CHANGED_FILES unchanged from previous round |
| "Unfixable" | 2+ different implementation strategies attempted + reviewer confirms none satisfy AC |
| "Contract gap" | Reviewer report explicitly states "AC insufficient to judge correctness" or "missing product decision" |

#### Stall Detection

Consecutive 2 rounds meeting ALL of the following → trigger `BLOCKER_TYPE: exhausted`, record reviewer issue list, and stop:

1. PREV_REVIEW and current REVIEWER_REPORT Critical lists are identical (same items, same file paths)
2. implementer CHANGED_FILES unchanged from previous round (same file count delta)
3. implementer made no new strategy attempt (no explicit strategy switch in SUMMARY or SELF_REVIEW)

Iteration termination rules:
- Any reviewer report with Authority-type (contract gap) Critical/Important → transition to `needs-info`, do not continue iterating
- Stall detection triggered → `exhausted`, stop
- Decision matrix "Recoverable + new evidence" → continue iteration
- Decision matrix "Recoverable + no new evidence" → `exhausted`
- Decision matrix "Agent-unrecoverable" → `needs-info` or `external-unavailable`

---

## Phase 1: 调度循环

Maintain `retry_count = 0` for round tracking and suggestion matching. No hard round cap — iteration termination is decided by the decision matrix + stall detection:

- `retry_count = 0`: first implementation

### 更新状态（抽象）

- Local canonical: edit the canonical `status:` line in `issue_file`.
- Local legacy: edit the `Status:` line in legacy `issue.md`.
- GitHub: update labels through MCP or `gh issue edit`.

### 追加注释（抽象）

- Local canonical: append to `## Comments` in `issue_file`.
- Local legacy: append to `## Comments` in legacy `issue.md`.
- GitHub: add an issue comment through MCP or `gh issue comment`.

### 交叉 Issue Suggestion 匹配

Before dispatching the implementer, check whether `.scratch/<feature>/suggestions.json` exists and contains entries with `status: "pending"`. Match them against the current contract: the canonical `issue_file` body or legacy `AGENT-BRIEF.md`.

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
- Local canonical mode: canonical `issue_file` absolute path
- Local legacy mode: legacy issue directory absolute path
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

Before reviewer dispatch, collect already resolved sibling ticket context for the same spec:

1. Extract the spec parent link from the current issue body if present.
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
  - Follow stall detection rules:
    - Contract-gap Critical/Important → mark `needs-info`
    - Stall detection triggered or decision matrix "no new evidence" → record `BLOCKER_TYPE: exhausted`, comment with the reviewer problem list, defer, then return to scanning.
    - Decision matrix "Recoverable + new evidence" → repeat implementer dispatch with `PREV_REVIEW`.
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
- All resolved issue contracts, from canonical local `issue_file` bodies, legacy `AGENT-BRIEF.md` files, or GitHub issue bodies

Review dimensions:

1. ADR/spec global constraints and plan fidelity.
2. Cross-module consistency: entry patterns, import style, error handling, logging, algorithms, and file layout.
3. Unplanned changes: orphan files, undeclared dependencies, stale references, undeleted files, and hidden side effects.
4. AC coverage for every resolved issue.

### Parallel Review

Start two independent reviews:

1. Orchestrator self-review using local searches and file reads.
2. Spawn reviewer for an independent read-only global review:

```text
spawn agent autopilot-reviewer with task: "Perform global meta-review over the whole codebase against ADRs, specs, and resolved issue contracts. Report Critical, Important, Suggestion, and VERDICT."
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

### Spec Resolution

After meta-review repairs:

1. Collect specs skipped during scanning plus explicitly targeted specs.
2. Find child tickets by `Parent` links in GitHub issue bodies and local issue files.
3. If every child is `resolved`, mark the spec `resolved` and comment `All child tickets resolved + meta-review passed.`
4. If unresolved children remain, keep the spec current state and report the unresolved list.

## FINAL_ACCEPTANCE_REPORT

After Phase 2 repairs, produce the cross-issue suggestion acceptance report described in `references/acceptance-report.md`:

1. Scan `.scratch/*/suggestions.json`.
2. In GitHub mode, also aggregate comments matching `autopilot suggestion [<status>]: <body>` from processed issues.
3. Group suggestions by `pending`, `rejected`, and `resolved`.
4. Output with header `FINAL_ACCEPTANCE_REPORT:`.
5. Verify that resolved entries have `resolved_in_issue`, rejected entries have `rejected_reason`, pending entries are not incorrectly marked resolved, counts match, and no entry has empty `content`.
