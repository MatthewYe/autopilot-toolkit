---
name: toolkit-setup
description: Orchestrates end-to-end autopilot-toolkit installation and update. Discovers expected skill set, diagnoses current state, executes minimal sync/unlink/link-principles operations, and verifies. Use when setting up the toolkit for the first time, after pulling updates, or when skill symlinks may be broken.
runAs: inline
---

# Toolkit Setup

Orchestrate the end-to-end install-or-update workflow for autopilot-toolkit. Runs inside the project repo; requires `PROJECT_ROOT` set to the repo root.

## Input Parameter

```
--target reasonix|codex|ksana   (required)
```

The `--target` parameter selects the target runtime. All skills are installed into the appropriate directories based on their category (see Step 1b). Principles are always installed to the shared `~/.agents/principles/` regardless of target.

Do not infer a default runtime. If the user did not supply `--target`, ask them to choose one before changing any links. This prevents a Codex user from silently receiving the Reasonix workflow variants.

## Canonical Command

`install.rs setup` is the authoritative, deterministic implementation of this workflow. Once a target is known, run:

```bash
rust-script "$PROJECT_ROOT/install.rs" setup --target "$TARGET"
```

It discovers the expected set, performs preflight conflict checks, applies the minimum safe repairs, removes toolkit-owned orphaned symlinks, links principles, and verifies the final state. Relay its structured report to the user.

Use the detailed steps below only to explain the command's behavior or if the command itself cannot run. Do not reimplement the synchronization loop in ad-hoc shell commands when `install.rs setup` is available.

## Scope

This setup manages **only** autopilot-toolkit's own skills — derived from `.skill-lock.json` (upstream) and `skills/autopilot/*/` (autopilot). It does not inspect or modify other projects' skills that may coexist in the install directories.

## Step 1: Discover Expected Set

Derive the authoritative expected set from two sources — no hardcoded list.

### Upstream skills (from `.skill-lock.json`)

Read `$PROJECT_ROOT/.skill-lock.json`, parse the `skills` object keys. Each key is an upstream skill name. Extract the `skillPath` for each — it gives the relative path (e.g. `skills/engineering/tdd/SKILL.md`). Construct the expected source dir as:

```
$PROJECT_ROOT/skills/upstream/<skillPath directory>
```

(install.rs uses the same pattern: `$PROJECT_ROOT/skills/upstream/$skill_path` then `dirname`.)

Use python3 for JSON parsing if available:

```bash
python3 -c "
import json, os
with open('$PROJECT_ROOT/.skill-lock.json') as f:
    data = json.load(f)
for name, info in data.get('skills', {}).items():
    skill_path = info['skillPath']
    src_dir = os.path.join('$PROJECT_ROOT', 'skills/upstream', os.path.dirname(skill_path))
    print(f'{name}|{src_dir}')
"
```

If python3 is not available, fall back to grep-based extraction (less reliable but functional).

### Autopilot skills (from filesystem)

Scan `$PROJECT_ROOT/skills/autopilot/*/`. A directory is a skill if it contains either:
- `SKILL.md` directly (agnostic skill, e.g. `toolkit-setup`, `zoom-out`)
- `reasonix/SKILL.md` (runtime-coupled skill with per-runtime variants)

```bash
for skill_dir in "$PROJECT_ROOT"/skills/autopilot/*/; do
  if [ -f "$skill_dir/SKILL.md" ] || [ -f "$skill_dir/reasonix/SKILL.md" ]; then
    name="$(basename "$skill_dir")"
    abs_dir="$(cd "$skill_dir" && pwd)"
    echo "$name|$abs_dir"
  fi
done
```

### Combined

Union these into a single expected set: `{name → expected_source_dir}`. The expected count is derived dynamically — do not hardcode a number.

## Step 1b: Categorize Skills

For each skill in the expected set, determine its category:

```bash
categorize_skill() {
  local src="$1"
  if [ -f "$src/reasonix/SKILL.md" ]; then
    echo "coupled"   # Has per-runtime variants (reasonix/codex/ksana subdirectories)
  else
    echo "agnostic"   # No runtime variants — works on any agent
  fi
}
```

Categories:

