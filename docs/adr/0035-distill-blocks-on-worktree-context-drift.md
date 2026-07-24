# Distill blocks on worktree context drift

Before activating a stage, Distill compares the current worktree, HEAD, branch, and relevant domain documents with the run's recorded context. Drift blocks progression until the executor either submits a reasoned acknowledgment that the change is immaterial or supersedes the run because a completed stage is now obsolete; the runner never merges, resets, or silently ignores external changes.
