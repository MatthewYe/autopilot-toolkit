# Distill runs are worktree-local

A Distill run belongs to the concrete worktree root in which it was created, not to the repository's shared Git common directory. Another worktree may not advance it implicitly, and session takeover does not move it; an explicit transfer must copy and validate persisted state, input snapshots, and the relevant working-tree and domain-document preconditions so requirements are never distilled against the wrong branch context.
