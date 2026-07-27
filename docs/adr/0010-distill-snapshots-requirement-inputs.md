# Distill snapshots requirement inputs

Every Distill run captures an immutable snapshot of its original requirement inputs, including user text, uploaded file contents, and fetched link contents with provenance and content hashes. Later stages read the snapshot instead of mutable source files or URLs, trading additional storage and data-handling responsibility for reproducible clarification, reliable resume behavior, and an auditable connection from the original request to the resulting PRD and issues.
