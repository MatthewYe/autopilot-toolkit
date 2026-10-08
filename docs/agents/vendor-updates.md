# Vendor skills: how to update one

Runbook for moving a skill under `skills/vendor/<name>/` to a newer upstream
commit. Audience: whoever maintains `skills/vendor/` — agents and humans alike.

This is the manual procedure that [ADR 0051](../adr/0051-defer-vendor-sync-tool.md)
defers tooling to, on top of the vendor source rules of
[ADR 0042](../adr/0042-vendor-skill-source.md). The integrity rules it relies on
live in [scripts/check.rs](../../scripts/check.rs) and
[crates/skill-check/](../../crates/skill-check/src/lib.rs).

## When to use this

Use it when an upstream repo has moved and you want that change in a vendored
skill.

**Never use `scripts/sync-upstream.rs` for a vendor skill.** That script belongs
to `skills/upstream/` only ([ADR 0037](../adr/0037-upstream-sync-full-replacement.md),
[ADR 0049](../adr/0049-typed-lock-write-path-and-upstream-sync-crate.md)) and
does **verbatim replacement**: clone a ref, copy the skill directory exactly as
upstream wrote it, replace the whole tree, rewrite `.skill-lock.json`, run the
hash check. Applied to a vendor skill that is wrong twice over:

- **Vendored skills carry local patches.** `show-me`'s final open-the-HTML step
  was rewritten from Claude Code's `Bash(open …)` into runtime-neutral wording
  (its `PROVENANCE.md` § Modifications). A verbatim copy silently reverts that.
- **`PROVENANCE.md` lives inside the vendored directory** and has no upstream
  counterpart. A verbatim replacement deletes it, and the next check FAILs with
  `missing PROVENANCE.md (vendor provenance and license required)`.

The vendor lock entry carries a `skillPath` that looks like an upstream skill
path (`plugins/show-me/skills/show-me/SKILL.md`). That is provenance metadata,
not an instruction to run the upstream sync.

## Procedure

Worked example: updating `show-me` from `humanlayer/skills`. Substitute `<name>`,
`sourceUrl`, and the target commit from the `.vendor-lock.json` entry.

1. **Clone the new ref shallowly into scratch space.**

   ```bash
   # By tag or branch:
   git clone --depth 1 --branch <ref> <sourceUrl> /tmp/vendor-upstream
   # By commit (clone --branch takes branches and tags only):
   git init /tmp/vendor-upstream
   git -C /tmp/vendor-upstream remote add origin <sourceUrl>
   git -C /tmp/vendor-upstream fetch --depth 1 origin <commit>
   git -C /tmp/vendor-upstream checkout FETCH_HEAD
   ```

2. **Diff upstream against the vendored copy.** The upstream source directory is
   `skillPath` minus `/SKILL.md`:

   ```bash
   diff -ru /tmp/vendor-upstream/plugins/show-me/skills/show-me skills/vendor/show-me
   ```

   Read every hunk. Your own patches appear in reverse (the vendored side
   differs); everything else is an upstream change to judge against the
   checklist below.

3. **Apply the upstream tree over the vendored one**, keeping the locally
   authored provenance file:

   ```bash
   rsync -a --delete --exclude PROVENANCE.md \
     /tmp/vendor-upstream/plugins/show-me/skills/show-me/ \
     skills/vendor/show-me/
   ```

   `--delete` removes files upstream deleted. `cp -R` does not, so it leaves
   orphans behind. Excluded files are not deleted, so `PROVENANCE.md` survives.
   `-a` preserves file modes, which matters because modes are part of the hash
   (see Pitfalls). Add one `--exclude` per platform subdirectory you are
   rejecting.

4. **Re-apply the local patches** the diff showed in reverse, and record each one
   in `PROVENANCE.md` § Modifications. A patch that upstream has since adopted
   should be dropped from the list rather than kept.

5. **Update `PROVENANCE.md`**: upstream URL, source path, plugin version, pinned
   commit, the Modifications section, and the license section if upstream
   changed it.

6. **Edit `.vendor-lock.json` minimally.** Exactly these fields:

   | Field | New value |
   | --- | --- |
   | `sourceCommit` | the commit you copied |
   | `pluginVersion` | the upstream plugin version at that commit |
   | `updatedAt` | now, ISO 8601 (e.g. `2026-10-08T00:00:00.000Z`) |
   | `skillFolderHash` | `"TODO-recalculated"` |

   Leave `source`, `sourceType`, `sourceUrl`, `skillPath`, `vendorPath`,
   `pluginName`, `license`, and `installedAt` alone unless they actually
   changed. Keys in this file are alphabetical; the typed writer keeps them that
   way.

