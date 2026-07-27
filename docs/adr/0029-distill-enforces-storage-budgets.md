# Distill enforces storage budgets

Distill enforces default binary storage limits of 50 MiB per input source, 256 MiB per run, and 2 GiB per project, with individual events capped at 64 KiB and each run's event log capped at 32 MiB within its run budget. Writes are preflighted and atomic; quota exhaustion blocks the run without truncation or automatic eviction, while explicit project-level quota changes are audited and may not set a limit below current usage. Large content lives in artifacts referenced by hash from events, and cold-data compression must be lossless.
