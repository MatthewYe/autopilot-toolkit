## Parent

Parent PRD: `docs/prd/0003-kimi-code-adaptation.md`

## What to build

Fix the invalid-YAML frontmatter that makes Kimi Code silently drop two runtime-coupled skills, and extend frontmatter validation so this bug class cannot recur.

End-to-end behavior:

1. The Reasonix variant sources of `autopilot-orchestrator` and `autopilot-reviewer` get their `description` values quoted, converting invalid YAML (unquoted `": "` inside a plain scalar) to valid YAML with an identical parsed value.
2. The frontmatter validator gains a strict-YAML parse check: any variant source whose frontmatter fails standard YAML parsing is rejected with a clear error.
3. The validation runner's discovery is generalized to cover all variant sources per runtime-coupled skill (existing `reasonix/`, `codex/`, and future `kimi/` directories), not a hardcoded list.

Verified through the validation seam: the two fixed files pass, a fixture with an unquoted-colon description is rejected, and the runner reports on every variant source present in the tree.

## Acceptance criteria

- [ ] Both fixed Reasonix variant sources parse with a strict YAML parser (e.g. `yaml.safe_load`) and their parsed `description` text is unchanged
- [ ] Validator rejects a fixture skill whose description contains an unquoted `": "` sequence, with an actionable error message
- [ ] Existing assert-style validation tests cover both the accept and reject cases
- [ ] Validation runner discovers and validates every variant source directory under each runtime-coupled skill, including directories that do not exist yet (e.g. a future `kimi/`) without code changes
- [ ] `validation/run.rs` passes on the whole tree
- [ ] Reasonix still loads all 4 autopilot skills (quoting fix is behavior-neutral)

## Blocked by

None - can start immediately