7. **Let `check.rs` compute the hash.** Do this *after* every other edit:

   ```bash
   rust-script scripts/check.rs
   ```

   Expected on the first run:

   ```text
   FIX: show-me → <40-hex tree sha>
   PASS: show-me
   ...
   ALL PASS
   ```

   `FIX` records the recomputed hash, `PASS` reports the skill green after the
   lock repair, and the real hash is written into `.vendor-lock.json` — nothing
   is hand-computed. Run it a second time: it must print a plain `PASS` with no
   `FIX`.

8. **Run the rest of the gates.**

   ```bash
   rust-script validation/run.rs            # SKILL.md frontmatter, all variants
   rust-script --test tests/test_check.rs   # check.rs CLI contract
   ```

9. **Review the final diff** (`git diff --stat`, then
   `git diff .vendor-lock.json skills/vendor/show-me`): the lock should differ
   in exactly the four fields from step 6, and the skill tree in exactly the
   accepted upstream changes plus your patches.

## Deciding what to take

Judge each upstream hunk rather than merging wholesale:

- **Behavior-affecting frontmatter is an explicit accept/reject.** A new
  `disable-model-invocation: true` hides the skill from the model catalog under
  DSH ([runtimes/dsh.md](runtimes/dsh.md)); a changed `name` no longer matches
  the vendor directory or the lock key; a rewritten `description` changes when
  the model reaches for the skill. Decide, and record the outcome in
  `PROVENANCE.md` § Modifications either way.
- **Platform subdirectories named `codex`, `kimi`, `reasonix`, or `dsh` must
  never be copied into a vendor directory.** `crates/skill-index/`'s
  `RUNTIME_VARIANTS` + `classify_skill` classify any skill that contains one as
  runtime-**coupled**: the vendor skill would flip from agnostic to coupled, and
  `deploy`/`pack` would stage it in the runtime-router layout instead of
  installing `SKILL.md` as the skill root. Reject them in step 3
  (`--exclude codex --exclude kimi --exclude reasonix --exclude dsh`) and note
  the omission in `PROVENANCE.md`.
- **Everything else is inert but ships.** Files like `agents/openai.yaml` (an
  invocation policy read by other runtimes, not by this toolkit's discovery),
  extra references, or assets do not influence classification, but they are part
  of the folder hash and are shipped by the symlink install and the `pack` copy.
  Keep them when the skill body references them; omit them when they are
  Claude-Code-only scaffolding.
- **License and attribution changes are mandatory.** If upstream changed the
  license text, copy it and update `PROVENANCE.md`; a changed license value also
  belongs in the lock's `license` field.

## Pitfalls

- **Never hand-compute `skillFolderHash`.** It is not `sha1sum SKILL.md` and not
  `git hash-object`: it is `git_utils::compute_tree_hash(folder)` — a scratch
  `git init`, `git add -A` over the whole vendor folder, then `git write-tree`.
  The result is a 40-hex **tree** SHA covering every file (including
  `PROVENANCE.md`), the directory layout, and file modes; flipping an exec bit
  or adding a stray file changes it, and a `.gitignore` inside the folder
  silently excludes matching files from the hash. Let `check.rs` produce it.
- **Edit metadata before running `check.rs`.** The repair only fires on the
  literal `TODO-recalculated`. Any edit after the `FIX` run — a typo fix in
  `SKILL.md`, a `PROVENANCE.md` touch-up, a new file — breaks the hash again and
  turns the next check into `FAIL: <name> (computed: …, lockfile: …)`. If that
  happens, set the hash back to `TODO-recalculated` and re-run.
- **`set_folder_hash` does not stamp `updatedAt`.** The typed writer in
  [crates/shared/](../../crates/shared/src/lib.rs) writes only
  `skillFolderHash`; `updatedAt` is yours to set in step 6.
- **`sourceCommit`, `pluginVersion`, and `skillPath` are never verified against
  upstream.** `check.rs` has no network access and no upstream checkout: it
  verifies the folder hash and that `PROVENANCE.md` exists, nothing else.
  Reviewing these fields is the reason this document exists.
- **A wrong `vendorPath` fails open.** A missing `vendorPath` falls back to
  `skills/vendor/<name>` (`LockedSkill::vendor_dir`), so a typo either hashes a
  different directory or fails with `folder not found`. Keep `vendorPath`
  pointing at the directory you actually edited.
- **Unknown keys in a lock entry are dropped.** `LockedSkill` is not
  `deny_unknown_fields`: a custom field added to `.vendor-lock.json` parses fine
  and then disappears the next time `check.rs` rewrites the file. Stick to the
  known fields.
- **CI is the backstop, not the reviewer.** `.github/workflows/ci.yml` runs
  `rust-script scripts/check.rs` on every pull request, so a wrong hash cannot
  merge. But a local `ALL PASS` only proves the lock matches the tree on disk —
  it says nothing about whether that tree matches upstream.
