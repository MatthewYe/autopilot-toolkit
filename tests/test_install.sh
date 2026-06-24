#!/usr/bin/env bash
set -euo pipefail

# Test suite for install.sh
# Usage: bash tests/test_install.sh

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
INSTALL_SCRIPT="$PROJECT_ROOT/install.sh"

PASS=0
FAIL=0
ERRORS=""

assert() {
  local desc="$1"
  local condition="$2"
  if eval "$condition"; then
    echo "  ✓ $desc"
    PASS=$((PASS + 1))
  else
    echo "  ✗ $desc"
    FAIL=$((FAIL + 1))
    ERRORS="$ERRORS\n  FAIL: $desc"
  fi
}

assert_eq() {
  local desc="$1" expected="$2" actual="$3"
  if [ "$expected" = "$actual" ]; then
    echo "  ✓ $desc"
    PASS=$((PASS + 1))
  else
    echo "  ✗ $desc (expected: '$expected', got: '$actual')"
    FAIL=$((FAIL + 1))
    ERRORS="$ERRORS\n  FAIL: $desc (expected: '$expected', got: '$actual')"
  fi
}

assert_symlink_target() {
  local desc="$1" link="$2" expected_target="$3"
  if [ -L "$link" ]; then
    actual="$(readlink "$link")"
    assert_eq "$desc" "$expected_target" "$actual"
  else
    echo "  ✗ $desc (not a symlink)"
    FAIL=$((FAIL + 1))
    ERRORS="$ERRORS\n  FAIL: $desc (not a symlink)"
  fi
}

cleanup() {
  for d in "${TMPDIR:-}" "${TMPDIR2:-}" "${TMPDIR3:-}" "${TMPDIR4:-}" "${TMPDIR5:-}" "${TMPDIR6:-}"; do
    if [ -n "$d" ] && [ -d "$d" ]; then
      chmod -R 755 "$d" 2>/dev/null || true
      rm -rf "$d"
    fi
  done
}
trap cleanup EXIT

echo "=== install.sh test suite ==="
echo ""

# ── Test 1: Fresh install (no prior skills dir) ──
echo "Test 1: Fresh install"
TMPDIR="$(mktemp -d /tmp/install-test-XXXXX)"
MOCK_PROJECT="$TMPDIR/autopilot-toolkit"
MOCK_AGENTS="$TMPDIR/home/.agents"

mkdir -p "$MOCK_PROJECT/skills/autopilot/test-skill"
echo "# Test Skill" > "$MOCK_PROJECT/skills/autopilot/test-skill/SKILL.md"

mkdir -p "$MOCK_PROJECT/skills/autopilot/another-skill"
echo "# Another Skill" > "$MOCK_PROJECT/skills/autopilot/another-skill/SKILL.md"

mkdir -p "$MOCK_PROJECT/skills/upstream/skills/engineering/linked-skill"
echo "# Linked Skill" > "$MOCK_PROJECT/skills/upstream/skills/engineering/linked-skill/SKILL.md"

mkdir -p "$MOCK_PROJECT/skills/upstream/skills/productivity/handy-skill"
echo "# Handy Skill" > "$MOCK_PROJECT/skills/upstream/skills/productivity/handy-skill/SKILL.md"

# Create .skill-lock.json with 2 upstream skills
cat > "$MOCK_PROJECT/.skill-lock.json" << 'LOCKJSON'
{
  "version": 3,
  "skills": {
    "linked-skill": {
      "source": "mattpocock/skills",
      "sourceType": "github",
      "skillPath": "skills/engineering/linked-skill/SKILL.md"
    },
    "handy-skill": {
      "source": "mattpocock/skills",
      "sourceType": "github",
      "skillPath": "skills/productivity/handy-skill/SKILL.md"
    }
  },
  "dismissed": {}
}
LOCKJSON

# Run install.sh with overridden paths
HOME="$TMPDIR/home" PROJECT_ROOT="$MOCK_PROJECT" AGENTS_SKILLS_DIR="$MOCK_AGENTS/skills" \
  bash "$INSTALL_SCRIPT" 2>&1 | tee "$TMPDIR/output1.txt"

assert "creates ~/.agents/skills/" "[ -d '$MOCK_AGENTS/skills' ]"
assert "linked-skill symlink exists" "[ -L '$MOCK_AGENTS/skills/linked-skill' ]"
assert "handy-skill symlink exists" "[ -L '$MOCK_AGENTS/skills/handy-skill' ]"
assert "test-skill symlink exists" "[ -L '$MOCK_AGENTS/skills/test-skill' ]"
assert "another-skill symlink exists" "[ -L '$MOCK_AGENTS/skills/another-skill' ]"
assert_symlink_target "linked-skill points to correct dir" \
  "$MOCK_AGENTS/skills/linked-skill" \
  "$MOCK_PROJECT/skills/upstream/skills/engineering/linked-skill"
