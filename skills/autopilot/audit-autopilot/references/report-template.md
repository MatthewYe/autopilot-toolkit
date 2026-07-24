# Report Template

ALWAYS use this exact template for the audit output. Replace placeholders with actual values.

```markdown
# AUDIT REPORT: <session-id>

**Autopilot Session**: `<session-id>`
**Audit Date**: <YYYY-MM-DD>
**Issues Audited**: <count> (<list of slugs or issue numbers>)
**Total Rounds**: <count across all issues>
**Fidelity Score**: <PASS count>/9 (<percentage>%)

---

## Executive Summary

<2-3 sentence summary of overall autopilot execution quality. State the PASS rate, highlight the most critical finding (if any), and give a bottom-line assessment.>

---

## Scorecard

| # | Layer | Question | Score | Rationale |
|---|-------|----------|-------|-----------|
| Q1 | Fidelity | Intent Translation | PASS/WARN/FAIL | One-line summary |
| Q2 | Fidelity | AC Coverage | PASS/WARN/FAIL | One-line summary |
| Q3 | Fidelity | Report Credibility | PASS/WARN/FAIL | One-line summary |
| Q4 | Errors | Unfixed Criticals | PASS/WARN/FAIL | One-line summary |
| Q5 | Errors | Verdict Consistency | PASS/WARN/FAIL | One-line summary |
| Q6 | Errors | Suggestion Chain Integrity | PASS/WARN/FAIL | One-line summary |
| Q7 | Friction & Drift | Retry Efficacy | PASS/WARN/FAIL | One-line summary |
| Q8 | Friction & Drift | Scope Creep | PASS/WARN/FAIL | One-line summary |
| Q9 | Friction & Drift | TDD Discipline | PASS/WARN/FAIL | One-line summary |

---

## Findings

### FAIL

<For each FAIL, provide a detailed entry:>

#### <Q#>: <Question Title> — FAIL

**Severity**: Blocking | Advisory
**Evidence Anchor**:
- Session: `<session-id>`
- Message: `<message-id>`
- Excerpt: `<key snippet from trace>`

**Description**: <What went wrong. Include root cause analysis if Phase 2 deep-dive was performed.>

---

### WARN

<For each WARN, provide:>

#### <Q#>: <Question Title> — WARN

**Severity**: Advisory
**Evidence Anchor**:
- Session: `<session-id>`
- Message: `<message-id>`
- Excerpt: `<key snippet>`

**Description**: <What looks suspicious and why it couldn't be confirmed or cleared.>

---

## Recommendations

<1-5 concrete, actionable recommendations. Each should target either the autopilot configuration (agent prompts, command logic) or the contract quality (AGENT-BRIEF clarity, AC specificity).>

1. **<Title>**: <Description of what to change and why.>
```
