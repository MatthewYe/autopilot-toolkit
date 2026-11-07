# Upstream Sync Runbook

How to replace the vendored snapshot at `skills/upstream/` with a newer
[mattpocock/skills](https://github.com/mattpocock/skills) revision. Audience: a
maintainer or an agent running a one-shot sync. The design is recorded in
[ADR 0037](../adr/0037-upstream-sync-full-replacement.md) (verbatim replacement),
[ADR 0043](../adr/0043-upstream-sync-explicit-ref-and-in-progress-allowlist.md)
(explicit ref and the `in-progress` allowlist),
[ADR 0053](../adr/0053-upstream-sync-failure-atomicity.md) (atomic failure
handling; this runbook is its operating procedure), and
[ADR 0052](../adr/0052-upstream-pin-and-drift-check.md) (`.upstream-pin.json`,
the machine-verifiable record of the synced ref).

The sync replaces two things wholesale — the `skills/upstream/` tree and
`.skill-lock.json` — and writes a third: `.upstream-pin.json`. It does not touch
`skills/vendor/` or `skills/autopilot/` (third-party skills follow
[vendor-updates.md](vendor-updates.md), ADR 0051). Since ADR 0053 the sync is
all-or-nothing: the pre-sync tree, lock, and pin are snapshotted to a temp
directory first and restored byte-for-byte if any post-sync gate fails.

## When to use this

- Upstream publishes a new release tag and the snapshot should move to it.
- A specific upstream commit or branch is needed (for example to pull a fix that
  has not been tagged yet).
- An existing sync attempt failed and needs re-running after the cause was
  fixed (safe: a failed attempt already restored the previous state).

## Preconditions

The sync refuses to start rather than risk mixing local changes into the
snapshot:

- tracked files in the worktree must be clean (`git status --porcelain -uno`);
- `skills/upstream/` must not contain untracked files
  (`git status --porcelain -uall -- skills/upstream`).

Either condition can be overridden with `--force`. Forced runs print every path
they overwrite and still snapshot the pre-sync state (including the dirty files),
so `--force` is recoverable — but it is for throwaway checkouts, not for
committing a sync on top of unreviewed local edits.

A project that is not a git worktree (CI fixture, source tarball) skips the
precondition instead of failing.

`git` must be on `PATH`; the upstream GitHub remote is reachable. For a fork run,
point `AUTOPILOT_UPSTREAM_REPO` at the fork's clone URL — the sync records it as
the source URL.

## Procedure

1. Pick a ref. Prefer a release tag so the snapshot never tracks a moving branch:

   ```bash
   git ls-remote --tags --refs https://github.com/mattpocock/skills.git \
     | awk -F/ '{print $NF}' | grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$' | sort -V | tail -1
   ```

2. Sync to it. The script prints the resolved commit, every discovered skill, the
   `in-progress` allowlist notes, the added/updated/removed lists, and the pin it
   wrote:

   ```bash
   rust-script scripts/sync-upstream.rs <ref>
   ```

   In a sandboxed session use
   `bash scripts/sandboxed-rust-script.sh scripts/sync-upstream.rs <ref>`.
   To overwrite dirty or untracked local files on purpose, add `--force`
   (anywhere after the script path). A branch or commit SHA is refused unless
   paired with `--reason "…"`, because a non-tag pin records an exception that
   must stay auditable:

   ```bash
   rust-script scripts/sync-upstream.rs --reason "main carries the 1.3.3 release candidate" main
   ```

   Prefer a release tag. A non-tag ref is a deliberate, temporary exception —
   the next release sync should move back to a tag.

3. Wait for the post-sync gates. The build runs the hash check
   (`crates/skill-check`) and the full validation suite (`validation/run.rs`) in
   process, against the tree and lock it just wrote. Only after both pass does it
   write `.upstream-pin.json`. A failure restores the pre-sync state (tree, lock,
   and pin) and exits non-zero — the working tree is still on the previous
   snapshot, so nothing is half-synced.

4. If the validation gate fails on the **documented inventory**, update the
   prose counts and re-run. An upstream release that adds or removes a skill
   changes `total` / `upstream` counts, and `validation/run.rs` compares those
   against the discovered set. The error report names each stale document and the
   exact expected phrase, for example:

   ```
   AGENTS.md is missing "33 upstream skills from mattpocock/skills"
   ```

   Update `AGENTS.md`, `README.md`, and `GLOSSARY.md` (the phrases live in
   `crates/validation-runner/src/lib.rs`) and re-run the sync. The first attempt
   rolled back, so the second still starts from the clean pre-sync tree.

5. Review the diff. What to expect:

   | path | expected change |
   | --- | --- |
   | `skills/upstream/` | verbatim upstream files: added/removed skills, changed content |
   | `.skill-lock.json` | one entry per discovered skill; `skillFolderHash`, `updatedAt` |
   | `.upstream-pin.json` | `ref`, `refKind`, `resolvedCommit`, upstream `version`, `syncedAt` |
   | `README.md` | the prose pin must mention the new `ref` and full `resolvedCommit` |
   | `docs` inventory counts | only when the skill count changed (step 4) |

   `git diff --stat skills/upstream .skill-lock.json .upstream-pin.json` should
   match the added/updated/removed lists printed by the script, and the README
   sentence must be updated to the same ref/commit. `scripts/check.rs` fails the
   PR if it is not.

6. Verify and open a PR. The sync is complete only when the repository gates pass
   and the resolved commit is recorded in the PR body:

   ```bash
   rust-script scripts/check.rs      # ALL PASS, including the upstream pin
   rust-script validation/run.rs     # exit 0
   rust-script --test tests/test_sync_upstream.rs
   ```

   Commit `skills/upstream/`, `.skill-lock.json`, and `.upstream-pin.json`
   together, open a PR with the issue it closes, and paste the resolved commit
   printed in step 2.

## The upstream pin

`.upstream-pin.json` is written by the sync, never by hand (ADR 0052):

| field | meaning |
| --- | --- |
| `schemaVersion` | always `1` for this format |
| `source` | `mattpocock/skills` |
| `sourceUrl` | clone URL the sync used (`AUTOPILOT_UPSTREAM_REPO` for a fork run) |
| `ref` | the ref asked for: release tag, branch name, or commit |
| `refKind` | `tag` (preferred), `branch`, or `commit` |
| `resolvedCommit` | full 40-character commit the ref resolved to |
| `version` | upstream `package.json` version at that commit |
| `syncedAt` | UTC timestamp of the successful sync |
| `reason` | required for `branch` and `commit` pins |

`scripts/check.rs` enforces the record end to end: the JSON parses with unknown
fields rejected, `resolvedCommit` is 40 lowercase hex characters, `syncedAt` is
RFC 3339 UTC, `refKind != tag` carries a `reason`, `README.md` mentions both the
ref and the full commit, and a tag pin's `version` matches
`skills/upstream/package.json`. A missing pin next to a vendored tree is a
failure, not a skip. The bootstrap pin in this repository was produced by
running the sync, not typed.

## Scheduled drift check

A sync is still a manual, reviewable act, but nobody should have to remember to
look for a new upstream release. `.github/workflows/upstream-drift.yml` runs
every Monday at 09:00 UTC and can be dispatched by hand:

```bash
gh workflow run upstream-drift.yml
gh workflow run upstream-drift.yml -f simulate_tag=v1.3.3   # exercise the drift path
```

The job reads `.upstream-pin.json`, asks upstream for its tags
(`git ls-remote --tags --refs`), keeps the newest `vMAJOR.MINOR.PATCH` release,
and compares its version with the pin's `version`. It never clones the snapshot,
runs a sync, commits, or merges — the only side effect is a GitHub issue.

| Situation | What the workflow does |
| --- | --- |
| pin is current (or ahead) | closes the single open `upstream drift:` issue, if any; nothing else |
| a newer release exists | opens one issue labelled `enhancement`, or updates it in place; closes duplicates |
| check cannot run (missing pin, no release tags) | fails the job — it is a pin/tag problem to fix, not drift |

The tracking issue is a reminder, not an action: work through the Procedure above
on a clean branch and open a normal sync PR. The issue closes itself on the next
run after the pin catches up. Its title always starts with `upstream drift:`;
that prefix is how the workflow finds the issue again, so do not rename it. The
`simulate_tag` input bypasses the network and feeds the tag straight into the
comparison — it is the supported way to test the issue path without waiting for a
real release.

## Failure and rollback semantics

| Failure | Behavior |
| --- | --- |
| tracked files dirty, or untracked files under `skills/upstream/` | refused before cloning; lists the paths; `--force` proceeds |
| unknown ref / clone error | no local changes yet; exits non-zero |
| branch or commit ref without `--reason` | refused before cloning; no local changes |
| malformed `.skill-lock.json` | refused before cloning; no local changes |
| allowlisted `in-progress` skill missing at the ref | no local changes |
| post-sync hash check fails, or a check skips | tree + lock + pin restored; exits non-zero |
| post-sync validation fails | tree + lock + pin restored; exits non-zero with the report |
| restore itself fails | exits non-zero and prints the backup directory — keep it |
| panic during the destructive phase | `Backup::drop` restores before unwinding |

The backup lives under the system temp directory as
`upstream-sync-backup-<pid>-<counter>/` and is deleted only after a fully
successful sync.

## Pitfalls

- Do not edit `.skill-lock.json` by hand — the sync rewrites it and
  `scripts/check.rs` verifies the hashes. A hand-edit fails CI.
- Do not edit `.upstream-pin.json` by hand either. If the pin disagrees with the
  tree or the README, re-run the sync (or fix the README and re-run the sync);
  `scripts/check.rs` fails a hand-edited pin whose commit, ref, or version does
  not match the synced state.
- Update `README.md` in the same PR. The check requires the pinned ref and the
  full `resolvedCommit` to appear there; prose drift fails CI even when the pin
  file is correct.
- Do not add upstream fixes locally. The snapshot is verbatim by design; send the
  fix upstream, then sync the tag that contains it (ADR 0037).
- Do not use `--force` to carry local edits into a release PR. It exists for
  throwaway checkouts and to recover from leftover generated files.
- The `in-progress/` bucket is allowlisted, not universal: only `loop-me` is
  vendored (ADR 0043). A new upstream beta skill needs an allowlist entry first.
- The sync copies symlinks as symlinks and excludes `.git` metadata; a stray
  `skills/upstream/.../2` companion directory means a previous manual copy, not a
  sync artifact.
- `init`-only fixtures in tests are not a git worktree, so the dirty-tree
  precondition is skipped there; an integration test that wants the refusal path
  must create a real repository (see `init_project_git` in
  `tests/test_sync_upstream.rs`).
- The documented-inventory check walks the whole project for duplicate
  `X 2.ext` companion files and for duplicate ADR numbers; a stray file outside
  `CHECK_SKIP_DIRS` fails validation even if it is unrelated to the sync.
