# Distill state schemas migrate forward only

All persisted state, workflow snapshots, completion evidence, and reports carry explicit schema versions. A newer CLI may perform deterministic forward migrations after preserving the original state as a backup and recording the migration event; unknown versions or failed migrations stop closed, historical schema fixtures remain release-test inputs, and older CLIs never attempt to downgrade or interpret newer state.
