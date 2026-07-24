#!/usr/bin/env bash
# Uninstall autopilot-toolkit
# Run: bash ~/.agents/skills/.autopilot/uninstall.sh
set -euo pipefail

SKILLS_DIR="${AGENTS_SKILLS_DIR:-$HOME/.agents/skills}"
AUTOPILOT_DIR="${SKILLS_DIR}/.autopilot"
MANIFEST="${AUTOPILOT_DIR}/manifest.json"
REASONIX_SKILLS="${REASONIX_SKILLS_DIR:-$HOME/.reasonix/skills}"
CODEX_SKILLS="${CODEX_SKILLS_DIR:-$HOME/.codex/skills}"
CODEX_AGENTS="${CODEX_AGENTS_DIR:-$HOME/.codex/agents}"
PRINCIPLES_DIR="${AGENTS_PRINCIPLES_DIR:-$HOME/.agents/principles}"

echo "==> Uninstalling autopilot-toolkit..."

REMOVED=0

# 1. Remove manifest-listed skill directories
if [[ -f "${MANIFEST}" ]]; then
    echo "   Reading manifest..."
    # Extract skill names from manifest.json using python3 or jq
    if command -v python3 &>/dev/null; then
        skills=$(python3 -c "
import json, sys
with open('${MANIFEST}') as f:
    data = json.load(f)
for name in data.get('skills', {}):
    print(name)
" 2>/dev/null)
    elif command -v jq &>/dev/null; then
        skills=$(jq -r '.skills | keys[]' "${MANIFEST}" 2>/dev/null)
    else
        echo "   WARNING: neither python3 nor jq found — cannot read manifest"
        echo "   Remove ${SKILLS_DIR} manually to clean up"
        exit 1
    fi

    while IFS= read -r name; do
        [[ -z "${name}" ]] && continue
        dir="${SKILLS_DIR}/${name}"
        if [[ -d "${dir}" ]]; then
            rm -rf "${dir}"
            REMOVED=$((REMOVED + 1))
        fi
    done <<< "${skills}"
fi

# 2. Remove .autopilot/ metadata directory
if [[ -d "${AUTOPILOT_DIR}" ]]; then
    rm -rf "${AUTOPILOT_DIR}"
    REMOVED=$((REMOVED + 1))
fi

# 3. Remove bootstrap symlinks from agent-exclusive directories
cleanup_symlinks() {
    local dir="$1"
    [[ -d "${dir}" ]] || return
    for entry in "${dir}"/*; do
        if [[ -L "${entry}" ]]; then
            target="$(readlink "${entry}" 2>/dev/null || true)"
            if [[ "${target}" == "${SKILLS_DIR}"/* ]]; then
                rm -f "${entry}"
                REMOVED=$((REMOVED + 1))
            fi
        fi
    done
}

cleanup_symlinks "${REASONIX_SKILLS}"
cleanup_symlinks "${CODEX_SKILLS}"

# 4. Remove Codex agent.toml symlinks
if [[ -d "${CODEX_AGENTS}" ]]; then
    for entry in "${CODEX_AGENTS}"/*.toml; do
        [[ -f "${entry}" ]] || continue
        if [[ -L "${entry}" ]]; then
            target="$(readlink "${entry}" 2>/dev/null || true)"
            if [[ "${target}" == "${SKILLS_DIR}"/* ]]; then
                rm -f "${entry}"
                REMOVED=$((REMOVED + 1))
            fi
        fi
    done
fi

# 5. Remove principles (only if deployed by autopilot)
if [[ -f "${PRINCIPLES_DIR}/karpathy.md" ]]; then
    echo "   Removing principles..."
    rm -rf "${PRINCIPLES_DIR}"
    REMOVED=$((REMOVED + 1))
fi

echo "==> Done: ${REMOVED} items removed."
echo "   autopilot-toolkit has been uninstalled."
