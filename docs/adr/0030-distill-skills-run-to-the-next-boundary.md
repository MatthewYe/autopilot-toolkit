# Distill skills run to the next boundary

Once invoked, a Distill skill variant repeatedly executes the runner's currently authorized action and advances completed stages without requiring another user invocation. It yields only at `waiting`, `blocked`, `needs-reconciliation`, or a terminal run state, and every yield reports the run identity, current stage, revision, and required next action; this provides continuous in-session flow while the runner retains transition authority.
