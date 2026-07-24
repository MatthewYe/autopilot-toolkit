#!/usr/bin/env bash
set -euo pipefail
shopt -s nullglob

# Env var overrides for testability
SSOT="${AGENTS_SKILLS_DIR:-${HOME}/.agents/skills}"

usage() {
    cat <<EOF
Usage: bootstrap.sh --target <runtime>

Create symlinks from ~/.agents/skills/ into agent-exclusive skill directories.

Arguments:
  --target reasonix   Bootstrap for Reasonix (~/.reasonix/skills/)
  --target codex      Bootstrap for Codex (~/.codex/skills/ + agents)

Behavior:
  - Scans ~/.agents/skills/ for directories containing <runtime>/SKILL.md
  - Creates ln -sf from SSOT/<name>/<runtime> to ~/.<runtime>/skills/<name>
  - For codex: also deploys agent.toml files to ~/.codex/agents/
  - Removes stale bootstrap symlinks not matching current SSOT state
  - Always idempotent: existing correct symlinks are skipped
EOF
    exit 1
}

# ── parse args ───────────────────────────────────────────────────────────

TARGET=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --target)
            shift
            TARGET="$1"
            ;;
        --help|-h)
            usage
            ;;
        *)
            echo "ERROR: unknown argument: $1"
            usage
            ;;
    esac
    shift
done

if [[ -z "${TARGET}" ]]; then
    echo "ERROR: --target is required (reasonix or codex)"
    usage
fi

if [[ "${TARGET}" != "reasonix" && "${TARGET}" != "codex" ]]; then
    echo "ERROR: unknown target '${TARGET}'. Expected reasonix or codex"
    usage
fi

# ── resolve paths ────────────────────────────────────────────────────────

if [[ "${TARGET}" == "reasonix" ]]; then
    TARGET_SKILLS_DIR="${REASONIX_SKILLS_DIR:-${HOME}/.reasonix/skills}"
elif [[ "${TARGET}" == "codex" ]]; then
    TARGET_SKILLS_DIR="${CODEX_SKILLS_DIR:-${HOME}/.codex/skills}"
fi

if [[ "${TARGET}" == "codex" ]]; then
    TARGET_AGENTS_DIR="${CODEX_AGENTS_DIR:-${HOME}/.codex/agents}"
else
    TARGET_AGENTS_DIR=""  # unused for non-codex targets
fi

mkdir -p "${TARGET_SKILLS_DIR}"

# ── skill symlinks ───────────────────────────────────────────────────────

EXPECTED_LINKS=""  # newline-separated list of expected symlink names

if [[ ! -d "${SSOT}" ]]; then
    echo "SSOT directory ${SSOT} does not exist. Nothing to bootstrap."
    exit 0
fi

for skill_dir in "${SSOT}"/*/; do
    skill_name="$(basename "${skill_dir}")"
    variant_dir="${skill_dir}${TARGET}"

    # Check if this skill has a variant for the target runtime
    if [[ -f "${variant_dir}/SKILL.md" ]]; then
        link_path="${TARGET_SKILLS_DIR}/${skill_name}"

        # Remove broken or wrong symlinks
        if [[ -L "${link_path}" ]]; then
            existing_target="$(readlink "${link_path}")"
            if [[ "${existing_target}" != "${variant_dir}" ]]; then
                rm -f "${link_path}"
            fi
        elif [[ -e "${link_path}" ]]; then
            # Real directory — skip (don't overwrite user data)
            echo "  WARNING: ${link_path} exists as a real directory, skipping"
            EXPECTED_LINKS="${EXPECTED_LINKS}${skill_name}"$'\n'
            continue
        fi

        if [[ ! -e "${link_path}" ]]; then
            ln -sf "${variant_dir}" "${link_path}"
        fi
        EXPECTED_LINKS="${EXPECTED_LINKS}${skill_name}"$'\n'
    fi
done

# ── codex agent.toml deployment ──────────────────────────────────────────

if [[ "${TARGET}" == "codex" ]]; then
    EXPECTED_AGENTS=""  # newline-separated list of expected agent.toml names
    mkdir -p "${TARGET_AGENTS_DIR}"

    for skill_dir in "${SSOT}"/*/; do
        skill_name="$(basename "${skill_dir}")"
        agent_toml="${skill_dir}codex/agent.toml"

        if [[ -f "${agent_toml}" ]]; then
            agent_link="${TARGET_AGENTS_DIR}/${skill_name}.toml"
            EXPECTED_AGENTS="${EXPECTED_AGENTS}${skill_name}.toml"$'\n'

            if [[ -L "${agent_link}" ]]; then
                existing_target="$(readlink "${agent_link}")"
                if [[ "${existing_target}" != "${agent_toml}" ]]; then
                    rm -f "${agent_link}"
                fi
            elif [[ -e "${agent_link}" ]]; then
                echo "  WARNING: ${agent_link} exists as a real file, skipping"
                continue
            fi

            if [[ ! -e "${agent_link}" ]]; then
                ln -sf "${agent_toml}" "${agent_link}"
            fi
        fi
    done

    # Clean up stale agent.toml symlinks
    for agent_file in "${TARGET_AGENTS_DIR}"/*.toml; do
        agent_name="$(basename "${agent_file}")"
        if ! echo "${EXPECTED_AGENTS}" | grep -qxF "${agent_name}"; then
            if [[ -L "${agent_file}" ]]; then
                echo "  Removing stale agent symlink: ${agent_file}"
                rm -f "${agent_file}"
            fi
        fi
    done
fi

# ── remove stale symlinks ────────────────────────────────────────────────

for existing_link in "${TARGET_SKILLS_DIR}"/*; do
    # Match real entries and broken symlinks (broken symlinks fail -e but pass -L)
    [[ -e "${existing_link}" || -L "${existing_link}" ]] || continue
    link_name="$(basename "${existing_link}")"
    if ! echo "${EXPECTED_LINKS}" | grep -qxF "${link_name}"; then
        if [[ -L "${existing_link}" ]]; then
            echo "  Removing stale symlink: ${existing_link}"
            rm -f "${existing_link}"
        fi
    fi
done

echo "Bootstrap complete for target: ${TARGET}"
