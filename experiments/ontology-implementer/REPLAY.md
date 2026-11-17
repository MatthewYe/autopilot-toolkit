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
deterministic plan from `scripts/render_plan.py`.

## Commands

```bash
# prepare the pilot worktrees (already run)
python3 scripts/prepare_replay.py

# evaluate one run
python3 scripts/evaluate_replay.py \
  --repo replay/T126/prose-1/repo \
  --out results/phase2/prose-1.json

# structural hidden check for T126
python3 scripts/check_t126_contract.py replay/T126/prose-1/repo
```

Checks: `cargo test --workspace --no-fail-fast`,
`rust-script --test tests/test_toolkit_setup.rs`,
`rust-script scripts/check.rs`, `rust-script validation/run.rs`,
`cargo clippy --workspace --all-targets -- -D warnings`.

## Results

T126 pilot, 2 runs per arm. Hidden checks are the merged test suite (inline test
modules overlaid from `e3818a8`, so the agent cannot pass by rewriting tests)
plus the structural entry-field check.

| run | arm | `cargo test --workspace` | serial | structural | report | status |
|---|---|---|---|---|---|---|
| prose-1 | prose | 0 | — | pass | pass | DONE |
| prose-2 | prose | 0 | — | pass | pass | DONE |
| ontology-1 | ontology | 101 | 0 | pass | pass | DONE |
| ontology-2 | ontology | 0 | — | pass | pass | DONE |

All four also passed `toolkit_setup`, `scripts/check.rs`, `validation/run.rs`,
and `clippy --workspace --all-targets -- -D warnings` (all exit 0).

ontology-1's parallel `cargo test` failed on the known
`shared::tests::load_skill_lock_from_temp_project` `PROJECT_ROOT` env race; the
serial rerun (`cargo test --workspace -- --test-threads=1`) passed all 23
suites. This is a harness flake, not an implementation difference.

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
| ontology-1 | ontology | 48 | 99,577 | 24,500 | 1,928,704 | 2,052,781 | 124,077 |
| ontology-2 | ontology | 58 | 90,871 | 22,535 | 2,767,744 | 2,881,150 | 113,406 |
| **prose avg** | | **58** | **105,352** | **25,782** | **2,974,848** | **3,105,982** | **131,134** |
| **ontology avg** | | **53** | **95,224** | **23,517** | **2,348,224** | **2,466,965** | **118,741** |

The ontology arm averages 20.6% fewer total processed tokens and 9.5% fewer
non-cached tokens, but this is N=2 and the ranges overlap
(ontology 2.05-2.88M total vs prose 2.83-3.38M). The plan is 7,007 characters
and is read once as a tool result (~1.8k-2.0k tokens); the first model call
differs by only +60 input tokens because the plan is not inlined into the
system prompt. The difference is therefore mostly fewer exploration/tool
steps, not a direct injection saving, and it is not evidence of a cost win.
Raw numbers: `results/phase2/tokens.json`.

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
