#!/usr/bin/env bash
# Integration tests for bootstrap.sh
# Run: bash tests/test_bootstrap.sh
set -euo pipefail
trap 'echo "ERROR at line $LINENO"' ERR

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BOOTSTRAP_SCRIPT="${SCRIPT_DIR}/../bootstrap.sh"

PASS=0
FAIL=0
TMP_BASE=""

cleanup() {
    if [[ -n "${TMP_BASE}" && -d "${TMP_BASE}" ]]; then
        rm -rf "${TMP_BASE}"
    fi
}
trap cleanup EXIT

assert_eq() {
    local label="$1" expected="$2" actual="$3"
    if [[ "${expected}" == "${actual}" ]]; then
        echo "  PASS: ${label}"
        PASS=$((PASS + 1))
    else
        echo "  FAIL: ${label}"
        echo "    expected: ${expected}"
        echo "    actual:   ${actual}"
        FAIL=$((FAIL + 1))
    fi
}

assert_file() {
    local label="$1" path="$2"
    if [[ -f "${path}" ]]; then
        echo "  PASS: ${label}"
        PASS=$((PASS + 1))
    else
        echo "  FAIL: ${label} — ${path} does not exist"
        FAIL=$((FAIL + 1))
    fi
}

assert_symlink() {
    local label="$1" path="$2" expected_target="$3"
    if [[ -L "${path}" ]]; then
        local actual_target
        actual_target="$(readlink "${path}")"
        if [[ "${actual_target}" == "${expected_target}" ]]; then
            echo "  PASS: ${label}"
            PASS=$((PASS + 1))
        else
            echo "  FAIL: ${label}"
            echo "    expected symlink target: ${expected_target}"
            echo "    actual:   ${actual_target}"
            FAIL=$((FAIL + 1))
        fi
    else
        echo "  FAIL: ${label} — ${path} is not a symlink"
        FAIL=$((FAIL + 1))
    fi
}

assert_not_exists() {
    local label="$1" path="$2"
    if [[ ! -e "${path}" ]]; then
        echo "  PASS: ${label}"
        PASS=$((PASS + 1))
    else
        echo "  FAIL: ${label} — ${path} exists but should not"
        FAIL=$((FAIL + 1))
    fi
}

# ── helper: build mock SSOT ──────────────────────────────────────────────

setup_mock_ssot() {
    local ssot="$1"

    mkdir -p "${ssot}"

    # Coupled skill: has reasonix/, codex/, kimi/ variants
    mkdir -p "${ssot}/autopilot-implementer/reasonix"
    mkdir -p "${ssot}/autopilot-implementer/codex"
    mkdir -p "${ssot}/autopilot-implementer/kimi"
    echo "# implementer reasonix" > "${ssot}/autopilot-implementer/reasonix/SKILL.md"
    echo "# implementer codex" > "${ssot}/autopilot-implementer/codex/SKILL.md"
    echo "# implementer kimi" > "${ssot}/autopilot-implementer/kimi/SKILL.md"
    echo '[agent]' > "${ssot}/autopilot-implementer/codex/agent.toml"

    # Coupled skill: has only reasonix and codex (no kimi)
    mkdir -p "${ssot}/autopilot-reviewer/reasonix"
    mkdir -p "${ssot}/autopilot-reviewer/codex"
    echo "# reviewer reasonix" > "${ssot}/autopilot-reviewer/reasonix/SKILL.md"
    echo "# reviewer codex" > "${ssot}/autopilot-reviewer/codex/SKILL.md"
    echo '[agent]' > "${ssot}/autopilot-reviewer/codex/agent.toml"

    # Coupled skill: reasonix-only (no codex variant)
    mkdir -p "${ssot}/reasonix-only-skill/reasonix"
    echo "# reasonix only" > "${ssot}/reasonix-only-skill/reasonix/SKILL.md"

    # Agnostic skill: just SKILL.md, no variant dirs
    mkdir -p "${ssot}/zoom-out"
    echo "# zoom out" > "${ssot}/zoom-out/SKILL.md"
}