| Category | Detection | Skills (conceptual) |
|----------|-----------|---------------------|
| **agnostic** | `SKILL.md` directly in source dir | All upstream skills + `toolkit-setup` + `zoom-out` |
| **coupled** | `reasonix/SKILL.md` exists | The 4 workflow skills: `audit-autopilot`, `autopilot-implementer`, `autopilot-orchestrator`, `autopilot-reviewer`. Each maintains three variant sources: `reasonix/`, `codex/`, `ksana/`. |

Runtime-agnostic skills go to the shared directory (`~/.agents/skills/`). Runtime-coupled skills go to the agent-exclusive directory for the target runtime (`~/.reasonix/skills/`, `~/.codex/skills/`, or `~/.ksana/skills/`) only when that target has a loadable `SKILL.md` variant.

For `--target codex` or `--target ksana`, `autopilot-implementer` and `autopilot-reviewer` are custom agents only. Their `codex/` (or `ksana/`) directories contain `agent.toml` without `SKILL.md`, so do not sync them into the target skills dir; sync their TOML files via `install.rs sync --target <target> --agent` instead.

### Also: Codex/Ksana custom agents

For `--target codex` or `--target ksana`, coupled skills may have `.toml` agent definition files in their `<target>/` subdirectory. Check generically against the selected target:

```bash
has_target_agents() {
  local src="$1" target="$2"
  [ -d "$src/$target" ] && ls "$src/$target"/*.toml >/dev/null 2>&1
}
```

If `.toml` files exist, they will be synced to `~/.<target>/agents/<name>.toml` (`~/.codex/agents/` or `~/.ksana/agents/`) via `install.rs sync --target <target> --agent`.

## Step 2: Diagnose

### 2a. Determine target directories

```bash
TARGET="<user-selected runtime>"   # --target reasonix|codex|ksana; no implicit default

# Shared skills directory (always ~/.agents/skills/)
SHARED_DIR="${AGENTS_SKILLS_DIR:-$HOME/.agents/skills}"

# Agent-exclusive skills directory
if [ "$TARGET" = "codex" ]; then
  TARGET_DIR="${CODEX_SKILLS_DIR:-$HOME/.codex/skills}"
  CODEX_AGENTS_DIR="${CODEX_AGENTS_DIR:-$HOME/.codex/agents}"
elif [ "$TARGET" = "ksana" ]; then
  TARGET_DIR="${KSANA_SKILLS_DIR:-$HOME/.ksana/skills}"
  KSANA_AGENTS_DIR="${KSANA_AGENTS_DIR:-$HOME/.ksana/agents}"
else
  TARGET_DIR="${REASONIX_SKILLS_DIR:-$HOME/.reasonix/skills}"
fi

# Principles directory (always shared)
PRINCIPLES_DIR="${AGENTS_PRINCIPLES_DIR:-$HOME/.agents/principles}"
```

### 2b. Check skills directories exist

```bash
ls -d "$SHARED_DIR" 2>/dev/null || true
ls -d "$TARGET_DIR" 2>/dev/null || true
```

If a directory does not exist, it will be created by `install.rs sync` on first use. Proceed — do not stop.

### 2c. Diagnose each expected skill

For each name in the expected set, determine the correct install directory and expected source path based on category:

- **Agnostic**: install to `$SHARED_DIR/<name>`, expected source = `<skill_source_dir>`
- **Coupled with target `SKILL.md`**: install to `$TARGET_DIR/<name>`, expected source = `<skill_source_dir>/<target>` (i.e. `reasonix/`, `codex/`, or `ksana/` variant)
- **Coupled without target `SKILL.md`**: skip skill-state diagnosis for `$TARGET_DIR/<name>`; this is valid for Codex/Ksana custom-agent-only variants

```bash
check_skill_state() {
  local name="$1" expected_src="$2" skills_dir="$3"
  local target="$skills_dir/$name"

  if [ ! -e "$target" ] && [ ! -L "$target" ]; then
    echo "missing"
    return
  fi

  if [ -e "$target" ] && [ ! -L "$target" ]; then
    echo "real_dir"
    return
  fi

  # It's a symlink
  local link_target
  link_target="$(readlink "$target" 2>/dev/null || true)"

  if [ "$link_target" != "$expected_src" ]; then
    echo "wrong_target"
    return
  fi

  if [ ! -d "$target" ]; then
    echo "broken"
    return
  fi

  echo "correct"
}
```

