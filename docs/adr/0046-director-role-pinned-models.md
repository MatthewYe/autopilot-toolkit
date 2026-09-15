# ADR 0046: Director workers use role-pinned models

> Narrows ADR 0039 decision 7 ("child role definitions do not pin a model") to the
> original five-skill autopilot loop. See CONTEXT.md "Autopilot Director" for the
> role vocabulary.

## Context

The new `autopilot-director` skill completes one whole spec: a Director (the main
effective model) drives ticket implementation, independent review gates, and merge
decisions, while code development is delegated to Worker subagents. ADR 0039 decision
7 required every autopilot child role to inherit the main effective model, because
v1.1.0 had suffered review passes silently running on weaker models. That rule was
written for a loop where the child roles implement and review with equal model
strength; it did not consider a deliberate hierarchy where mechanical implementation
is intentionally cheaper than judgment.

## Decision

In `autopilot-director` runs, the Worker role is bound to a role-pinned model — a
fast model (default `deepseek-flash`), overridable per invocation. The Director and
all Two-axis gate reviewers always run on the main effective model; reviewers are
fresh agents each round and never share context with the Worker they review. Model
pinning happens at dispatch time (the spawn call's model override), never in
committed agent definitions, so a host without the pinned model fails loudly at
dispatch rather than silently substituting.

ADR 0039 decision 7 remains in force for the original orchestrator loop
(orchestrator/implementer/reviewer/distill/audit-autopilot); it is narrowed, not
repealed.

## Alternatives considered

### A. Keep uniform model inheritance for director workers

Rejected: the Director re-verifies every Worker claim through code-run gates and
independent review, so Worker model strength is not load-bearing for correctness —
only the gates are. Paying lead-model prices for mechanical implement-and-fix loops
is pure cost with no safety gain.

### B. Pin reviewers to the fast model as well

Rejected: the two-axis gate is the correctness backstop. Fast-model reviewers of
fast-model code collapses the gate's independence in practice, and review quality
degrades exactly where the design concentrates trust.

### C. Pin the model in a committed agent definition (agent.toml)

Rejected: a committed pin cannot be overridden per invocation and silently produces
a different model on hosts lacking the pinned one — the exact failure ADR 0039
decision 7 was written to prevent. Dispatch-time pinning keeps the override
explicit and the failure loud.

## Consequences

- CONTEXT.md gains the "Role-pinned model" term; "Effective agent model" keeps its
  meaning for the original loop.
- Worker competence is never assumed: every Worker output is treated as unverified
  until code-run gates and the Two-axis gate pass (ADR 0048).
- Per-invocation model override is a supported, documented parameter of the skill.
