---
name: autopilot-director
description: "Complete one whole spec as a single PR: a Director drives child tickets through code-run gates and a two-axis review gate, delegating code development to a role-pinned fast-model Worker. State transitions are owned by the director CLI. Use when the user asks to run a spec end to end."
---

Execute the autopilot-director Spec run below.

## What this skill does

A **Spec run** completes one spec issue and its child tickets as a single Spec
PR. The Director (you, running on the main effective model) dispatches a
**Worker** subagent per ticket for code development, re-verifies every claim
with code-run gates, and advances a ticket only when a **Two-axis gate**
(Standards + Spec, run by fresh reviewers) reaches **absolute zero**. Every
state transition is owned by the `director` CLI — a change you cannot record
through the CLI did not happen.

One run lands exactly one PR; each ticket contributes exactly one Ticket
boundary commit. The vocabulary this file uses is defined in `CONTEXT.md`
("Autopilot Director"); the decisions behind it are ADR 0046 (role-pinned
models), ADR 0047 (one Spec PR, one commit per ticket) and ADR 0048 (two-axis
gate, absolute-zero bar, escalation).

## Parameters

| parameter | default | meaning |
| --- | --- | --- |
| spec issue | required | the spec issue number to complete |
| worker model | `deepseek-flash` | the Role-pinned model the Worker is dispatched on; pin it at dispatch time, never in a committed agent definition |
| auto-merge | off | off means: open the Spec PR and stop for human approval. On means: merge once CI is green and both gates are at zero |
| worktree | current checkout | the target worktree; run state lives in its `.director/` (project-local, worktree-local, git-ignored) |

## Preconditions

1. The `director` CLI is available. When the toolkit is installed, read
   `~/.agents/skills/.autopilot/director.env` if it exists and use
   `AUTOPILOT_DIRECTOR_BIN` as the executable path; otherwise use the stable
   `~/.agents/skills/.autopilot/bin/director`. In a source checkout, build it
   with `cargo build --release -p director-cli`. Verify with `director --help`.
2. `gh` is authenticated for the repository that hosts the spec.
3. You are in the target worktree, and `/.director/` is git-ignored (the CLI
   establishes the rule or refuses — never override it).

## Roles

- **Director** — the main effective model. Dispatches Workers, runs the repo's
  test gates itself, adjudicates findings, squashes boundary commits, opens the
  Spec PR. Never pinned to a fast model.
- **Worker** — one per ticket, dispatched on the Role-pinned fast model, bound
  by `references/worker-contract.md` and the upstream `tdd` skill. Its own
  reports are never gate evidence.
- **Reviewers** — fresh effective-model agents, one per axis, one per round.
  They never see Worker context and never review their own round.

## The loop

### 1. Read the spec

`gh issue view <spec> --json number,title,body` and list its children
(`gh issue list --state open --json number,title,body` — children name the spec
in a `Parent` section). Build the task graph from the `Blocked by` sections:
that graph, not ticket order, defines the frontier. v1 runs tickets serially.

### 2. Open or resume the run

- Fresh: `director init --worktree <wt> --spec-issue <N> --slug <slug>` where
  `<slug>` is the title-derived slug of the spec branch
  (`codex/spec-<N>-<slug>`). Then
  `director run transition --worktree <wt> --to running`.
- Resuming: `director resume --worktree <wt>`. A drift error is a hard stop:
  inspect what moved and re-run with `--accept-drift` only after a human agrees,
  or abandon and report.
- Register every child ticket once:
  `director ticket add --worktree <wt> --ticket <n> --title <title> [--blocked-by <m>]...`.

### 3. Per-ticket loop

For the next ticket on the frontier:

1. `director ticket transition --ticket <n> --to implementing`.
2. `director dispatch begin --ticket <n> --worker <label>` (the attempt is
   recorded before the Worker exists).
