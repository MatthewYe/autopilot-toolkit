# ADR 0047: Director lands one PR per spec, with one commit per ticket

## Context

The established ticket contract in this ecosystem lands one branch and one PR per
ticket, merged to main with `Closes #N` (proven across the auraforge M1 run). The
`autopilot-director` skill completes a whole spec AFK; per-ticket PRs would mean N
human approval interruptions per spec, which defeats the skill's purpose. The merge
shape had to balance three forces: small review diffs (gate quality degrades on
large diffs), one approval point per spec, and a clean, bisectable history.

## Decision

A Spec run works on a single local spec branch (`codex/spec-<N>-<slug>`), pushed
only when the Spec PR opens. Each ticket ends as exactly one Ticket boundary commit:
the Worker commits freely during implement-and-fix rounds, and the Director squashes
the ticket's work-in-progress commits into one boundary commit (message references
the ticket, e.g. `feat: <title> (ticket #N)`) immediately after the ticket's gate
reaches zero — before the next ticket starts. After every ticket gate and the
aggregate spec-level gate reach zero, the Director pushes the branch and opens the
single Spec PR, whose body lists all tickets with `Closes #…`. The Spec PR must be
merged with a merge commit; squash-merge is forbidden because it would flatten the
boundary commits.

The branch may carry at most two commits that are not Ticket boundary commits, and
both exist to keep the run honest:

- one run-setup chore when the run needs a repository change to exist at all (for
  example `chore: ignore .director/ run state`), and
- one aggregate-gate fix commit when the spec-level Two-axis gate finds something
  after every ticket is done — the ticket boundary commits are already frozen at
  that point, so the fix cannot be folded into one of them.

Anything else on the branch is a shape violation: a Spec PR that reads as more than
"one commit per ticket plus these two" hides work no ticket owns.

## Alternatives considered

### A. Per-ticket PRs to main (status quo)

Rejected: N approval interruptions per spec contradicts the AFK purpose, and each
interruption is a session-resume point. Ticket quality is already enforced by the
ticket-level gate (ADR 0048), which does not need a PR to bite.

### B. Spec integration branch with per-ticket PRs into it

Rejected: the final PR to main becomes the giant diff the gate design avoids,
ticket issues stay open until the final merge, and the long-lived branch drifts
against main. The atomicity it buys is illusory — tickets merged under (A) each
independently passed their gate and are independently sound.

### C. One branch, tickets tracked only in run state

Rejected: without boundary commits, the ticket-level review diff
("previous boundary..HEAD") is no longer trivially computable, and mid-run
escalation leaves an unpartitioned pile of WIP commits to report on.

## Consequences

- Ticket issues close in a batch when the Spec PR merges; per-ticket `Closes #N`
  lines live in the Spec PR body.
- Mid-run escalation leaves an unpublished local branch whose completed tickets are
  already final history — reporting and resume stay simple.
- The default merge authority stays with the human (approve the Spec PR), with an
  invocation-time auto-merge parameter as the explicit escape.
