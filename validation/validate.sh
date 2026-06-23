#!/usr/bin/env bash
# validation/validate.sh — source-able SKILL.md frontmatter validation library
#
# Usage: source this file, then call validate_skill "<content>" to get:
#   VALIDATE_PASSED=true|false
#   VALIDATE_ISSUES=()       # array of issue strings
#
# Also exports: parse_frontmatter "<content>" sets:
#   FM_NAME, FM_DESCRIPTION, FM_RUNAS, FM_ALLOWED_TOOLS, etc.
#   FM_PARSE_ERRORS=()        # array of parse error strings

# ── helpers ──

OPENCODE_FIELDS=(
  compatibility mode
  permission hidden arguments
)
NAME_REGEX='^[a-zA-Z0-9][a-zA-Z0-9._-]{0,63}$'

# ── parse_frontmatter ──

# Parse YAML-like frontmatter from SKILL.md content.
# Sets global associative vars for each key found, plus FM_PARSE_ERRORS.
parse_frontmatter() {
  local content="$1"
  FM_PARSE_ERRORS=()
  # Clear any previous parsed fields (all known + opencode fields)
  FM_NAME=""; FM_DESCRIPTION=""; FM_RUNAS=""; FM_ALLOWED_TOOLS=""
  for _f in "${OPENCODE_FIELDS[@]}"; do
    local _varname="FM_$(echo "$_f" | tr 'a-z-' 'A-Z_')"
    printf -v "$_varname" "%s" ""
  done

  # Split content into lines
  IFS=$'\n' read -rd '' -a lines <<<"$content" || true

  # Check opening delimiter (line 0)
  if [ "${lines[0]}" != "---" ] && [ "${lines[0]}" != $'---\r' ]; then
    FM_PARSE_ERRORS+=("Missing opening --- delimiter")
    return 1
  fi

  # Find closing delimiter
  local end_idx=-1
  local len=${#lines[@]}
  for ((i=1; i<len; i++)); do
    local trimmed="${lines[$i]}"$'\n'
    trimmed="${trimmed%$'\n'}"
    trimmed="${trimmed%"${trimmed##*[![:space:]]}"}"
    trimmed="${trimmed#"${trimmed%%[![:space:]]*}"}"
    if [ "$trimmed" = "---" ]; then
      end_idx=$i
      break
    fi
  done

  if [ $end_idx -eq -1 ]; then
    FM_PARSE_ERRORS+=("Missing closing --- delimiter")
    return 1
  fi

  # Parse key: value lines between delimiters
  local i=1
  while [ $i -lt $end_idx ]; do
    local line="${lines[$i]}"
    # Strip trailing \r for CRLF
    line="${line%$'\r'}"

    # Match key: value
    if [[ "$line" =~ ^([a-zA-Z][a-zA-Z0-9_-]*):[[:space:]]*(.*)$ ]]; then
      local key="${BASH_REMATCH[1]}"
      local value="${BASH_REMATCH[2]}"

      # Trim leading/trailing whitespace from value
      value="${value#"${value%%[![:space:]]*}"}"
      value="${value%"${value##*[![:space:]]}"}"

      # Handle folded block scalar (>)
      if [ "$value" = ">" ] || [ "$value" = ">-" ]; then
        value=""
        i=$((i + 1))
        while [ $i -lt $end_idx ]; do
          local cline="${lines[$i]}"
          cline="${cline%$'\r'}"
          if [[ "$cline" =~ ^[[:space:]]{2,} ]]; then
            local folded="${cline#"${cline%%[![:space:]]*}"}"
            [ -n "$value" ] && value="$value $folded" || value="$folded"
            i=$((i + 1))
          else
            break
          fi
        done
        i=$((i - 1))  # step back; loop will increment
      fi

      # Handle literal block scalar (|)
      if [ "$value" = "|" ] || [ "$value" = "|-" ]; then
        value=""
        i=$((i + 1))
        while [ $i -lt $end_idx ]; do
          local cline="${lines[$i]}"
          cline="${cline%$'\r'}"
          if [[ "$cline" =~ ^[[:space:]]{2,} ]]; then
            local lit="${cline#"${cline%%[![:space:]]*}"}"
            [ -n "$value" ] && value=$value$'\n'"$lit" || value="$lit"
            i=$((i + 1))
          else
            break
          fi
        done
        i=$((i - 1))
      fi

      # Store field (uppercase key, hyphens → underscores)
      local varname="FM_$(echo "$key" | tr 'a-z-' 'A-Z_')"
      printf -v "$varname" "%s" "$value"
    fi
    i=$((i + 1))
  done

  return 0
}

# ── validate_skill ──

# Validate frontmatter after parsing.
# Sets VALIDATE_PASSED (true/false) and VALIDATE_ISSUES (array).
validate_skill() {
  VALIDATE_ISSUES=()

  # Parse first
  if ! parse_frontmatter "$1"; then
    VALIDATE_ISSUES=("${FM_PARSE_ERRORS[@]}")
    VALIDATE_PASSED=false
    return 0
  fi

  # Check 1: Required fields
  if [ -z "$FM_NAME" ]; then
    VALIDATE_ISSUES+=("Missing required field: name")
  fi
  if [ -z "$FM_DESCRIPTION" ]; then
    VALIDATE_ISSUES+=("Missing required field: description")
  fi

  # Check 2: Name format
  if [ -n "$FM_NAME" ] && ! [[ "$FM_NAME" =~ $NAME_REGEX ]]; then
    VALIDATE_ISSUES+=("Name \"$FM_NAME\" does not match pattern ^[a-zA-Z0-9][a-zA-Z0-9._-]{0,63}$")
  fi

  # Check 3: No opencode fields
  for field in "${OPENCODE_FIELDS[@]}"; do
    local varname="FM_$(echo "$field" | tr 'a-z-' 'A-Z_')"
    if [ -n "${!varname:-}" ]; then
      VALIDATE_ISSUES+=("OpenCode-specific field present: $field")
    fi
  done

  # Check 4: runAs valid
  if [ -n "${FM_RUNAS:-}" ]; then
    if [ "$FM_RUNAS" != "inline" ] && [ "$FM_RUNAS" != "subagent" ]; then
      VALIDATE_ISSUES+=("Invalid runAs value \"$FM_RUNAS\" — must be \"inline\" or \"subagent\"")
    fi
  fi

  # Check 5: allowed-tools for subagents
  if [ "${FM_RUNAS:-}" = "subagent" ]; then
    if [ -z "${FM_ALLOWED_TOOLS:-}" ]; then
      VALIDATE_ISSUES+=("runAs is \"subagent\" but allowed-tools is not defined")
    fi
  fi

  if [ ${#VALIDATE_ISSUES[@]} -eq 0 ]; then
    VALIDATE_PASSED=true
  else
    VALIDATE_PASSED=false
  fi
}
