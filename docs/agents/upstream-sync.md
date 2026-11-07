# Upstream Sync Runbook

How to replace the vendored snapshot at `skills/upstream/` with a newer
[mattpocock/skills](https://github.com/mattpocock/skills) revision. Audience: a
maintainer or an agent running a one-shot sync. The design is recorded in
[ADR 0037](../adr/0037-upstream-sync-full-replacement.md) (verbatim replacement),
[ADR 0043](../adr/0043-upstream-sync-explicit-ref-and-in-progress-allowlist.md)
(explicit ref and the `in-progress` allowlist), and
[ADR 0053](../adr/0053-upstream-sync-failure-atomicity.md) (atomic failure
handling; this runbook is its operating procedure).

The sync replaces two things wholesale: the `skills/upstream/` tree and
`.skill-lock.json`. It does not touch `skills/vendor/` or `skills/autopilot/`
(third-party skills follow [vendor-updates.md](vendor-updates.md), ADR 0051).
Since ADR 0053 the sync is all-or-nothing: the pre-sync tree, lock, and pin are
snapshotted to a temp directory first and restored byte-for-byte if any
post-sync gate fails.

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
   `in-progress` allowlist notes, and the added/updated/removed lists:

   ```bash
   rust-script scripts/sync-upstream.rs <ref>
   ```

   In a sandboxed session use
   `bash scripts/sandboxed-rust-script.sh scripts/sync-upstream.rs <ref>`.
   To overwrite dirty or untracked local files on purpose, add `--force`
   (anywhere after the script path).

3. Wait for the post-sync gates. The build runs the hash check
   (`crates/skill-check`) and the full validation suite (`validation/run.rs`) in
   process, against the tree and lock it just wrote. A failure restores the
   pre-sync state and exits non-zero — the working tree is still on the previous
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
   | `docs` inventory counts | only when the skill count changed (step 4) |

   `git diff --stat skills/upstream .skill-lock.json` should match the
   added/updated/removed lists printed by the script.

6. Verify and open a PR. The sync is complete only when the repository gates pass
   and the resolved commit is recorded in the PR body:

   ```bash
   rust-script scripts/check.rs      # ALL PASS
   rust-script validation/run.rs     # exit 0
   rust-script --test tests/test_sync_upstream.rs
   ```

   Commit `skills/upstream/` and `.skill-lock.json` together, open a PR with the
   issue it closes, and paste the resolved commit printed in step 2.

## Failure and rollback semantics

| Failure | Behavior |
| --- | --- |
| tracked files dirty, or untracked files under `skills/upstream/` | refused before cloning; lists the paths; `--force` proceeds |
| unknown ref / clone error | no local changes yet; exits non-zero |
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
