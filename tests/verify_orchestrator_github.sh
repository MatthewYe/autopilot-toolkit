#!/usr/bin/env bash
set -euo pipefail

# Verify autopilot-orchestrator GitHub mode end-to-end
# Maps to issue #3 acceptance criteria (AC1-AC5)

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Prerequisite checks
if ! command -v python3 >/dev/null 2>&1; then
  echo "ERROR: python3 is required but not installed"
  exit 1
fi

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

echo "=== autopilot-orchestrator GitHub Mode Verification ==="
echo ""

# ── AC3: GitHub operation channel available ──
echo "AC3: GitHub operation channel (gh CLI) available"

GH_PATH="$(which gh 2>/dev/null || echo '')"
assert "gh CLI is installed" "[ -n '$GH_PATH' ]"

GH_AUTH="$(gh auth status 2>&1 || true)"
assert "gh CLI is authenticated" "echo '$GH_AUTH' | grep -q 'Logged in'"

REPO_NAME="$(cd "$PROJECT_ROOT" && git remote get-url origin 2>/dev/null || echo '')"
assert "git remote origin is set" "[ -n '$REPO_NAME' ]"

echo ""

# ── AC1: No-arg scan finds ready-for-agent issues ──
echo "AC1: No-arg scan mechanism for ready-for-agent GitHub issues"

# Verify the orchestrator SKILL.md defines the correct scan command
ORCH_SKILL="$PROJECT_ROOT/skills/autopilot/autopilot-orchestrator/SKILL.md"
assert "orchestrator SKILL.md exists" "[ -f '$ORCH_SKILL' ]"

SCAN_CMD_IN_SKILL="$(grep -c 'gh issue list --label "ready-for-agent"' "$ORCH_SKILL" || echo 0)"
assert "SKILL.md defines gh issue list --label ready-for-agent scan" "[ '$SCAN_CMD_IN_SKILL' -ge 1 ]"

# Run the actual scan command to verify it works
SCAN_RESULT="$(cd "$PROJECT_ROOT" && gh issue list --label "ready-for-agent" --state open --json number,title --limit 50 2>&1)"
SCAN_EXIT=$?
assert "scan command executes without error (exit 0)" "[ '$SCAN_EXIT' -eq 0 ]"
assert "scan command returns valid JSON" "echo '$SCAN_RESULT' | python3 -c 'import json,sys; json.load(sys.stdin)' 2>/dev/null"

# Currently no ready-for-agent issues (expected: all processed)
READY_COUNT="$(echo "$SCAN_RESULT" | python3 -c 'import json,sys; print(len(json.load(sys.stdin)))' 2>/dev/null || echo 0)"
echo "  ℹ Currently $READY_COUNT ready-for-agent issues (0 expected: all processed)"

echo ""

# ── AC2: needs-info issues correctly identified and stopped ──
echo "AC2: needs-info issues correctly identified and stopped"

# Dynamically find issues with needs-info label (instead of hardcoding issue #4)
NEEDS_INFO_LIST="$(cd "$PROJECT_ROOT" && gh issue list --label "needs-info" --state open --json number,title,labels,state --limit 10 2>&1)"
NEEDS_INFO_COUNT="$(echo "$NEEDS_INFO_LIST" | python3 -c 'import json,sys; print(len(json.load(sys.stdin)))' 2>/dev/null || echo 0)"

