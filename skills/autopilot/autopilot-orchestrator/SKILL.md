---
name: autopilot-orchestrator
description: "Autopilot issue resolution loop: scan, dispatch implementer and reviewer sub-agents per ready-for-agent issue, then run global meta-review. Use when processing autopilot issues from any source."
---

Execute the autopilot orchestrator workflow below.

## Sub-agent dispatch model

This orchestrator dispatches implementer and reviewer as sub-agents. Use your runtime's sub-agent mechanism (for example, a spawn-subagent tool or agent-dispatcher). In each dispatch prompt:

- The first paragraph must instruct the sub-agent to read and follow its skill definition (autopilot-implementer or autopilot-reviewer).
- The second paragraph is the task description (formats defined below under "Execute implementer" and "Process implementer result").
- Wait for the sub-agent to return, then parse `IMPLEMENTER_REPORT:` / `REVIEWER_REPORT:` from the response text.

For file operations, use your runtime's native tools: read files, edit frontmatter lines, write structured output, scan directories, search content, and run shell commands.

## Issue source identification

autopilot supports two issue sources. Determine which based on the `target` parameter or scan results:

| target characteristic | source | state machine | contract file |
|---|---|---|---|
| Path containing `/` | Local `.scratch/` | frontmatter `status:` | flat `issue_file` body |
| `#N` or bare number `N` | GitHub Issue | labels | issue body (contains AC) |
| No parameter, scan finds local | Local `.scratch/` | frontmatter `status:` | flat `issue_file` body |
| No parameter, scan finds GitHub | GitHub Issue | labels | issue body |

## Prerequisites

### Local issue mode

- Canonical issues are flat `.scratch/<feature>/issues/<NN-slug>.md` files. Carry the selected path as `issue_file`.
- Require lower-case `key`, `title`, `type`, `status`, and `parent` frontmatter plus exact `## What to build`, `## Acceptance Criteria`, `## Blocked by`, and `## Comments` headings.
- Update state by editing only the lower-case `status:` line in `issue_file`.
- Extract the contract from `## What to build` and `## Acceptance Criteria` in the same file, and append comments to its existing `## Comments` section.
- Legacy issue directories containing `issue.md` plus `AGENT-BRIEF.md` are read-only compatibility inputs. Detect them explicitly; never require that directory shape for canonical flat files.

### GitHub Issue mode

- Infer the repo from `git remote -v`.
- State is expressed via labels: `in-progress`, `resolved`, `needs-info`.
- Append comments: `gh issue comment <N> --body "..."`.
- Contract comes from the issue body (contains Acceptance Criteria and What to build, created by `to-tickets`).
- Read issue: `gh issue view <N> --json number,title,body,labels,state`.

### Shared concepts

- `status: ready-for-agent` (canonical local frontmatter) ↔ label `ready-for-agent` (GitHub)
- `status: in-progress` ↔ label `in-progress`
- `status: resolved` ↔ label `resolved`
- `status: needs-info` ↔ label `needs-info`

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

---

## Spec detection and skip

A spec describes overall design and does not contain directly-implementable `## Acceptance Criteria` or `## What to build`. Specs should not be dispatched to the implementer — child tickets carry the concrete work.

### Detection signals (bidirectional compatible, zero upstream dependency)

| signal | local markdown | GitHub |
|---|---|---|
| **Primary** (content pattern) | body contains `## Problem Statement` + `## Solution` but **not** `## What to build` or `## Acceptance Criteria` | same |
| **Accelerator** (optional) | frontmatter `Type: spec` or legacy `Type: prd` | label `spec` or legacy `prd` |

### Behavior

Whether an explicit target is given or during scan mode, when a spec is detected:
- **Skip** — do not enter the Phase 1 dispatch loop.
- Respond with the reason: `"<id> is a spec, not directly implementable. Process its child tickets instead."`
- Do not modify the spec's status (keep original, processed in Phase 2).

---

## If a target is specified

### target is a path (contains `/`)

