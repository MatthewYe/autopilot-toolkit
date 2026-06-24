#!/usr/bin/env bash
set -euo pipefail

# install.sh — Link project skills and principles into ~/.agents/
#
# Discovery:
#   - Upstream: reads .skill-lock.json for installed skills
#   - Autopilot: scans skills/autopilot/*/SKILL.md
#   - Principles: links principles/ directory to ~/.agents/principles/
#
# Idempotent: valid symlinks are skipped; broken ones are replaced.
# Output: summary with created / skipped / replaced counts.

PROJECT_ROOT="${PROJECT_ROOT:-$(cd "$(dirname "$0")" && pwd)}"
SKILLS_DIR="${AGENTS_SKILLS_DIR:-$HOME/.agents/skills}"
PRINCIPLES_DIR="${AGENTS_PRINCIPLES_DIR:-$HOME/.agents/principles}"

skills_created=0
skills_skipped=0
skills_replaced=0
principles_created=0
principles_skipped=0
principles_replaced=0
principles_deployed=false

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
    skills_skipped=$((skills_skipped + 1))
    echo "skipped"
    return
  fi

  if [ -L "$target" ]; then
    local existing
    existing="$(resolve_link "$target")"
    if [ "$existing" = "$src" ] && [ -d "$target" ]; then
      # Valid symlink pointing to correct source — skip
      skills_skipped=$((skills_skipped + 1))
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
    skills_skipped=$((skills_skipped + 1))
    echo "skipped"
    return
  fi

  ln -sfn "$src" "$target" || {
    warn "failed to create symlink: $target -> $src"
    skills_skipped=$((skills_skipped + 1))
    echo "skipped"
    return
  }

  # Determine if this was a new creation or replacement
  if [ -n "${existing:-}" ]; then
    skills_replaced=$((skills_replaced + 1))
    echo "replaced"
  else
    skills_created=$((skills_created + 1))
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

# ── deploy principles/ ──

PRINCIPLES_SRC="$PROJECT_ROOT/principles"
if [ -d "$PRINCIPLES_SRC" ]; then
  principles_deployed=true
  # Create parent directory for the symlink target (e.g. ~/.agents/)
  if ! mkdir -p "$(dirname "$PRINCIPLES_DIR")"; then
    warn "Failed to create parent directory for $PRINCIPLES_DIR — skipping principles deployment"
  else
    if [ -e "$PRINCIPLES_DIR" ] && [ ! -L "$PRINCIPLES_DIR" ]; then
      # Real directory exists at target — warn and skip
      warn "$PRINCIPLES_DIR exists as a real directory (not a symlink) — skipping to avoid destructive overwrite"
      principles_skipped=$((principles_skipped + 1))
    elif [ -L "$PRINCIPLES_DIR" ]; then
      existing_principles="$(resolve_link "$PRINCIPLES_DIR")"
      if [ "$existing_principles" = "$PRINCIPLES_SRC" ] && [ -d "$PRINCIPLES_DIR" ]; then
        : # Valid symlink — nothing to do
        principles_skipped=$((principles_skipped + 1))
      else
        rm -f "$PRINCIPLES_DIR"
        ln -sfn "$PRINCIPLES_SRC" "$PRINCIPLES_DIR"
        principles_replaced=$((principles_replaced + 1))
      fi
    else
      ln -sfn "$PRINCIPLES_SRC" "$PRINCIPLES_DIR"
      principles_created=$((principles_created + 1))
    fi
  fi
fi

# ── summary ──

echo "Install complete:"
echo "  Skills: $skills_created created, $skills_skipped skipped, $skills_replaced replaced"
if $principles_deployed; then
  echo "  Principles: $principles_created created, $principles_skipped skipped, $principles_replaced replaced"
fi
