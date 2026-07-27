# Distill freezes publication payloads

Before publishing a PRD or implementation issue, Distill stores and hashes the exact outbound payload under the run's project-local artifacts. The configured issue tracker remains the collaborative authority, while the frozen payload supports idempotent reconciliation, recovery of publication records, and drift detection; later human edits in the tracker are reported but never automatically overwritten.