States:
- **correct** — symlink exists, target matches expected, target is a valid directory
- **missing** — no entry at the name
- **wrong_target** — symlink points to a different path than expected
- **broken** — symlink exists but its target is not a valid directory (dangling)
- **real_dir** — a real (non-symlink) directory occupies the name

### 2d. Diagnose custom agents (codex/ksana target only)

For `--target codex` or `--target ksana`, also check if `autopilot-implementer` and `autopilot-reviewer` have `.toml` agent files in their source's `<target>/` directory. If present, treat them like symlinked skills: check whether `$AGENTS_DIR/<name>.toml` is a symlink pointing to the expected source `<skill_source_dir>/<target>/agent.toml`. Use the same five states as skill diagnosis (correct / missing / wrong_target / broken / real_file). `$AGENTS_DIR` is `$CODEX_AGENTS_DIR` (`~/.codex/agents`) for codex, or `$KSANA_AGENTS_DIR` (`~/.ksana/agents`) for ksana.

### 2e. Find orphaned symlinks

Orphaned symlinks: entries in either `$SHARED_DIR` or `$TARGET_DIR` that are symlinks pointing under `PROJECT_ROOT` but whose names are NOT expected for that install directory. These are leftovers from removed skills or from older routing rules.

Use directory-specific expected names:

- `$SHARED_DIR`: agnostic skill names only
- `$TARGET_DIR`: coupled skill names whose `<skill_source_dir>/<target>/SKILL.md` exists only

Check BOTH directories:

```bash
find_orphans() {
  local dir="$1"
  [ -d "$dir" ] || return
  for entry in "$dir"/*/; do
    [ -d "$entry" ] || continue
    name="$(basename "$entry")"
    [ -L "$dir/$name" ] || continue

    # Check if name is in expected set
    in_expected=false
    for ename in $EXPECTED_NAMES; do
      [ "$ename" = "$name" ] && in_expected=true && break
    done
    if [ "$in_expected" = true ]; then continue; fi

    link_target="$(readlink "$dir/$name" 2>/dev/null || true)"
    case "$link_target" in
      "$PROJECT_ROOT"|"$PROJECT_ROOT/"*)
        echo "$name|$dir"  # orphaned: name and which directory
        ;;
    esac
  done
}

find_orphans "$SHARED_DIR"
find_orphans "$TARGET_DIR"
```

## Step 3: Execute

Use `install.rs` subcommands with the appropriate flags. See `install.rs --help` for full reference.

### Actions by state (per category)

For **agnostic** skills — use `--shared` flag to install to the shared directory:

| State | Action | Command |
|-------|--------|---------|
| missing | Create symlink | `install.rs sync <name> <src> --shared` |
| broken | Remove broken + recreate | `install.rs sync <name> <src> --shared` |
| wrong_target | Replace with correct target | `install.rs sync <name> <src> --shared` |
| real_dir | **WARN** — do NOT touch | Report conflict, skip |
| correct | No-op | — |

For **coupled** skills — first check whether the target variant is a loadable skill:

```bash
variant_src="$src/$target"
if [ ! -f "$variant_src/SKILL.md" ]; then
  # Agent-only variants, such as autopilot-implementer/codex/agent.toml or
  # autopilot-implementer/ksana/agent.toml, are not skills. Skip sync; still
  # deploy agent TOMLs below.
  skip_skill_sync=true
fi
```

When `SKILL.md` exists, use `--target` flag with the variant source path:

| State | Action | Command |
|-------|--------|---------|
| missing | Create symlink | `install.rs sync <name> <src>/<target> --target <target>` |
| broken | Remove broken + recreate | `install.rs sync <name> <src>/<target> --target <target>` |
| wrong_target | Replace with correct target | `install.rs sync <name> <src>/<target> --target <target>` |
| real_dir | **WARN** — do NOT touch | Report conflict, skip |
| correct | No-op | — |

