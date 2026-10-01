# ADR 0050: A Shared Run-State Persistence Substrate

> Applies [ADR 0009](0009-codebase-deepening-shared-crate-and-logic-sink.md)
> (a shared crate for duplicated logic) to the published CLIs' run-state
> substrate. Implements the guard rule of
> [ADR 0025](0025-distill-state-must-be-git-ignored-before-capture.md) once for
> both CLIs. Amends [ADR 0040](0040-distill-run-state-typed-module.md) for the
> substrate only: its single-consumer rejection still applies to the state
> schema, which stays in-crate. Leaves
> [ADR 0019](0019-distill-ships-as-a-precompiled-rust-cli.md) untouched — the
> new crate is compiled in, adds no runtime dependency, and is not shipped as
> source in any artifact.

## Context

`crates/distill-cli/src/storage.rs` and `crates/director-cli/src/storage.rs` each
grew the same substrate: project-local state-directory hygiene plus an atomic
file write. The architecture review measured normalized similarity of 0.982 for
`ensure_*_ignored`, 0.993 for `ensure_*_path_safe`, and 0.939 for
`ensure_worktree` — duplication with different names, not two implementations
with different needs.

`atomic_write` had already drifted. The distill version writes through a
millisecond-suffixed temp name opened with `create_new(true)`, so a leftover temp
file from a crashed writer is never reused and two writers in the same
millisecond cannot interleave into one temp file. The director version wrote
through a fixed `state.json.tmp` opened with `create(true).truncate(true)`:
concurrent writers could interleave into that one temp file, and only the
director removed the temp file when the rename failed.

The parts that genuinely differ stayed where they were. Distill carries a
cross-process lock protocol (`create_new` lock file plus `"stale":true`
magic-string recovery) that the director has no counterpart for, and the two read
paths disagree on real behavior: missing `schema_version` handling,
`deny_unknown_fields` versus a flatten catch-all, and distill's forward-only
migration chain.

Fact-finding settled the constraints that could have blocked extraction. Neither
CLI has a zero-dependency requirement: ADR 0019 only promises that *end users* do
not need Rust. Builds and releases run in the full workspace in place
(`crates/deploy/src/artifacts.rs:173-183` builds `--release --bin <name>` from the
project root; `.github/workflows/release.yml:24-32` invokes the artifact steps
from a full checkout), and the tarball ships no crate sources. ADR 0040 rejected
a library crate for run state on "single consumer" grounds and pointed at
ADR 0009 as the precedent; the substrate now has two consumers, so the precedent
applies to the substrate while the schema keeps its single-consumer argument.

## Decision

**A new leaf crate `crates/state-store/` owns the substrate.** Four public
functions, no schema, no other responsibility:

- `ensure_ignored(worktree, dir_name, ignore_rule)` — refuse a symlinked
  `.gitignore`, append the exact rule only when it is absent, refuse when the
  state directory already exists before that rule is effective, then revalidate
  the directory path.
- `ensure_path_safe(worktree, dir_name) -> Result<PathBuf, String>` — refuse a
  state path that is a symlink, is not a directory, or canonicalizes outside the
  worktree; return the validated path.
- `ensure_worktree(worktree, dir_name)` — require a real worktree directory whose
  state directory passes the guard above.
- `atomic_write(path, bytes)` — the merged write: millisecond-suffixed temp name,
  `create_new(true)` on the temp file, `write_all`, `sync_all`, `rename`, and
  removal of the temp file when the rename fails.

**The scope is narrow on purpose.** `read_state`, `state_path`,
`deserialize_state`, `run_dir`/`validate_run_id`, and distill's lock protocol stay
in their crates. Absorbing the read path would require genericizing over two
schemas that genuinely differ, which is the merge this decision avoids.

**Each CLI keeps its domain vocabulary and its refusal text.** Both CLIs keep one
path dependency on the crate and thin adapters that bind their own directory name
and rule (`/.distill/`, `/.director/`), so every observable error string, every
call site shape, and every existing test stays as it was:
`.distill already exists before ignore is effective` and
`.director already exists before ignore is effective` are the same code path with
different parameters. Distill keeps its local `atomic_write_json` convenience
wrapper, delegating to the shared write.

**The `create_new: bool` parameter and its hard-link branch are deleted.** All
seven call sites passed `false` and no test covered the branch. Publication
payload immutability is not lost with it: immutability is enforced by content
hash, not by link semantics —
`crates/distill-cli/src/publication.rs:260-268` reads an existing frozen payload,
compares `sha256_hex(&frozen_bytes)` against the hash of the bytes it was about
to write, and refuses with `frozen publication payload changed for stable
operation`. The link branch was a second, untested mechanism for the same
property.

**The Director is the sole writer of a Spec run by design.** That is a decision,
not a mechanism: there is no second writer to lock against, so the director gets
no lock protocol, and no `--expected-revision` guard. A concurrent writer or a
stale state file is detected through the run-state revision, not prevented by
locking. Recorded in `GLOSSARY.md` under "Autopilot Director".

## Alternatives considered

### A. Extend `crates/shared/` instead of a new crate

Rejected. ADR 0009 positioned `shared` as the toolkit's infrastructure layer —
project-root derivation and `.skill-lock.json` / `.vendor-lock.json` types for the
rust-script tooling. The published CLIs' run-state substrate is a different domain
with a different release story; sinking it into the lock-schema crate would make
one crate answer two unrelated questions.

### B. Merge the two state schemas into one shared model

Rejected: it fails the deletion test. The schema is the part with real
divergence, not duplication — the read paths already differ on missing
`schema_version`, unknown-key policy, and migrations. Unifying them would force
migrations on live `.distill/` and `.director/` state to buy nothing the two
CLIs need. ADR 0040's single-consumer reasoning still holds for the schema, which
is why this ADR amends, rather than supersedes, its rejection.

### C. Adopt distill's lock protocol in the director

Rejected: no second writer exists, so the lock would guard nothing. A mechanism
with one hypothetical caller is a hypothetical seam; recording the single-writer
decision is the honest version of the same statement.

### D. Keep both copies and only align `atomic_write`

Rejected: the drift was the symptom, not the disease. Three hygiene functions at
0.939–0.993 similarity will keep drifting as long as each CLI owns a copy, and
the two copies had already diverged on temp-file handling — the one place where
divergence means lost bytes.

## Consequences

- The atomic-write and hygiene invariants have exactly one implementation, in a
  leaf crate whose threat matrix is tested directly: symlinked `.gitignore`,
  missing rule, present rule, a state directory that predates the rule, symlink
  escape, non-directory state path, an oversized path, temp-name collision, and
  rename-failure cleanup.
- `cargo test --workspace`, `cargo clippy --workspace --all-targets`, and
  `cargo build --release --bin distill` / `--bin director` cover the new crate
  through the existing `crates/*` workspace glob; the release path is unchanged.
- Behavior preservation is judged by the unmodified integration tests of both
  CLIs: 180 distill tests and 150 director tests pass with no test edit, and both
  binaries' error strings are byte-identical because the messages are
  parameterized with each CLI's own directory name and rule.
- Risk concentrates in one crate instead of two: a future hygiene change is made
  once and both CLIs inherit it, and a future divergence must be argued for
  explicitly rather than accreting by accident.
- Distill's lock protocol and its `state.lock` / `start.lock` semantics are
  untouched by this decision; they remain distill-only, and the Director's
  single-writer property remains a documented design decision rather than a
  locking mechanism.
