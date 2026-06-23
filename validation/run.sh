#!/usr/bin/env bash
# validation/run.sh — validate all toolkit SKILL.md frontmatter files
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

source "$SCRIPT_DIR/validate.sh"

# ── skill inventory ──
# Derived from SSOT: .skill-lock.json (upstream) + skills/autopilot/*/SKILL.md (autopilot)
# Parallel arrays for bash 3.2 compatibility
SKILL_NAMES=()
SKILL_PATHS_ARR=()
SKILL_SOURCES_ARR=()

add_skill() {
  SKILL_NAMES+=("$1")
  SKILL_PATHS_ARR+=("$2")
  SKILL_SOURCES_ARR+=("$3")
}

# Upstream: parse .skill-lock.json (same pattern as install.sh)
LOCKFILE="$PROJECT_ROOT/.skill-lock.json"
if [ -f "$LOCKFILE" ] && command -v python3 &>/dev/null; then
  while IFS=$'\t' read -r name skill_path; do
    [ -n "$name" ] || continue
    [ -n "$skill_path" ] || continue
    add_skill "$name" "skills/upstream/$skill_path" upstream
  done < <(python3 -c "
import json, sys
with open(sys.argv[1]) as f:
    data = json.load(f)
for name, info in data.get('skills', {}).items():
    sp = info.get('skillPath', '')
    if sp:
        print(f'{name}\t{sp}')
" "$LOCKFILE")
fi

# Autopilot: scan skills/autopilot/*/SKILL.md
AUTOPILOT_DIR="$PROJECT_ROOT/skills/autopilot"
if [ -d "$AUTOPILOT_DIR" ]; then
  for skill_md in "$AUTOPILOT_DIR"/*/SKILL.md; do
    [ -f "$skill_md" ] || continue
    name="$(basename "$(dirname "$skill_md")")"
    rel="skills/autopilot/$name/SKILL.md"
    add_skill "$name" "$rel" autopilot
  done
fi

TOTAL=${#SKILL_NAMES[@]}

# ── results storage ──
RESULT_PASSED_ARR=()
RESULT_ISSUES_ARR=()
PASS_COUNT=0
FAIL_COUNT=0
UPSTREAM_PASS=0; UPSTREAM_FAIL=0
AUTOPILOT_PASS=0; AUTOPILOT_FAIL=0

# ── validate all ──

for ((idx=0; idx<TOTAL; idx++)); do
  name="${SKILL_NAMES[$idx]}"
  path="${SKILL_PATHS_ARR[$idx]}"
  src="${SKILL_SOURCES_ARR[$idx]}"
  full_path="${PROJECT_ROOT}/${path}"

  if [ ! -f "$full_path" ]; then
    RESULT_PASSED_ARR+=("false")
    RESULT_ISSUES_ARR+=("File not found: $full_path")
    FAIL_COUNT=$((FAIL_COUNT + 1))
    continue
  fi

  content="$(cat "$full_path")"
  validate_skill "$content"

  if [ "$VALIDATE_PASSED" = "true" ]; then
    RESULT_PASSED_ARR+=("true")
    RESULT_ISSUES_ARR+=("")
    PASS_COUNT=$((PASS_COUNT + 1))
  else
    RESULT_PASSED_ARR+=("false")
    joined=""
    for issue in "${VALIDATE_ISSUES[@]}"; do
      joined="${joined}${issue}"$'\n'
    done
    joined="${joined%$'\n'}"
    RESULT_ISSUES_ARR+=("$joined")
    FAIL_COUNT=$((FAIL_COUNT + 1))
  fi

  if [ "$src" = "upstream" ]; then
    [ "${RESULT_PASSED_ARR[$idx]}" = "true" ] && UPSTREAM_PASS=$((UPSTREAM_PASS + 1)) || UPSTREAM_FAIL=$((UPSTREAM_FAIL + 1))
  else
    [ "${RESULT_PASSED_ARR[$idx]}" = "true" ] && AUTOPILOT_PASS=$((AUTOPILOT_PASS + 1)) || AUTOPILOT_FAIL=$((AUTOPILOT_FAIL + 1))
  fi
done

# ── report helpers ──

SEP="$(printf '=%.0s' {1..70})"
report=""
section() { report+="$1"$'\n'; }

section "$SEP"
section "FRONTMATTER VALIDATION REPORT — reasonix compatibility"
section "$SEP"
section "Date: $(date -u +%Y-%m-%dT%H:%M:%S.000Z)"
section "Total skills validated: $TOTAL | Passed: $PASS_COUNT | Failed: $FAIL_COUNT"
section ""

UPSTREAM_TOTAL=$((UPSTREAM_PASS + UPSTREAM_FAIL))
section "--- Upstream Skills ($UPSTREAM_TOTAL) ---"
section "Passed: $UPSTREAM_PASS / Failed: $UPSTREAM_FAIL"
section ""

for ((idx=0; idx<TOTAL; idx++)); do
  name="${SKILL_NAMES[$idx]}"
  src="${SKILL_SOURCES_ARR[$idx]}"
  path="${SKILL_PATHS_ARR[$idx]}"
  [ "$src" != "upstream" ] && continue

  if [ "${RESULT_PASSED_ARR[$idx]}" = "true" ]; then
    section "  [PASS] $name"
    section "       File: ${PROJECT_ROOT}/${path}"
    section "       ✓ All checks passed"
  else
    section "  [FAIL] $name"
    section "       File: ${PROJECT_ROOT}/${path}"
    echo "${RESULT_ISSUES_ARR[$idx]}" | while IFS= read -r issue; do
      [ -z "$issue" ] && continue
      report+="       Issue: $issue"$'\n'
    done
  fi
  section ""
done

AUTOPILOT_TOTAL=$((AUTOPILOT_PASS + AUTOPILOT_FAIL))
section "--- Autopilot Skills ($AUTOPILOT_TOTAL) ---"
section "Passed: $AUTOPILOT_PASS / Failed: $AUTOPILOT_FAIL"
section ""

for ((idx=0; idx<TOTAL; idx++)); do
  name="${SKILL_NAMES[$idx]}"
  src="${SKILL_SOURCES_ARR[$idx]}"
  path="${SKILL_PATHS_ARR[$idx]}"
  [ "$src" != "autopilot" ] && continue

  if [ "${RESULT_PASSED_ARR[$idx]}" = "true" ]; then
    section "  [PASS] $name"
    section "       File: ${PROJECT_ROOT}/${path}"
    # Show runAs / allowed-tools for autopilot skills
    content="$(cat "${PROJECT_ROOT}/${path}")"
    parse_frontmatter "$content" 2>/dev/null || true
    [ -n "${FM_RUNAS:-}" ] && section "       runAs: $FM_RUNAS"
    [ -n "${FM_ALLOWED_TOOLS:-}" ] && section "       allowed-tools: $FM_ALLOWED_TOOLS"
  else
    section "  [FAIL] $name"
    section "       File: ${PROJECT_ROOT}/${path}"
    echo "${RESULT_ISSUES_ARR[$idx]}" | while IFS= read -r issue; do
      [ -z "$issue" ] && continue
      report+="       Issue: $issue"$'\n'
    done
  fi
  section ""
done

# ── global checks ──

section "$SEP"
section "GLOBAL CHECKS"
section "$SEP"
section ""

# Check: 0 opencode-specific fields
oc_count=0
for ((idx=0; idx<TOTAL; idx++)); do
  issues="${RESULT_ISSUES_ARR[$idx]}"
  for field in "${OPENCODE_FIELDS[@]}"; do
    if echo "$issues" | grep -q "OpenCode-specific field present: $field" 2>/dev/null; then
      oc_count=$((oc_count + 1))
    fi
  done
done
section "Check: 0 opencode-specific fields across all $TOTAL skills"
[ $oc_count -eq 0 ] && section "Result: ✓ PASS" || section "Result: ✗ FAIL — $oc_count opencode field(s) found"
section ""

# Check: all subagent skills have allowed-tools
sub_missing=""
for ((idx=0; idx<TOTAL; idx++)); do
  name="${SKILL_NAMES[$idx]}"
  path="${PROJECT_ROOT}/${SKILL_PATHS_ARR[$idx]}"
  content="$(cat "$path" 2>/dev/null)" || continue
  parse_frontmatter "$content" 2>/dev/null || continue
  if [ "${FM_RUNAS:-}" = "subagent" ]; then
    if [ -z "${FM_ALLOWED_TOOLS:-}" ]; then
      sub_missing="$sub_missing $name"
    fi
  fi
done
section "Check: All subagent skills have allowed-tools defined"
if [ -z "$sub_missing" ]; then
  section "Result: ✓ PASS"
else
  section "Result: ✗ FAIL — missing: $sub_missing"
fi
section ""

# ── overall ──

section "$SEP"
section "OVERALL RESULT"
section "$SEP"
if [ $FAIL_COUNT -eq 0 ]; then
  section "All skills PASS validation."
else
  section "$FAIL_COUNT skill(s) FAIL validation. See individual entries above for issue details."
fi

echo "$report"

# Save report
echo "$report" > "$PROJECT_ROOT/validation/report.txt"
echo ""
echo "Report saved to: validation/report.txt"