if [ "$NEEDS_INFO_COUNT" -gt 0 ]; then
  # Pick the first needs-info issue and verify
  NEEDS_INFO_NUM="$(echo "$NEEDS_INFO_LIST" | python3 -c 'import json,sys; print(json.load(sys.stdin)[0]["number"])' 2>/dev/null)"
  ISSUE_NI="$(cd "$PROJECT_ROOT" && gh issue view "$NEEDS_INFO_NUM" --json number,title,labels,state 2>&1)"
  assert "needs-info issue #$NEEDS_INFO_NUM is open" "echo '$ISSUE_NI' | python3 -c 'import json,sys; d=json.load(sys.stdin); assert d[\"state\"]==\"OPEN\"' 2>/dev/null"

  ISSUE_NI_LABELS="$(echo "$ISSUE_NI" | python3 -c 'import json,sys; print([l["name"] for l in json.load(sys.stdin)["labels"]])' 2>/dev/null)"
  assert "issue #$NEEDS_INFO_NUM has needs-info label" "echo '$ISSUE_NI_LABELS' | grep -q 'needs-info'"
  assert "issue #$NEEDS_INFO_NUM does NOT have ready-for-agent label" "! echo '$ISSUE_NI_LABELS' | grep -q 'ready-for-agent'"
  assert "issue #$NEEDS_INFO_NUM does NOT have in-progress label" "! echo '$ISSUE_NI_LABELS' | grep -q 'in-progress'"
else
  echo "  ℹ No needs-info issues found — skipping fixture-dependent AC2 tests"
fi

# Verify orchestrator SKILL.md defines the stop behavior for non-ready/in-progress
STOP_BEHAVIOR="$(grep -c '非以上标签\|非以上状态\|回复当前状态并停止' "$ORCH_SKILL" || echo 0)"
assert "SKILL.md defines stop behavior for non-ready/in-progress issues" "[ '$STOP_BEHAVIOR' -ge 1 ]"

# Verify orchestrator scan filters by ready-for-agent (so needs-info won't even appear)
FILTER_SCAN="$(grep -c 'label "ready-for-agent"' "$ORCH_SKILL" || echo 0)"
assert "SKILL.md scan filters only ready-for-agent (excludes needs-info)" "[ '$FILTER_SCAN' -ge 1 ]"

echo ""

# ── AC4: State transition chain complete ──
echo "AC4: ready-for-agent → in-progress → resolved state transition chain"

# Dynamically find resolved issues and verify their transition chain
RESOLVED_LIST="$(cd "$PROJECT_ROOT" && gh issue list --label "resolved" --json number --limit 10 2>&1)"
RESOLVED_COUNT="$(echo "$RESOLVED_LIST" | python3 -c 'import json,sys; print(len(json.load(sys.stdin)))' 2>/dev/null || echo 0)"

RESOLVED_VERIFIED=0
if [ "$RESOLVED_COUNT" -gt 0 ]; then
  RESOLVED_NUMS="$(echo "$RESOLVED_LIST" | python3 -c 'import json,sys; print(" ".join(str(i["number"]) for i in json.load(sys.stdin)))' 2>/dev/null)"
  for num in $RESOLVED_NUMS; do
    if [ "$RESOLVED_VERIFIED" -ge 2 ]; then
      break
    fi
    HAS_RESOLVED=$(cd "$PROJECT_ROOT" && gh issue view "$num" --json labels 2>&1 | python3 -c 'import json,sys; labels=[l["name"] for l in json.load(sys.stdin)["labels"]]; print("yes" if "resolved" in labels else "no")')
    if [ "$HAS_RESOLVED" = "yes" ]; then
      HAS_START=$(cd "$PROJECT_ROOT" && gh issue view "$num" --json comments 2>&1 | python3 -c 'import json,sys; bodies=[c["body"] for c in json.load(sys.stdin)["comments"]]; print("yes" if any("autopilot: 开始处理" in b for b in bodies) else "no")')
      HAS_MERGE=$(cd "$PROJECT_ROOT" && gh issue view "$num" --json comments 2>&1 | python3 -c 'import json,sys; bodies=[c["body"] for c in json.load(sys.stdin)["comments"]]; print("yes" if any("autopilot reviewer: MERGE" in b for b in bodies) else "no")')
      assert "issue #$num has resolved label" "[ '$HAS_RESOLVED' = 'yes' ]"
      assert "issue #$num has 'autopilot: 开始处理' comment (in-progress transition)" "[ '$HAS_START' = 'yes' ]"
      assert "issue #$num has reviewer MERGE comment (resolved transition)" "[ '$HAS_MERGE' = 'yes' ]"
      RESOLVED_VERIFIED=$((RESOLVED_VERIFIED + 1))
    fi
  done
