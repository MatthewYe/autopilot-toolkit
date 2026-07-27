# Distill state transitions use revisions

Distill permits one logical writer per run and guards every mutation with both the bound runtime-native session identity and an expected state revision. The runner updates project-local state atomically and rejects stale turns, pre-takeover sessions, and concurrent submissions instead of applying last-write-wins; publication intents participate in the same protocol before adapters perform external side effects.