1. Resolve `<target>` against the worktree.
2. If it is a `.md` file, treat it as canonical `issue_file`: validate the required frontmatter and headings, require `status` to be `ready-for-agent` or `in-progress`, run spec detection, edit only `status:` to `in-progress`, extract the contract from the same file, and derive the feature directory from `.scratch/<feature>/issues/<file>.md`.
3. Otherwise, if it is a legacy issue directory, require `<target>/issue.md` and `<target>/AGENT-BRIEF.md`, read them without changing their shape, and use the brief as the contract.
4. Otherwise report that the target is neither a canonical issue_file nor a supported legacy issue directory.
5. Set `source = "local"`, `id = issue_file` for canonical input (or the directory for legacy input), then jump to suggestion matching.

### target is a GitHub issue number (`#N` or bare number `N`)

Extract the numeric part as `issueNumber`:

1. `gh issue view <issueNumber> --json number,title,body,labels,state` to get issue info.
2. Check labels contain `ready-for-agent` or `in-progress`.
3. Otherwise — report current state and stop.
4. **Spec detection**: check labels for `spec` or legacy `prd`, or body for spec content pattern. If hit — respond and stop.
5. Replace `ready-for-agent` label with `in-progress`: `gh issue edit <issueNumber> --add-label "in-progress" --remove-label "ready-for-agent"`.
6. Append comment: `gh issue comment <issueNumber> --body "autopilot: starting work"`.
7. Extract Acceptance Criteria and What to build from issue body as contract text.
8. Set `source = "github"`, `id = <issueNumber>`, `contract = <parsed contract text>`.
9. Generate feature slug from issue title (e.g. `Implement Suggestion matching` → `suggestion-matching` → `.scratch/suggestion-matching/`).
10. Jump to "Cross-issue Suggestion matching".

---

## Otherwise (no parameter): scan mode

Scan both sources simultaneously:

### Local scan

1. Scan `.scratch/*/issues/*.md` recursively.
2. For each file, validate canonical frontmatter, select `status: ready-for-agent`, and carry the path as `issue_file`.
3. For matches, check for spec: read frontmatter `Type: spec` or legacy `Type: prd` field, or body for spec content pattern. Spec entries are **excluded from the dispatch queue**, recorded separately.
4. Collect all non-spec matches in natural path order.

#### LOCAL_ISSUE_DEDUP_CONTRACT

Before sorting or dispatching local candidates, group files with identical full Markdown bytes (a SHA-256 comparison is sufficient) and retain one candidate per group. If an identical group contains both `.scratch/distill-tracer/issues/` and another local issue path, prefer the non-tracer path; otherwise keep the first path in natural order. Report every suppressed duplicate path. A sole issue under `.scratch/distill-tracer/issues/` remains implementable and must not be excluded.

### GitHub scan

5. `gh issue list --label "ready-for-agent" --state open --json number,title,labels --limit 50`.
6. Filter out entries with label `spec` or legacy `prd`.
7. For remaining entries, use `gh issue view <N> --json body` to check body for spec content pattern. Matching entries excluded from dispatch queue, recorded separately.
8. Collect all non-spec matches.

### Select and report

9. Merge non-spec results from both sources. List found implementable issues, also report skipped spec count (e.g. "skipped 1 spec: #12").
10. Pick the first (local-first, then GitHub, each in natural order), announce which is being processed.
11. If zero implementable issues — jump to "Phase 2: Global meta-review".
12. Based on the selected issue's source, follow the corresponding initialization flow.

---

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

## Phase 1: Dispatch loop

Maintain `retry_count = 0` for round tracking and suggestion matching. No hard round cap — iteration termination is decided by the decision matrix + stall detection:
- retry_count = 0: first implementation

### Update status (abstract)

- **local**: edit the canonical `status:` line in `issue_file`
- **github**: `gh issue edit <N> --add-label "<new>" --remove-label "<old>"`

### Append comment (abstract)

- **local**: append the entry to the `## Comments` section in `issue_file`
- **github**: `gh issue comment <N> --body "<timestamp> autopilot: <content>"`

### Cross-issue Suggestion matching

