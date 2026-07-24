# Distill snapshots use explicit retention

Completed Distill runs retain their requirement snapshots and interaction records by default so their outputs remain auditable and reproducible. A user may explicitly purge a run; purge removes the replayable content but preserves a minimal tombstone containing the run identity, content hashes, purge time, and published PRD and issue references, and the runner no longer represents that run as fully replayable.