Where `<target>` is `reasonix`, `codex`, or `ksana`, and `<src>/<target>` is the variant source directory (e.g. `skills/autopilot/audit-autopilot/codex` or `.../ksana`). Do not run `install.rs sync` for a variant directory that lacks `SKILL.md`.

### Codex/Ksana custom agents

For `--target codex` or `--target ksana`, if a coupled skill has `.toml` agent files in its `<target>/` subdirectory, sync each as a file symlink (not a copy — `install.rs sync --agent` uses symlinks like skills do):

```bash
install.rs sync <agent_name> <skill_source_dir>/<target>/agent.toml --target <target> --agent
```

This creates/repairs a file symlink `~/.<target>/agents/<agent_name>.toml` → `<skill_source_dir>/<target>/agent.toml` (i.e. `~/.codex/agents/` or `~/.ksana/agents/`). Behaviour mirrors skill sync: creates if missing, replaces if broken/wrong, warns on real-file conflict. Symlinks ensure source updates take effect immediately without re-running setup.

Agent names are derived from the parent skill directory name (e.g. `skills/autopilot/autopilot-implementer/ksana/agent.toml` -> agent name `autopilot-implementer`).

### Orphaned symlinks

For each orphaned symlink found in Step 2e, remove it from the appropriate directory:

```bash
# Agnostic orphan (in shared dir):
install.rs unlink <name> --shared

# Coupled orphan (in target dir):
install.rs unlink <name> --target <target>
```

This removes the symlink only if its target lies under PROJECT_ROOT (safe).

### Link principles

Ensure principles symlink (unchanged — always goes to shared location):

```bash
install.rs link-principles "$PROJECT_ROOT/principles"
```

This creates/repairs `~/.agents/principles` → `$PROJECT_ROOT/principles`. Behaviour mirrors sync: creates if missing, replaces if broken/wrong, warns on real-dir conflict.

### Execution order

1. Process all expected agnostic skills (sync to `$SHARED_DIR`)
2. Process all expected coupled skills (sync to `$TARGET_DIR`)
3. If codex or ksana target: sync agent `.toml` files for implementer/reviewer via `sync --target <target> --agent`
4. Clean up orphaned symlinks from BOTH directories (unlink)
5. Ensure principles symlink (link-principles)

Track each action taken — the report must list specific skill names, operations, and flags used.

## Step 4: Verify

Re-run Step 2c diagnosis on all expected skills. Every skill should now be `correct`.

For codex or ksana target, also verify that `$AGENTS_DIR/<name>.toml` is a symlink pointing to the expected source file for implementer and reviewer (if they had `.toml` sources). Use `readlink` to verify the symlink target matches. `$AGENTS_DIR` is `$CODEX_AGENTS_DIR` (`~/.codex/agents`) for codex, or `$KSANA_AGENTS_DIR` (`~/.ksana/agents`) for ksana.

Verify principles symlink:

```bash
[ -L "$PRINCIPLES_DIR" ] && [ "$(readlink "$PRINCIPLES_DIR")" = "$PROJECT_ROOT/principles" ] && [ -d "$PRINCIPLES_DIR" ]
```

### Directory layout verification

After a successful setup, the expected directory layout per target:

**`--target reasonix`:**
```
~/.agents/skills/           # Agnostic skills (shared)
├── diagnosing-bugs → .../skills/upstream/skills/engineering/diagnosing-bugs
├── tdd → .../skills/upstream/skills/engineering/tdd
├── ... (all upstream skills)
├── toolkit-setup → .../skills/autopilot/toolkit-setup
├── zoom-out → .../skills/autopilot/zoom-out

~/.reasonix/skills/         # Coupled skills (reasonix variants)
├── audit-autopilot → .../skills/autopilot/audit-autopilot/reasonix
├── autopilot-implementer → .../skills/autopilot/autopilot-implementer/reasonix
├── autopilot-orchestrator → .../skills/autopilot/autopilot-orchestrator/reasonix
├── autopilot-reviewer → .../skills/autopilot/autopilot-reviewer/reasonix

~/.agents/principles → $PROJECT_ROOT/principles
```

