#!/usr/bin/env bash
# validation/validate.test.sh — unit tests for validate.sh
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/validate.sh"

PASS=0
FAIL=0
ERRORS=""

assert_pass() {
  local desc="$1" content="$2"
  validate_skill "$content"
  if [ "$VALIDATE_PASSED" = "true" ]; then
    echo "  ✓ $desc"
    PASS=$((PASS + 1))
  else
    echo "  ✗ $desc"
    echo "     issues: ${VALIDATE_ISSUES[*]}"
    FAIL=$((FAIL + 1))
    ERRORS="$ERRORS"$'\n'"  FAIL: $desc"
  fi
}

assert_fail() {
  local desc="$1" content="$2" expected_substr="$3"
  validate_skill "$content"
  if [ "$VALIDATE_PASSED" = "false" ]; then
    local found=0
    for issue in "${VALIDATE_ISSUES[@]}"; do
      if [[ "$issue" == *"$expected_substr"* ]]; then
        found=1
        break
      fi
    done
    if [ $found -eq 1 ]; then
      echo "  ✓ $desc"
      PASS=$((PASS + 1))
    else
      echo "  ✗ $desc (fail but wrong reason: ${VALIDATE_ISSUES[*]})"
      FAIL=$((FAIL + 1))
      ERRORS="$ERRORS"$'\n'"  FAIL: $desc (wrong reason)"
    fi
  else
    echo "  ✗ $desc (expected fail, got pass)"
    FAIL=$((FAIL + 1))
    ERRORS="$ERRORS"$'\n'"  FAIL: $desc (expected fail)"
  fi
}

echo "validate.sh unit tests"
echo "======================"
echo ""

# ── Check 1: Required fields ──
assert_fail "fails when name is missing" \
"---
description: A test skill
---
# Test" \
"name"

assert_fail "fails when description is missing" \
"---
name: test-skill
---
# Test" \
"description"

assert_pass "passes with valid minimal frontmatter" \
"---
name: my-skill
description: Does something useful.
---
# My Skill"

# ── Check 2: Name format ──
assert_fail "fails when name starts with non-alphanumeric" \
"---
name: _bad-name
description: A test
---
# Test" \
"Name"

assert_fail "fails when name exceeds 64 characters" \
"---
name: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
description: A test
---
# Test" \
"Name"

assert_pass "accepts valid name with dots and hyphens" \
"---
name: my-skill.v2_test
description: A test
---
# Test"

# ── Check 3: No opencode fields ──
assert_fail "fails when disable-model-invocation is present" \
"---
name: test-skill
description: A test
disable-model-invocation: true
---
# Test" \
"disable-model-invocation"

assert_fail "fails when compatibility is present" \
"---
name: test-skill
description: A test
compatibility: \">=1.0\"
---
# Test" \
"compatibility"

assert_fail "fails when multiple opencode fields are present" \
"---
name: test-skill
description: A test
mode: chat
hidden: true
---
# Test" \
"mode"

# ── Check 4: runAs valid ──
assert_pass "accepts runAs: inline" \
"---
name: test-skill
description: A test
runAs: inline
---
# Test"

assert_pass "accepts runAs: subagent with allowed-tools" \
"---
name: test-skill
description: A test
runAs: subagent
allowed-tools: read, write
---
# Test"

assert_fail "fails when runAs has invalid value" \
"---
name: test-skill
description: A test
runAs: agent
---
# Test" \
"runAs"

# ── Check 5: allowed-tools for subagents ──
assert_fail "fails when runAs is subagent but allowed-tools missing" \
"---
name: test-skill
description: A test
runAs: subagent
---
# Test" \
"allowed-tools"

assert_pass "accepts subagent with TODO allowed-tools" \
"---
name: test-skill
description: A test
runAs: subagent
allowed-tools: TODO
---
# Test"

# ── Check 6: Frontmatter well-formed ──
assert_fail "fails when no opening --- delimiter" \
"name: test-skill
description: A test
---
# Test" \
"opening"

assert_fail "fails when no closing --- delimiter" \
"---
name: test-skill
description: A test
# Test" \
"closing"

# ── Complex cases ──
assert_fail "reports multiple issues at once" \
"---
name: _bad-name
compatibility: \">1.0\"
runAs: agent
---
# Test" \
"description"

echo ""
echo "=== Results ==="
echo "Passed: $PASS"
echo "Failed: $FAIL"
echo ""

if [ $FAIL -gt 0 ]; then
  echo "Errors:"
  echo "$ERRORS"
  exit 1
fi
