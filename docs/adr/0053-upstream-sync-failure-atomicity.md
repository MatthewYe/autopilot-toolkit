# ADR 0053: Make the Upstream Sync Atomic and Fail Closed

> Extends [ADR 0037](0037-upstream-sync-full-replacement.md) (the upstream
> snapshot is replaced verbatim) and [ADR 0049](0049-typed-lock-write-path-and-upstream-sync-crate.md)
> (the typed lock write path lived in a `crates/upstream-sync` crate) with what
> happens when the post-sync gates fail. Prepares the verifiable pin recorded as
> ADR 0052 by #150: this sync already backs up and restores the pin file.

## Context

`scripts/sync-upstream.rs` runs clone → discover → classify → replace
`skills/upstream/` → rewrite `.skill-lock.json` → verify hashes. ADR 0037 and
ADR 0049 established that replacement is verbatim and the lock is machine
written. Neither decided the failure behavior, and the implementation had two
gaps:

- The gates ran **after** the tree and lock had been replaced. A failing
  `validation/run.rs` or hash check returned `Err` but left the new snapshot on
  disk, so the working tree said "synced" while CI and the report said
  "rejected". The next command ran on top of a half-applied sync.
- `CheckResult::Skip` exited 0 and was formatted as `SKIP`. A check whose target
  file did not exist was silently treated as a pass, so the hash gate could
  report success while checking nothing.

A sync also started on top of uncommitted changes. With the tree replaced
wholesale, local edits under `skills/upstream/` could be mixed into the snapshot
and later deleted without a trace. Cloning first and only then checking the tree
was too late: the destructive step had to be prevented before the first write.

One structural constraint pushes back on a naive fix: the post-sync gates are
not pure in-process checks. The real validation suite reads the discovered set
through `crates/skill-index`, which is driven by `.skill-lock.json`; using it as
a gate therefore requires the lock to be written **before** the gate runs. The
sync cannot validate a candidate set without materializing it.

## Decision

**A sync either fully lands or leaves the pre-sync state byte-for-byte
untouched.** The contract is:

1. **Preconditions run before the clone.** `check_worktree_preconditions`
   refuses to start when tracked files are dirty, or when `skills/upstream/`
   holds untracked files. `--force` overrides the refusal, prints every path it
   is about to overwrite, and still goes through the backup below. A missing git
   worktree (tests, tarballs) skips the precondition rather than failing.
2. **The destructive phase is wrapped in a backup.** Before replacing anything,
   `Backup::create` copies `skills/upstream/` and the raw bytes of
   `.skill-lock.json` and `.upstream-pin.json` (when present) into a temp
   directory. On success the backup is disarmed and deleted; on any failure —
   including a panic, via `Drop` — the tree and lock are restored before the
   error is returned.
3. **Both gates are hard gates.** `skill_check::check_skills` must be all
   `Pass`/`Fix`; `CheckResult::Skip` now fails closed
   (`skill_check::any_not_pass`), and the error names the skipped skill. Then
   `validation_runner::run_validation` runs in-process against the written lock,
   so the sync sees exactly what CI would see. Any `has_failures` restores and
   returns the report.
4. **Restore failure is loud.** If restoring itself fails, the error says so and
   keeps the snapshot directory instead of deleting the only copy of the old
   state.
5. **Failure is a non-zero exit with a restore note.** The CLI keeps printing
   the resolved commit, and any gate failure exits non-zero after the restore.
   The message ends with `pre-sync state restored (skills/upstream,
   .skill-lock.json, .upstream-pin.json).`

The `--force` flag is a deliberate escape hatch, not the default: the snapshot is
a vendored, verbatim copy, so a dirty tree under `skills/upstream/` is always a
mistake that review will otherwise hide.

## Rationale

**Materialize then validate, but roll back as one unit.** The viability
constraint above means the candidate tree and lock must exist before the gates
can read them. The alternative — a "dry-run" tree somewhere else — would
duplicate the validation path and drift from CI. Backup-and-restore keeps one
validation implementation while preserving the all-or-nothing contract.

**Fail closed on SKIP.** A skip is an absent check, and an absent check in a gate
that is supposed to catch stale hashes is not a pass. The only allowed exception
stays the pre-existing `found_github == false` case, where the repo genuinely
has nothing to check - there `scripts/check.rs` reports `ALL PASS (no github
skills found)`.

**Temp directory over git checkout.** Restoring with `git checkout --` would
require the paths to be tracked, would clobber the index, and could not restore a
previously absent `.upstream-pin.json`. A temp snapshot is independent of repo
state and works for the tests' synthetic projects too.

## Alternatives considered

### A. Never write until the gates have passed

Rejected: the gates need the lock. A separate "candidate" store would fork
`skill-index`'s discovery path — the very code the gates exist to protect — and
reintroduce the risk that the gate validates something other than the final
files.

### B. Use `git checkout` / `git stash` for rollback

Rejected: the precondition already guarantees a clean tracked tree, so git could
restore tracked files; but it cannot restore untracked files that the sync would
delete, cannot restore a `.upstream-pin.json` that did not exist, and mutates the
user's index/stash behind their back. A plain byte snapshot is simpler and
testable.

### C. Refuse on any dirty tree, including under `skills/vendor/` and `skills/autopilot/`

Rejected: those directories are not touched by this sync. The precondition is
scoped to paths the sync owns (`skills/upstream/`, `.skill-lock.json`,
`.upstream-pin.json`). A repo-wide cleanliness requirement would block unrelated
work for no invariant.

### D. Keep "validation optional" so a sync can proceed without the docs

Rejected: the documented-inventory check is part of `validation/run.rs` and it
compares the discovered upstream count against AGENTS.md/README/GLOSSARY. When an
upstream version adds or removes a skill, the sync's own gate legitimately fails
until the prose counts are updated. That is a real two-step operation (sync →
read the expected counts from the error → update the three documents → re-run),
documented in the runbook; making the gate optional to avoid it would let a stale
inventory merge.

## Consequences

- Re-running a failed sync is safe and produces the same result: the pre-sync
  tree, lock, and (from ADR 0052) pin are restored byte-for-byte before the error
  surfaces. Tests assert the restore by comparing recursive tree snapshots.
- Tracking dirty changes under `skills/upstream/` is still possible on a
  throwaway checkout with `--force`, which prints the overwritten paths; the
  backup includes those dirty files, so even the forced path is reversible.
- The sync is slightly slower (one directory copy before the destructive step)
  and needs a writable temp directory. Accepted.
- `scripts/check.rs` now exits non-zero on `SKIP` for the *real* repository too.
  That is intentional fail-closed behavior; a check that cannot compute a hash is
  a broken check, not a pass.
- Updating the documented inventory counts from the sync error is the expected
  workflow when the upstream skill count changes; the runbook
  ([docs/agents/upstream-sync.md](../agents/upstream-sync.md)) spells it out.
- Accepted risk: concurrent syncs in the same working tree are not serialized;
  the last writer wins. The precondition and the CI-only usage make that
  unlikely; a lock file would be the next step if it ever bites.
