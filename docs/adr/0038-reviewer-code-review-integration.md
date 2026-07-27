# ADR 0010: Autopilot-Reviewer Fifth Axis — Upstream Code-Review Sub-Agent

## Context

Upstream v1.1.0 ships a hardened `code-review` skill with a two-axis review (Standards + Spec, with a Fowler smell baseline) run as parallel sub-agents. `autopilot-reviewer` currently runs a four-axis review (Behavior Alignment, TDD Discipline, Code Quality, Plan Fidelity). The two have complementary strengths — upstream code-review focuses on repo coding standards and spec fidelity, while autopilot-reviewer covers the autopilot loop's specific concerns (AC coverage, TDD discipline through public interfaces, cross-module consistency).

## Decision

**Integrate upstream `code-review` as a parallel fifth sub-agent in autopilot-reviewer's Code Quality dimension, not as a replacement for any existing axis.** The reviewer launches `/code-review` as a sub-agent scoped to the implementer's changed files, collects its output, and incorporates findings into the Code Quality section of the reviewer report. The upstream review's findings are presented as `Important`-tier issues unless the code-review agent itself flags a `Critical` pattern that maps to an autopilot dealbreaker.

## Alternatives considered

### A. Replace autopilot-reviewer with upstream code-review

Rejected: upstream code-review cannot evaluate TDD discipline (not all repos use TDD), cannot check cross-module consistency against a spec/ADR, and has no notion of AC coverage. Autopilot's closed-loop workflow requires all of these.

### B. Inline Fowler smells as a checklist (no sub-agent)

Rejected: the upstream code-review skill carries more than just a smell list — it runs parallel Standards/Spec sub-agents, resolves repo-specific coding standards, and traces spec fulfillment against the originating issue. Inlining loses all of this.

### C. Skip — autopilot-reviewer is self-sufficient

Rejected: the autopilot-reviewer's Code Quality dimension currently lacks a standardized coding-standards baseline. The Fowler smell baseline and repo-standards resolution in upstream code-review fill this gap at zero implementation cost.

## Consequences

- **Review latency**: One additional sub-agent spawn per review cycle, though it runs in parallel with existing axes.
- **Noise risk**: Upstream code-review may flag style issues the team already accepts. Mitigation: findings are downgradeable by the autopilot-reviewer's own judgement.
- **Divergence risk**: If upstream code-review evolves its output format, autopilot-reviewer's integration code must be updated. Mitigation: the integration reads the upstream SKILL.md's output specification at review time.