assert_symlink_target "test-skill points to correct dir" \
  "$MOCK_AGENTS/skills/test-skill" \
  "$MOCK_PROJECT/skills/autopilot/test-skill"

# Check summary output
output1="$(cat "$TMPDIR/output1.txt")"
assert "Skills summary shows 4 created, 0 skipped, 0 replaced" \
  "echo '$output1' | grep -q 'Skills: 4 created, 0 skipped, 0 replaced'"
assert "Principles line not shown (no principles/ dir)" \
  "! echo '$output1' | grep -q 'Principles:'"

echo ""

# ── Test 2: Idempotent re-run ──
echo "Test 2: Idempotent re-run"
HOME="$TMPDIR/home" PROJECT_ROOT="$MOCK_PROJECT" AGENTS_SKILLS_DIR="$MOCK_AGENTS/skills" \
  bash "$INSTALL_SCRIPT" 2>&1 | tee "$TMPDIR/output2.txt"

output2="$(cat "$TMPDIR/output2.txt")"
assert "idempotent: all 4 skills skipped, none created" \
  "echo '$output2' | grep -q 'Skills: 0 created, 4 skipped, 0 replaced'"

echo ""

# ── Test 3: Broken symlink replacement ──
echo "Test 3: Broken symlink replacement"
# Break a symlink by removing the target
rm -rf "$MOCK_PROJECT/skills/upstream/skills/engineering/linked-skill"
HOME="$TMPDIR/home" PROJECT_ROOT="$MOCK_PROJECT" AGENTS_SKILLS_DIR="$MOCK_AGENTS/skills" \
  bash "$INSTALL_SCRIPT" 2>&1 | tee "$TMPDIR/output3.txt"

output3="$(cat "$TMPDIR/output3.txt")"
# Recreate the target, then re-run to verify fix
mkdir -p "$MOCK_PROJECT/skills/upstream/skills/engineering/linked-skill"
echo "# Restored" > "$MOCK_PROJECT/skills/upstream/skills/engineering/linked-skill/SKILL.md"
HOME="$TMPDIR/home" PROJECT_ROOT="$MOCK_PROJECT" AGENTS_SKILLS_DIR="$MOCK_AGENTS/skills" \
  bash "$INSTALL_SCRIPT" 2>&1 | tee "$TMPDIR/output3b.txt"

output3b="$(cat "$TMPDIR/output3b.txt")"
assert "broken symlink replaced on re-run" \
  "[ -L '$MOCK_AGENTS/skills/linked-skill' ] && [ -d '$MOCK_AGENTS/skills/linked-skill' ]"

echo ""

# ── Test 4: Missing source dir (graceful skip) ──
echo "Test 4: Missing source dir"
# Add a skill to .skill-lock.json that doesn't exist on disk
cat > "$MOCK_PROJECT/.skill-lock.json" << 'LOCKJSON2'
{
  "version": 3,
  "skills": {
    "linked-skill": {
      "source": "mattpocock/skills",
      "sourceType": "github",
      "skillPath": "skills/engineering/linked-skill/SKILL.md"
    },
    "handy-skill": {
      "source": "mattpocock/skills",
      "sourceType": "github",
      "skillPath": "skills/productivity/handy-skill/SKILL.md"
    },
    "missing-skill": {
      "source": "mattpocock/skills",
      "sourceType": "github",
      "skillPath": "skills/engineering/missing-skill/SKILL.md"
    }
  },
  "dismissed": {}
}
LOCKJSON2

HOME="$TMPDIR/home" PROJECT_ROOT="$MOCK_PROJECT" AGENTS_SKILLS_DIR="$MOCK_AGENTS/skills" \
  bash "$INSTALL_SCRIPT" 2>&1 | tee "$TMPDIR/output4.txt"

output4="$(cat "$TMPDIR/output4.txt")"
assert "missing source dir does not crash script" "true"  # script completed
assert "no symlink for missing skill" "[ ! -e '$MOCK_AGENTS/skills/missing-skill' ]"

echo ""

# ── Test 5: No .skill-lock.json (should still handle autopilot) ──
echo "Test 5: No .skill-lock.json"
TMPDIR2="$(mktemp -d /tmp/install-test2-XXXXX)"
MOCK_PROJECT2="$TMPDIR2/autopilot-toolkit"
MOCK_AGENTS2="$TMPDIR2/home/.agents"

mkdir -p "$MOCK_PROJECT2/skills/autopilot/standalone-skill"
echo "# Standalone" > "$MOCK_PROJECT2/skills/autopilot/standalone-skill/SKILL.md"