3. Dispatch the Worker on the pinned model with: the ticket issue reference,
   the worktree and branch, the ticket's `Seam:` annotation or a
   `Seam(inferred)` you supply, and — on a fix round — the findings to resolve.
   The Worker reads `references/worker-contract.md`, follows `tdd`, and returns
   a `WORKER_REPORT:` envelope.
4. Validate the envelope: `director report validate --ticket <n>` (stdin) or
   `--file <path>`. Then
   `director dispatch finish --ticket <n> --outcome ok|failed [--reason <text>]`.
   A malformed envelope or a `blocked` report is a failed dispatch; the state
   machine allows exactly one same-Worker retry and then escalates by itself.
5. Run the repo's test gates yourself (`cargo test`, the suite the ticket
   names). Worker self-reports are not evidence. Failing gates send the same
   Worker back to work: `--to implementing`.
6. `director ticket transition --ticket <n> --to gating`, then run the review
   rounds below.

### 4. Two-axis gate (per ticket, then once for the spec)

Flattened dispatch — spawn the two axis reviewers directly; do not invoke the
`code-review` skill as a nested layer:

- `director round open (--ticket <n> | --spec)`.
- Spawn a **Standards** reviewer and a **Spec** reviewer, each fresh, each with
  the full axis prompt of the upstream `code-review` process: the smell
  baseline pasted in full, the standards sources listed, the spec content
  quoted. Reviewers never see the Worker's context or each other's findings.
- Record every finding:
  `director finding record (--ticket <n> | --spec) --round <k> --axis
  <standards|spec> --id <id> --hash <hash> --summary <text>`.
- `director round close (--ticket <n> | --spec) --round <k>`.
- Adjudicate to absolute zero: each finding is either fixed by the Worker or
  rejected by you with a written reason —
  `director finding dispose ... --fixed <commit>` or
  `director finding dispose ... --rejected "<reason>"`.
- `director gate (--ticket <n> | --spec)` must exit 0 (zero) before you
  advance. If findings remain, transition the ticket `--to fixing`, hand the
  findings to the same Worker, and open the next round.

The cap is three rounds per layer and it is enforced in code: a fourth
`round open` refuses and moves the layer to `escalated`.

### 5. Ticket boundary commit

Once the ticket's gate is at zero: squash its work-in-progress commits into one
commit whose message references the ticket (`feat: <title> (ticket #N)`), then
`director ticket transition --ticket <n> --to done` (refused while the gate is
open). Start the next frontier ticket.

### 6. Aggregate spec gate and the Spec PR

1. When every ticket is `done`:
   `director run transition --to spec-gating`.
2. Run the same two-axis gate over the aggregate diff (`--spec`), with the same
   absolute-zero bar and round cap.
3. Push the branch only now, open the single Spec PR whose body lists every
   ticket with `Closes #…`, then `director run transition --to pr-open`.
4. Default (auto-merge off): stop and hand the PR to the human for approval.
   With auto-merge on: wait for CI green plus `director gate` at zero, merge
   with a **merge commit** (squash-merge is forbidden — it would flatten the
   boundary commits), then `director run transition --to done`.
5. Verify each ticket issue closed, then report.

## Escalation

An escalation is not a failure you may retry your way out of. When a layer
escalates (round cap exhausted, dispatch budget exhausted, or an unparseable
report twice), stop the run and report:

1. the finding evolution across rounds (what each round found, what was fixed,
   what you rejected and why),
2. the Worker's self-reports,
3. your diagnosis and the decision you need (grant more rounds, implement
   directly, or abandon the ticket).

The run resumes only on an explicit human decision, recorded by the allowed
transition (`escalated → reviewing` buys another cap's worth of rounds;
`escalated → implementing` means you implement it directly).

## Boundaries

- One Spec PR per run; never per-ticket PRs.
- Never merge, label, or close anything the CLI did not first make legal, and
  never claim a gate passed on prose evidence.
- Do not modify `autopilot-orchestrator` or `autopilot-reviewer`; the two
  workflows are meant to be comparable side by side.
- Workers never review; reviewers never implement; the Director never
  delegates a judgment it can check itself.
