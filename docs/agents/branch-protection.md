# Branch Protection

## Main branch (`main`)

All changes must go through pull requests. Direct push is blocked — including for administrators.

| Rule | Value |
|---|---|
| Require a PR before merging | ✅ |
| Required approvals | 0 |
| Require status checks | `validation`, `test` |
| Require branches up to date | ❌ |
| Allow force pushes | ❌ |
| Allow deletions | ❌ |
| Do not allow bypassing (incl. admins) | ✅ |

## CI Workflow

`.github/workflows/ci.yml` — triggers on `pull_request`:

- **validation** — `bash validation/run.sh` (SKILL.md frontmatter checks)
- **test** — `bash tests/test_install.sh` (install script integration tests)

Both jobs run in parallel on `ubuntu-latest` with minimal permissions (`permissions: {}`).

## Bootstrapping notes

- First version of `ci.yml` was pushed directly to main before protection was enabled (one-time bootstrap).
- A temporary `push` trigger was added to register the `validation` and `test` status check names, then reverted.
- Protection was configured via `gh api`:
  ```
  gh api repos/MatthewYe/autopilot-toolkit/branches/main/protection \
    --method PUT --input /path/to/payload.json
  ```
