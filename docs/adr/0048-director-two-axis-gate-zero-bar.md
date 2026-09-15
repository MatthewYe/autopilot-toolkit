# ADR 0048: Director gate is a dual-layer two-axis review with an absolute-zero bar

## Context

The toolkit has two review traditions: the five-axis `autopilot-reviewer` used by
the orchestrator loop (tiered findings, merged verdict, per ADR 0038/0039), and the
upstream `code-review` skill's two axes — Standards and Spec — reported side by
side without merging. `autopilot-director` needed a gate strong enough to replace
per-ticket human review under AFK operation, cheap enough to run every round, and
with a mechanically checkable pass condition so the state machine (not prose) owns
advancement.

## Decision

The Director's hard gate is the upstream two-axis review — Standards and Spec —
run by fresh effective-model reviewers on every round, with the dispatch flattened:
the Director spawns the two axis agents directly following the `code-review`
process instead of invoking the skill as a nested layer. The pass condition is
absolute zero: every finding is either fixed or rejected by the Director with a
written reason, recorded as a Finding disposition in run state. The gate applies at
two layers: per ticket (must reach zero before the ticket's boundary commit) and
per spec (aggregate diff must reach zero before the Spec PR opens). Each layer is
capped at three review rounds; exhausting the cap moves the ticket (or run) to
Escalation — the Director stops and reports to the human, and the run resumes only
on an explicit human decision. Test discipline is enforced dually: the Director
runs the repo's test gates itself (Worker self-reports are never gate evidence),
and the Spec axis verifies that every acceptance criterion has executable
verification evidence.

## Alternatives considered

### A. Reuse the five-axis autopilot-reviewer as the gate

Rejected: five axes with tiered, merged verdicts is heavier than the gate needs,
and the merge step is exactly what the two-axis separation exists to prevent. The
orchestrator loop keeps the five-axis reviewer; the director does not inherit it.

### B. Tiered findings, fix only the severe ones

Rejected: silently ignoring judgement-call findings erodes the gate over long AFK
runs. Absolute zero with recorded Director rejections keeps the bar at zero without
busywork — adjudication is precisely the judgment the lead model is paid for.

### C. Invoke the code-review skill as a nested layer

Rejected: a subagent spawning review subagents is an unverified mechanism and adds
an indirection layer. Flattening preserves the two-axis semantics exactly while the
Director stays the single dispatcher.

## Consequences

- The Two-axis gate's absolute-zero condition is computed by the run state machine
  from recorded finding dispositions, never asserted by the Director in prose.
- Escalation reports include the finding evolution across rounds, the Worker's
  self-reports, and the Director's diagnosis; the human decides whether to grant
  more rounds, have the Director implement directly, or abandon the ticket.
- ADR 0038's fifth-axis integration and ADR 0039's review contract remain the
  orchestrator loop's rules and are untouched by this ADR.
