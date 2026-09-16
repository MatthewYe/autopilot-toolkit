# The `WORKER_REPORT` envelope

The envelope is the only channel through which a Worker's claims enter run
state. `director report validate --worktree <p> --ticket <n> [--file <path>]`
reads it (stdin when `--file` is absent) and validates it field by field
against `src/report.rs`. A malformed envelope is a **recorded dispatch
failure** — it never counts as a pass, and partial acceptance does not exist.

## Wire format

A line reading `WORKER_REPORT:` on its own (`WORKER_REPORT` without the colon
is accepted too), followed by one JSON object. A single ```json fence around
the object is tolerated. Prose before the marker and after the object is
ignored: the first JSON value after the marker is the envelope.

```
WORKER_REPORT:
{
  "status": "done",
  "branch": "codex/132-worker-report",
  "commits": [
    { "sha": "47ddc78", "subject": "feat(director-cli): validate the envelope" }
  ],
  "tests": [
    {
      "command": "cargo test -p director-cli",
      "outcome": "pass",
      "evidence": "71 passed; 0 failed"
    }
  ],
  "acceptance": [
    {
      "criterion": "malformed envelopes are recorded as dispatch failures",
      "evidence": "tests/dispatch.rs::a_malformed_report_records_a_dispatch_failure"
    }
  ],
  "design_notes": "the envelope is validated before it is attached",
  "blockers": []
}
```

## Fields

| field | type | rule |
| --- | --- | --- |
| `status` | `"done"` \| `"blocked"` | required |
| `branch` | string | required, non-empty |
| `commits` | array of `{ "sha", "subject" }` | required; at least one entry when `status` is `done`; both strings non-empty |
| `tests` | array of `{ "command", "outcome", "evidence" }` | required; at least one entry when `done`; `outcome` is `"pass"` or `"fail"`; every string non-empty |
| `acceptance` | array of `{ "criterion", "evidence" }` | required; at least one entry when `done`; both strings non-empty |
| `design_notes` | string, optional | when present, non-empty |
| `blockers` | array of strings | required; non-empty when `status` is `blocked`, empty when `done` |

Unknown fields are refused (`deny_unknown_fields`), so the envelope cannot
quietly grow a field nobody validates.

## Contradictions that fail validation

- `status: "done"` with no commits, no test evidence, no per-criterion
  self-check, or with any `tests[].outcome` of `"fail"`.
- `status: "done"` carrying blockers — a done ticket has none.
- `status: "blocked"` naming no blocker.

## What the Director does with it

- `director dispatch begin --ticket <n> --worker <id>` opens an attempt.
- `director report validate` attaches a valid envelope; a malformed one is
  recorded as a failed attempt.
- `director dispatch finish --ticket <n> --outcome ok` accepts only a validated
  `done` report. A `blocked` report must be finished as
  `--outcome failed --reason <blockers>`.
- The retry budget is `1`: one failed attempt plus exactly one same-Worker
  retry. The failure past the budget transitions the ticket to `escalated` and
  is refused as a retry. Resuming an escalated ticket to `implementing` opens a
  fresh budget window.
