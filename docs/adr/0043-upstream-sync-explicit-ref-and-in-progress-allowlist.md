# ADR 0043: Upstream Sync Takes an Explicit Ref and Ships an Allowlisted In-Progress Subset

> Amends [ADR 0037](0037-upstream-sync-full-replacement.md) for ref selection.
> Full replacement, orphan removal, and the `check.rs` gate remain in force.

## Context

ADR 0037 fixed the sync shape: replace `skills/upstream/` wholesale from a mattpocock/skills
release tag, compute each skill's `skillFolderHash`, write `.skill-lock.json`, and verify with
`check.rs`. Two things have changed since.

Upstream now keeps beta skills in `skills/in-progress/`, deliberately excluded from its plugin and
documented as changeable or removable without warning. The toolkit wanted three of them —
`loop-me`, `implement-spec`, `retro` — and the last two exist only on `main`: the newest release
tag predates them. A tag-only sync cannot carry them at all.

The old default ref made that gap dangerous. With a default in place, a bare
`rust-script scripts/sync-upstream.rs` would sync to a ref that predates the allowlisted skills,
so the orphan scan would delete them from `.skill-lock.json` — undoing the feature the allowlist
exists to provide.

## Decision

`scripts/sync-upstream.rs` takes a **required** ref: a release tag, branch, or commit SHA. There
is no default. The script prints the resolved commit so the sync is traceable, and it still
replaces the whole tree, rewrites `.skill-lock.json`, and runs `check.rs`.

`IN_PROGRESS_ALLOWLIST` names the in-progress skills the toolkit ships. They are ordinary
upstream entries in `.skill-lock.json` — same provenance fields, same hash verification, same
install path as any stable skill — and nothing is picked up from `in-progress/` implicitly.

If an allowlisted name is absent at the synced ref, the sync **fails before the tree or the lock
is touched**. A name that graduates into a stable bucket is not an error: the stable entry wins.

## Alternatives considered

### A. Vendor the three skills with a separate lock (ADR 0042 machinery)

Rejected: they are not third-party. Vendoring would misstate their provenance, require hand
re-pinning on every upstream edit, and collide with the upstream copy once one graduates.

### B. Keep a default ref

Rejected: any ref older than the skills themselves silently removes them from the lock. The
value of a default (one less argument) does not pay for that failure mode.

### C. Tag-only sync

Rejected: `implement-spec` and `retro` have no release tag yet, so this defers the toolkit's
stated need to a release upstream does not control.

## Consequences

- A sync ref is chosen deliberately; the allowlist's membership is checked at that ref and a
  miss stops the run.
- Advancing the snapshot also advances the stable skills to that ref, including unreleased
  content when the ref is a commit. Review the vendored diff as part of the sync.
- Removing a skill from upstream without removing it from `IN_PROGRESS_ALLOWLIST` now fails
  loudly instead of quietly dropping the lock entry.
