# Distill records a dirty-worktree baseline

Distill may start in a dirty worktree, but captures the worktree identity, HEAD, branch, Git status, and existing domain-document hashes before producing artifacts. Completion evidence and reports distinguish pre-existing changes from edits attributable to the run, and Distill never stages, commits, reverts, or cleans either category.
