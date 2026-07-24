# Issue #62 Kimi Distill Smoke Evidence

Status: completed with bounded native-session Kimi prompt-mode smoke.

Machine-readable evidence is archived in `docs/reports/62-kimi-distill-smoke-evidence.json`.

## Runtime Discovery

- `command -v kimi` produced no output, so `kimi` is not in PATH for this worktree shell.
- Repository/runtime-native discovery found `~/.kimi-code/bin/kimi`.
- `~/.kimi-code/bin/kimi --version` (`/Users/xlchen/.kimi-code/bin/kimi --version`) returned `0.29.0`.
- `~/.kimi-code/bin/kimi --help` confirmed non-interactive prompt mode via `-p, --prompt <prompt>`, session resume via `--session`, and custom skill loading via `--skills-dir`.
- `~/.kimi-code/session_index.jsonl` exists and contains Kimi session records with `sessionId`, `sessionDir`, and `workDir`.

## Native Session Proof

The retry smoke did not set or use `KIMI_SESSION_ID`.

Kimi matched exactly one native current session by reading `~/.kimi-code/session_index.jsonl` and the indexed `state.json` files:

- native session id: `session_d03aeda7-b1a1-4c89-8cfb-0cc78213531d`
- session directory hash: `6f7e0e501ad1588986d473646d483cf4272ec39e58211bd7e280258d1a345673`
- `state.json` hash: `4dd7abc55d021637a9b9b4b5f3aedbcf4f985ee34d5c91b35b5c81fc6859b43b`
- match rule: fixture `workDir`, `lastPrompt` marker `ISSUE62_NATIVE_SMOKE_R1_20260724`, and `agents.main.homedir` inside the matched `sessionDir`
- match count: 1

The `session_dir_sha256` value is a SHA-256 of the exact native session directory path string recorded in the JSON evidence, not a hash of directory contents.

## Live Kimi Smoke

Kimi used its native session id as the Distill `--session-id` and executed `distill start --json --runtime kimi` in a local-markdown fixture. Main run:

- run_id: `run-sessiond03aeda7-b1a1-4c8-1784897249926`
- runtime/session: `kimi` / `session_d03aeda7-b1a1-4c89-8cfb-0cc78213531d`
- final status: completed
- terminal stage: completed
- final revision: 4
- session release: released at revision 4
- report paths: `.distill/runs/run-sessiond03aeda7-b1a1-4c8-1784897249926/report.json` and `.distill/runs/run-sessiond03aeda7-b1a1-4c8-1784897249926/report.md`
- canonical report hash: `5cd6d78020905668528a379fde2aab248b213f1be884abfa021f09907351e1b0`
- PRD payload hash: `0dfce2f76e4baea31ddca9f60cc8f926eb1e5ff057eb9fb841a08d9258e1273a`
- issue payload hashes: `aceda22295a44e3ebc3fc7eaed9210f156ba1f5e7fb7683b24860793261d56cc`, `49664ec0253c0d80573004dd978b7366a1729571f140c0ba4a7381167fa43c88`

The run advanced only through the shared runner stages:

- intake: `explicit-text-captured`
- clarification: `clarification-complete`
- PRD: `testing-seam-confirmed`
- issues: `slice-breakdown-approved`

Every runner yield included the shared interface fields: `run_id`, `stage`, `revision`, `next_action`, and `authorized_action` or terminal report paths.

## Lifecycle Smoke

A second Kimi-owned lifecycle run exercised the session guards:

- lifecycle run_id: `run-sessiond03aeda7-b1a1-4c8-1784897346858`
- same-session resume: repeated `start --json --runtime kimi` with the native session returned the same unfinished run at revision 1 and stage `clarification`.
- cross-session rejection: `submit-evidence` with `session_00000000-0000-4000-8000-000000000062` failed with `ERROR: session id does not match active run binding`.
- explicit user-authorized takeover: `takeover --from-session session_d03aeda7-b1a1-4c89-8cfb-0cc78213531d --to-session session_00000000-0000-4000-8000-000000000063 --expected-revision 1 --user-authorized` succeeded at revision 2.
- stale-revision recovery: a valid `submit-evidence` with `--expected-revision 1` after takeover failed with `ERROR: expected revision 1 is stale; current revision is 2`; `inspect` recovered revision 2 and stage `clarification`.
- cleanup: `purge` returned status `purged`, stage `purged`, and revision 2.

## Archive Policy

The generated `.distill/` and `.scratch/` fixture state was not retained in the repository. This report and `62-kimi-distill-smoke-evidence.json` archive the bounded evidence needed for issue #62.
