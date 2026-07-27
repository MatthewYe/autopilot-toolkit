# ADR 0009: Upstream Sync via Full Replacement with Automation Script

## Context

The vendored upstream `skills/upstream/` (mattpocock/skills v1.0.1) must be updated to v1.1.0, which renamed `to-prd` to `to-spec`, replaced `to-issues` with `to-tickets`, added new skills (`code-review`, `research`, `wayfinder`), and modified existing ones (TDD became reference-oriented, grilling gained a confirmation gate). Future upstream releases will require this again.

## Decision

**Sync by full replacement with an automation script (`scripts/sync-upstream.rs`).** The script clones the requested release tag (defaulting to the currently pinned `v1.1.0`), computes each skill's `skillFolderHash`, replaces the entire `skills/upstream/` directory (excluding `.git/`), writes `.skill-lock.json`, and runs `check.rs` as a final integrity check. Subsequent syncs use one command with an explicit tag when advancing releases.

## Alternatives considered

### A. Incremental update (add/replace only changed skill directories)

Rejected: requires manual diffing between releases. Misses renamed skills silently — `to-issues/` and `to-prd/` would linger as orphan directories. Also misses metadata file updates (package.json, CHANGELOG.md, CLAUDE.md).

### B. git subtree binding

Rejected: adds a git remote dependency. The Meituan internal repo cannot pull from public GitHub in CI. Additionally, subtree merging would pull every upstream commit's history into this repo's tree, bloating it.

## Consequences

- **Script maintenance**: `sync-upstream.rs` must stay in sync with `check.rs`'s hash computation logic and `.skill-lock.json` schema.
- **Orphan skill cleanup**: When upstream removes or renames a skill, the sync script must detect orphan entries in `.skill-lock.json` and remove them (`to-issues`, `to-prd` are the first such case).
- **Verification**: After sync, `check.rs` should report all PASS. `install.rs setup --target <runtime>` must re-deploy symlinks correctly.