# ── Test: reasonix bootstrap creates correct symlinks ────────────────────

test_reasonix_bootstrap() {
    echo ""
    echo "=== test: reasonix bootstrap creates correct symlinks ==="

    TMP_BASE="$(mktemp -d)"
    local ssot="${TMP_BASE}/ssot"
    local reasonix_skills="${TMP_BASE}/reasonix/skills"

    setup_mock_ssot "${ssot}"

    AGENTS_SKILLS_DIR="${ssot}" \
    REASONIX_SKILLS_DIR="${reasonix_skills}" \
    HOME="${TMP_BASE}" \
        bash "${BOOTSTRAP_SCRIPT}" --target reasonix

    # Has reasonix variant → should be symlinked
    assert_symlink "implementer → reasonix/variant" \
        "${reasonix_skills}/autopilot-implementer" \
        "${ssot}/autopilot-implementer/reasonix"

    assert_symlink "reviewer → reasonix/variant" \
        "${reasonix_skills}/autopilot-reviewer" \
        "${ssot}/autopilot-reviewer/reasonix"

    # reasonix-only-skill has reasonix variant → should be symlinked
    assert_symlink "reasonix-only → reasonix/variant" \
        "${reasonix_skills}/reasonix-only-skill" \
        "${ssot}/reasonix-only-skill/reasonix"

    # zoom-out is agnostic (no reasonix variant) → should NOT be symlinked
    assert_not_exists "zoom-out not symlinked (agnostic)" \
        "${reasonix_skills}/zoom-out"
}

# ── Test: codex bootstrap creates skill symlinks + agent.toml ────────────

test_codex_bootstrap() {
    echo ""
    echo "=== test: codex bootstrap creates skill symlinks + agent.toml ==="

    TMP_BASE="$(mktemp -d)"
    local ssot="${TMP_BASE}/ssot"
    local codex_skills="${TMP_BASE}/codex/skills"
    local codex_agents="${TMP_BASE}/codex/agents"

    setup_mock_ssot "${ssot}"

    AGENTS_SKILLS_DIR="${ssot}" \
    CODEX_SKILLS_DIR="${codex_skills}" \
    CODEX_AGENTS_DIR="${codex_agents}" \
    HOME="${TMP_BASE}" \
        bash "${BOOTSTRAP_SCRIPT}" --target codex

    # Has codex variant → should be symlinked
    assert_symlink "implementer → codex/variant" \
        "${codex_skills}/autopilot-implementer" \
        "${ssot}/autopilot-implementer/codex"

    assert_symlink "reviewer → codex/variant" \
        "${codex_skills}/autopilot-reviewer" \
        "${ssot}/autopilot-reviewer/codex"

    # reasonix-only-skill has no codex variant → should NOT be symlinked
    assert_not_exists "reasonix-only not symlinked (no codex variant)" \
        "${codex_skills}/reasonix-only-skill"

    # zoom-out is agnostic → should NOT be symlinked
    assert_not_exists "zoom-out not symlinked (agnostic)" \
        "${codex_skills}/zoom-out"

    # agent.toml symlinks
    assert_symlink "implementer agent.toml" \
        "${codex_agents}/autopilot-implementer.toml" \
        "${ssot}/autopilot-implementer/codex/agent.toml"

    assert_symlink "reviewer agent.toml" \
        "${codex_agents}/autopilot-reviewer.toml" \
        "${ssot}/autopilot-reviewer/codex/agent.toml"
}

# ── Test: bootstrap is idempotent ────────────────────────────────────────