**`--target codex`:**
```
~/.agents/skills/           # Agnostic skills (shared)
├── ... (same as reasonix)

~/.codex/skills/            # Coupled skills (codex variants)
├── audit-autopilot → .../skills/autopilot/audit-autopilot/codex
├── autopilot-orchestrator → .../skills/autopilot/autopilot-orchestrator/codex

~/.codex/agents/            # Codex custom agents (symlinked, not copied)
├── autopilot-implementer.toml → .../autopilot-implementer/codex/agent.toml
├── autopilot-reviewer.toml → .../autopilot-reviewer/codex/agent.toml

~/.agents/principles → $PROJECT_ROOT/principles
```

**`--target ksana`:**
```
~/.agents/skills/           # Agnostic skills (shared)
├── ... (same as reasonix)

~/.ksana/skills/            # Coupled skills (ksana variants)
├── audit-autopilot → .../skills/autopilot/audit-autopilot/ksana
├── autopilot-orchestrator → .../skills/autopilot/autopilot-orchestrator/ksana

~/.ksana/agents/            # Ksana custom agents (symlinked, not copied)
├── autopilot-implementer.toml → .../autopilot-implementer/ksana/agent.toml
├── autopilot-reviewer.toml → .../autopilot-reviewer/ksana/agent.toml

~/.agents/principles → $PROJECT_ROOT/principles
```

Note: `~/.reasonix/skills/`, `~/.codex/skills/`, and `~/.ksana/skills/` are **agent-exclusive** — only the target runtime scans them. This prevents one runtime's variant of a coupled skill from being discovered by another.

## Report Template

Output a structured report — list specific skill names, operations, and flags used.

```
TOOLKIT_SETUP_REPORT:
Target: reasonix | codex | ksana

## Expected Set
  N skills (K upstream + A autopilot)
  Agnostic: N  Coupled: M

## Actions Taken
  SYNC <name> → <src> (shared)
  SYNC <name> → <variant_src> (--target reasonix|codex|ksana)
  SYNC <name> → <src> (--target codex|ksana --agent)
  UNLINK <name> (orphaned, shared)
  UNLINK <name> (orphaned, --target codex|ksana)
  LINK-PRINCIPLES → <src>
  — or —
  (none — all skills already correct)

## Warnings
  WARN: <name> is a real directory at <path> — skipping
  — or —
  (none)

## Verification
  [PASS] <name>
  [PASS] <name>
  ...
  [PASS] principles
  Total: N expected, 0 missing, 0 damaged
  ALL PASS
```

If any skill remains missing/broken/wrong_target after execute, report as FAIL and do NOT output ALL PASS.

## Edge Cases

- **Skills directory missing**: Created automatically by `install.rs sync` on first use.
- **Variant source directory missing**: Report as WARN and skip sync. Do not call `install.rs sync` for a missing target variant.
- **Variant has no SKILL.md (codex/ksana)**: The `codex/` or `ksana/` subdirectory may contain only `.toml` agent files (implementer, reviewer). Skip skill sync explicitly; still sync agents via `sync --target <target> --agent`. If a stale toolkit-owned symlink for that name exists in the target skills dir, unlink it as an invalid target skill entry.
- **Real file conflict (agents)**: A real (non-symlink) file at `~/.codex/agents/<name>.toml` or `~/.ksana/agents/<name>.toml`. Reported as WARN. install.rs refuses to overwrite real files. User must remove manually before sync can replace it with a symlink.
- **Real directory conflict**: Reported as WARN. install.rs refuses to overwrite real directories. User must resolve manually.
- **No changes needed**: Report "all skills already correct", ALL PASS.
- **python3 unavailable**: Fall back to grep-based parsing of `.skill-lock.json`. Less robust but functional for standard JSON layouts.
- **Empty .skill-lock.json skills**: Only autopilot skills in expected set. Valid scenario for minimal installs.
- **No `--target` argument**: Ask the user to select `reasonix`, `codex`, or `ksana`; do not modify the installation until they choose.
- **Both targets on same machine**: Running `--target reasonix` then `--target codex` on the same machine is supported. Agnostic skills are shared (installed once); coupled skills go to separate agent-exclusive directories.
