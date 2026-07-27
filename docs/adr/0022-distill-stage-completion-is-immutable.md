# Distill stage completion is immutable

Once the runner accepts a stage's completion evidence, that stage is not rewound or rewritten. Material information discovered later terminates the current run as `superseded` and creates a linked successor in the same session, which may reuse prior snapshots and confirmed conclusions; published artifacts remain visible and must be explicitly updated or marked superseded rather than silently deleted or overwritten.
