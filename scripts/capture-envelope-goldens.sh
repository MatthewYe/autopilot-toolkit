#!/usr/bin/env bash
# Capture the command envelopes of the CURRENT director binary as golden
# fixtures (ticket 19, characterization step). Run against the binary built from
# the commit whose envelope contract must hold (during ticket 19: a frozen
# pre-refactor build), then byte-compare with tests/envelope_contract.rs.
#
# The committed goldens are stable across runs and temp dirs: none of them
# embeds a path, a hash, or a clock, and the revision is a pure function of the
# command sequence below. Re-running this script over an existing fixture set is
# the ONLY sanctioned way to change a golden, and it is a behavior change.
#
# EXCEPTION — `init`: it is captured (the differential below needs it) but
# excluded from the committed goldens, because its envelope embeds the temp
# worktree path and the captured HEAD sha and so is nondeterministic. When
# capturing into tests/fixtures/envelopes/, do not commit init.stdout, the
# *.stderr files, or MANIFEST.tsv: the fixture set is exactly the 23
# deterministic *.stdout goldens that envelope_contract.rs pins.
#
# usage: bash scripts/capture-envelope-goldens.sh <director-binary> <out-dir>
set -euo pipefail

BIN="${1:?usage: capture-envelope-goldens.sh <director-binary> <out-dir>}"
OUT="${2:?usage: capture-envelope-goldens.sh <director-binary> <out-dir>}"
BIN="$(cd "$(dirname "$BIN")" && pwd)/$(basename "$BIN")"

# Never let an empty or root-ish destination delete or litter the checkout: the
# first thing below is an `rm -rf`.
if [ -z "$OUT" ] || [ "$OUT" = "/" ] || [ "$OUT" = "." ]; then
  echo "refusing to capture into '$OUT'" >&2
  exit 2
fi

rm -rf "$OUT"
mkdir -p "$OUT"

WT="$(mktemp -d)"
trap 'rm -rf "$WT"' EXIT

git -C "$WT" init -q -b main
git -C "$WT" -c user.email=test@example.com -c user.name=Test commit -q --allow-empty -m init

cat >"$WT/good-envelope.txt" <<'ENVELOPE'
WORKER_REPORT:
{
  "status": "done",
  "branch": "codex/132-worker-report",
  "commits": [{ "sha": "abc1234", "subject": "feat(director-cli): envelope" }],
  "tests": [
    { "command": "cargo test -p director-cli", "outcome": "pass", "evidence": "71 passed" }
  ],
  "acceptance": [
    { "criterion": "envelope is validated", "evidence": "tests/dispatch.rs" }
  ],
  "blockers": []
}
ENVELOPE

MANIFEST="$OUT/MANIFEST.tsv"
: >"$MANIFEST"

# run <name> <expected-exit> <args...>
# Captures stdout bytes verbatim, and state.json when the command wrote one.
run() {
  local name="$1" expect="$2"
  shift 2
  local stdout_file="$OUT/$name.stdout"
  set +e
  "$BIN" "$@" --worktree "$WT" >"$stdout_file" 2>"$OUT/$name.stderr"
  local code=$?
  set -e
  if [ "$code" != "$expect" ]; then
    echo "FAIL $name: expected exit $expect, got $code" >&2
    cat "$OUT/$name.stderr" >&2
    exit 1
  fi
  local state="absent"
  if [ -f "$WT/.director/state.json" ]; then
    state="present"
  fi
  printf '%s\t%s\t%s\t%s\n' \
    "$name" "$expect" "$state" "$(shasum -a 256 "$stdout_file" | cut -d' ' -f1)" >>"$MANIFEST"
  echo "captured $name (exit $expect, state $state, $(wc -c <"$stdout_file" | tr -d ' ') bytes)"
}

run init 0 init --spec-issue 128 --slug autopilot-director

# ── run + ticket registration ──
run ticket-add 0 ticket add --ticket 200 --title "demo ticket"
run run-transition 0 run transition --to running
run ticket-implementing 0 ticket transition --ticket 200 --to implementing

# ── dispatch layer: begin → validate → finish ok ──
run dispatch-begin 0 dispatch begin --ticket 200 --worker worker-a
run report-validate 0 report validate --ticket 200 --file "$WT/good-envelope.txt"
run dispatch-finish-ok 0 dispatch finish --ticket 200 --outcome ok

# ── dispatch layer: begin → finish failed (sanctioned retry) ──
run dispatch-begin-2 0 dispatch begin --ticket 200 --worker worker-a
run dispatch-finish-failed 0 dispatch finish --ticket 200 --outcome failed --reason "cargo cannot run offline"

# ── ticket layer: gating → round-open → finding → close → done ──
run ticket-gating 0 ticket transition --ticket 200 --to gating
run round-open-ticket 0 round open --ticket 200
run finding-record 0 finding record --ticket 200 --round 1 --axis standards \
  --id f1 --hash hash-f1 --summary "duplicated load path"
run finding-dispose 0 finding dispose --ticket 200 --round 1 --id f1 --fixed abc1234
run round-close-ticket 0 round close --ticket 200 --round 1
run gate-ticket-zero 0 gate --ticket 200
run ticket-done 0 ticket transition --ticket 200 --to done

# ── second ticket: add → full cycle to done → run spec-gating ──
run ticket-add-201 0 ticket add --ticket 201 --title "second ticket"
run ticket-201-implementing 0 ticket transition --ticket 201 --to implementing
run ticket-201-gating 0 ticket transition --ticket 201 --to gating
run round-open-201 0 round open --ticket 201
run round-close-201 0 round close --ticket 201 --round 1
run ticket-201-done 0 ticket transition --ticket 201 --to done
run run-spec-gating 0 run transition --to spec-gating

# ── spec layer: round-open --spec ──
run round-open-spec 0 round open --spec

echo
echo "manifest: $MANIFEST"
cat "$MANIFEST"
