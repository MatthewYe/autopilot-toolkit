# Build pipeline: install.rs build

**Status**: ready-for-agent
**Parent**: [PRD 0004](../prd/0004-self-contained-tarball-install.md)

## What to build

Add a `build` subcommand to `install.rs` that assembles a self-contained tarball from the source tree and outputs it to `dist/`.

The build command must:
- Scan `skills/autopilot/*/` and `skills/upstream/` (via `.skill-lock.json`) to collect all toolkit skills
- Generate `manifest.json` describing every skill (type: agnostic/coupled/upstream, variants list, codex_agent flag)
- Read version from `git rev-parse HEAD`
- Generate `install.sh` from a template file, substituting `__VERSION__` with the git hash
- Pack everything into `dist/autopilot-toolkit-<hash>.tar.gz` with the structure defined in the PRD

## Acceptance criteria

- [ ] `install.rs build` produces a tarball at `dist/autopilot-toolkit-<hash>.tar.gz`
- [ ] Tarball contains `skills/` with all autopilot (agnostic + coupled with all variant subdirectories) and upstream skill directories
- [ ] Tarball contains `.autopilot/` with bootstrap.sh, manifest.json, .version, .skill-lock.json
- [ ] Tarball contains `principles/`
- [ ] `manifest.json` correctly classifies each skill (type, variants, codex_agent)
- [ ] `.version` matches `git rev-parse HEAD`
- [ ] `install.sh` embeds the correct version string and is executable
- [ ] `install.rs sync` still works (dev flow unchanged)
- [ ] Integration test: build → extract → verify structure and metadata

## Blocked by

None - can start immediately.

## Seam

Seam 1: tarball boundary. Test the output artifact, not the internal assembly logic.
