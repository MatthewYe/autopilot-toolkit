#!/usr/bin/env bash
# Integration tests for bootstrap.sh
# Run: bash tests/test_bootstrap.sh
set -euo pipefail

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

assert_symlink() {
    local label="$1" path="$2" expected_target="$3"
    if [[ -L "${path}" && "$(readlink "${path}")" == "${expected_target}" ]]; then
        echo "  PASS: ${label}"
        PASS=$((PASS + 1))
    else
        echo "  FAIL: ${label}"
        FAIL=$((FAIL + 1))
    fi
}

assert_not_exists() {
    local label="$1" path="$2"
    if [[ ! -e "${path}" && ! -L "${path}" ]]; then
        echo "  PASS: ${label}"
        PASS=$((PASS + 1))
    else
        echo "  FAIL: ${label} — ${path} exists but should not"
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

setup_mock_ssot() {
    local ssot="$1"
    for skill in autopilot-implementer autopilot-reviewer; do
        mkdir -p "${ssot}/${skill}/runtime"/{default,reasonix,codex,kimi}
        echo "# ${skill} router" > "${ssot}/${skill}/SKILL.md"
        for runtime in default reasonix codex kimi; do
            echo "# ${skill} ${runtime}" \
                > "${ssot}/${skill}/runtime/${runtime}/INSTRUCTIONS.md"
        done
        echo '[agent]' > "${ssot}/${skill}/runtime/codex/agent.toml"
    done
    mkdir -p "${ssot}/zoom-out"
    echo "# zoom out" > "${ssot}/zoom-out/SKILL.md"
}

run_bootstrap() {
    local target="$1" ssot="$2" runtime_skills="$3" codex_agents="${4:-}"
    AGENTS_SKILLS_DIR="${ssot}" \
    REASONIX_SKILLS_DIR="${runtime_skills}" \
    CODEX_SKILLS_DIR="${runtime_skills}" \
    CODEX_AGENTS_DIR="${codex_agents}" \
    HOME="${TMP_BASE}" \
        bash "${BOOTSTRAP_SCRIPT}" --target "${target}"
}

test_shared_router_avoids_runtime_duplicates() {
    echo ""
    echo "=== test: shared router avoids runtime duplicate skills ==="
    TMP_BASE="$(mktemp -d)"
    local ssot="${TMP_BASE}/ssot"
    local reasonix_skills="${TMP_BASE}/reasonix/skills"
    local codex_skills="${TMP_BASE}/codex/skills"
    local codex_agents="${TMP_BASE}/codex/agents"
    setup_mock_ssot "${ssot}"

    run_bootstrap reasonix "${ssot}" "${reasonix_skills}"
    run_bootstrap codex "${ssot}" "${codex_skills}" "${codex_agents}"

    assert_not_exists "Reasonix has no duplicate implementer entry" \
        "${reasonix_skills}/autopilot-implementer"
    assert_not_exists "Codex has no duplicate implementer entry" \
        "${codex_skills}/autopilot-implementer"
    assert_file "shared implementer router remains discoverable" \
        "${ssot}/autopilot-implementer/SKILL.md"
    assert_symlink "Codex implementer agent is deployed" \
        "${codex_agents}/autopilot-implementer.toml" \
        "${ssot}/autopilot-implementer/runtime/codex/agent.toml"
    assert_symlink "Codex reviewer agent is deployed" \
        "${codex_agents}/autopilot-reviewer.toml" \
        "${ssot}/autopilot-reviewer/runtime/codex/agent.toml"
}

test_legacy_runtime_links_are_removed() {
    echo ""
    echo "=== test: legacy toolkit runtime links are removed ==="
    TMP_BASE="$(mktemp -d)"
    local ssot="${TMP_BASE}/ssot"
    local reasonix_skills="${TMP_BASE}/reasonix/skills"
    local codex_skills="${TMP_BASE}/codex/skills"
    local codex_agents="${TMP_BASE}/codex/agents"
    setup_mock_ssot "${ssot}"
    mkdir -p "${reasonix_skills}" "${codex_skills}"
    ln -s "${ssot}/autopilot-implementer/reasonix" \
        "${reasonix_skills}/autopilot-implementer"
    ln -s "${ssot}/autopilot-implementer/codex" \
        "${codex_skills}/autopilot-implementer"

    run_bootstrap reasonix "${ssot}" "${reasonix_skills}"
    run_bootstrap codex "${ssot}" "${codex_skills}" "${codex_agents}"

    assert_not_exists "legacy Reasonix link removed" \
        "${reasonix_skills}/autopilot-implementer"
    assert_not_exists "legacy Codex link removed" \
        "${codex_skills}/autopilot-implementer"
}

test_user_entries_are_preserved() {
    echo ""
    echo "=== test: unrelated user entries are preserved ==="
    TMP_BASE="$(mktemp -d)"
    local ssot="${TMP_BASE}/ssot"
    local reasonix_skills="${TMP_BASE}/reasonix/skills"
    local external="${TMP_BASE}/external-skill"
    setup_mock_ssot "${ssot}"
    mkdir -p "${reasonix_skills}/autopilot-implementer" "${external}" "${ssot}/user-shared"
    echo "user data" > "${reasonix_skills}/autopilot-implementer/custom.txt"
    ln -s "${external}" "${reasonix_skills}/user-symlink"
    ln -s "${ssot}/user-shared" "${reasonix_skills}/user-shared"

    run_bootstrap reasonix "${ssot}" "${reasonix_skills}"

    assert_file "real user directory preserved" \
        "${reasonix_skills}/autopilot-implementer/custom.txt"
    assert_symlink "unrelated user symlink preserved" \
        "${reasonix_skills}/user-symlink" "${external}"
    assert_symlink "user link into shared directory preserved" \
        "${reasonix_skills}/user-shared" "${ssot}/user-shared"
}

test_agent_cleanup_and_idempotency() {
    echo ""
    echo "=== test: Codex agent links are idempotent and stale-safe ==="
    TMP_BASE="$(mktemp -d)"
    local ssot="${TMP_BASE}/ssot"
    local codex_skills="${TMP_BASE}/codex/skills"
    local codex_agents="${TMP_BASE}/codex/agents"
    setup_mock_ssot "${ssot}"

    run_bootstrap codex "${ssot}" "${codex_skills}" "${codex_agents}"
    run_bootstrap codex "${ssot}" "${codex_skills}" "${codex_agents}"
    assert_symlink "agent link remains correct after second run" \
        "${codex_agents}/autopilot-implementer.toml" \
        "${ssot}/autopilot-implementer/runtime/codex/agent.toml"

    rm -rf "${ssot}/autopilot-implementer"
    run_bootstrap codex "${ssot}" "${codex_skills}" "${codex_agents}"
    assert_not_exists "stale toolkit agent link removed" \
        "${codex_agents}/autopilot-implementer.toml"
    assert_symlink "remaining agent link preserved" \
        "${codex_agents}/autopilot-reviewer.toml" \
        "${ssot}/autopilot-reviewer/runtime/codex/agent.toml"
}

test_empty_and_invalid_inputs() {
    echo ""
    echo "=== test: empty and invalid inputs ==="
    TMP_BASE="$(mktemp -d)"
    local ssot="${TMP_BASE}/ssot"
    mkdir -p "${ssot}"

    if run_bootstrap reasonix "${ssot}" "${TMP_BASE}/reasonix/skills"; then
        echo "  PASS: empty SSOT exits successfully"
        PASS=$((PASS + 1))
    else
        echo "  FAIL: empty SSOT should succeed"
        FAIL=$((FAIL + 1))
    fi

    if AGENTS_SKILLS_DIR="${TMP_BASE}/missing" HOME="${TMP_BASE}" \
        bash "${BOOTSTRAP_SCRIPT}" --target reasonix; then
        echo "  PASS: missing SSOT exits successfully"
        PASS=$((PASS + 1))
    else
        echo "  FAIL: missing SSOT should succeed"
        FAIL=$((FAIL + 1))
    fi

    if HOME="${TMP_BASE}" bash "${BOOTSTRAP_SCRIPT}" --target kimi >/dev/null 2>&1; then
        echo "  FAIL: unsupported Kimi bootstrap should fail"
        FAIL=$((FAIL + 1))
    else
        echo "  PASS: unsupported Kimi bootstrap fails clearly"
        PASS=$((PASS + 1))
    fi
}

echo "bootstrap.sh integration tests"
echo "================================"

test_shared_router_avoids_runtime_duplicates
test_legacy_runtime_links_are_removed
test_user_entries_are_preserved
test_agent_cleanup_and_idempotency
test_empty_and_invalid_inputs

echo ""
echo "================================"
echo "Results: ${PASS} passed, ${FAIL} failed"
if [[ ${FAIL} -gt 0 ]]; then
    exit 1
fi
