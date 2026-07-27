# Distill publication is idempotent

PRD and implementation-issue publication uses stable operation identities and persists each resulting external artifact identity as soon as it is known. On resume, adapters verify and reuse confirmed artifacts, continue only unpublished slices after partial success, and enter reconciliation when an external result is uncertain instead of blindly retrying. This adds adapter complexity but prevents duplicate tracker artifacts across crashes, interrupted conversations, and retries.
