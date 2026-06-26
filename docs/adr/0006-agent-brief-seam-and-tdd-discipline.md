# AGENT-BRIEF Seam annotation and TDD discipline alignment

Audited `autopilot-implementer` against upstream `implement` skill. Found three missing disciplines: explicit typechecking after GREEN, full test suite before self-review, and TDD-at-seams. We decided to add all three; commit and /review are intentionally deferred — the orchestrator handles both after APPROVED.

Seam is an optional free-text field on each AGENT-BRIEF Acceptance Criterion: `Seam: <boundary description>`. It tells the implementer where to write tests (above the seam, caller-perspective) and what to mock (below). Human-authored AGENT-BRIEF seams take priority; the orchestrator may supplement with `Seam(inferred)` when the human didn't annotate.

Why not structured Seam (file/interface): ACs describe behavior, not code structure; a free-text Seam is sufficient as a testing-boundary hint. Structured seams would require the AC author to already know the implementation's module names — that's the implementer's job to resolve.
