# Upstream code-review bundle

Record `WORK_BASE = git rev-parse HEAD` before implementation. Afterwards run
two independent read-only reviews over `git diff <WORK_BASE>`:

1. **Standards** — repository instructions, nearby conventions, tests, and the
   upstream `code-review` Fowler baseline.
2. **Spec** — the authoritative `contract`, linked spec, ADRs, and scope.

Run both in parallel when supported. Do not pin a child model: inherit the main
effective model. If equality cannot be proven, run the pass inline.

Merge them as `UPSTREAM_REVIEW_REPORT` with `WORK_BASE`, `STANDARDS`, and
`SPEC` fields, then pass it to `autopilot-reviewer`. Missing either pass is an
Important finding. Promote a finding to Critical only when it matches the main
reviewer's Critical rules.
