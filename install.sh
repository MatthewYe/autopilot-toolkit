#!/usr/bin/env bash
set -euo pipefail

# install.sh — Link project skills into ~/.agents/skills/
#
# Discovery:
#   - Upstream: reads .skill-lock.json for installed skills
#   - Autopilot: scans skills/autopilot/*/SKILL.md
#
# Idempotent: valid symlinks are skipped; broken ones are replaced.
# Output: summary with created / skipped / replaced counts.

PROJECT_ROOT="${PROJECT_ROOT:-$(cd "$(dirname "$0")" && pwd)}"
SKILLS_DIR="${AGENTS_SKILLS_DIR:-$HOME/.agents/skills}"

created=0
skipped=0
replaced=0

# ── helpers ──

warn()  { echo "WARNING: $*" >&2; }
info()  { echo "INFO: $*" >&2; }

# Resolve a symlink to its absolute target (empty string if broken or not a link)
resolve_link() {
  local link="$1"
  if [ -L "$link" ]; then
    readlink "$link" 2>/dev/null || true
  fi
}

# Check if a symlink is valid (points to an existing directory)
is_valid_symlink() {
  local link="$1"
  [ -L "$link" ] && [ -d "$link" ]
}

# Create or update a symlink. Returns: created/replaced/skipped via stdout.
install_link() {
  local src="$1"    # absolute path to the skill source directory
  local name="$2"   # skill name (basename)
  local target="$SKILLS_DIR/$name"

  # If a real directory (not a symlink) exists at target, warn and skip
  if [ -e "$target" ] && [ ! -L "$target" ]; then
    warn "$target exists as a real directory (not a symlink) — skipping to avoid destructive overwrite"
    skipped=$((skipped + 1))
    echo "skipped"
    return
  fi

  if [ -L "$target" ]; then
    local existing
    existing="$(resolve_link "$target")"
    if [ "$existing" = "$src" ] && [ -d "$target" ]; then
      # Valid symlink pointing to correct source — skip
      skipped=$((skipped + 1))
      echo "skipped"
      return
    else
      # Broken or wrong target — replace
      rm -f "$target"
    fi
  fi

  # Check if source directory exists
  if [ ! -d "$src" ]; then
    warn "source directory does not exist: $src (skipping $name)"
    skipped=$((skipped + 1))
    echo "skipped"
    return
  fi

  ln -sfn "$src" "$target" || {
    warn "failed to create symlink: $target -> $src"
    skipped=$((skipped + 1))
    echo "skipped"
    return
  }

  # Determine if this was a new creation or replacement
  if [ -n "${existing:-}" ]; then
    replaced=$((replaced + 1))
    echo "replaced"
  else
    created=$((created + 1))
    echo "created"
  fi
}

# ── ensure target directory exists ──

mkdir -p "$SKILLS_DIR" || {
  warn "cannot create $SKILLS_DIR — check permissions"
  exit 1
}

# ── discover autopilot skills ──

AUTOPILOT_DIR="$PROJECT_ROOT/skills/autopilot"
if [ -d "$AUTOPILOT_DIR" ]; then
  for skill_md in "$AUTOPILOT_DIR"/*/SKILL.md; do
    [ -f "$skill_md" ] || continue
    src="$(cd "$(dirname "$skill_md")" && pwd)"
    name="$(basename "$src")"
    install_link "$src" "$name" > /dev/null
  done
fi

# ── discover upstream skills (via .skill-lock.json) ──

LOCKFILE="$PROJECT_ROOT/.skill-lock.json"
if [ -f "$LOCKFILE" ]; then
  # Parse .skill-lock.json: output "name\tskillPath" per line
  parse_lockfile() {
    if command -v python3 &>/dev/null; then
      python3 -c "
import json, sys
try:
    with open(sys.argv[1]) as f:
        data = json.load(f)
    for name, info in data.get('skills', {}).items():
        sp = info.get('skillPath', '')
        if sp:
            print(f'{name}\t{sp}')
except Exception as e:
    print(f'error: {e}', file=sys.stderr)
    sys.exit(0)
" "$LOCKFILE"
    elif command -v jq &>/dev/null; then
      jq -r '.skills // {} | to_entries[] | "\(.key)\t\(.value.skillPath // empty)"' "$LOCKFILE" 2>/dev/null || true
    else
      warn "neither python3 nor jq found — cannot parse .skill-lock.json"
    fi
  }

  while IFS=$'\t' read -r name skill_path; do
    [ -n "$name" ] || continue
    [ -n "$skill_path" ] || continue
    src="$PROJECT_ROOT/skills/upstream/$skill_path"
    src="$(dirname "$src")"   # remove /SKILL.md, keep the skill directory
    install_link "$src" "$name" > /dev/null
  done < <(parse_lockfile)
fi

# ── summary ──

echo "Install complete: $created created, $skipped skipped, $replaced replaced"
