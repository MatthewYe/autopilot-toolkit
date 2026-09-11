#!/usr/bin/env bash
# Run rust-script with its caches redirected into a writable temp dir.
#
# Sandboxed agent sessions keep HOME's cache directories read-only, so
# rust-script fails with "Operation not permitted (os error 1)" when it tries
# to write its compiled-binary cache under ~/Library/Caches. This wrapper
# points HOME and CARGO_HOME at a persistent writable directory, reuses the
# real cargo registry read-only via a symlink, and runs cargo offline.
#
# Usage: bash scripts/sandboxed-rust-script.sh <rust-script args...>
set -euo pipefail

run_home="${RUST_SCRIPT_SANDBOX_HOME:-${TMPDIR:-/tmp}/rust-script-sandbox-home}"
real_cargo="${CARGO_HOME:-$HOME/.cargo}"

mkdir -p "$run_home/.cargo"
if [[ -f "$real_cargo/config.toml" && ! -e "$run_home/.cargo/config.toml" ]]; then
  cp "$real_cargo/config.toml" "$run_home/.cargo/config.toml"
fi
if [[ -d "$real_cargo/registry" && ! -e "$run_home/.cargo/registry" ]]; then
  ln -sfn "$real_cargo/registry" "$run_home/.cargo/registry"
fi

export HOME="$run_home"
export CARGO_HOME="$run_home/.cargo"
export CARGO_NET_OFFLINE="${CARGO_NET_OFFLINE:-true}"

exec rust-script "$@"
