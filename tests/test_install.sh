#!/usr/bin/env bash
# Integration tests for install.sh
# Run: bash tests/test_install.sh
set -euo pipefail
trap 'echo "ERROR at line $LINENO"' ERR

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BOOTSTRAP_SCRIPT="${SCRIPT_DIR}/../bootstrap.sh"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

# We need a built install.sh — use the build pipeline output
TARBALL_DIR="${SCRIPT_DIR}/../dist"
TARBALL_NAME="autopilot-toolkit.tar.gz"
FULL_TARBALL="${TARBALL_DIR}/${TARBALL_NAME}"

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

assert_dir() {
    local label="$1" path="$2"
    if [[ -d "${path}" ]]; then
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

# Use the full tarball (built from the pipeline).
# Extract install.sh from it for testing.
extract_install_sh() {
    local dest="$1"
    mkdir -p "$(dirname "${dest}/.autopilot/install.sh")" && cp "${PROJECT_ROOT}/dist/install.sh" "${dest}/.autopilot/install.sh"
}

# ── helper: build a minimal mock tarball for fast tests ──────────────────

build_mock_tarball() {
    local tarball_path="$1"
    local version="${2:-mock-version-001}"

    local staging="${TMP_BASE}/mock-staging"
    mkdir -p "${staging}"/{.autopilot,skills,principles}

    # .autopilot/ contents
    echo "${version}" > "${staging}/.autopilot/.version"

    # manifest.json with a few skills
    cat > "${staging}/.autopilot/manifest.json" << JSONEOF
{
  "version": "${version}",
  "skills": {
    "toolkit-setup": {"type": "agnostic"},
    "autopilot-implementer": {"type": "coupled", "variants": ["reasonix", "codex", "kimi"], "codex_agent": true},
    "zoom-out": {"type": "agnostic"}
  }
}
JSONEOF

    # Copy bootstrap.sh into staging
    cp "${BOOTSTRAP_SCRIPT}" "${staging}/.autopilot/bootstrap.sh"

    # Copy install.sh from the full tarball
    mkdir -p "$(dirname "${staging}/.autopilot/install.sh")" && cp "${PROJECT_ROOT}/dist/install.sh" "${staging}/.autopilot/install.sh"

    # Add some mock skill directories
    mkdir -p "${staging}/skills/toolkit-setup"
    echo "# toolkit-setup" > "${staging}/skills/toolkit-setup/SKILL.md"

    mkdir -p "${staging}/skills/autopilot-implementer/reasonix"
    mkdir -p "${staging}/skills/autopilot-implementer/codex"
    mkdir -p "${staging}/skills/autopilot-implementer/kimi"
    echo "# impl reasonix" > "${staging}/skills/autopilot-implementer/reasonix/SKILL.md"
    echo "# impl codex" > "${staging}/skills/autopilot-implementer/codex/SKILL.md"
    echo "# impl kimi" > "${staging}/skills/autopilot-implementer/kimi/SKILL.md"
    echo '[agent]' > "${staging}/skills/autopilot-implementer/codex/agent.toml"

    mkdir -p "${staging}/skills/zoom-out"
    echo "# zoom" > "${staging}/skills/zoom-out/SKILL.md"

    # principles
    echo "# Karpathy's Principles" > "${staging}/principles/karpathy.md"

    # Create tarball
    tar -czf "${tarball_path}" -C "${staging}" .
}

# ── Test: fresh install extracts skills and deploys principles ───────────

test_fresh_install_extraction() {
    echo ""
    echo "=== test: fresh install extracts skills and deploys principles ==="

    TMP_BASE="$(mktemp -d)"
    local home="${TMP_BASE}/home"
    local skills_dir="${home}/.agents/skills"
    local principles_dir="${home}/.agents/principles"
    local mock_tarball="${TMP_BASE}/mock-toolkit.tar.gz"

    build_mock_tarball "${mock_tarball}" "test-version-001"

    # Extract install.sh
    local install_sh="${TMP_BASE}/install.sh"
    extract_install_sh "${TMP_BASE}"
    cp "${PROJECT_ROOT}/dist/install.sh" "${install_sh}"

    HOME="${home}" \
    AGENTS_SKILLS_DIR="${skills_dir}" \
    AGENTS_PRINCIPLES_DIR="${principles_dir}" \
        bash "${install_sh}" --tarball "${mock_tarball}" --version "test-version-001"

    # Skills extracted
    assert_dir "skills/toolkit-setup extracted" "${skills_dir}/toolkit-setup"
    assert_dir "skills/autopilot-implementer extracted" "${skills_dir}/autopilot-implementer"
    assert_dir "skills/zoom-out extracted" "${skills_dir}/zoom-out"

    # .autopilot/ metadata
    assert_dir ".autopilot/ exists" "${skills_dir}/.autopilot"
    assert_file ".autopilot/.version exists" "${skills_dir}/.autopilot/.version"
    assert_file ".autopilot/manifest.json exists" "${skills_dir}/.autopilot/manifest.json"
    assert_file ".autopilot/bootstrap.sh exists" "${skills_dir}/.autopilot/bootstrap.sh"

    local installed_version
    installed_version="$(cat "${skills_dir}/.autopilot/.version")"
    assert_eq ".version content matches" "test-version-001" "${installed_version}"

    # Principles deployed
    assert_file "principles/karpathy.md deployed" "${principles_dir}/karpathy.md"
}

# ── Test: install.sh --version overrides embedded version ────────────────

test_version_override() {
    echo ""
    echo "=== test: --version overrides embedded version ==="

    TMP_BASE="$(mktemp -d)"
    local home="${TMP_BASE}/home"
    local skills_dir="${home}/.agents/skills"
    local mock_tarball="${TMP_BASE}/mock-toolkit.tar.gz"

    build_mock_tarball "${mock_tarball}" "custom-hash-abc123"

    local install_sh="${TMP_BASE}/install.sh"
    extract_install_sh "${TMP_BASE}"
    cp "${PROJECT_ROOT}/dist/install.sh" "${install_sh}"

    HOME="${home}" \
    AGENTS_SKILLS_DIR="${skills_dir}" \
        bash "${install_sh}" --tarball "${mock_tarball}" --version "custom-hash-abc123" 2>&1

    local installed_version
    installed_version="$(cat "${skills_dir}/.autopilot/.version")"
    assert_eq "version file matches --version override" "custom-hash-abc123" "${installed_version}"
}

# ── Test: already-installed detection (same version) ─────────────────────

test_already_installed_same_version() {
    echo ""
    echo "=== test: already-installed detection (same version) ==="

    TMP_BASE="$(mktemp -d)"
    local home="${TMP_BASE}/home"
    local skills_dir="${home}/.agents/skills"
    local mock_tarball="${TMP_BASE}/mock-toolkit.tar.gz"

    build_mock_tarball "${mock_tarball}" "v1.0.0"

    local install_sh="${TMP_BASE}/install.sh"
    extract_install_sh "${TMP_BASE}"
    cp "${PROJECT_ROOT}/dist/install.sh" "${install_sh}"

    # First install
    HOME="${home}" AGENTS_SKILLS_DIR="${skills_dir}" \
        bash "${install_sh}" --tarball "${mock_tarball}" --version "v1.0.0" > /dev/null 2>&1

    # Capture mtime of .version
    local mtime1
    mtime1="$(stat -f "%m" "${skills_dir}/.autopilot/.version" 2>/dev/null || stat -c "%Y" "${skills_dir}/.autopilot/.version")"

    # Second install — same version
    local output
    output="$(HOME="${home}" AGENTS_SKILLS_DIR="${skills_dir}" \
        bash "${install_sh}" --tarball "${mock_tarball}" --version "v1.0.0" 2>&1)"

    echo "${output}" | grep -q "already installed" && echo "  PASS: reports already installed" && PASS=$((PASS + 1)) || { echo "  FAIL: should report already installed"; FAIL=$((FAIL + 1)); }

    # .version should be unchanged
    local mtime2
    mtime2="$(stat -f "%m" "${skills_dir}/.autopilot/.version" 2>/dev/null || stat -c "%Y" "${skills_dir}/.autopilot/.version")"
    assert_eq "version file unchanged (no re-extract)" "${mtime1}" "${mtime2}"
}

# ── Test: upgrade from older version ─────────────────────────────────────

test_upgrade() {
    echo ""
    echo "=== test: upgrade from older version ==="

    TMP_BASE="$(mktemp -d)"
    local home="${TMP_BASE}/home"
    local skills_dir="${home}/.agents/skills"

    local old_tarball="${TMP_BASE}/old-toolkit.tar.gz"
    local new_tarball="${TMP_BASE}/new-toolkit.tar.gz"

    build_mock_tarball "${old_tarball}" "old-version"
    build_mock_tarball "${new_tarball}" "new-version"

    local install_sh="${TMP_BASE}/install.sh"
    extract_install_sh "${TMP_BASE}"
    cp "${PROJECT_ROOT}/dist/install.sh" "${install_sh}"

    # Install old version
    HOME="${home}" AGENTS_SKILLS_DIR="${skills_dir}" \
        bash "${install_sh}" --tarball "${old_tarball}" --version "old-version" > /dev/null 2>&1

    assert_eq "old version installed" "old-version" "$(cat "${skills_dir}/.autopilot/.version")"

    # Install new version
    local output
    output="$(HOME="${home}" AGENTS_SKILLS_DIR="${skills_dir}" \
        bash "${install_sh}" --tarball "${new_tarball}" --version "new-version" 2>&1)"

    echo "${output}" | grep -q "Upgrading" && echo "  PASS: reports upgrading" && PASS=$((PASS + 1)) || { echo "  FAIL: should report upgrading"; FAIL=$((FAIL + 1)); }

    assert_eq "new version installed" "new-version" "$(cat "${skills_dir}/.autopilot/.version")"
}

# ── Test: auto-detects Codex runtime and bootstraps ──────────────────────

test_autodetect_codex() {
    echo ""
    echo "=== test: auto-detects Codex runtime and bootstraps ==="

    TMP_BASE="$(mktemp -d)"
    local home="${TMP_BASE}/home"
    local skills_dir="${home}/.agents/skills"
    local codex_dir="${home}/.codex"
    local codex_skills="${codex_dir}/skills"
    local codex_agents="${codex_dir}/agents"
    local mock_tarball="${TMP_BASE}/mock-toolkit.tar.gz"

    # Create ~/.codex/ to trigger auto-detection
    mkdir -p "${codex_dir}"

    build_mock_tarball "${mock_tarball}" "test-codex-001"

    local install_sh="${TMP_BASE}/install.sh"
    extract_install_sh "${TMP_BASE}"
    cp "${PROJECT_ROOT}/dist/install.sh" "${install_sh}"

    HOME="${home}" \
    AGENTS_SKILLS_DIR="${skills_dir}" \
    CODEX_SKILLS_DIR="${codex_skills}" \
    CODEX_AGENTS_DIR="${codex_agents}" \
        bash "${install_sh}" --tarball "${mock_tarball}" --version "test-codex-001"

    # Codex skill symlink created
    assert_symlink "codex: implementer skill symlinked" \
        "${codex_skills}/autopilot-implementer" \
        "${skills_dir}/autopilot-implementer/codex"

    # Codex agent.toml symlink created
    assert_symlink "codex: implementer agent.toml symlinked" \
        "${codex_agents}/autopilot-implementer.toml" \
        "${skills_dir}/autopilot-implementer/codex/agent.toml"

    # Agnostic skill should NOT be symlinked (no codex variant)
    assert_not_exists "codex: zoom-out not symlinked (agnostic)" \
        "${codex_skills}/zoom-out"
}

# ── Test: auto-detects Reasonix runtime and bootstraps ───────────────────

test_autodetect_reasonix() {
    echo ""
    echo "=== test: auto-detects Reasonix runtime and bootstraps ==="

    TMP_BASE="$(mktemp -d)"
    local home="${TMP_BASE}/home"
    local skills_dir="${home}/.agents/skills"
    local reasonix_dir="${home}/.reasonix"
    local reasonix_skills="${reasonix_dir}/skills"
    local mock_tarball="${TMP_BASE}/mock-toolkit.tar.gz"

    # Create ~/.reasonix/ to trigger auto-detection
    mkdir -p "${reasonix_dir}"

    build_mock_tarball "${mock_tarball}" "test-reasonix-001"

    local install_sh="${TMP_BASE}/install.sh"
    extract_install_sh "${TMP_BASE}"
    cp "${PROJECT_ROOT}/dist/install.sh" "${install_sh}"

    HOME="${home}" \
    AGENTS_SKILLS_DIR="${skills_dir}" \
    REASONIX_SKILLS_DIR="${reasonix_skills}" \
        bash "${install_sh}" --tarball "${mock_tarball}" --version "test-reasonix-001"

    # Reasonix skill symlink created
    assert_symlink "reasonix: implementer skill symlinked" \
        "${reasonix_skills}/autopilot-implementer" \
        "${skills_dir}/autopilot-implementer/reasonix"

    # Agnostic skill should NOT be symlinked (no reasonix variant)
    assert_not_exists "reasonix: zoom-out not symlinked (agnostic)" \
        "${reasonix_skills}/zoom-out"
}

# ── Test: both runtimes detected and bootstrapped ────────────────────────

test_autodetect_both() {
    echo ""
    echo "=== test: both runtimes detected and bootstrapped ==="

    TMP_BASE="$(mktemp -d)"
    local home="${TMP_BASE}/home"
    local skills_dir="${home}/.agents/skills"
    local reasonix_skills="${home}/.reasonix/skills"
    local codex_skills="${home}/.codex/skills"
    local codex_agents="${home}/.codex/agents"
    local mock_tarball="${TMP_BASE}/mock-toolkit.tar.gz"

    mkdir -p "${home}/.reasonix"
    mkdir -p "${home}/.codex"

    build_mock_tarball "${mock_tarball}" "test-both-001"

    local install_sh="${TMP_BASE}/install.sh"
    extract_install_sh "${TMP_BASE}"
    cp "${PROJECT_ROOT}/dist/install.sh" "${install_sh}"

    HOME="${home}" \
    AGENTS_SKILLS_DIR="${skills_dir}" \
    REASONIX_SKILLS_DIR="${reasonix_skills}" \
    CODEX_SKILLS_DIR="${codex_skills}" \
    CODEX_AGENTS_DIR="${codex_agents}" \
        bash "${install_sh}" --tarball "${mock_tarball}" --version "test-both-001"

    assert_symlink "reasonix symlink exists" \
        "${reasonix_skills}/autopilot-implementer" \
        "${skills_dir}/autopilot-implementer/reasonix"

    assert_symlink "codex symlink exists" \
        "${codex_skills}/autopilot-implementer" \
        "${skills_dir}/autopilot-implementer/codex"

    assert_symlink "codex agent.toml exists" \
        "${codex_agents}/autopilot-implementer.toml" \
        "${skills_dir}/autopilot-implementer/codex/agent.toml"
}

# ── Test: no runtimes → no bootstrap errors ──────────────────────────────

test_no_runtimes() {
    echo ""
    echo "=== test: no runtimes detected → no bootstrap errors ==="

    TMP_BASE="$(mktemp -d)"
    local home="${TMP_BASE}/home"
    local skills_dir="${home}/.agents/skills"
    local mock_tarball="${TMP_BASE}/mock-toolkit.tar.gz"

    build_mock_tarball "${mock_tarball}" "test-nort-001"

    local install_sh="${TMP_BASE}/install.sh"
    extract_install_sh "${TMP_BASE}"
    cp "${PROJECT_ROOT}/dist/install.sh" "${install_sh}"

    # No ~/.reasonix/ or ~/.codex/ directories
    if HOME="${home}" AGENTS_SKILLS_DIR="${skills_dir}" \
        bash "${install_sh}" --tarball "${mock_tarball}" --version "test-nort-001" > /dev/null 2>&1; then
        echo "  PASS: install succeeds without runtimes"
        PASS=$((PASS + 1))
    else
        echo "  FAIL: install should succeed even without runtimes"
        FAIL=$((FAIL + 1))
    fi

    # Skills and .autopilot should still be extracted
    assert_file ".version exists" "${skills_dir}/.autopilot/.version"
}

# ── Test: existing manifest triggers old-skill cleanup ───────────────────

test_upgrade_cleans_old_skills() {
    echo ""
    echo "=== test: upgrade cleans old toolkit-owned skills ==="

    TMP_BASE="$(mktemp -d)"
    local home="${TMP_BASE}/home"
    local skills_dir="${home}/.agents/skills"

    local old_tarball="${TMP_BASE}/old-toolkit.tar.gz"
    local new_tarball="${TMP_BASE}/new-toolkit.tar.gz"

    # Build old tarball with skill "old-skill"
    local old_staging="${TMP_BASE}/old-staging"
    mkdir -p "${old_staging}"/{.autopilot,skills/old-skill,principles}
    echo "old-version" > "${old_staging}/.autopilot/.version"
    cat > "${old_staging}/.autopilot/manifest.json" << 'JSONEOF'
{"version":"old-version","skills":{"old-skill":{"type":"agnostic"}}}
JSONEOF
    cp "${BOOTSTRAP_SCRIPT}" "${old_staging}/.autopilot/bootstrap.sh"
    mkdir -p "$(dirname "${old_staging}/.autopilot/install.sh")" && cp "${PROJECT_ROOT}/dist/install.sh" "${old_staging}/.autopilot/install.sh"
    mkdir -p "${old_staging}/skills/old-skill"
    echo "# old" > "${old_staging}/skills/old-skill/SKILL.md"
    echo "# old principles" > "${old_staging}/principles/karpathy.md"
    tar -czf "${old_tarball}" -C "${old_staging}" .

    # Build new tarball without "old-skill"
    local new_staging="${TMP_BASE}/new-staging"
    mkdir -p "${new_staging}"/{.autopilot,skills/new-skill,principles}
    echo "new-version" > "${new_staging}/.autopilot/.version"
    cat > "${new_staging}/.autopilot/manifest.json" << 'JSONEOF'
{"version":"new-version","skills":{"new-skill":{"type":"agnostic"}}}
JSONEOF
    cp "${BOOTSTRAP_SCRIPT}" "${new_staging}/.autopilot/bootstrap.sh"
    mkdir -p "$(dirname "${new_staging}/.autopilot/install.sh")" && cp "${PROJECT_ROOT}/dist/install.sh" "${new_staging}/.autopilot/install.sh"
    mkdir -p "${new_staging}/skills/new-skill"
    echo "# new" > "${new_staging}/skills/new-skill/SKILL.md"
    echo "# new principles" > "${new_staging}/principles/karpathy.md"
    tar -czf "${new_tarball}" -C "${new_staging}" .

    local install_sh="${TMP_BASE}/install.sh"
    cp "${PROJECT_ROOT}/dist/install.sh" "${install_sh}"
    chmod +x "${install_sh}"

    # Install old version
    HOME="${home}" AGENTS_SKILLS_DIR="${skills_dir}" \
        bash "${install_sh}" --tarball "${old_tarball}" --version "old-version" > /dev/null 2>&1

    assert_dir "old-skill exists before upgrade" "${skills_dir}/old-skill"

    # Install new version — should clean up old-skill
    HOME="${home}" AGENTS_SKILLS_DIR="${skills_dir}" \
        bash "${install_sh}" --tarball "${new_tarball}" --version "new-version" > /dev/null 2>&1

    if command -v python3 &>/dev/null; then
        assert_not_exists "old-skill removed during upgrade" "${skills_dir}/old-skill"
    else
        echo "  SKIP: python3 not available for manifest-based cleanup"
        PASS=$((PASS + 1))
    fi

    assert_dir "new-skill exists after upgrade" "${skills_dir}/new-skill"
}

# ── Test: full directory tree verification ───────────────────────────────

test_full_tree_verification() {
    echo ""
    echo "=== test: full directory tree verification ==="

    TMP_BASE="$(mktemp -d)"
    local home="${TMP_BASE}/home"
    local skills_dir="${home}/.agents/skills"
    local principles_dir="${home}/.agents/principles"
    local reasonix_skills="${home}/.reasonix/skills"
    local codex_skills="${home}/.codex/skills"
    local codex_agents="${home}/.codex/agents"
    local mock_tarball="${TMP_BASE}/mock-toolkit.tar.gz"

    mkdir -p "${home}/.reasonix"
    mkdir -p "${home}/.codex"

    build_mock_tarball "${mock_tarball}" "full-tree-001"

    local install_sh="${TMP_BASE}/install.sh"
    extract_install_sh "${TMP_BASE}"
    cp "${PROJECT_ROOT}/dist/install.sh" "${install_sh}"

    HOME="${home}" \
    AGENTS_SKILLS_DIR="${skills_dir}" \
    AGENTS_PRINCIPLES_DIR="${principles_dir}" \
    REASONIX_SKILLS_DIR="${reasonix_skills}" \
    CODEX_SKILLS_DIR="${codex_skills}" \
    CODEX_AGENTS_DIR="${codex_agents}" \
        bash "${install_sh}" --tarball "${mock_tarball}" --version "full-tree-001"

    echo ""
    echo "  --- Directory tree ---"
    find "${home}" -not -path '*/.DS_Store' | sort | while read -r line; do
        local_indent="${line#${home}}"
        if [[ -L "${line}" ]]; then
            local link_target
            link_target="$(readlink "${line}")"
            echo "  ${local_indent} -> ${link_target}"
        elif [[ -d "${line}" ]]; then
            echo "  ${local_indent}/"
        else
            echo "  ${local_indent}"
        fi
    done
    echo "  --- End tree ---"
    echo ""

    # Verify skills
    assert_dir "skills/toolkit-setup/" "${skills_dir}/toolkit-setup"
    assert_dir "skills/autopilot-implementer/" "${skills_dir}/autopilot-implementer"
    assert_dir "skills/zoom-out/" "${skills_dir}/zoom-out"

    # Verify .autopilot metadata
    assert_dir ".autopilot/" "${skills_dir}/.autopilot"
    assert_file ".autopilot/.version" "${skills_dir}/.autopilot/.version"
    assert_file ".autopilot/manifest.json" "${skills_dir}/.autopilot/manifest.json"
    assert_file ".autopilot/bootstrap.sh" "${skills_dir}/.autopilot/bootstrap.sh"

    # Verify principles
    assert_file "principles/karpathy.md" "${principles_dir}/karpathy.md"

    # Verify Reasonix symlinks
    assert_symlink "reasonix: autopilot-implementer" \
        "${reasonix_skills}/autopilot-implementer" \
        "${skills_dir}/autopilot-implementer/reasonix"

    # Verify Codex symlinks
    assert_symlink "codex: autopilot-implementer" \
        "${codex_skills}/autopilot-implementer" \
        "${skills_dir}/autopilot-implementer/codex"
    assert_symlink "codex: agent.toml" \
        "${codex_agents}/autopilot-implementer.toml" \
        "${skills_dir}/autopilot-implementer/codex/agent.toml"

    # Verify version
    local installed_version
    installed_version="$(cat "${skills_dir}/.autopilot/.version")"
    assert_eq "version correct" "full-tree-001" "${installed_version}"

    echo ""
    echo "  PASS: full tree verification complete"
    PASS=$((PASS + 1))
}

# ── Test: user-added skills survive upgrade ─────────────────────────────

test_user_skill_preservation() {
    echo ""
    echo "=== test: user-added skills survive upgrade ==="

    TMP_BASE="$(mktemp -d)"
    local home="${TMP_BASE}/home"
    local skills_dir="${home}/.agents/skills"

    local v1_tarball="${TMP_BASE}/v1-toolkit.tar.gz"
    local v2_tarball="${TMP_BASE}/v2-toolkit.tar.gz"

    # Build v1 with skills: toolkit-setup, zoom-out
    local v1_staging="${TMP_BASE}/v1-staging"
    mkdir -p "${v1_staging}"/{.autopilot,skills,principles}
    echo "v1.0.0" > "${v1_staging}/.autopilot/.version"
    cat > "${v1_staging}/.autopilot/manifest.json" << 'JSONEOF'
{"version":"v1.0.0","skills":{"toolkit-setup":{"type":"agnostic"},"zoom-out":{"type":"agnostic"}}}
JSONEOF
    cp "${BOOTSTRAP_SCRIPT}" "${v1_staging}/.autopilot/bootstrap.sh"
    mkdir -p "$(dirname "${v1_staging}/.autopilot/install.sh")" && cp "${PROJECT_ROOT}/dist/install.sh" "${v1_staging}/.autopilot/install.sh"
    mkdir -p "${v1_staging}/skills/toolkit-setup"
    echo "# toolkit-setup" > "${v1_staging}/skills/toolkit-setup/SKILL.md"
    mkdir -p "${v1_staging}/skills/zoom-out"
    echo "# zoom-out" > "${v1_staging}/skills/zoom-out/SKILL.md"
    echo "# principles" > "${v1_staging}/principles/karpathy.md"
    tar -czf "${v1_tarball}" -C "${v1_staging}" .

    # Build v2 with skills: toolkit-setup, zoom-out, autopilot-implementer (one added)
    local v2_staging="${TMP_BASE}/v2-staging"
    mkdir -p "${v2_staging}"/{.autopilot,skills,principles}
    echo "v2.0.0" > "${v2_staging}/.autopilot/.version"
    cat > "${v2_staging}/.autopilot/manifest.json" << 'JSONEOF'
{"version":"v2.0.0","skills":{"toolkit-setup":{"type":"agnostic"},"zoom-out":{"type":"agnostic"},"autopilot-implementer":{"type":"coupled","variants":["reasonix","codex","kimi"],"codex_agent":true}}}
JSONEOF
    cp "${BOOTSTRAP_SCRIPT}" "${v2_staging}/.autopilot/bootstrap.sh"
    mkdir -p "$(dirname "${v2_staging}/.autopilot/install.sh")" && cp "${PROJECT_ROOT}/dist/install.sh" "${v2_staging}/.autopilot/install.sh"
    mkdir -p "${v2_staging}/skills/toolkit-setup"
    echo "# toolkit-setup v2" > "${v2_staging}/skills/toolkit-setup/SKILL.md"
    mkdir -p "${v2_staging}/skills/zoom-out"
    echo "# zoom-out v2" > "${v2_staging}/skills/zoom-out/SKILL.md"
    mkdir -p "${v2_staging}/skills/autopilot-implementer/codex"
    echo "# impl codex v2" > "${v2_staging}/skills/autopilot-implementer/codex/SKILL.md"
    echo "# principles v2" > "${v2_staging}/principles/karpathy.md"
    tar -czf "${v2_tarball}" -C "${v2_staging}" .

    local install_sh="${TMP_BASE}/install.sh"
    cp "${PROJECT_ROOT}/dist/install.sh" "${install_sh}"
    chmod +x "${install_sh}"

    # Install v1
    HOME="${home}" AGENTS_SKILLS_DIR="${skills_dir}" \
        bash "${install_sh}" --tarball "${v1_tarball}" --version "v1.0.0" > /dev/null 2>&1

    # Add user skill
    mkdir -p "${skills_dir}/defuddle"
    echo "# user skill" > "${skills_dir}/defuddle/SKILL.md"
    echo "user config" > "${skills_dir}/defuddle/config.txt"
    assert_dir "user skill defuddle/ created" "${skills_dir}/defuddle"

    # Also add another user skill
    mkdir -p "${skills_dir}/obsidian-bases"
    echo "# obsidian bases" > "${skills_dir}/obsidian-bases/SKILL.md"
    assert_dir "user skill obsidian-bases/ created" "${skills_dir}/obsidian-bases"

    # Install v2 (upgrade)
    HOME="${home}" AGENTS_SKILLS_DIR="${skills_dir}" \
        bash "${install_sh}" --tarball "${v2_tarball}" --version "v2.0.0" > /dev/null 2>&1

    # User skills survive
    assert_dir "user skill defuddle/ survives upgrade" "${skills_dir}/defuddle"
    assert_file "defuddle/SKILL.md intact" "${skills_dir}/defuddle/SKILL.md"
    assert_file "defuddle/config.txt intact" "${skills_dir}/defuddle/config.txt"

    assert_dir "user skill obsidian-bases/ survives upgrade" "${skills_dir}/obsidian-bases"
    assert_file "obsidian-bases/SKILL.md intact" "${skills_dir}/obsidian-bases/SKILL.md"

    # V2 skills present
    assert_dir "v2: toolkit-setup present" "${skills_dir}/toolkit-setup"
    assert_dir "v2: zoom-out present" "${skills_dir}/zoom-out"
    assert_dir "v2: autopilot-implementer present" "${skills_dir}/autopilot-implementer"

    # Version updated
    assert_eq "version updated to v2" "v2.0.0" "$(cat "${skills_dir}/.autopilot/.version")"
}

# ── Test: full upgrade cycle (AC #7) ─────────────────────────────────────

test_full_upgrade_cycle() {
    echo ""
    echo "=== test: full upgrade cycle — v1→v2 with user skill ==="

    TMP_BASE="$(mktemp -d)"
    local home="${TMP_BASE}/home"
    local skills_dir="${home}/.agents/skills"
    local codex_dir="${home}/.codex"
    local codex_skills="${codex_dir}/skills"
    mkdir -p "${codex_dir}"

    local v1_tarball="${TMP_BASE}/v1-toolkit.tar.gz"
    local v2_tarball="${TMP_BASE}/v2-toolkit.tar.gz"

    # Build v1: skills: toolkit-setup, zoom-out, old-skill-only
    local v1_staging="${TMP_BASE}/v1-staging"
    mkdir -p "${v1_staging}"/{.autopilot,skills,principles}
    echo "v1.0.0" > "${v1_staging}/.autopilot/.version"
    cat > "${v1_staging}/.autopilot/manifest.json" << 'JSONEOF'
{"version":"v1.0.0","skills":{"toolkit-setup":{"type":"agnostic"},"zoom-out":{"type":"agnostic"},"old-skill-only":{"type":"agnostic"}}}
JSONEOF
    cp "${BOOTSTRAP_SCRIPT}" "${v1_staging}/.autopilot/bootstrap.sh"
    mkdir -p "$(dirname "${v1_staging}/.autopilot/install.sh")" && cp "${PROJECT_ROOT}/dist/install.sh" "${v1_staging}/.autopilot/install.sh"
    mkdir -p "${v1_staging}/skills/toolkit-setup"
    echo "# toolkit-setup v1" > "${v1_staging}/skills/toolkit-setup/SKILL.md"
    mkdir -p "${v1_staging}/skills/zoom-out"
    echo "# zoom-out v1" > "${v1_staging}/skills/zoom-out/SKILL.md"
    mkdir -p "${v1_staging}/skills/old-skill-only"
    echo "# old skill" > "${v1_staging}/skills/old-skill-only/SKILL.md"
    echo "# principles v1" > "${v1_staging}/principles/karpathy.md"
    tar -czf "${v1_tarball}" -C "${v1_staging}" .

    # Build v2: skills: toolkit-setup, zoom-out, autopilot-implementer (old-skill-only REMOVED)
    local v2_staging="${TMP_BASE}/v2-staging"
    mkdir -p "${v2_staging}"/{.autopilot,skills,principles}
    echo "v2.0.0" > "${v2_staging}/.autopilot/.version"
    cat > "${v2_staging}/.autopilot/manifest.json" << 'JSONEOF'
{"version":"v2.0.0","skills":{"toolkit-setup":{"type":"agnostic"},"zoom-out":{"type":"agnostic"},"autopilot-implementer":{"type":"coupled","variants":["codex"],"codex_agent":true}}}
JSONEOF
    cp "${BOOTSTRAP_SCRIPT}" "${v2_staging}/.autopilot/bootstrap.sh"
    mkdir -p "$(dirname "${v2_staging}/.autopilot/install.sh")" && cp "${PROJECT_ROOT}/dist/install.sh" "${v2_staging}/.autopilot/install.sh"
    mkdir -p "${v2_staging}/skills/toolkit-setup"
    echo "# toolkit-setup v2" > "${v2_staging}/skills/toolkit-setup/SKILL.md"
    mkdir -p "${v2_staging}/skills/zoom-out"
    echo "# zoom-out v2" > "${v2_staging}/skills/zoom-out/SKILL.md"
    mkdir -p "${v2_staging}/skills/autopilot-implementer/codex"
    echo "# impl v2" > "${v2_staging}/skills/autopilot-implementer/codex/SKILL.md"
    echo '[agent]' > "${v2_staging}/skills/autopilot-implementer/codex/agent.toml"
    echo "# principles v2" > "${v2_staging}/principles/karpathy.md"
    tar -czf "${v2_tarball}" -C "${v2_staging}" .

    local install_sh="${TMP_BASE}/install.sh"
    cp "${PROJECT_ROOT}/dist/install.sh" "${install_sh}"
    chmod +x "${install_sh}"

    # === Phase 1: Install v1 ===
    HOME="${home}" AGENTS_SKILLS_DIR="${skills_dir}" \
        CODEX_SKILLS_DIR="${codex_skills}" \
        bash "${install_sh}" --tarball "${v1_tarball}" --version "v1.0.0" > /dev/null 2>&1

    assert_eq "v1 installed" "v1.0.0" "$(cat "${skills_dir}/.autopilot/.version")"
    assert_dir "v1: toolkit-setup" "${skills_dir}/toolkit-setup"
    assert_dir "v1: zoom-out" "${skills_dir}/zoom-out"
    assert_dir "v1: old-skill-only" "${skills_dir}/old-skill-only"

    # Add user skill between versions
    mkdir -p "${skills_dir}/defuddle"
    echo "# defuddle skill" > "${skills_dir}/defuddle/SKILL.md"
    echo "data" > "${skills_dir}/defuddle/data.json"

    # === Phase 2: Upgrade to v2 ===
    local upgrade_output
    upgrade_output="$(HOME="${home}" AGENTS_SKILLS_DIR="${skills_dir}" \
        CODEX_SKILLS_DIR="${codex_skills}" \
        bash "${install_sh}" --tarball "${v2_tarball}" --version "v2.0.0" 2>&1)"

    echo "${upgrade_output}" | grep -q "Upgrading" && echo "  PASS: reports upgrading" && PASS=$((PASS + 1)) || { echo "  FAIL: should report upgrading"; FAIL=$((FAIL + 1)); }

    # AC: user skill present
    assert_dir "user skill defuddle/ present after upgrade" "${skills_dir}/defuddle"
    assert_file "defuddle/SKILL.md intact" "${skills_dir}/defuddle/SKILL.md"
    assert_file "defuddle/data.json intact" "${skills_dir}/defuddle/data.json"

    # AC: v1-only dirs gone
    assert_not_exists "v1-only old-skill-only removed" "${skills_dir}/old-skill-only"

    # AC: v2 dirs present
    assert_dir "v2: toolkit-setup present" "${skills_dir}/toolkit-setup"
    assert_dir "v2: zoom-out present" "${skills_dir}/zoom-out"
    assert_dir "v2: autopilot-implementer present" "${skills_dir}/autopilot-implementer"

    # AC: .version updated
    assert_eq "version updated to v2" "v2.0.0" "$(cat "${skills_dir}/.autopilot/.version")"

    # Verify codex bootstrap ran
    assert_symlink "codex: implementer symlinked" \
        "${codex_skills}/autopilot-implementer" \
        "${skills_dir}/autopilot-implementer/codex"
}

# ── Test: manifest edge cases ────────────────────────────────────────────

test_manifest_edge_cases() {
    echo ""
    echo "=== test: manifest edge cases — missing manifest, empty manifest ==="

    local install_sh
    install_sh="$(mktemp)"
    cp "${PROJECT_ROOT}/dist/install.sh" "${install_sh}"
    chmod +x "${install_sh}"

    # ── subtest: missing manifest (install on top of previous without manifest) ──

    echo "  --- subtest: missing manifest ---"

    TMP_BASE="$(mktemp -d)"
    local home="${TMP_BASE}/home"
    local skills_dir="${home}/.agents/skills"

    # Build a tarball with NO manifest.json
    local no_manifest_tarball="${TMP_BASE}/no-manifest.tar.gz"
    local nm_staging="${TMP_BASE}/nm-staging"
    mkdir -p "${nm_staging}"/{.autopilot,skills/toolkit-setup,principles}
    echo "v1.0.0" > "${nm_staging}/.autopilot/.version"
    cp "${BOOTSTRAP_SCRIPT}" "${nm_staging}/.autopilot/bootstrap.sh"
    mkdir -p "$(dirname "${nm_staging}/.autopilot/install.sh")" && cp "${PROJECT_ROOT}/dist/install.sh" "${nm_staging}/.autopilot/install.sh"
    echo "# toolkit" > "${nm_staging}/skills/toolkit-setup/SKILL.md"
    echo "# p" > "${nm_staging}/principles/karpathy.md"
    tar -czf "${no_manifest_tarball}" -C "${nm_staging}" .

    # Install first time
    HOME="${home}" AGENTS_SKILLS_DIR="${skills_dir}" \
        bash "${install_sh}" --tarball "${no_manifest_tarball}" --version "v1.0.0" > /dev/null 2>&1

    # Now build a second tarball with manifest (different version)
    local with_manifest_tarball="${TMP_BASE}/with-manifest.tar.gz"
    local wm_staging="${TMP_BASE}/wm-staging"
    mkdir -p "${wm_staging}"/{.autopilot,skills/toolkit-setup,skills/zoom-out,principles}
    echo "v2.0.0" > "${wm_staging}/.autopilot/.version"
    cat > "${wm_staging}/.autopilot/manifest.json" << 'JSONEOF'
{"version":"v2.0.0","skills":{"toolkit-setup":{"type":"agnostic"},"zoom-out":{"type":"agnostic"}}}
JSONEOF
    cp "${BOOTSTRAP_SCRIPT}" "${wm_staging}/.autopilot/bootstrap.sh"
    mkdir -p "$(dirname "${wm_staging}/.autopilot/install.sh")" && cp "${PROJECT_ROOT}/dist/install.sh" "${wm_staging}/.autopilot/install.sh"
    echo "# toolkit v2" > "${wm_staging}/skills/toolkit-setup/SKILL.md"
    mkdir -p "${wm_staging}/skills/zoom-out"
    echo "# zoom" > "${wm_staging}/skills/zoom-out/SKILL.md"
    echo "# p v2" > "${wm_staging}/principles/karpathy.md"
    tar -czf "${with_manifest_tarball}" -C "${wm_staging}" .

    # Upgrade: previous install had NO manifest → cleanup skipped → should still work
    if HOME="${home}" AGENTS_SKILLS_DIR="${skills_dir}" \
        bash "${install_sh}" --tarball "${with_manifest_tarball}" --version "v2.0.0" > /dev/null 2>&1; then
        echo "  PASS: upgrade with missing previous manifest succeeds"
        PASS=$((PASS + 1))
    else
        echo "  FAIL: upgrade with missing previous manifest should succeed"
        FAIL=$((FAIL + 1))
    fi

    assert_eq "version updated after missing-manifest upgrade" "v2.0.0" "$(cat "${skills_dir}/.autopilot/.version")"

    # ── subtest: empty manifest (manifest with no skills) ──

    echo "  --- subtest: empty manifest ---"

    TMP_BASE="$(mktemp -d)"
    home="${TMP_BASE}/home"
    skills_dir="${home}/.agents/skills"

    local empty_manifest_tarball="${TMP_BASE}/empty-manifest.tar.gz"
    local em_staging="${TMP_BASE}/em-staging"
    mkdir -p "${em_staging}"/{.autopilot,skills/toolkit-setup,principles}
    echo "v3.0.0" > "${em_staging}/.autopilot/.version"
    cat > "${em_staging}/.autopilot/manifest.json" << 'JSONEOF'
{"version":"v3.0.0","skills":{}}
JSONEOF
    cp "${BOOTSTRAP_SCRIPT}" "${em_staging}/.autopilot/bootstrap.sh"
    mkdir -p "$(dirname "${em_staging}/.autopilot/install.sh")" && cp "${PROJECT_ROOT}/dist/install.sh" "${em_staging}/.autopilot/install.sh"
    echo "# toolkit v3" > "${em_staging}/skills/toolkit-setup/SKILL.md"
    echo "# p v3" > "${em_staging}/principles/karpathy.md"
    tar -czf "${empty_manifest_tarball}" -C "${em_staging}" .

    # Fresh install with empty manifest should succeed
    if HOME="${home}" AGENTS_SKILLS_DIR="${skills_dir}" \
        bash "${install_sh}" --tarball "${empty_manifest_tarball}" --version "v3.0.0" > /dev/null 2>&1; then
        echo "  PASS: install with empty manifest succeeds"
        PASS=$((PASS + 1))
    else
        echo "  FAIL: install with empty manifest should succeed"
        FAIL=$((FAIL + 1))
    fi
}

# ── Run all tests ────────────────────────────────────────────────────────

if [[ ! -f "${FULL_TARBALL}" ]]; then
    echo "ERROR: Build tarball not found: ${FULL_TARBALL}"
    echo "Run 'rust-script deploy.rs pack' first."
    exit 1
fi

echo "install.sh integration tests"
echo "=============================="

test_fresh_install_extraction
test_version_override
test_already_installed_same_version
test_upgrade
test_autodetect_codex
test_autodetect_reasonix
test_autodetect_both
test_no_runtimes
test_upgrade_cleans_old_skills
test_full_tree_verification
test_user_skill_preservation
test_full_upgrade_cycle
test_manifest_edge_cases

echo ""
echo "=============================="
echo "Results: ${PASS} passed, ${FAIL} failed"
if [[ ${FAIL} -gt 0 ]]; then
    exit 1
fi
