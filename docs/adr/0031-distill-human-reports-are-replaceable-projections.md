# Distill human reports are replaceable projections

A validated machine-readable completion report is the canonical final run artifact and determines workflow completion. Human-readable output is produced by a replaceable post-completion renderer hook—v1 ships a basic Markdown renderer, while later versions may invoke a reporting skill or visual client; renderer failure records a retryable warning but never reopens or blocks an otherwise completed run.