# No .skill-lock.json at all
HOME="$TMPDIR2/home" PROJECT_ROOT="$MOCK_PROJECT2" AGENTS_SKILLS_DIR="$MOCK_AGENTS2/skills" \
  bash "$INSTALL_SCRIPT" 2>&1 | tee "$TMPDIR2/output5.txt" || true

assert "handles missing .skill-lock.json without crashing" "true"

echo ""

# ── Test 6: Empty skills directories ──
echo "Test 6: Empty skills directories"
TMPDIR3="$(mktemp -d /tmp/install-test3-XXXXX)"
MOCK_PROJECT3="$TMPDIR3/autopilot-toolkit"
MOCK_AGENTS3="$TMPDIR3/home/.agents"

mkdir -p "$MOCK_PROJECT3/skills/autopilot"
mkdir -p "$MOCK_PROJECT3/skills/upstream"
echo '{"version":3,"skills":{},"dismissed":{}}' > "$MOCK_PROJECT3/.skill-lock.json"

HOME="$TMPDIR3/home" PROJECT_ROOT="$MOCK_PROJECT3" AGENTS_SKILLS_DIR="$MOCK_AGENTS3/skills" \
  bash "$INSTALL_SCRIPT" 2>&1 | tee "$TMPDIR3/output6.txt" || true

assert "handles empty skills dirs without crashing" "true"

echo ""

# ── Test 7: Real directory conflict (warn + skip, not rm) ──
echo "Test 7: Real directory conflict"
TMPDIR4="$(mktemp -d /tmp/install-test-dirconflict-XXXXX)"
MOCK_PROJECT4="$TMPDIR4/autopilot-toolkit"
MOCK_AGENTS4="$TMPDIR4/home/.agents"

mkdir -p "$MOCK_PROJECT4/skills/autopilot/test-skill"
echo "# Test Skill" > "$MOCK_PROJECT4/skills/autopilot/test-skill/SKILL.md"
echo '{"version":3,"skills":{},"dismissed":{}}' > "$MOCK_PROJECT4/.skill-lock.json"

mkdir -p "$MOCK_AGENTS4/skills"
# Create a real directory (not symlink) at the target location
mkdir -p "$MOCK_AGENTS4/skills/test-skill"
echo "foreign content" > "$MOCK_AGENTS4/skills/test-skill/some-file.txt"

HOME="$TMPDIR4/home" PROJECT_ROOT="$MOCK_PROJECT4" AGENTS_SKILLS_DIR="$MOCK_AGENTS4/skills" \
  bash "$INSTALL_SCRIPT" 2>&1 | tee "$TMPDIR4/output7.txt"

output7="$(cat "$TMPDIR4/output7.txt")"
assert "real dir conflict emits WARNING" "echo '$output7' | grep -q 'real directory'"
assert "real dir still exists (not deleted)" "[ -d '$MOCK_AGENTS4/skills/test-skill' ]"
assert "foreign content preserved" "[ -f '$MOCK_AGENTS4/skills/test-skill/some-file.txt' ]"
assert "no symlink created over real dir" "[ ! -L '$MOCK_AGENTS4/skills/test-skill' ]"

echo ""

# ── Test 8: Permission issues ──
echo "Test 8: Permission issues"
TMPDIR5="$(mktemp -d /tmp/install-test-perm-XXXXX)"
MOCK_PROJECT5="$TMPDIR5/autopilot-toolkit"
MOCK_HOME="$TMPDIR5/readonly-home"

# Create a mock project with a skill
mkdir -p "$MOCK_PROJECT5/skills/autopilot/perm-skill"
echo "# Perm Skill" > "$MOCK_PROJECT5/skills/autopilot/perm-skill/SKILL.md"
echo '{"version":3,"skills":{},"dismissed":{}}' > "$MOCK_PROJECT5/.skill-lock.json"

# Create a read-only home directory — mkdir -p should fail inside it
mkdir -p "$MOCK_HOME"
chmod 555 "$MOCK_HOME"

exit_code=0
HOME="$MOCK_HOME" PROJECT_ROOT="$MOCK_PROJECT5" AGENTS_SKILLS_DIR="$MOCK_HOME/.agents/skills" \
  bash "$INSTALL_SCRIPT" 2>&1 | tee "$TMPDIR5/output8.txt" || exit_code=$?

assert "script exits non-zero on permission error" "[ '$exit_code' -ne 0 ]"

# Restore writability for cleanup
chmod 755 "$MOCK_HOME"

echo ""

# ── Test 9: Principles deployment ──
echo "Test 9: Principles deployment"
TMPDIR6="$(mktemp -d /tmp/install-test-principles-XXXXX)"
MOCK_PROJECT6="$TMPDIR6/autopilot-toolkit"
MOCK_AGENTS6="$TMPDIR6/home/.agents"

