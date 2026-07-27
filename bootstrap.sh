#!/usr/bin/env bash
set -euo pipefail
shopt -s nullglob

# Env var overrides for testability
SSOT="${AGENTS_SKILLS_DIR:-${HOME}/.agents/skills}"

usage() {
    cat <<EOF
Usage: bootstrap.sh --target <runtime>

Clean legacy runtime skill links and deploy runtime-specific support files.

Arguments:
  --target reasonix   Clean legacy Reasonix skill links
  --target codex      Clean legacy Codex skill links and deploy agents
  --target opencode   Clean legacy OpenCode skill links and deploy agents

Behavior:
  - Shared router SKILL.md files remain the only discoverable skill entries
  - Removes legacy runtime skill symlinks that point into the shared SSOT
  - For Codex: deploys runtime/codex/agent.toml to ~/.codex/agents/
  - For OpenCode: deploys runtime/opencode/agent.md to ~/.opencode/skills/
  - Preserves unrelated user directories and symlinks
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
    echo "ERROR: --target is required (reasonix, codex, or opencode)"
    usage
fi

if [[ "${TARGET}" != "reasonix" && "${TARGET}" != "codex" && "${TARGET}" != "opencode" ]]; then
    echo "ERROR: unknown target '${TARGET}'. Expected reasonix, codex, or opencode"
    usage
fi

# ── resolve paths ────────────────────────────────────────────────────────

if [[ "${TARGET}" == "reasonix" ]]; then
    TARGET_SKILLS_DIR="${REASONIX_SKILLS_DIR:-${HOME}/.reasonix/skills}"
elif [[ "${TARGET}" == "codex" ]]; then
    TARGET_SKILLS_DIR="${CODEX_SKILLS_DIR:-${HOME}/.codex/skills}"
elif [[ "${TARGET}" == "opencode" ]]; then
    TARGET_SKILLS_DIR="${OPENCODE_SKILLS_DIR:-${HOME}/.opencode/skills}"
fi

if [[ "${TARGET}" == "codex" ]]; then
    TARGET_AGENTS_DIR="${CODEX_AGENTS_DIR:-${HOME}/.codex/agents}"
elif [[ "${TARGET}" == "opencode" ]]; then
    TARGET_AGENTS_DIR="${OPENCODE_AGENTS_DIR:-${HOME}/.opencode/agents}"
else
    TARGET_AGENTS_DIR=""  # unused for non-agent targets
fi

mkdir -p "${TARGET_SKILLS_DIR}"

if [[ ! -d "${SSOT}" ]]; then
    echo "SSOT directory ${SSOT} does not exist. Nothing to bootstrap."
    exit 0
fi

# Runtime-coupled skills now expose one router SKILL.md from the shared
# directory. Remove old toolkit-owned runtime links so each runtime discovers
# that shared entry exactly once. Preserve unrelated user symlinks.
for existing_link in "${TARGET_SKILLS_DIR}"/*; do
    [[ -L "${existing_link}" ]] || continue
    link_name="$(basename "${existing_link}")"
    existing_target="$(readlink "${existing_link}")"
    case "${existing_target}" in
        "${SSOT}/${link_name}/"*)
            if [[ -d "${SSOT}/${link_name}/runtime" ]]; then
                echo "  Removing legacy runtime skill symlink: ${existing_link}"
                rm -f "${existing_link}"
            fi
            ;;
    esac
done

# ── codex agent.toml deployment ──────────────────────────────────────────

if [[ "${TARGET}" == "codex" ]]; then
    EXPECTED_AGENTS=""  # newline-separated list of expected agent.toml names
    mkdir -p "${TARGET_AGENTS_DIR}"

    for skill_dir in "${SSOT}"/*/; do
        skill_name="$(basename "${skill_dir}")"
        agent_toml="${skill_dir}runtime/codex/agent.toml"

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
                existing_target="$(readlink "${agent_file}")"
                case "${existing_target}" in
                    "${SSOT}"/*)
                        echo "  Removing stale agent symlink: ${agent_file}"
                        rm -f "${agent_file}"
                        ;;
                esac
            fi
        fi
    done
fi

# ── opencode agent.md deployment ──────────────────────────────────────────

if [[ "${TARGET}" == "opencode" ]]; then
    EXPECTED_AGENTS=""  # newline-separated list of expected agent.md names
    mkdir -p "${TARGET_SKILLS_DIR}"

    for skill_dir in "${SSOT}"/*/; do
        skill_name="$(basename "${skill_dir}")"
        agent_md="${skill_dir}runtime/opencode/agent.md"

        if [[ -f "${agent_md}" ]]; then
            agent_link="${TARGET_SKILLS_DIR}/${skill_name}.md"
            EXPECTED_AGENTS="${EXPECTED_AGENTS}${skill_name}.md"$'\n'

            if [[ -L "${agent_link}" ]]; then
                existing_target="$(readlink "${agent_link}")"
                if [[ "${existing_target}" != "${agent_md}" ]]; then
                    rm -f "${agent_link}"
                fi
            elif [[ -e "${agent_link}" ]]; then
                echo "  WARNING: ${agent_link} exists as a real file, skipping"
                continue
            fi

            if [[ ! -e "${agent_link}" ]]; then
                ln -sf "${agent_md}" "${agent_link}"
            fi
        fi
    done

    # Clean up stale agent.md symlinks
    for agent_file in "${TARGET_SKILLS_DIR}"/*.md; do
        agent_name="$(basename "${agent_file}")"
        if ! echo "${EXPECTED_AGENTS}" | grep -qxF "${agent_name}"; then
            if [[ -L "${agent_file}" ]]; then
                existing_target="$(readlink "${agent_file}")"
                case "${existing_target}" in
                    "${SSOT}"/*)
                        echo "  Removing stale agent symlink: ${agent_file}"
                        rm -f "${agent_file}"
                        ;;
                esac
            fi
        fi
    done
fi

echo "Bootstrap complete for target: ${TARGET}"