test_idempotent() {
    echo ""
    echo "=== test: bootstrap is idempotent ==="

    TMP_BASE="$(mktemp -d)"
    local ssot="${TMP_BASE}/ssot"
    local reasonix_skills="${TMP_BASE}/reasonix/skills"

    setup_mock_ssot "${ssot}"

    # First run
    AGENTS_SKILLS_DIR="${ssot}" \
    REASONIX_SKILLS_DIR="${reasonix_skills}" \
    HOME="${TMP_BASE}" \
        bash "${BOOTSTRAP_SCRIPT}" --target reasonix

    # Capture mtime of the symlink after first run
    local mtime1
    mtime1="$(stat -f "%m" "${reasonix_skills}/autopilot-implementer" 2>/dev/null || stat -c "%Y" "${reasonix_skills}/autopilot-implementer")"

    # Second run
    AGENTS_SKILLS_DIR="${ssot}" \
    REASONIX_SKILLS_DIR="${reasonix_skills}" \
    HOME="${TMP_BASE}" \
        bash "${BOOTSTRAP_SCRIPT}" --target reasonix

    # Verify symlinks still point to correct targets
    assert_symlink "implementer still correct after 2nd run" \
        "${reasonix_skills}/autopilot-implementer" \
        "${ssot}/autopilot-implementer/reasonix"

    assert_symlink "reviewer still correct after 2nd run" \
        "${reasonix_skills}/autopilot-reviewer" \
        "${ssot}/autopilot-reviewer/reasonix"

    # Symlink mtime should be unchanged (wasn't recreated)
    local mtime2
    mtime2="$(stat -f "%m" "${reasonix_skills}/autopilot-implementer" 2>/dev/null || stat -c "%Y" "${reasonix_skills}/autopilot-implementer")"
    assert_eq "symlink mtime unchanged (idempotent)" "${mtime1}" "${mtime2}"
}

# ── Test: stale symlink removal ──────────────────────────────────────────

test_stale_symlink_removal() {
    echo ""
    echo "=== test: stale symlink removal ==="

    TMP_BASE="$(mktemp -d)"
    local ssot="${TMP_BASE}/ssot"
    local reasonix_skills="${TMP_BASE}/reasonix/skills"

    setup_mock_ssot "${ssot}"

    # First run: all symlinks created
    AGENTS_SKILLS_DIR="${ssot}" \
    REASONIX_SKILLS_DIR="${reasonix_skills}" \
    HOME="${TMP_BASE}" \
        bash "${BOOTSTRAP_SCRIPT}" --target reasonix

    assert_symlink "before: implementer exists" \
        "${reasonix_skills}/autopilot-implementer" \
        "${ssot}/autopilot-implementer/reasonix"

    # Remove a skill from SSOT
    rm -rf "${ssot}/autopilot-implementer"

    # Second run: stale symlink should be cleaned up
    AGENTS_SKILLS_DIR="${ssot}" \
    REASONIX_SKILLS_DIR="${reasonix_skills}" \
    HOME="${TMP_BASE}" \
        bash "${BOOTSTRAP_SCRIPT}" --target reasonix

    assert_not_exists "stale implementer symlink removed" \
        "${reasonix_skills}/autopilot-implementer"

    # Other symlinks still exist
    assert_symlink "reviewer still exists" \
        "${reasonix_skills}/autopilot-reviewer" \
        "${ssot}/autopilot-reviewer/reasonix"
}

# ── Test: stale agent.toml removal ───────────────────────────────────────

test_stale_agent_removal() {
    echo ""
    echo "=== test: stale agent.toml removal ==="

    TMP_BASE="$(mktemp -d)"
    local ssot="${TMP_BASE}/ssot"
    local codex_skills="${TMP_BASE}/codex/skills"
    local codex_agents="${TMP_BASE}/codex/agents"

    setup_mock_ssot "${ssot}"

    # First run: agent.toml symlinks created
    AGENTS_SKILLS_DIR="${ssot}" \
    CODEX_SKILLS_DIR="${codex_skills}" \
    CODEX_AGENTS_DIR="${codex_agents}" \
    HOME="${TMP_BASE}" \
        bash "${BOOTSTRAP_SCRIPT}" --target codex

    assert_symlink "before: implementer agent exists" \
        "${codex_agents}/autopilot-implementer.toml" \
        "${ssot}/autopilot-implementer/codex/agent.toml"

    # Remove a skill that had an agent.toml
    rm -rf "${ssot}/autopilot-implementer"

    # Second run
    AGENTS_SKILLS_DIR="${ssot}" \
    CODEX_SKILLS_DIR="${codex_skills}" \
    CODEX_AGENTS_DIR="${codex_agents}" \
    HOME="${TMP_BASE}" \
        bash "${BOOTSTRAP_SCRIPT}" --target codex

    assert_not_exists "stale implementer agent.toml removed" \
        "${codex_agents}/autopilot-implementer.toml"

    # Other agent still exists
    assert_symlink "reviewer agent still exists" \
        "${codex_agents}/autopilot-reviewer.toml" \
        "${ssot}/autopilot-reviewer/codex/agent.toml"
}

