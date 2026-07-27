# Distill state is project-local

Each target project stores authoritative Distill run state under `.distill/runs/<run-id>/`, including input snapshots, stage completion evidence, and references to published artifacts. The directory is ignored by Git by default: this keeps potentially sensitive intake material out of repository history while making runs easy for agents and a future project-facing UI to discover and resume. PRDs and implementation issues remain durable in the project's configured issue tracker.
