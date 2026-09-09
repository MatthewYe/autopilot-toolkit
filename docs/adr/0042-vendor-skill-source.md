# ADR 0042: Third-Party Skills Live Under skills/vendor with a Separate Lock

## Context

`show-me` (humanlayer/skills, MIT, plugin v1.0.1) is a useful
visual-explanation skill, but the toolkit's source taxonomy had only two
categories: `upstream` (a full snapshot of mattpocock/skills under
`skills/upstream/`, replaced wholesale by `scripts/sync-upstream.rs`) and
`autopilot` (locally authored workflow skills under `skills/autopilot/`).
Placing a third-party skill in either category would either corrupt the
taxonomy or be deleted by the next upstream sync. `.skill-lock.json` is
rewritten wholesale by that sync, so vendor provenance cannot live there.

## Decision

Add a third skill source: vendor skills live under `skills/vendor/<name>/`,
are tracked in `.vendor-lock.json` (separate from `.skill-lock.json`), and
carry their provenance and license inside the skill directory
(`PROVENANCE.md`). `scripts/check.rs` verifies each vendor skill's git tree
hash and can fill `TODO-recalculated` entries in `.vendor-lock.json`; drift is
a FAIL. Vendor skills are runtime-agnostic by default: `show-me`'s only
Claude Code-specific invocation (`Bash(open ...)`) was replaced with
runtime-neutral wording.

## Alternatives considered

### A. Put the skill under skills/autopilot/

Rejected: `autopilot` means locally authored workflow skills; a third-party
vendored skill would make the source label untrustworthy and hide its
provenance.

### B. Put the skill under skills/upstream/ and add a .skill-lock.json entry

Rejected: `scripts/sync-upstream.rs` replaces `skills/upstream/` and rewrites
`.skill-lock.json` wholesale, so the skill and its provenance would be
destroyed on the next upstream sync (ADR 0037).

### C. One vendor subtree per upstream repository

Rejected: a single generic `skills/vendor/` plus `.vendor-lock.json` covers all
third-party skills without per-vendor discovery logic.

## Consequences

- Skill discovery, dev symlinking, tarball packing, frontmatter validation,
  and integrity checking must understand the `vendor` source.
- `manifest.json` classifies vendor skills with type `vendor`; install and
  uninstall only need the directory names, so no install-script change is
  required.
- Vendor provenance and license ship with the installed skill.
- Adding a third-party skill requires a `.vendor-lock.json` entry and a
  `PROVENANCE.md`; adding a new third-party source does not require a new
  top-level directory.