# ── Test: empty SSOT exits gracefully ────────────────────────────────────

test_empty_ssot() {
    echo ""
    echo "=== test: empty SSOT exits gracefully ==="

    TMP_BASE="$(mktemp -d)"
    local ssot="${TMP_BASE}/ssot"
    local reasonix_skills="${TMP_BASE}/reasonix/skills"

    # Empty SSOT (no skill directories)
    mkdir -p "${ssot}"

    if AGENTS_SKILLS_DIR="${ssot}" \
        REASONIX_SKILLS_DIR="${reasonix_skills}" \
        HOME="${TMP_BASE}" \
        bash "${BOOTSTRAP_SCRIPT}" --target reasonix; then
        echo "  PASS: empty SSOT exits 0"
        PASS=$((PASS + 1))
    else
        echo "  FAIL: empty SSOT should exit 0"
        FAIL=$((FAIL + 1))
    fi
}

# ── Test: non-existent SSOT exits gracefully ─────────────────────────────

test_nonexistent_ssot() {
    echo ""
    echo "=== test: non-existent SSOT exits gracefully ==="

    TMP_BASE="$(mktemp -d)"

    if AGENTS_SKILLS_DIR="${TMP_BASE}/nonexistent" \
        REASONIX_SKILLS_DIR="${TMP_BASE}/reasonix/skills" \
        HOME="${TMP_BASE}" \
        bash "${BOOTSTRAP_SCRIPT}" --target reasonix; then
        echo "  PASS: non-existent SSOT exits 0"
        PASS=$((PASS + 1))
    else
        echo "  FAIL: non-existent SSOT should exit 0"
        FAIL=$((FAIL + 1))
    fi
}

# ── Test: kimi requires no bootstrap ─────────────────────────────────────

test_kimi_no_bootstrap() {
    echo ""
    echo "=== test: kimi requires no bootstrap ==="

    TMP_BASE="$(mktemp -d)"
    local ssot="${TMP_BASE}/ssot"

    setup_mock_ssot "${ssot}"

    # bootstrap.sh should reject --target kimi
    if AGENTS_SKILLS_DIR="${ssot}" \
        HOME="${TMP_BASE}" \
        bash "${BOOTSTRAP_SCRIPT}" --target kimi 2>/dev/null; then
        echo "  FAIL: --target kimi should not be accepted"
        FAIL=$((FAIL + 1))
    else
        echo "  PASS: --target kimi rejected (kimi needs no bootstrap)"
        PASS=$((PASS + 1))
    fi
}

# ── Test: real directory not overwritten ─────────────────────────────────

