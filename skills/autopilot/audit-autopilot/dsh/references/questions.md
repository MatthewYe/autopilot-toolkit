# Analysis Questions

Nine fixed questions across three fidelity layers. Each question includes the scoring rubric specific to that question.

## Layer 1: Fidelity (high-level intent alignment)

### Q1: Intent Translation
Does AGENT-BRIEF faithfully capture issue.md's core intent, or was meaning lost/added in translation?

- **PASS**: AGENT-BRIEF's ACs align with issue.md's described problem. No AC addresses a concern not present in issue.md, and no issue.md concern is absent from the ACs without explicit scope narrowing.
- **WARN**: Minor divergence — an AC adds detail not in issue.md but arguably within scope, or issue.md mentions a non-critical concern omitted from ACs.
- **FAIL**: AGENT-BRIEF added constraints or goals absent from issue.md (scope expansion) OR omitted a core concern from issue.md (scope gap).

**Evidence**: Compare issue.md problem description against AGENT-BRIEF AC list. Cite specific lines from each.

### Q2: AC Coverage
Are all Acceptance Criteria implemented? Is there code or behavior with no corresponding AC?

- **PASS**: Every AC has corresponding implementation evidence (test file, code change, or report confirmation). No extraneous changes beyond AC scope.
- **WARN**: One AC has weak implementation evidence (only report claims, no test). OR one minor extraneous change found.
- **FAIL**: An AC is clearly unimplemented (no test, no code, no mention in CHANGED_FILES). OR significant code changes with no AC justification.

**Evidence**: Map each AC to implementation evidence. For missing ACs, cite the absence in CHANGED_FILES and session trace. For extraneous changes, cite the change and the AC that does NOT cover it.

### Q3: Report Credibility
Does the IMPLEMENTER_REPORT's claims match the evidence in the session trace?

- **PASS**: All claims in SELF_REVIEW and STATUS align with trace evidence. STATUS=DONE only when all ACs show implementation evidence. SELF_REVIEW findings are reflected in code changes.
- **WARN**: SELF_REVIEW claims "no issues" but trace shows minor uncorrected problems (e.g., a skipped edge case). Non-critical discrepancy.
- **FAIL**: STATUS=DONE claimed but AC evidence is missing. SELF_REVIEW claimed to fix an issue that trace shows was not fixed. STATUS=BLOCKED but no diagnose loop evidence in trace.

**Evidence**: Compare each SELF_REVIEW claim against the implementer session's tool call sequence. Cite specific message IDs.

## Layer 2: Errors (hard defects)

### Q4: Unfixed Criticals
Did any Critical or Important reviewer finding go unfixed across retry rounds?

- **PASS**: Every Critical/Important item from every REVIEWER_REPORT either: (a) was fixed in a subsequent round with trace evidence, or (b) the issue was resolved via MERGE with no Criticals/Importants.
- **WARN**: A Critical/Important was marked fixed by implementer but trace evidence of the fix is weak or ambiguous.
- **FAIL**: A Critical/Important finding appeared in a reviewer report, the issue received RETRY, but the next implementer round did not address it, AND the issue was subsequently MERGEd or retry limit was hit.

**Evidence**: Track each Critical/Important item across rounds. Cite the reviewer report where it appeared, the implementer round that should have fixed it, and the missing fix evidence.

### Q5: Verdict Consistency
Is the reviewer's VERDICT consistent with their own checklist findings?

- **PASS**: VERDICT follows the rules exactly: MERGE only when 0 Critical AND 0 Important; RETRY when 1+ Critical or Important; BLOCKED for directional errors.
- **WARN**: VERDICT is technically correct per the rules but the checklist assessment seems inconsistent (e.g., marking a clearly blocking issue as Suggestion).
- **FAIL**: VERDICT contradicts the checklist (e.g., MERGE with listed Criticals, RETRY with no Criticals/Importants, or BLOCKED without explanation).

**Evidence**: Cite the REVIEWER_REPORT's checklist items and the VERDICT line. Show the contradiction.

### Q6: Suggestion Chain Integrity
Did cross-issue suggestions get properly matched, passed, and resolved?

- **PASS**: Every pending suggestion matched to the current issue appears in the implementer's SUGGESTION_RESOLUTIONS with a clear resolution (resolved/rejected/deferred). Resolved suggestions show trace evidence of implementation.
- **WARN**: A matched suggestion was resolved without trace evidence, or deferred without justification.
- **FAIL**: A matched suggestion was completely absent from the implementer's SUGGESTION_RESOLUTIONS. A suggestion marked resolved but no implementation evidence exists.

**Evidence**: Cross-reference suggestions.json entries against IMPLEMENTER_REPORT SUGGESTION_RESOLUTIONS. Cite the missing link.

## Layer 3: Friction & Drift

### Q7: Retry Efficacy
Did retry rounds make substantive progress, or was there churn without forward motion?

- **PASS**: Each retry round shows: (a) new changes addressing the specific Critical/Important items from PREV_REVIEW, and (b) the next reviewer VERDICT improved (more items fixed, fewer new issues). Or no retries occurred (first round MERGE).
- **WARN**: Retry rounds fixed some but not all flagged items, or introduced new issues while fixing old ones. Net progress but imperfect.
- **FAIL**: Multiple retry rounds with no substantive difference in CHANGED_FILES or reviewer findings. Implementer repeatedly failed to address the same Critical items. Hit max retries (3) with unresolved issues.

**Evidence**: Compare CHANGED_FILES and REVIEWER_REPORTs across rounds. Cite the stagnation pattern.

### Q8: Scope Creep
Did the implementer add, modify, or touch anything outside the AGENT-BRIEF scope?

- **PASS**: All CHANGED_FILES and behaviors map to at least one AC. Nothing in the "Out of scope" section was implemented.
- **WARN**: Minor tangentially-related changes that are arguably implied by the ACs but not explicitly stated (e.g., adding an import for a utility used by the AC implementation).
- **FAIL**: Explicit Out of scope item was implemented. New files with no AC justification. Behavior changes in modules not mentioned in the AGENT-BRIEF. New dependencies added without AC justification.

**Evidence**: List the extraneous file/behavior and the Out of scope section or AC list that does NOT cover it. Cite specific message IDs showing the implementation.

### Q9: TDD Discipline
Did the implementer follow TDD discipline — failing test first, no production code without tests?

- **PASS**: For each AC, the implementer session shows a test tool call BEFORE the corresponding production code edit. All production code has test coverage. No mock of internal modules. Tests verify behavior through public interfaces.
- **WARN**: Test and production code order is ambiguous in the trace. Minor gaps — one AC might have only an integration test without a unit test. One internal mock found but arguably at a module boundary.
- **FAIL**: Production code written with no preceding test. Mock of internal/private methods. Tests assert implementation details (private function calls, internal state). Test tool calls absent entirely despite IMPLEMENTER_REPORT claiming TDD.

**Evidence**: Show the message sequence: production file edit with no preceding test call. Cite tool call IDs and message timestamps.
