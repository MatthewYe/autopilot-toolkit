#!/usr/bin/env bash
# Refresh rust-script's compiled-binary cache for this repo's scripts.
#
# rust-script only discards a cached binary when the script file itself
# changes; changes in a path dependency (`crates/*`) never invalidate it, so
# local runs can silently execute stale code (upstream rust-script#122).
# Run this after editing anything under crates/ before running the suites.
# It is cheap (~50 ms): the next invocation recompiles and cargo reuses the
# shared target dir.
set -euo pipefail

cache_dirs=(
  "${HOME}/Library/Caches/rust-script/binaries"             # macOS
  "${XDG_CACHE_HOME:-${HOME}/.cache}/rust-script/binaries"  # Linux
)

# One entry per rust-script file in this repo (cache name = file stem).
# Add new rust-script files here.
names=(
  deploy check run env-check sync-upstream
  test_build test_install test_toolkit_setup test_check test_github_verify test_kimi_distill
)

removed=0
for dir in "${cache_dirs[@]}"; do
  [[ -d "$dir" ]] || continue
  for name in "${names[@]}"; do
    while IFS= read -r -d '' file; do
      rm -f "$file"
      removed=$((removed + 1))
    done < <(find "$dir" -type f -name "${name}_*" -print0)
  done
done

echo "rust-script cache refreshed: ${removed} file(s) removed."