test_real_dir_not_overwritten() {
    echo ""
    echo "=== test: real directory is not overwritten by symlink ==="

    TMP_BASE="$(mktemp -d)"
    local ssot="${TMP_BASE}/ssot"
    local reasonix_skills="${TMP_BASE}/reasonix/skills"

    setup_mock_ssot "${ssot}"

    # Create a real directory where the symlink would go
    mkdir -p "${reasonix_skills}/autopilot-implementer"
    echo "user data" > "${reasonix_skills}/autopilot-implementer/custom.txt"

    AGENTS_SKILLS_DIR="${ssot}" \
    REASONIX_SKILLS_DIR="${reasonix_skills}" \
    HOME="${TMP_BASE}" \
        bash "${BOOTSTRAP_SCRIPT}" --target reasonix 2>&1 || true

    # Should still be a real directory, not replaced
    if [[ -d "${reasonix_skills}/autopilot-implementer" && ! -L "${reasonix_skills}/autopilot-implementer" ]]; then
        echo "  PASS: real directory preserved"
        PASS=$((PASS + 1))
    else
        echo "  FAIL: real directory was replaced"
        FAIL=$((FAIL + 1))
    fi

    assert_file "user file inside real dir still exists" \
        "${reasonix_skills}/autopilot-implementer/custom.txt"
}

# ── Test: broken stale symlink removal ───────────────────────────────────

test_broken_stale_symlink_removal() {
    echo ""
    echo "=== test: broken stale symlink removal (SSOT source deleted) ==="

    TMP_BASE="$(mktemp -d)"
    local ssot="${TMP_BASE}/ssot"
    local reasonix_skills="${TMP_BASE}/reasonix/skills"
    local codex_skills="${TMP_BASE}/codex/skills"
    local codex_agents="${TMP_BASE}/codex/agents"

    setup_mock_ssot "${ssot}"

    # First run: create all symlinks for both targets
    AGENTS_SKILLS_DIR="${ssot}" \
    REASONIX_SKILLS_DIR="${reasonix_skills}" \
    HOME="${TMP_BASE}" \
        bash "${BOOTSTRAP_SCRIPT}" --target reasonix

    AGENTS_SKILLS_DIR="${ssot}" \
    CODEX_SKILLS_DIR="${codex_skills}" \
    CODEX_AGENTS_DIR="${codex_agents}" \
    HOME="${TMP_BASE}" \
        bash "${BOOTSTRAP_SCRIPT}" --target codex

    assert_symlink "before: reasonix implementer exists" \
        "${reasonix_skills}/autopilot-implementer" \
        "${ssot}/autopilot-implementer/reasonix"

    assert_symlink "before: codex implementer exists" \
        "${codex_skills}/autopilot-implementer" \
        "${ssot}/autopilot-implementer/codex"

    # Delete SSOT source — symlinks are now broken
    rm -rf "${ssot}/autopilot-implementer"

    # Verify symlinks are broken
    if [[ -L "${reasonix_skills}/autopilot-implementer" ]] && [[ ! -e "${reasonix_skills}/autopilot-implementer" ]]; then
        echo "  PASS: reasonix symlink is broken after source deletion"
        PASS=$((PASS + 1))
    else
        echo "  FAIL: reasonix symlink should be broken"
        FAIL=$((FAIL + 1))
    fi

    # Re-run bootstrap — should clean up broken symlinks
    AGENTS_SKILLS_DIR="${ssot}" \
    REASONIX_SKILLS_DIR="${reasonix_skills}" \
    HOME="${TMP_BASE}" \
        bash "${BOOTSTRAP_SCRIPT}" --target reasonix

    assert_not_exists "broken reasonix symlink removed" \
        "${reasonix_skills}/autopilot-implementer"

    # Other valid symlinks still exist
    assert_symlink "reasonix reviewer still valid" \
        "${reasonix_skills}/autopilot-reviewer" \
        "${ssot}/autopilot-reviewer/reasonix"
}

# ── Run all tests ────────────────────────────────────────────────────────

echo "bootstrap.sh integration tests"
echo "================================"

test_reasonix_bootstrap
test_codex_bootstrap
test_idempotent
test_stale_symlink_removal
test_stale_agent_removal
test_empty_ssot
test_nonexistent_ssot
test_kimi_no_bootstrap
test_real_dir_not_overwritten
test_broken_stale_symlink_removal

echo ""
echo "================================"
echo "Results: ${PASS} passed, ${FAIL} failed"
if [[ ${FAIL} -gt 0 ]]; then
    exit 1
fi