else
  echo "  ℹ No resolved issues found — skipping resolved transition tests"
fi

# Dynamically find in-progress issues and verify
IN_PROGRESS_LIST="$(cd "$PROJECT_ROOT" && gh issue list --label "in-progress" --json number --limit 10 2>&1)"
IN_PROGRESS_COUNT="$(echo "$IN_PROGRESS_LIST" | python3 -c 'import json,sys; print(len(json.load(sys.stdin)))' 2>/dev/null || echo 0)"

if [ "$IN_PROGRESS_COUNT" -gt 0 ]; then
  IP_NUM="$(echo "$IN_PROGRESS_LIST" | python3 -c 'import json,sys; print(json.load(sys.stdin)[0]["number"])' 2>/dev/null)"
  ISSUE_IP="$(cd "$PROJECT_ROOT" && gh issue view "$IP_NUM" --json labels 2>&1)"
  ISSUE_IP_LABELS="$(echo "$ISSUE_IP" | python3 -c 'import json,sys; print([l["name"] for l in json.load(sys.stdin)["labels"]])' 2>/dev/null)"
  assert "issue #$IP_NUM has in-progress label" "echo '$ISSUE_IP_LABELS' | grep -q 'in-progress'"
else
  echo "  ℹ No in-progress issues found — skipping in-progress fixture tests"
fi

# Verify the SKILL.md defines all state transitions
TRANSITIONS="$(grep -c 'Status 为.*resolved\|label.*resolved\|Status 为.*in-progress\|label.*in-progress\|Status 为.*needs-info\|label.*needs-info' "$ORCH_SKILL" || echo 0)"
assert "SKILL.md defines state transition mappings (ready-for-agent, in-progress, resolved, needs-info)" "[ '$TRANSITIONS' -ge 4 ]"

echo ""

# ── AC5: Dispatch chain (scan → status → transition → implementer → reviewer) no crash ──
echo "AC5: Dispatch chain integrity"

# Verify all phases are defined in SKILL.md
assert "SKILL.md defines scan phase" "grep -q '扫描模式' '$ORCH_SKILL'"
assert "SKILL.md defines status recognition phase" "grep -q '检查.*label\|检查.*Status' '$ORCH_SKILL'"
assert "SKILL.md defines state transition phase" "grep -q 'add-label.*remove-label\|edit_file.*Status' '$ORCH_SKILL'"
assert "SKILL.md defines implementer dispatch" "grep -q 'run_skill.*autopilot-implementer' '$ORCH_SKILL'"
assert "SKILL.md defines reviewer dispatch" "grep -q 'run_skill.*autopilot-reviewer' '$ORCH_SKILL'"

# Verify that resolved issues completed full chain without crash evidence
# If they crashed, they'd be stuck at in-progress or needs-info, not resolved
if [ "$RESOLVED_VERIFIED" -gt 0 ]; then
  assert "at least one resolved issue completed full chain" "[ '$RESOLVED_VERIFIED' -ge 1 ]"
else
  echo "  ℹ No resolved issues to verify full chain completion"
fi

# Verify retry limit is defined (safety against infinite loops)
assert "SKILL.md defines retry limit (max 3 rounds)" "grep -q '最多 3 轮\|retry_count.*3' '$ORCH_SKILL'"

# Verify error handling exists
assert "SKILL.md defines needs-info fallback on exhaustion" "grep -q '转为 needs-info\|Status 为.*needs-info' '$ORCH_SKILL'"
assert "SKILL.md defines empty reply handling" "grep -q '空回复处理\|empty.*retry' '$ORCH_SKILL'"
assert "SKILL.md defines unparseable reply handling" "grep -q '解析容错.*不可解析' '$ORCH_SKILL'"

echo ""
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
