## Parent

Parent PRD: `docs/prd/0003-kimi-code-adaptation.md`

## What to build

Unify the two on-disk clones of the toolkit so every runtime loads skills from the single canonical working clone, `/Users/neil/Dev/autopilot-toolkit`.

End-to-end behavior:

1. Verify `/Users/neil/build/autopilot-toolkit` has no uncommitted divergence from the Dev clone (git status + diff of the skill trees). If divergence exists, stop and surface it to the user instead of proceeding.
2. Re-point every toolkit-owned symlink under `~/.reasonix/skills/` from the `build` clone to the same variant source in the `Dev` clone (same relative paths, so Reasonix behavior is unchanged).
3. Smoke-check: Reasonix's skill list is identical before and after the re-point.

Deletion of the old `build` clone is explicitly left to the user as a manual step afterward.

## Acceptance criteria

- [ ] Parity check between the two clones is performed and recorded in the issue/PR before any symlink changes
- [ ] Every symlink under `~/.reasonix/skills/` that pointed into `/Users/neil/build/autopilot-toolkit` now resolves to the matching path under `/Users/neil/Dev/autopilot-toolkit`
- [ ] No symlink targets outside the toolkit source tree are touched
- [ ] Reasonix skill list is unchanged after the re-point (same skills, same variant sources)
- [ ] The `build` clone directory is left in place for the user to delete manually

## Blocked by

None - can start immediately
