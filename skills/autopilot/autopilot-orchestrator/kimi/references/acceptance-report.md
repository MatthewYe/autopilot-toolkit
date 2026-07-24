# FINAL_ACCEPTANCE_REPORT

Produced after meta-review repairs complete. A cross-issue suggestion acceptance report for human sign-off.

## 1. Aggregate suggestions

Scan all feature directories' `suggestions.json`:

- Glob `.scratch/*/suggestions.json`, read each file
- Merge all entries into a unified list, preserving source feature info

### GitHub Issue mode additional aggregation

When Phase 1 processed GitHub issues, extract suggestions from issue comments and merge with local `suggestions.json`:

1. For each processed GitHub issue, read all comments (`gh issue view <N> --json comments`)
2. Filter for `autopilot suggestion [<status>]: <body>` format
3. Extract: `status` (from `[<status>]` block), `content` (`:` onwards), `source_issue` (`#<N>`)
4. Merge with local `suggestions.json` entries, deduplicating by `content` (local entries take precedence — keep their full fields)

## 2. Group by status

| Group | Content | Source |
|-------|---------|--------|
| **Pending** | `status: "pending"` | List `content`, `source_issue`, `keywords`; note `deferred_by` if present |
| **Rejected** | `status: "rejected"` | List `content`, `source_issue`, `rejected_reason` |
| **Resolved** | `status: "resolved"` | List `content`, `resolved_in_issue`, original `source_issue` |

## 3. Output format

Open with `FINAL_ACCEPTANCE_REPORT:` header:

```
FINAL_ACCEPTANCE_REPORT:

## Pending（需处理）
- <content>
  - 来源: <source_issue>
  - 关键词: <keywords>
  - [deferred by: <issue-slug>]
...（如无 pending，写 "无"）

## Rejected（已拒绝）
- <content>
  - 来源: <source_issue>
  - 理由: <rejected_reason>
...（如无 rejected，写 "无"）

## Resolved（已解决）
- <content>
  - 来源: <source_issue>
  - 由 <resolved_in_issue> 处理
...（如无 resolved，写 "无"）
```

## 4. Edge cases

- `suggestions.json` absent (glob returns nothing) → report "No suggestions.json found. Skipping acceptance report." (**does not block meta-review**)
- Exists but no pending → report "All suggestions resolved. Ready for sign-off."
- Has pending → report "The following suggestions require human attention:" + list each + recommend human judgment on direction (create follow-up issue or mark rejected)
- Only GitHub issue comments have suggestions, no local `suggestions.json` → use comment aggregation results, still output full report

## 5. Self-verification

After outputting FINAL_ACCEPTANCE_REPORT, run this checklist:

- [ ] Every `status: "resolved"` entry in `suggestions.json` has `resolved_in_issue` field
- [ ] Every `status: "rejected"` entry has `rejected_reason` field
- [ ] No `status: "pending"` entry incorrectly marked with `resolved_in_issue` (only `resolved` should have it)
- [ ] FINAL_ACCEPTANCE_REPORT Pending + Rejected + Resolved group counts = total `suggestions.json` entries (after dedup)
- [ ] No entry has empty `content` field
- [ ] Anomalies found → record in `## Self-Verification Issues` section at end of report, for human follow-up