Before dispatching implementer, if `.scratch/<feature>/suggestions.json` exists and has `status: "pending"` entries, match against the current issue's AGENT-BRIEF using the algorithm in `references/suggestion-matching.md`. Pass matched entries as `CROSS_ISSUE_SUGGESTIONS` to implementer; otherwise skip.

### Execute implementer

#### Pre-flight: toolchain detection

Before dispatching implementer, detect whether the project toolchain is available:

1. Infer test command from project type (Rust → `cargo test`, Node → `npm test`, Python → `pytest` or `uv run pytest`).
2. Run `which <tool>` to check toolchain presence (e.g. `which cargo`, `which npm`).
3. If unavailable, try common install paths (`~/.cargo/bin/cargo`, `~/.rustup/toolchains/*/bin/cargo`).
4. Set `TOOLCHAIN: available` or `TOOLCHAIN: unavailable`, pass into implementer's dispatch prompt.

#### Pre-flight: REFACTORING mode detection

Analyze contract content to detect whether the current issue is a pure refactoring task:

1. Scan contract keywords: `replace`, `consolidate`, `extract`, `delete`, `Remove`, `Replace`, `inline`, `shared function`, `duplicated` — hit 2+ and lacking `Add`, `new feature`, `Implement` (as new feature) — flag `REFACTORING: true`.
2. Cross-check AC: if all ACs describe "replace" or "delete" rather than "new feature" — `REFACTORING: true`.
3. Set `REFACTORING: true|false`, pass into implementer's dispatch prompt.

#### Dispatch

Use your runtime's sub-agent mechanism to dispatch an implementer sub-agent. Prompt format:

```
Read the autopilot-implementer skill definition and follow its methodology strictly.

<task description below>

<dynamically generated based on retry_count and mode>
```

Task description conveys:
- **Common**: `source`, `id`, `contract` (contract content), `TOOLCHAIN: <available|unavailable>`, `REFACTORING: <true|false>`, plus:
  - First run (retry_count = 0): `ROUND: 0`
  - Retry (retry_count >= 1): `ROUND: <retry_count>` + `PREV_REVIEW: <full text of previous REVIEWER_REPORT>`
  - If matched CROSS_ISSUE_SUGGESTIONS exist, include them.
- **Local mode**: additionally pass the issue directory absolute path.
- **GitHub mode**: additionally pass issue body (with AC) + `IS_GITHUB: true`.

Wait for implementer response, parse `IMPLEMENTER_REPORT:`.

**Empty response handling**: if implementer returns empty (no `IMPLEMENTER_REPORT:` header), auto-retry once (re-dispatch same prompt). Both attempts empty — update Status to `needs-info` and stop.

**Parse tolerance**: if `IMPLEMENTER_REPORT:` header is not found in the response — treat as unparseable, update Status to `needs-info` with raw response, stop.

### First run: check SELF_REVIEW

When retry_count = 0, check for `SELF_REVIEW:` section in the report:

- STATUS: DONE — "no issues" or "found issues → fixed" — pass.
- STATUS: UNVERIFIED — must annotate each AC with verification method (test run / code structure analysis). Missing annotations but STATUS: UNVERIFIED — pass (UNVERIFIED itself declares incomplete verification).
- STATUS: DONE or UNVERIFIED but missing SELF_REVIEW section — flag as `needs-info`, stop.

Retry rounds (retry_count >= 1) do not check SELF_REVIEW.

### Collect SIBLING_CONTEXT

Before dispatching reviewer, automatically collect info about all resolved sibling modules under the current issue's spec:

1. Extract the spec issue number from the current issue body's `Parent` link.
2. `gh issue list --label "resolved" --json number,title` to get all resolved issues.
3. For each resolved issue (excluding current), extract its title and key conventions (entry pattern, test framework, file layout).
4. Assemble as `SIBLING_CONTEXT` string: "Completed sibling modules: #N title — key conventions: ..."

### Process implementer result

- **STATUS: DONE** — dispatch reviewer sub-agent (same sub-agent mechanism). Prompt format:

```
Read the autopilot-reviewer skill definition and follow its methodology strictly.

<task description below>
```

