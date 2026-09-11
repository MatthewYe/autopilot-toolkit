# ADR 0044: Single Owner for the Expected Set

> Implements spec [#110](https://github.com/MatthewYe/autopilot-toolkit/issues/110)
> (one owner for the Expected set). Builds on ADR 0001 (selfcheck derives the expected
> set from the sources of truth) and ADR 0009 (skill-index as the discovery crate), and
> completes the vendor-source work of ADR 0042.

## Context

The Expected set — the authoritative list of skills the toolkit owns — was re-derived by
every consumer. `skill-index` modelled it, but discarded the source directory it resolved
for each entry, so `dev`, `pack`, `validation-runner`, and `skill-check` each walked the
sources again with their own path dialect; a fourth, already-stale copy lived in the
toolkit-setup tests. Adding a vendor skill (ADR 0042) required edits in four call sites
plus tests; `pack` enumerated twice (staging vs. `manifest.json`) so the tarball contents
and the ownership manifest could silently disagree; validation dropped a lock entry whose
directory was missing and still reported PASS.

## Decision

`skill-index`'s enumerator is the single owner of Expected-set enumeration. An
Expected-set entry carries its name, Skill source, skill type, variants, codex-agent flag,
resolved source directory, and resolution status (`Resolved`, or `Missing { reason }`). A
lock entry whose directory is absent yields a failed entry that is still returned, with
its reason; a missing lock file means the tree has no locked skills (not a failure), and a
malformed lock file is a hard error from the enumerator. Entries are returned in a
deterministic order — autopilot, then vendor, then upstream, name-sorted within each group
— and that order is part of the interface contract.

Strictness lives at the consumer, not in the enumerator. `dev` warns and skips a failed
entry, so a partial checkout stays usable; `pack` fails; validation FAILs. `pack` stages
tarball contents and builds `manifest.json` from one enumeration, so the shipped skill
directories and the ownership manifest are structurally coupled. The shared lock module
owns the mapping from a lock entry's path to its file and directory locations;
`skill-check`'s two private path helpers retire. The runtime variant list has exactly one
definition (`skill_index::RUNTIME_VARIANTS`) consumed by `deploy`. Installed-layout policy
(router vs. direct per ADR 0036, instruction renaming, runtime directories, fallback
priority) stays in `deploy`.

## Alternatives considered

### A. A global strictness parameter on the enumerator

Rejected: one setting cannot serve both the partial-checkout `dev` workflow (which must
warn and skip) and the release gates (which must fail). Making `dev` strict breaks the
partial-checkout workflow; making `pack` lenient ships tarballs with missing skills.

### B. A problems-list side channel

Rejected: returning entries plus a separate problems list makes every caller re-implement
status handling and lets the two views drift. Failure status belongs on the entry itself.

### C. Move installed-layout policy into skill-index

Rejected: staging policy is not enumeration, is already single-sourced in `deploy`, and
moving it would make the discovery crate depend on install mechanics. Only the runtime
variant list is shared.

### D. Let skill-index own the lock path mapping

Rejected: `skill-check` needs the mapping without needing enumeration; depending on
`skill-index` for two pure functions would drag the discovery surface into a consumer
that only hashes trees. The shared lock module already owns the lock file format.

## Consequences

- Three deliberate behavior changes: `pack` fails on failed entries (was warn-and-skip);
  validation FAILs on failed entries (was a silent drop that still reported PASS);
  iteration order becomes deterministic (was filesystem read order).
- Everything else keeps its behavior: `dev` policy, tarball layout, installed file
  layout, and lock file format are unchanged.
- A malformed lock is a hard error surfaced by the enumerator; `pack` and validation fail
  on it, and `dev` propagates it instead of skipping.
- The shared lock module owns the lock-path mapping, so `skill-check`'s private helpers
  are gone and hash outcomes are unchanged.
- Adding a skill source is one enumerator change plus the lock module, not four call
  sites; `pack`'s manifest and tarball contents can no longer disagree.
- The stale expected-set model in the toolkit-setup tests is deleted; test suites consume
  the enumerator instead of a parallel derivation (ticket #115).
