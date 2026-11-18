# Phase 2 replay (implementer)

This directory is the Phase 2 harness for the implementer proposal. It runs
the same ticket through two arms in isolated, history-free worktrees:

- **prose**: the repository's `autopilot-implementer/SKILL.md` only.
- **ontology**: the same skill plus the selected clause/context plan
  (`results/plan-T126.md`, advisory; prose stays authoritative).

The agent never sees the merged diff, PR body, or hidden checks. The worktree
is a `git archive` of the ticket's base commit with a fresh single-commit git
history, so `git log`, remotes, and `gh` cannot reveal the original solution.

## Pilot scope

Phase 2 in `DESIGN.md` asks for 5 tickets × 2 arms × 3 runs = 30 runs. This
pilot uses one ticket (`T126`, the contract-retirement ticket) and two runs per
arm (4 runs) to validate the harness and get a first signal. It is **not** the
full 30-run go/no-go experiment; the result is reported as a pilot.

Deviations from the registered protocol:

- model temperature is the DSH default, not explicitly 0;
- N=2 per arm, one ticket only;
- hidden checks are the repository's merged test suite plus one structural
  check for the retired entry-level field, not a full per-ticket hidden-test
  extraction;
- the report gate is checked structurally (required sections/status) rather
  than fed through the SHACL report gate.

## Layout

```
replay/T126/
  prose-1/{TASK.md,repo/}
  prose-2/{TASK.md,repo/}
  ontology-1/{TASK.md,ONTOLOGY_PLAN.md,repo/}
  ontology-2/{TASK.md,ONTOLOGY_PLAN.md,repo/}
  gold/repo/            # e3818a8, for check calibration only
```

`TASK.md` is the GitHub issue body, no PR metadata. `ONTOLOGY_PLAN.md` is the
deterministic plan from `scripts/render_plan.py`. Every plan excerpt is read from
the base commit `98270ef` with `git show`, not from the merged worktree; the
merged PR diff and post-merge tests are not part of the plan.

## Commands

```bash
# prepare the pilot worktrees (already run; refuses to replace existing runs
# unless --force is passed and verifies base + hidden-test commits first)
python3 scripts/prepare_replay.py

# evaluate one run (snapshot the agent diff/report, overlay hidden tests in a
# temporary copy, persist checks + structural result + report text). Serial
# workspace tests avoid the shared PROJECT_ROOT env race.
REPLAY_TEST_THREADS=1 python3 scripts/run_phase2_eval.py --runs prose-1

# structural hidden check for T126 (also run inside the command above)
python3 scripts/check_t126_contract.py replay/T126/prose-1/repo
```

Checks: `cargo test --workspace --no-fail-fast`,
`rust-script --test tests/test_toolkit_setup.rs`,
`rust-script scripts/check.rs`, `rust-script validation/run.rs`,
`cargo clippy --workspace --all-targets -- -D warnings`.

## Results

The prepared base includes every `corpus/issue-*.json` the plan names; the
preparer verifies the pointer is readable before dispatch and checks that the
plan provenance matches the requested base (`results/plan-provenance.json`).

T126 pilot, 2 runs per arm. Hidden checks are the merged test suite (inline test
modules overlaid from `e3818a8`, so the agent cannot pass by rewriting tests)
plus the structural entry-field check. The original agent diff (including new
files) and report are snapshotted before the overlay, and the overlay runs on a
temporary copy; the agent worktree is not modified.

| run | arm | `cargo test --workspace` | serial | structural | report | status |
|---|---|---|---|---|---|---|
| prose-1 | prose | 0 | — (parallel) | pass | pass | DONE |
| prose-2 | prose | 0 | — (parallel) | pass | pass | DONE |
| ontology-1 | ontology | 0 | 0 | pass | pass | DONE |
| ontology-2 | ontology | 0 | 0 | pass | pass | DONE |

All four also passed `toolkit_setup`, `scripts/check.rs`, `validation/run.rs`,
and `clippy --workspace --all-targets -- -D warnings` (all exit 0).

Prose runs used the default parallel test configuration. The ontology runs
used `--test-threads=1` after an earlier parallel ontology-1 run hit the known
`shared::tests::load_skill_lock_from_temp_project` `PROJECT_ROOT` env race
(exit 101), but the serial overlay run passed. This is a harness flake, not an
implementation difference.

Interpretation: the pilot validates the harness and produces a null result on
this ticket — both arms implement the contract, pass hidden tests, keep the
failure messages, and produce a conforming report. With N=2 and one
well-specified ticket, it cannot estimate a 10pp effect. The full 30-run
protocol was not run. Raw logs are in `results/phase2/`; agent diffs are saved
as `*-agent.diff` before the hidden-test overlay.

## Token cost

Measured from the DSH subagent session usage records (sum of every
`assistant/message.data.usage`). `totalTokens` includes cache reads, so it is a
processed-token count, not a billed-cost count. `non-cached` is
`inputTokens + outputTokens`.

| run | arm | model calls | input | output | cache read | total | non-cached |
|---|---|---:|---:|---:|---:|---:|---:|
| prose-1 | prose | 65 | 117,380 | 30,789 | 3,230,848 | 3,379,017 | 148,169 |
| prose-2 | prose | 51 | 93,324 | 20,775 | 2,718,848 | 2,832,947 | 114,099 |
| ontology-1 | ontology | 92 | 99,172 | 38,122 | 4,159,744 | 4,297,038 | 137,294 |
| ontology-2 | ontology | 75 | 221,614 | 44,485 | 3,796,224 | 4,062,323 | 266,099 |
| **prose avg** | | **58** | **105,352** | **25,782** | **2,974,848** | **3,105,982** | **131,134** |
| **ontology avg** | | **84** | **160,393** | **41,304** | **3,977,984** | **4,179,681** | **201,696** |

Ontology total processed tokens are higher (4.18M vs 3.11M, +34.6%), and
non-cached input+output is also higher (201,696 vs 131,134, +53.8%); output is
higher (41,304 vs 25,782, +60.2%). N=2 and the total ranges do not overlap
(prose max 3.38M < ontology min 4.06M), so this is not evidence of a token win. The plan is 6,767 characters (7,077 bytes) and is read once as a tool
result (~1.8k-2.0k tokens); it is not inlined into the system prompt. Raw
numbers: `results/phase2/tokens.json`.

Why the ontology arm costs more: the plan is only 6.7K characters, but it
points at full source files. Actual tool-read volume differed: prose-1 16 read
calls / 136K chars; prose-2 10 / 100K; ontology-1 11 / 88K plus 69 bash calls /
167K; ontology-2 25 / 229K. These tool results stay in the conversation and are
reprocessed on every later model call; the ontology runs also made more calls
(92 and 75 vs 65 and 51). The plan is not the main cost; reading the selected
sources exhaustively is.

## Limitations

- The full 30-run protocol (5 tickets x 2 arms x 3 runs, temperature 0, blind
  review) was not run. This is a 4-run pilot on one ticket.
- `tests/test_build.rs` / `tests/test_install.rs` were not completed. The
  integration attempt on prose-1 was SIGTERM'd at 65.2s while `test_build`'s
  `all_tests` was still running; exit 101 is the timeout signal, not an assertion
  failure. A later manual attempt stalled in `deploy.rs dev` waiting on a
  rust-script cargo build. These suites are therefore not counted as pass for any
  arm, even though the workspace tests, `toolkit_setup`, `check.rs`,
  `validation/run.rs`, and clippy all passed.
