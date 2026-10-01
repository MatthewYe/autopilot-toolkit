# ADR 0049: Typed Lock Write Path and an Upstream-Sync Crate

> Extends [ADR 0009](0009-codebase-deepening-shared-crate-and-logic-sink.md)
> (shared crate as the lock schema owner) from the read side to the write side.
> Supersedes the self-check mechanism of
> [ADR 0037](0037-upstream-sync-full-replacement.md) (subprocess `check.rs` →
> in-process `skill_check::check_skills`); full replacement and orphan removal
> remain in force. Leaves the sync policy of
> [ADR 0043](0043-upstream-sync-explicit-ref-and-in-progress-allowlist.md)
> (explicit ref, in-progress allowlist) unchanged. Uses the malformed-lock
> vocabulary of [ADR 0044](0044-single-owner-for-the-expected-set.md).

## Context

ADR 0009 sank `scripts/check.rs`, `deploy.rs`, and `validation/run.rs` into
workspace crates behind thin rust-script entries. `scripts/sync-upstream.rs`
missed that round — it did not exist yet (ADR 0037 introduced it later) — and
grew to 482 lines of business logic: clone-at-ref, skill discovery, allowlist
handling, lock construction, tree replacement, and a final self-check.

The lock schema was typed only on the read side. ADR 0009 §5 made `shared`'s
`SkillLock` / `LockedSkill` the single parse entrypoint, but both writers
operated on untyped `serde_json::Value`: `skill-check`'s hash-repair writer
patched `skillFolderHash` by string key, and `sync-upstream` built the whole
`.skill-lock.json` value tree by hand. Field names, the two on-disk key orders,
and the timestamp merge rules were string literals and convention the compiler
could not check, and the two writers could drift from the reader and from each
other.

The sync's final integrity gate was a subprocess call to
`rust-script -f scripts/check.rs`, re-paying rust-script's compile latency on
every sync and piping the verdict back through an exit code, even though the
same logic already lived in-process as `crates/skill-check/`.

## Decision

**`shared` owns the lock schema for writing as well as reading.** `LockedSkill`
is a full-fidelity model: every field seen in either real lock file is
represented — the eight upstream entry fields plus the vendor-only fields as
`Option`, and the top-level `dismissed` object kept verbatim.
`SkillLock::to_bytes()` serializes byte-stably: a lock parsed and
re-serialized without modification reproduces the original file bytes
exactly. The two flavors use
different key orders on disk (`.skill-lock.json` uses insertion-order entry
fields and top-level `version, skills, dismissed`; `.vendor-lock.json` is
strictly alphabetical), so `LockFlavor` selects the order and serialization
renders a small ordered-JSON tree directly instead of deriving `Serialize`.
The `preserve_order` serde_json feature is deliberately not enabled: feature
unification is workspace-wide, and flipping it would change the byte format of
every other crate's JSON state files. Two typed mutators own the write
operations: `set_folder_hash` (update one entry's hash, nothing else) and
`replace_skills` (merge a freshly discovered skill set: `installedAt` carries
over, `updatedAt` is stamped only when the hash changed or the entry is new,
orphans are removed, `dismissed` is never touched).

**`skill-check`'s two lock writers become thin calls** — `write_updated_lockfile`
and `write_updated_vendor_lockfile` load through `shared`, mutate via
`set_folder_hash`, and write `to_bytes()`.

**A new crate `crates/upstream-sync/` owns the sync workflow.** One public
entry, `sync_upstream(project_root, upstream_ref, repo_url, now)`: clone the
ref into a temp dir → discover skills (`BUCKET_DIRS` and
`IN_PROGRESS_ALLOWLIST` move with the logic) → merge via `replace_skills` →
replace the `skills/upstream/` tree → write the byte-stable lock → run
`skill_check::check_skills` in-process, where any FAIL fails the sync. The
entry script `scripts/sync-upstream.rs` degrades to a thin CLI (111 lines) that
only parses the ref, resolves the repo URL (overridable via
`AUTOPILOT_UPSTREAM_REPO`, which tests point at a local fixture repo), and
renders the `SyncOutcome` report.

**Three deliberate behavior changes** versus the legacy script:

1. An unparseable existing `.skill-lock.json` is a hard error. The legacy
   script warned and continued with an empty lock, which misclassified every
   skill as an orphan and would have emptied the lock. This aligns with
   ADR 0044's vocabulary: a malformed lock is a hard error, everywhere.
2. `updatedAt` is stamped only on entries whose hash changed or that are new;
   the legacy script re-stamped every entry. Re-syncing the same ref now
   produces a byte-identical lock file.
3. The `dismissed` object is preserved verbatim instead of being reset to `{}`
   (a brand-new lock still starts with `dismissed: {}`).

## Alternatives considered

### A. Derive `Serialize` on the lock types

Rejected: serde's derived serialization cannot express two different key orders
for the two flavors, and `serde_json::Map` without `preserve_order` sorts keys
alphabetically. The ordered-JSON tree keeps both byte formats exact with the
type system still owning field names.

### B. Enable serde_json's `preserve_order` feature

Rejected: features unify across the workspace, so one crate's convenience would
silently change the serialization of every other crate that writes JSON state
files — a workspace-wide side effect to fix a local formatting need.

### C. A separate write-side type hierarchy

Rejected: one file format with two type sets (read types in `shared`, write
types elsewhere) invites the exact drift this ADR removes. Full-fidelity
`LockedSkill` serves both directions.

### D. Sink the sync workflow into `skill-check` or `skill-index`

Rejected: `skill-check` verifies hashes of an existing tree; it does not fetch
or replace one. `skill-index` enumerates the Expected set of this repo; it does
not mutate it. Sync is a third responsibility and gets its own crate.

### E. Keep the subprocess self-check

Rejected: it re-pays rust-script's compile latency on every sync and moves the
verdict through an exit code when `skill_check::check_skills` already returns a
structured report in-process. Strategy (verify after sync) is unchanged; only
the mechanism moves.

## Consequences

- ADR 0009 §5's "shared is the single parse entrypoint for the lock files"
  extends to the write side: every read and every write of
  `.skill-lock.json` / `.vendor-lock.json` goes through `shared`'s types, and
  field names and merge rules are compiler-checked in one place.
- ADR 0037's "run `check.rs` as a final integrity check" is superseded in
  mechanism: the post-sync check runs in-process via
  `skill_check::check_skills`. Full replacement and orphan removal — the
  strategy ADR 0037 and ADR 0043 fixed — are unchanged.
- ADR 0043's sync policy is unchanged: the ref is still explicit with no
  default, and an allowlisted skill missing at the ref still fails the sync
  before the tree or the lock is touched. The `IN_PROGRESS_ALLOWLIST` constant
  now lives in `crates/upstream-sync/`, next to the logic that enforces it.
- Acceptance properties, enforced by tests: the read → `to_bytes()` round trip
  on the real `.skill-lock.json` and `.vendor-lock.json` is byte-identical
  (`shared` tests), and re-syncing the same ref leaves the lock bytes untouched
  (`upstream-sync`'s `sync_same_ref_twice_is_byte_identical`).
- The sync workflow runs under plain `cargo test` against local fixture
  repositories via `AUTOPILOT_UPSTREAM_REPO`; no network or real upstream is
  needed to test it.
