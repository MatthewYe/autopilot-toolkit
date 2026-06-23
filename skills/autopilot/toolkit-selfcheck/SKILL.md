---
name: toolkit-selfcheck
description: Validates autopilot-toolkit installation in ~/.agents/skills/ — checks symlink integrity, expected skill set via SSOT, frontmatter validity, and reports missing or conflicted skills. Use when you want to verify the toolkit is correctly installed and reasonix-ready.
---

# Toolkit Selfcheck

Validate that the autopilot-toolkit is properly installed and all skills are discoverable by reasonix. Runs inside the project repo; requires `PROJECT_ROOT` set to the repo root.

## Scope

This selfcheck verifies **only** autopilot-toolkit's own skills — derived from `.skill-lock.json` (upstream) and `skills/autopilot/*/SKILL.md` (autopilot). It does not inspect or judge other projects' skills that may coexist in `~/.agents/skills/`.

## Step 1: Derive the expected set

The expected set of toolkit skills comes from two sources — no hardcoded list:

### Upstream skills (from `.skill-lock.json`)

Read `$PROJECT_ROOT/.skill-lock.json`, parse the `skills` object keys. Each key is an upstream skill name. Also extract the `skillPath` for each — it gives the relative path (e.g. `skills/engineering/tdd/SKILL.md`). Construct the absolute expected source dir as:

```
$PROJECT_ROOT/skills/upstream/<skillPath directory>
```

(install.sh uses the same pattern: `$PROJECT_ROOT/skills/upstream/$skill_path` then `dirname`.)

### Autopilot skills (from filesystem)

Scan `$PROJECT_ROOT/skills/autopilot/*/SKILL.md`. Each parent directory name is an autopilot skill name, and the directory itself is its source dir.

### Combined

Union these into a single expected set: `{name → expected_source_dir}`. The expected count is derived dynamically — do not hardcode a number.

## Step 2: Check directory exists

```bash
ls -d ~/.agents/skills/
```

If `~/.agents/skills/` does not exist → **FAIL**. Stop here — no further checks.

## Step 3: Check for missing skills (first pass)

For each name in the expected set, check if `~/.agents/skills/<name>` exists:

```bash
[ -e "$HOME/.agents/skills/$name" ]
```

Collect all names that are missing. These go in the **Missing** section of the report.

## Step 4: Check integrity of present skills (second pass)

For each name that IS present, run these checks. Only iterate over toolkit names — do NOT loop over `~/.agents/skills/*/`.

### 4a. Must be a symlink

```bash
[ -L "$HOME/.agents/skills/$name" ]
```

If it's a real directory (not a symlink) → **FAIL**: `$name is a real directory, not a symlink`.

### 4b. Symlink target must exist

```bash
target=$(readlink "$HOME/.agents/skills/$name")
[ -d "$target" ]
```

If target missing → **FAIL**: `$name → $target (target missing)`.

### 4c. Symlink target must match expected source

The `readlink` result must equal the `expected_source_dir` from Step 1:

```bash
[ "$target" = "$expected_source_dir" ]
```

If mismatch → **FAIL**: `$name → $target (expected $expected_source_dir — same-name conflict or wrong target)`.

### 4d. Target must contain SKILL.md

```bash
[ -f "$target/SKILL.md" ]
```

If missing → **FAIL**: `$name → $target (SKILL.md missing)`.

## Step 5: Check frontmatter validity

For each present toolkit skill, read its `SKILL.md` and verify:

- Has `name:` in YAML frontmatter
- Has `description:` in YAML frontmatter

```bash
content=$(cat "$HOME/.agents/skills/$name/SKILL.md")
fm_name=$(echo "$content" | sed -n '/^---$/,/^---$/p' | grep '^name:' | head -1)
fm_desc=$(echo "$content" | sed -n '/^---$/,/^---$/p' | grep '^description:' | head -1)
[ -z "$fm_name" ] → FAIL: missing 'name'
[ -z "$fm_desc" ] → FAIL: missing 'description'
```

## Report Template

```
TOOLKIT_SELFCHECK_REPORT:

## Directory
  [PASS|FAIL] ~/.agents/skills/ exists

## Missing (N)
  [PASS] None missing
  — or —
  [FAIL] name1, name2, ...

## Damaged / Conflicted (N)
  [PASS] All present skills intact
  — or —
  [FAIL] name — reason (not a symlink / target missing / wrong target / SKILL.md missing / frontmatter issue)

## Summary
  Total expected: M present + intact, K missing, D damaged (M+K+D = total derived from sources)
  [ALL PASS] or [N checks FAILED]
```