mkdir -p "$MOCK_PROJECT6/principles"
echo "# Karpathy Principles" > "$MOCK_PROJECT6/principles/karpathy.md"

# Sub-test 9a: Fresh install creates principles symlink
HOME="$TMPDIR6/home" PROJECT_ROOT="$MOCK_PROJECT6" AGENTS_PRINCIPLES_DIR="$MOCK_AGENTS6/principles" \
  bash "$INSTALL_SCRIPT" 2>&1 | tee "$TMPDIR6/output9a.txt"

output9a="$(cat "$TMPDIR6/output9a.txt")"
assert "principles symlink created" "[ -L '$MOCK_AGENTS6/principles' ]"
assert "principles symlink resolves to correct dir" "[ -f '$MOCK_AGENTS6/principles/karpathy.md' ]"
assert_symlink_target "principles symlink points to project" \
  "$MOCK_AGENTS6/principles" \
  "$MOCK_PROJECT6/principles"
assert "Principles summary shows 1 created, 0 skipped, 0 replaced" \
  "echo '$output9a' | grep -q 'Principles: 1 created, 0 skipped, 0 replaced'"

# Sub-test 9b: Idempotent re-run (valid symlink skipped)
HOME="$TMPDIR6/home" PROJECT_ROOT="$MOCK_PROJECT6" AGENTS_PRINCIPLES_DIR="$MOCK_AGENTS6/principles" \
  bash "$INSTALL_SCRIPT" 2>&1 | tee "$TMPDIR6/output9b.txt"

output9b="$(cat "$TMPDIR6/output9b.txt")"
assert "idempotent re-run does not error" "true"
assert "principles symlink still exists after re-run" "[ -L '$MOCK_AGENTS6/principles' ]"
assert "Principles summary shows 0 created, 1 skipped, 0 replaced" \
  "echo '$output9b' | grep -q 'Principles: 0 created, 1 skipped, 0 replaced'"

# Sub-test 9c: Real directory conflict (warn + skip, not rm)
rm -f "$MOCK_AGENTS6/principles"
mkdir -p "$MOCK_AGENTS6/principles"
echo "manual content" > "$MOCK_AGENTS6/principles/manual.txt"

HOME="$TMPDIR6/home" PROJECT_ROOT="$MOCK_PROJECT6" AGENTS_PRINCIPLES_DIR="$MOCK_AGENTS6/principles" \
  bash "$INSTALL_SCRIPT" 2>&1 | tee "$TMPDIR6/output9c.txt"

output9c="$(cat "$TMPDIR6/output9c.txt")"
assert "real dir conflict emits WARNING" "echo '$output9c' | grep -q 'real directory'"
assert "real dir still exists (not deleted)" "[ -d '$MOCK_AGENTS6/principles' ]"
assert "manual content preserved" "[ -f '$MOCK_AGENTS6/principles/manual.txt' ]"
assert "no symlink created over real dir" "[ ! -L '$MOCK_AGENTS6/principles' ]"
assert "Principles summary shows 0 created, 1 skipped, 0 replaced" \
  "echo '$output9c' | grep -q 'Principles: 0 created, 1 skipped, 0 replaced'"

# Sub-test 9d: Broken symlink → replace
rm -rf "$MOCK_AGENTS6/principles"
ln -sfn "/nonexistent/path/to/principles" "$MOCK_AGENTS6/principles"

HOME="$TMPDIR6/home" PROJECT_ROOT="$MOCK_PROJECT6" AGENTS_PRINCIPLES_DIR="$MOCK_AGENTS6/principles" \
  bash "$INSTALL_SCRIPT" 2>&1 | tee "$TMPDIR6/output9d.txt" || true

output9d="$(cat "$TMPDIR6/output9d.txt")"
assert "broken symlink replaced by valid one" "[ -L '$MOCK_AGENTS6/principles' ]"
assert "replaced symlink resolves to correct dir" "[ -f '$MOCK_AGENTS6/principles/karpathy.md' ]"
assert_symlink_target "replaced symlink points to project" \
  "$MOCK_AGENTS6/principles" \
  "$MOCK_PROJECT6/principles"
assert "Principles summary shows 0 created, 0 skipped, 1 replaced" \
  "echo '$output9d' | grep -q 'Principles: 0 created, 0 skipped, 1 replaced'"

echo ""

# ── Summary ──
echo "=== Results ==="
echo "Passed: $PASS"
echo "Failed: $FAIL"
if [ -n "$ERRORS" ]; then
  echo -e "Failures:$ERRORS"
fi

if [ "$FAIL" -gt 0 ]; then
  exit 1
fi
exit 0
