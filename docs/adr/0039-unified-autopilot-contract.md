# Unified ticket, AFK, review, and model contract

## Context

Version 1.1.0 exposed semantic drift between the vendored upstream skills,
internal tracker adapters, and four autopilot host variants. The drift produced
bundled `tickets.md` files, unrunnable issue shapes, unnecessary AFK pauses,
missing fifth-axis reviews, retry gaps, and child models that differed from the
main agent.

## Decision

1. The vendored upstream tree remains immutable. A same-name internal
   `to-tickets` overlay wins during installation and publishes one canonical
   local ticket per file at `.scratch/<feature>/issues/<NN-slug>.md`.
2. Each ticket uses the required core YAML keys `key`, `title`, `type`,
   `status`, `parent` and exact headings `What to build`,
   `Acceptance Criteria`, `Blocked by`, and `Comments`. A backend may append
   lifecycle identity fields such as `tracker` and `remote-id`, but may not
   replace the core keys or add competing body sections. `Blocked by` defines
   the runnable frontier.
3. Orchestrators pass `issue_file` plus normalized `contract`. Legacy issue
   directories are read-only compatibility inputs.
4. `Critical` and `Important` findings both force RETRY and both must be fixed.
5. The fifth axis is a real upstream code-review bundle: independent Standards
   and Spec passes are merged into `UPSTREAM_REVIEW_REPORT`.
6. Autopilot never requests sandbox escalation. External capability failures
   are recorded as `BLOCKER_TYPE: external-unavailable` with `TEST_EVIDENCE`;
   scan mode continues to another frontier ticket. `needs-info` is reserved for
   human decisions or missing contract information.
7. Child role definitions do not pin a model. They inherit the main effective
   model; a host that cannot prove equality runs the affected pass inline.
8. Upstream sync validation is fail-closed. A missing, unlaunchable, or failing
   post-sync checker makes sync fail.

## Consequences

The local ticket is simultaneously the tracker record and runnable contract.
Adapters and audits accept the old directory shape only for migration.
Autopilot can finish useful work during unavailable external tests without
claiming those tests passed, and reviewer behavior is consistent across all
hosts.
