#!/usr/bin/env bash
set -euo pipefail
shopt -s nullglob

# Env var overrides for testability
SSOT="${AGENTS_SKILLS_DIR:-${HOME}/.agents/skills}"

# ── helpers ────────────────────────────────────────────────────────────────

deploy_flat_link() {
    local link="$1" src="$2" ssot="$3"
    if [[ -f "${link}" && ! -L "${link}" ]]; then
        echo "  Skipping ${link}: user-owned real file"
        return
    fi
    if [[ -L "${link}" ]]; then
        existing="$(readlink "${link}")"
        if [[ "${existing}" != "${src}" ]]; then
            rm -f "${link}"
        fi
    fi
    if [[ ! -e "${link}" ]]; then
        ln -sf "${src}" "${link}"
    fi
}

# ── write_command_wrapper ───────────────────────────────────────────────────
# Generate a lightweight opencode command .md with frontmatter from the
# canonical skill, then instruct the model to read the full skill body.
write_command_wrapper() {
    local out="$1" name="$2" src="$3"

    # Extract name + description from source frontmatter
    local fm_name="" fm_desc=""
    local in_fm=0
    while IFS= read -r line; do
        [[ "${line}" == "---" ]] && { in_fm=$((in_fm + 1)); continue; }
        [[ ${in_fm} -ge 2 ]] && break
        [[ ${in_fm} -eq 1 ]] || continue
        case "${line}" in
            "name:"*)   fm_name="${line#name: }"; fm_name="${fm_name#\"}"; fm_name="${fm_name%\"}" ;;
            "description:"*) fm_desc="${line#description: }"; fm_desc="${fm_desc#\"}"; fm_desc="${fm_desc%\"}" ;;
        esac
    done < "${src}"

    [[ -z "${fm_name}" ]] && fm_name="${name}"
    [[ -z "${fm_desc}" ]] && fm_desc="See ${SSOT}/${name}/SKILL.md for details"

    local ssot_path="${SSOT}/${name}"
    # Determine the canonical source — prefer opencode-specific instructions
    local read_path="${ssot_path}"
    if [[ -f "${ssot_path}/runtime/opencode/INSTRUCTIONS.md" ]]; then
        read_path="${ssot_path}/runtime/opencode/INSTRUCTIONS.md"
    elif [[ -f "${ssot_path}/SKILL.md" ]]; then
        read_path="${ssot_path}/SKILL.md"
    fi

    cat > "${out}" <<WRAPPER
---
name: ${fm_name}
description: ${fm_desc}
---

You are running the **${fm_name}** skill.

1. Read the full skill definition from \`${read_path}\`.
2. Follow only that file's instructions. Do not load another runtime's variant.
WRAPPER
}

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
  - For OpenCode: deploys runtime/opencode/agent.md to ~/.opencode/agents/;
      all skills as flat .md to ~/.opencode/skills/
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

# ── opencode agent + command deployment ────────────────────────────────────
# OpenCode: subagents → ~/.opencode/agents/<name>.md (symlinked)
#           commands   → ~/.opencode/commands/<name>.md (wrapped)

if [[ "${TARGET}" == "opencode" ]]; then
    OPC_AGENTS_DIR="${OPENCODE_AGENTS_DIR:-${HOME}/.opencode/agents}"
    OPC_COMMANDS_DIR="${HOME}/.opencode/commands"
    EXPECTED_AGENT_MD=""
    EXPECTED_CMD_MD=""

    for skill_dir in "${SSOT}"/*/; do
        skill_name="$(basename "${skill_dir}")"
        agent_src="${skill_dir}runtime/opencode/agent.md"
        instr_src="${skill_dir}runtime/opencode/INSTRUCTIONS.md"
        skill_src="${skill_dir}SKILL.md"

        if [[ -f "${agent_src}" ]]; then
            # ── subagent → ~/.opencode/agents/<name>.md ──
            mkdir -p "${OPC_AGENTS_DIR}"
            agent_link="${OPC_AGENTS_DIR}/${skill_name}.md"
            EXPECTED_AGENT_MD="${EXPECTED_AGENT_MD}${skill_name}.md"$'\n'
            deploy_flat_link "${agent_link}" "${agent_src}" "${SSOT}"
        fi

        # ── command wrapper → ~/.opencode/commands/<name>.md ──
        if [[ -f "${instr_src}" && ! -f "${agent_src}" ]]; then
            cmd_src="${instr_src}"
        elif [[ -f "${skill_src}" && ! -f "${agent_src}" ]]; then
            cmd_src="${skill_src}"
        else
            cmd_src=""
        fi
        if [[ -n "${cmd_src}" ]]; then
            mkdir -p "${OPC_COMMANDS_DIR}"
            cmd_link="${OPC_COMMANDS_DIR}/${skill_name}.md"
            EXPECTED_CMD_MD="${EXPECTED_CMD_MD}${skill_name}.md"$'\n'
            # Generate wrapper: extract name + description from source, then
            # instruct the model to read the full skill body from SSOT.
            write_command_wrapper "${cmd_link}" "${skill_name}" "${cmd_src}"
        fi
    done

    # Clean up stale agent files
    if [[ -d "${OPC_AGENTS_DIR}" ]]; then
        for af in "${OPC_AGENTS_DIR}"/*.md; do
            [[ -f "${af}" ]] || continue
            an="$(basename "${af}")"
            if ! echo "${EXPECTED_AGENT_MD}" | grep -qxF "${an}"; then
                if [[ -L "${af}" ]]; then
                    rt="$(readlink "${af}")"
                    case "${rt}" in "${SSOT}"/*) rm -f "${af}";; esac
                fi
            fi
        done
    fi

    # Clean up stale command wrappers
    if [[ -d "${OPC_COMMANDS_DIR}" ]]; then
        for cf in "${OPC_COMMANDS_DIR}"/*.md; do
            [[ -f "${cf}" ]] || continue
            cn="$(basename "${cf}")"
            if ! echo "${EXPECTED_CMD_MD}" | grep -qxF "${cn}"; then
                rm -f "${cf}"
            fi
        done
    fi
fi

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

echo "Bootstrap complete for target: ${TARGET}"
