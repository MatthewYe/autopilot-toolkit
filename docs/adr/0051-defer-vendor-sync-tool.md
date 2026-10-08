# ADR 0051: Defer a Vendor Sync Tool and Document the Manual Procedure

> Extends [ADR 0042](0042-vendor-skill-source.md) (third-party skills under
> `skills/vendor/`, pinned by `.vendor-lock.json`) with the update path it left
> open. Contrasts with [ADR 0037](0037-upstream-sync-full-replacement.md) and
> [ADR 0049](0049-typed-lock-write-path-and-upstream-sync-crate.md), whose
> verbatim full replacement stays upstream-only.

## Context

`show-me` (humanlayer/skills, MIT, plugin v1.0.1) is the toolkit's only vendor
skill, pinned in `.vendor-lock.json` at commit `3c262914…` when this decision was
taken. ADR 0042 introduced the vendor source and its separate lock, but recorded
nothing about how an existing vendor skill moves to a newer upstream commit; the
first such update is now due.

Building `scripts/sync-vendor.rs` was evaluated as the mirror of the upstream
sync: move the clone helpers out of `crates/upstream-sync/` (`clone_at_ref`,
`copy_dir_except_git`) into `crates/git-utils/`, add a `crates/vendor-sync/` crate
behind a thin entry script, add the new files to the `rustfmt --check` and
clippy-scripts lists in `.github/workflows/ci.yml`, and document the workflow.
The estimate is ≈500 LOC of logic plus ≈300 LOC of tests — the shape of the
existing upstream-sync pair (897-line crate, 111-line CLI) — for a repository
with one vendor skill, one vendor source, and a first update.

Two existing properties bound the cost of not building it. Vendor entries are
already integrity-checked: `crates/skill-check/` requires `PROVENANCE.md` and
compares each vendor folder's git tree hash, repairs a literal
`TODO-recalculated` with FIX + PASS, and `.github/workflows/ci.yml` runs
`rust-script scripts/check.rs` on every pull request. The one mechanical failure
mode — a stale hash — therefore cannot merge. What nothing verifies is provenance
metadata: `sourceCommit`, `pluginVersion`, and `skillPath` are never compared
against upstream, because the check has no network access and no upstream
checkout.

## Decision

**Do not build `scripts/sync-vendor.rs` now.** Vendor updates run through the
documented manual procedure in
[docs/agents/vendor-updates.md](../agents/vendor-updates.md): shallow clone →
`diff -ru` against the vendored directory →
`rsync -a --delete --exclude PROVENANCE.md` → re-apply local patches → update
`PROVENANCE.md` → minimal `.vendor-lock.json` edit (`sourceCommit`,
`pluginVersion`, `updatedAt`, and `skillFolderHash: "TODO-recalculated"`) →
`rust-script scripts/check.rs` repairs and verifies the hash →
`rust-script validation/run.rs` and `rust-script --test tests/test_check.rs`.

Nothing in the procedure asks a human or an agent to compute a hash: the lock
hash stays machine-generated and CI-enforced.

**Triggers that flip this decision.** Revisit this ADR when any of them fires:

1. a second vendor skill, or a second vendor source repository;
2. two or more vendor updates per quarter;
3. a stale-metadata incident that escapes review (a wrong `sourceCommit` or
   `pluginVersion` merging unnoticed);
4. adoption of a machine-readable local patch file (for example
   `skills/vendor/<name>/local.patch`), which gives a tool a defined input format
   for the part that is currently prose in `PROVENANCE.md`.

## Rationale

**The two syncs are not the same operation.** `sync-upstream` replaces a snapshot
verbatim: every file is copied exactly as upstream wrote it, with no local edits
to preserve. A vendor skill by definition carries local patches — `show-me`'s
final HTML-open step was rewritten to runtime-neutral wording, and its
`PROVENANCE.md` lives inside the hashed directory with no upstream counterpart. A
correct vendor sync therefore needs a patch/local-mod mechanism: either a
machine-readable patch applied after a verbatim copy, or a tool that knows which
hunks are local and which are upstream drift. That is an ADR-level design
decision about how this repo records local modifications, not a re-run of an
existing script.

**The one hard failure mode already has a tested, CI-gated repair path.**
`TODO-recalculated` → FIX + PASS is covered by `crates/skill-check` unit tests,
by `tests/test_check.rs`, and by CI on every PR. Everything the procedure does
not verify is review, and the procedure makes that review explicit instead of
implicit.

**Proportionality.** One vendor skill, one source repo, the first update: ≈800
LOC of new tooling plus CI wiring and docs to replace a handful of commands that
run a few times a year. The procedure document is the artifact the next updater
— agent or human — actually needs.

## Alternatives considered

### A. Build `scripts/sync-vendor.rs` now

Deferred rather than rejected as impossible: see the triggers above. Built now, it
would either replace the vendor directory verbatim — silently reverting
`show-me`'s runtime-neutral patch and deleting `PROVENANCE.md`, which turns the
check into `missing PROVENANCE.md` — or need the patch mechanism first. Shipping
the shell and deferring the semantics would produce a tool that does the wrong
thing confidently.

### B. Extend `scripts/sync-upstream.rs` with a vendor mode

Rejected: the script is owned by the upstream snapshot — it replaces
`skills/upstream/`, rewrites `.skill-lock.json` wholesale, and applies the
`in-progress` allowlist ([ADR 0043](0043-upstream-sync-explicit-ref-and-in-progress-allowlist.md)).
ADR 0042 gave vendors a separate lock and tree precisely so an upstream sync can
never touch them; one entry point with a source-dependent mode would undo that
separation, and its no-local-edit assumption is wrong for every vendor
directory.

### C. Copy upstream verbatim and keep the patch elsewhere

Rejected: the only durable record of a local patch is `PROVENANCE.md`, inside the
hashed directory and shipped with the skill. `.scratch/` is gitignored agent
workspace and a PR body is not reference material (AGENTS.md), so a patch
recorded there is invisible to the next updater and to the agent that reads the
installed skill.

### D. Rely on CI alone, with no documented procedure

Rejected: `check.rs` catches a wrong hash, but the pitfalls that bite are the
ones it cannot catch, or catches only as a late failure — editing files after the
hash run, `cp -R` orphans, a platform subdirectory (`codex|kimi|reasonix|dsh`)
silently flipping the skill to runtime-coupled, and stale provenance fields that
nothing verifies. Those are review responsibilities and they need a checklist.

## Consequences

- Updating a vendor skill is a short, reviewed manual procedure: no new crate, no
  new CI job, no new failure surface. The procedure is
  [docs/agents/vendor-updates.md](../agents/vendor-updates.md); ADR 0042's source
  taxonomy and ADR 0037/0049's verbatim replacement are unchanged.
- The hash stays machine-generated and CI-enforced, so a hand-computed or stale
  hash cannot merge — the accepted risk is metadata drift, not corruption.
- Accepted risk: `sourceCommit` and `pluginVersion` can go stale without failing
  anything. The mitigation is the review checklist, plus an optional ~40 LOC
  provenance-consistency check in `crates/skill-check` that compares those fields
  against `PROVENANCE.md`. It is not built now — there is no incident to justify
  it — and it needs no network or clone, since `PROVENANCE.md` is the local
  source of truth. It becomes a candidate the moment a metadata incident shows
  the checklist is not enough.
- A future `sync-vendor` would replace only the copy-and-patch half of this
  procedure; the lock-and-hash half is already shared, so the tool can be added
  later without reworking anything recorded here.