Task description conveys `source`, `id`, `contract`, `CHANGED_FILES`, `SIBLING_CONTEXT` + previous `REVIEWER_REPORT` (if any).
  - **GitHub mode**: additionally pass `IS_GITHUB: true`.

- **STATUS: UNVERIFIED** — dispatch reviewer sub-agent (same prompt format). Additionally pass `UNVERIFIED: true` + implementer's full `SELF_REVIEW` section (with per-AC verification annotations). Reviewer focuses on:
  - Structural correctness (does code logic match AC).
  - Whether all ACs have corresponding code implementation.
  - VERDICT may be `VERIFY_NEEDED` (structure passes but needs toolchain verification) or `RETRY` (structure itself has issues).

- **STATUS: BLOCKED or NEEDS_CONTEXT** — update Status to `needs-info`, append comment with reason, **stop**.

#### Parse SUGGESTION_RESOLUTIONS

When STATUS: DONE, parse the `SUGGESTION_RESOLUTIONS:` section from `IMPLEMENTER_REPORT`, stage for execution after reviewer confirms:

1. If section content is "none" or absent — no cross-issue suggestions to process, skip.
2. Parse each line, format: `[resolved|rejected|deferred] source <source_issue> round <N>: <content summary> → <handling note>`.
3. Extract fields: `type` (resolved/rejected/deferred), `source_issue`, `round`, `summary` (before `→`), `detail` (after `→`).
4. Stage as `pending_resolutions` list; execute status updates after reviewer returns MERGE.

### Process reviewer result

Parse `REVIEWER_REPORT:`, read VERDICT. Reviewer task failure or missing `VERDICT:` — treat as BLOCKED, update Status to `needs-info`, stop.

**Parse tolerance**: `REVIEWER_REPORT:` header not found — unparseable, update Status to `needs-info` with raw response, stop.

#### Extract Suggestions and persist

After parsing REVIEWER_REPORT, regardless of VERDICT, extract all `## Suggestion` entries and write to `suggestions.json`:

1. Parse each `- [ ]` item under `## Suggestion`.
2. Auto-fill missing KEYWORDS / FILES from content text and implementer's CHANGED_FILES.
3. Derive feature directory and read/write `.scratch/<feature>/suggestions.json`.
4. Deduplicate by `content` field; new entries get `"status": "pending"`.
5. For GitHub-sourced issues, also post `gh issue comment` for each new suggestion.

VERDICT branches:

- **MERGE** — update Status to `resolved`, append reviewer conclusion. Run "Update Suggestion status" step, then **return to scan mode for next issue**.
- **VERIFY_NEEDED** — review passed (structure correct) but implementer toolchain unavailable. Attempt to run project test command in orchestrator environment. Pass → resolved; fail → needs-info with "Toolchain unavailable — requires manual verification".
- **RETRY** — `retry_count += 1`, clear `pending_resolutions`. Follow stall detection rules:
    - Contract-gap Critical/Important → mark `needs-info`
    - Stall detection triggered or decision matrix "no new evidence" → record `BLOCKER_TYPE: exhausted`, comment with the reviewer problem list, defer, then return to scan mode.
    - Decision matrix "Recoverable + new evidence" → repeat implementer dispatch with `PREV_REVIEW`.
- **BLOCKED** — update Status to `needs-info`, append reviewer conclusion, **return to scan mode**.

#### Update Suggestion status

On VERDICT: MERGE, update `suggestions.json` entries per `pending_resolutions` using three-level matching (issue → round → content substring). Handle status validation, multi-hit ambiguity, and GitHub comment sync for resolved/rejected entries.

### Phase 1 exit condition

When scan mode returns zero ready-for-agent issues, Phase 1 is complete. Enter Phase 2.

---

## Phase 2: Global Meta-Review

When all Phase 1 issues are processed (no ready-for-agent remaining), execute the global review process in `references/meta-review.md`: dispatch reviewer sub-agents in parallel + orchestrator self-review, merge reports, fix Critical/Important issues, parse specs.

### FINAL_ACCEPTANCE_REPORT

After meta-review fixes are complete, produce a cross-issue Suggestion acceptance report per `references/acceptance-report.md`.
