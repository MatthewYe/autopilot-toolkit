# ADR 0052: Record the Upstream Ref as a Machine-Verifiable Pin

> Extends [ADR 0037](0037-upstream-sync-full-replacement.md) (the snapshot is
> replaced verbatim) and [ADR 0049](0049-typed-lock-write-path-and-upstream-sync-crate.md)
> (machine-written lock, thin sync CLI) with the metadata that answers "which
> upstream commit is vendored?". Written by the sync made atomic in
> [ADR 0053](0053-upstream-sync-failure-atomicity.md). Consumed by the scheduled
> drift check added for #151.

## Context

Before this decision, a sync printed the resolved commit to stdout and stopped
there. The only durable record was prose in `README.md`:

```
The upstream snapshot is pinned to release v1.3.2 `d1478fcb140db0b93c0c8df17bcfd311844749d0`.
```

Nothing checked that sentence. `scripts/check.rs` verified lock hashes and
vendor hashes, but a tree synced from one commit could carry a README pin naming
another — or the README could keep an old pin after a re-sync — and CI would stay
green. Worse, the sentence format is free-form: any tool that wants to read the
pin (a scheduled drift check comparing upstream's latest tag against this repo)
would have to parse prose that a human may rephrase.

Two more requirements shaped the record:

- **The normal case is a release tag, but it is not the only case.** A branch or
  an untagged commit is sometimes the right ref (a fix that has not been released
  yet). Pinning a moving ref is an exception that must be explainable later, not
  an anonymous commit hash.
- **The pin must be written by the sync, never by hand.** A hand-edited pin can
  disagree with the tree it describes; CI must be able to tell that the tree,
  lock, and pin form one consistent snapshot.

## Decision

**Every sync writes `.upstream-pin.json` after all post-sync gates pass.**

```json
{
  "schemaVersion": 1,
  "source": "mattpocock/skills",
  "sourceUrl": "https://github.com/mattpocock/skills.git",
  "ref": "v1.3.2",
  "refKind": "tag",
  "resolvedCommit": "d1478fcb140db0b93c0c8df17bcfd311844749d0",
  "version": "1.3.2",
  "syncedAt": "2026-11-07T10:00:00Z",
  "reason": "only for refKind != tag"
}
```

- `ref` is the ref the maintainer asked for; `refKind` (`tag` | `branch` |
  `commit`) is how it was classified against the remote. `resolvedCommit` is the
  full 40-character SHA that ref resolved to; `version` is the upstream
  `package.json` version at that commit; `syncedAt` is a UTC timestamp.
- `reason` is required when `refKind` is not `tag`. The sync refuses to pin a
  branch or commit without one, and `scripts/check.rs` rejects a pin that omits
  it — the exception stays auditable.
- Non-tag pins may name `ref` as a branch name even though the vendored tree
  corresponds to `resolvedCommit`, which moves when the branch moves. The pin
  records both; the drift check compares `version`, never the branch name.

**`scripts/check.rs` verifies the pin, and CI runs it on every PR.** The checks
are fail-closed:

1. the file exists whenever `skills/upstream/` exists — a missing pin is a
   failure, not a skip;
2. the JSON parses into the typed `shared::UpstreamPin` (unknown fields
   rejected, declaration order preserved on write);
3. `schemaVersion` is known; `resolvedCommit` is 40 lowercase hex characters;
   `syncedAt` is RFC 3339 UTC (`Z`); `ref`, `source`, and `sourceUrl` are
   non-empty;
4. `refKind != tag` implies a non-empty `reason`;
5. `README.md` mentions both the pinned ref and the full resolved commit, so
   the prose and the machine record cannot drift apart;
6. for a tag pin, `skills/upstream/package.json`'s version equals the pinned
   `version` — the tree and the pin must come from the same commit.

The pin is a normal tracked file: it is committed with the sync, and the
bootstrap run that introduces this ADR commits it alongside the tooling. A
project with no upstream snapshot at all (synthetic fixtures, a tarball without
`skills/upstream/package.json`) skips the check, which keeps the vendored-hash
tests unchanged.

**A scheduled drift check reads the machine record, not the README.** The
weekly workflow (ADR-level decision lives here; implementation follows in #151)
compares the latest upstream release tag with `version` from the pin and opens
one `upstream drift:` issue when the pin is stale. It never edits the pin; only
a real sync writes it.

## Rationale

**Prose cannot be a contract.** The README sentence is for humans and is allowed
to change wording; making CI parse it would either freeze the prose or create a
second, competing format. JSON with `deny_unknown_fields` is a typed record that
fails loudly when it drifts, and it carries fields (ref kind, reason, upstream
version) that a sentence cannot express without convention.

**Verification must be cheap and total.** Every rule above is a pure local read
of committed files: no network, no registry, no git history. That means
`cargo test`, a contributor's `scripts/check.rs`, and CI all run the same
checks, and a pin/tree mismatch cannot merge. The alternative — verifying
`resolvedCommit` against the remote in CI — would make every PR depend on a
GitHub fetch and would still not prove the tree matches unless the tree were
re-hashed from a fresh clone.

**One writer.** `Backup` in the sync now snapshots `.upstream-pin.json` along
with the tree and lock (ADR 0053), so a failed sync cannot leave a pin that
describes a snapshot that was never written. The only path that writes the file
is `sync_upstream_with_options`; the runbook says never to edit it by hand.

## Alternatives considered

### A. Parse the README sentence in CI

A regex over "pinned to release … `…`" would avoid a second file, but any
wording change breaks the checker, the format has no room for `refKind`,
`reason`, or the upstream version, and drift tooling would be coupled to prose.
Rejected: the README stays human-written; the pin is the machine record.

### B. Store the pin in git tags or `git notes`

No extra file in the tree, but:
- the pin would not travel with a tarball or a shallow clone that lacks tags;
- CI checkouts often do not fetch tags by default, so the check would depend on
  checkout configuration;
- the backup/restore contract would need git plumbing instead of bytes.
Rejected: a tracked file is the simplest artifact that survives every checkout
strategy.

### C. Hand-maintained JSON

Maintaining the pin next to the sync would duplicate work and invite drift: it
is exactly the kind of metadata a human forgets to bump. Requiring the sync to
write it makes the file a by-product of the operation that changes the tree.
Rejected.

### D. No pin; let the drift check compare the README

The drift check only needs a version, and the README already states one. But
then the version is still unverified against `skills/upstream/package.json`, so
the drift check could report "no drift" for a stale tree. Rejected: the pin's
value is that the same record is checked by CI and read by the drift job.

## Consequences

- `sync-upstream` gains a third artifact: `skills/upstream/`, `.skill-lock.json`,
  and `.upstream-pin.json` always move together. A sync PR shows all three (plus
  inventory prose when the skill count changes).
- A non-tag sync needs `--reason "…"`; the CLI refuses to guess. This is a
  deliberate friction on the exception path.
- The real repository gains a bootstrap pin in this PR, produced by running the
  sync itself rather than typed by hand.
- `scripts/check.rs` no longer reports "no github skills found" as a silent pass
  when a pin is present: a checked pin forces the fail-closed verdict even in a
  project whose lock has no github entries.
- The scheduled drift check gets a stable input (`version`) and a stable parser
  (`shared::UpstreamPin`); its issue body can cite `ref`, `resolvedCommit`, and
  `syncedAt` without re-reading the tree.
- Accepted risk: `version` is only comparable when upstream bumps `package.json`.
  A release tag that reuses the previous version would look like "no drift"
  until someone reads `ref`. The drift check opens no issue on a same-version
  tag — acceptable, because upstream's release process bumps the version, and a
  human still opens the sync PR.
