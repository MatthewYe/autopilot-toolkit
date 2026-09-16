---
name: autopilot-director
description: "Codex autopilot-director Spec run: one spec, one PR, Workers on a pinned fast model, a code-owned state machine, and a two-axis gate that must reach absolute zero."
---

Execute one autopilot-director **Spec run**: drive a spec issue and its child
tickets to a single Spec PR. Every state transition goes through the `director`
CLI; a change the CLI cannot record did not happen.

Vocabulary lives in `CONTEXT.md` ("Autopilot Director"); the decisions behind
it are ADR 0046 (role-pinned models), ADR 0047 (one Spec PR, one commit per
ticket) and ADR 0048 (two-axis gate, absolute-zero bar, escalation).

## Parameters

| parameter | default | meaning |
| --- | --- | --- |
| spec issue | required | the spec to complete |
| worker model | `deepseek-flash` | the Role-pinned model the Worker is spawned on |
| auto-merge | off | off: open the Spec PR and stop for human approval; on: merge once CI is green and both gates are at zero |
| worktree | current checkout | run state lives in its git-ignored `.director/` |

## Preconditions

1. `director --help` works. When the toolkit is installed, prefer the path in
   `~/.agents/skills/.autopilot/director.env` (`AUTOPILOT_DIRECTOR_BIN`) and
   fall back to the stable `~/.agents/skills/.autopilot/bin/director`; in a
   source checkout, `cargo build --release -p director-cli`.
2. `gh` is authenticated for the spec's repository.
3. You are in the target worktree; never bypass the CLI's `.director/`
   git-ignore enforcement.

## Sub-agent dispatch model (Codex)

- **Worker** — spawn with an explicit model override:
  `spawn_agent(agent_type: "worker", model: "<worker model>", fork_turns: "none", message: <dispatch>)`.
  The model pin is per dispatch and never committed. `fork_turns: "none"` (or a
  positive count) is required: a full-history spawn inherits the lead model and
  cannot be pinned.
- The Worker's dispatch message carries the ticket issue reference, the
  worktree, the branch, the ticket's `Seam:` annotation (or your
  `Seam(inferred)`), the instruction to read
  `~/.agents/skills/autopilot-director/runtime/default/references/worker-contract.md`
  (`references/worker-contract.md` in a source checkout) and follow the upstream
  `tdd` skill, and — on a fix round — the findings to resolve.
- **Reviewers** — one spawn per axis per round, both on the default (lead)
  model, each fresh, each receiving only its own axis prompt: the smell
  baseline pasted in full, the standards sources listed, the spec content
  quoted. Never pass Worker context to a reviewer.
- **Spawn fallback** — if a spawned agent returns without a payload or without
  its `WORKER_REPORT:`, re-dispatch once; if that fails too, do that piece of
  work yourself and record the fallback in the run report. Do not spend the run
  fighting the spawn path.

## The loop

### 1. Read the spec

`gh issue view <spec> --json number,title,body`; list children with
`gh issue list --state open --json number,title,body` (children carry a
`Parent` section). The `Blocked by` sections define the task graph and its
frontier; v1 runs tickets serially off that frontier.

### 2. Open or resume the run

- Fresh: `director init --worktree <wt> --spec-issue <N> --slug <slug>` (the
  slug of `codex/spec-<N>-<slug>`), then
  `director run transition --worktree <wt> --to running`.
- Resuming: `director resume --worktree <wt>`; a drift error stops the run
  until a human settles or explicitly accepts the drift
  (`--accept-drift`).
- `director ticket add --worktree <wt> --ticket <n> --title <title>
  [--blocked-by <m>]...` for every child ticket, once.

### 3. Per-ticket loop

1. `director ticket transition --ticket <n> --to implementing`.
2. `director dispatch begin --ticket <n> --worker <label>`.
3. Spawn the Worker (see dispatch model). Wait for it, then
   `director report validate --ticket <n>` (stdin) or `--file <path>`, and
   `director dispatch finish --ticket <n> --outcome ok|failed [--reason <text>]`.
   Malformed envelope, a `blocked` report, or no commits: failed dispatch. The
   state machine permits one same-Worker retry, then escalates on its own.
4. Run the repo's test gates yourself; Worker claims are never gate evidence.
   Gates failing sends the same Worker back (`--to implementing`).
5. `director ticket transition --ticket <n> --to gating`, then the review
   rounds:
   - `director round open --ticket <n>`;
   - spawn the two fresh axis reviewers; record every finding with
     `director finding record --ticket <n> --round <k> --axis <standards|spec>
     --id <id> --hash <hash> --summary <text>`;
   - `director round close --ticket <n> --round <k>`;
   - adjudicate each finding —
     `director finding dispose ... --fixed <commit>` or
     `director finding dispose ... --rejected "<written reason>"`;
   - `director gate --ticket <n>` must exit 0. Otherwise
     `director ticket transition --ticket <n> --to fixing`, hand the findings
     back to the same Worker, and open the next round. Three rounds per layer
     maximum: a fourth `round open` refuses and escalates the layer.

### 4. Ticket boundary commit

At zero, squash the ticket's work-in-progress commits into one
`feat: <title> (ticket #N)` commit, then
`director ticket transition --ticket <n> --to done`.

### 5. Aggregate gate and the Spec PR

1. All tickets `done` → `director run transition --to spec-gating`.
2. Same two-axis gate over the aggregate diff (`--spec`), same zero bar, same
   three-round cap.
3. Push the branch only now; open the one Spec PR listing every ticket with
   `Closes #…`; `director run transition --to pr-open`.
4. Auto-merge off (default): stop and hand the PR to the human. Auto-merge on:
   wait for CI green and `director gate` at zero, merge with a **merge commit**
   (squash-merge would flatten the boundary commits), then
   `director run transition --to done`.
5. Confirm every ticket issue closed; report the run.

## Escalation

Stop the run and report — finding evolution across rounds, the Worker's
self-reports, your diagnosis, and the decision you need (grant more rounds,
implement directly, or abandon the ticket). Resume only on an explicit human
decision through the legal edges (`escalated → reviewing` buys another cap's
worth of rounds; `escalated → implementing` means you implement it directly).

## Boundaries

- One Spec PR per run; never per-ticket PRs, never squash-merges.
- Never assert a gate result: `director gate` is the only source of truth.
- Leave `autopilot-orchestrator` and `autopilot-reviewer` untouched.
- Workers never review; reviewers never implement; you never hand a judgment to
  a subagent that you can check with the CLI.
