//! The transition engine: the legal-edge tables and every
//! state-machine-owned mutation (ADR 0041 pattern — the code owns control,
//! prose only dispatches).
//!
//! Every mutation validates first, then mutates, bumps the revision, and
//! writes the state atomically; a refused command leaves the state untouched.

use std::path::Path;

use serde_json::{json, Value};

use crate::gate::{self, GateLayer};
use crate::report;
use crate::state::{
    self, DispatchRecord, DispatchStatus, FindingDisposition, ReviewAxis, ReviewFinding,
    ReviewRound, RoundStatus, RunState, RunStatus, TicketState, TicketStatus,
};
use crate::storage::{DEFAULT_ROUND_CAP, DISPATCH_RETRY_BUDGET};

// ── legal edges ──

/// The legal run-status edges: `init → running → spec-gating → pr-open →
/// done | escalated`, plus the human-decision edges that resume an escalated
/// run (ADR 0048).
pub(crate) fn run_edges(from: RunStatus) -> &'static [RunStatus] {
    match from {
        RunStatus::Init => &[RunStatus::Running, RunStatus::Escalated],
        RunStatus::Running => &[RunStatus::SpecGating, RunStatus::Escalated],
        RunStatus::SpecGating => &[RunStatus::PrOpen, RunStatus::Escalated],
        RunStatus::PrOpen => &[RunStatus::Done, RunStatus::Escalated],
        RunStatus::Done => &[],
        RunStatus::Escalated => &[RunStatus::Running, RunStatus::SpecGating],
    }
}

/// The legal ticket-status edges: the ticket cycle `pending → implementing →
/// gating → reviewing → fixing → done | escalated` with `fixing → reviewing`
/// closing the loop, plus the human-decision edges that resume an escalated
/// ticket.
pub(crate) fn ticket_edges(from: TicketStatus) -> &'static [TicketStatus] {
    match from {
        TicketStatus::Pending => &[TicketStatus::Implementing],
        TicketStatus::Implementing => &[TicketStatus::Gating, TicketStatus::Escalated],
        TicketStatus::Gating => &[
            TicketStatus::Reviewing,
            TicketStatus::Implementing,
            TicketStatus::Escalated,
        ],
        TicketStatus::Reviewing => &[
            TicketStatus::Fixing,
            TicketStatus::Done,
            TicketStatus::Escalated,
        ],
        TicketStatus::Fixing => &[
            TicketStatus::Reviewing,
            TicketStatus::Done,
            TicketStatus::Escalated,
        ],
        TicketStatus::Done => &[],
        TicketStatus::Escalated => &[TicketStatus::Implementing, TicketStatus::Reviewing],
    }
}

fn run_targets(from: RunStatus) -> String {
    run_edges(from)
        .iter()
        .map(|status| format!("`{}`", status.as_str()))
        .collect::<Vec<_>>()
        .join(", ")
}

fn ticket_targets(from: TicketStatus) -> String {
    ticket_edges(from)
        .iter()
        .map(|status| format!("`{}`", status.as_str()))
        .collect::<Vec<_>>()
        .join(", ")
}

// ── mutations ──

/// Register a ticket of the spec under a fresh `pending` state.
pub(crate) fn add_ticket(
    worktree: &Path,
    state: &mut RunState,
    ticket: u64,
    title: &str,
    blocked_by: &[u64],
) -> Result<Value, String> {
    if state.ticket(ticket).is_some() {
        return Err(format!("ticket #{ticket} is already registered"));
    }
    let title = title.trim();
    if title.is_empty() {
        return Err("--title must not be empty".to_string());
    }
    if blocked_by.contains(&ticket) {
        return Err(format!("ticket #{ticket} cannot be blocked by itself"));
    }
    let mut blocked_by = blocked_by.to_vec();
    blocked_by.sort_unstable();
    blocked_by.dedup();

    state.tickets.push(TicketState {
        ticket,
        title: title.to_string(),
        status: TicketStatus::Pending,
        round_cap: DEFAULT_ROUND_CAP,
        blocked_by: blocked_by.clone(),
        rounds: Vec::new(),
        dispatches: Vec::new(),
        dispatch_budget_base: 0,
    });
    write(worktree, state)?;
    Ok(json!({
        "command": "ticket-add",
        "ticket": ticket,
        "title": title,
        "status": TicketStatus::Pending.as_str(),
        "blocked_by": blocked_by,
        "revision": state.revision,
    }))
}

/// Move the run along its legal edges.
pub(crate) fn transition_run(
    worktree: &Path,
    state: &mut RunState,
    to: RunStatus,
) -> Result<Value, String> {
    let from = state.status;
    if !run_edges(from).contains(&to) {
        return Err(format!(
            "illegal transition: run cannot move from `{}` to `{}`; legal targets: {}",
            from.as_str(),
            to.as_str(),
            run_targets(from)
        ));
    }

    if to == RunStatus::SpecGating && from == RunStatus::Running {
        let blocked = state
            .tickets
            .iter()
            .filter(|ticket| ticket.status != TicketStatus::Done)
            .map(|ticket| format!("#{} ({})", ticket.ticket, ticket.status.as_str()))
            .collect::<Vec<_>>();
        if !blocked.is_empty() {
            return Err(format!(
                "cannot enter `spec-gating`: tickets not done: {}",
                blocked.join(", ")
            ));
        }
    }
    if to == RunStatus::PrOpen || to == RunStatus::Done {
        let verdict = gate::spec_verdict(state);
        if !verdict.zero {
            return Err(format!(
                "cannot move the run to `{}`: the spec gate is not at zero ({} undispositioned finding(s) across {} round(s))",
                to.as_str(),
                verdict.undispositioned,
                verdict.rounds_used
            ));
        }
    }
    // Resuming an escalated run on the human's "grant more rounds" decision
    // buys another cap's worth of spec rounds; the decision itself stays human.
    if to == RunStatus::SpecGating && from == RunStatus::Escalated {
        state.spec_gate.round_cap += DEFAULT_ROUND_CAP;
    }

    state.status = to;
    write(worktree, state)?;
    Ok(json!({
        "command": "run-transition",
        "from": from.as_str(),
        "to": to.as_str(),
        "revision": state.revision,
    }))
}

/// Move one ticket along its legal edges.
pub(crate) fn transition_ticket(
    worktree: &Path,
    state: &mut RunState,
    ticket: u64,
    to: TicketStatus,
) -> Result<Value, String> {
    let current = state
        .ticket(ticket)
        .ok_or_else(|| format!("ticket #{ticket} is not registered"))?;
    let from = current.status;
    if !ticket_edges(from).contains(&to) {
        return Err(format!(
            "illegal transition: ticket #{ticket} cannot move from `{}` to `{}`; legal targets: {}",
            from.as_str(),
            to.as_str(),
            ticket_targets(from)
        ));
    }

    match to {
        TicketStatus::Done => {
            let verdict = gate::ticket_verdict(current);
            if !verdict.zero {
                return Err(format!(
                    "cannot mark ticket #{ticket} `done`: its gate is not at zero ({} undispositioned finding(s) across {} round(s))",
                    verdict.undispositioned, verdict.rounds_used
                ));
            }
        }
        TicketStatus::Fixing => {
            let open = current
                .latest_round()
                .map(|round| !round.is_zero())
                .unwrap_or(false);
            if !open {
                return Err(format!(
                    "cannot move ticket #{ticket} to `fixing`: there is no round with open findings (the latest round is absent or already at zero)"
                ));
            }
        }
        _ => {}
    }

    if to == TicketStatus::Reviewing && from == TicketStatus::Escalated {
        let ticket_state = state
            .ticket_mut(ticket)
            .expect("ticket existence checked above");
        ticket_state.round_cap += DEFAULT_ROUND_CAP;
    }
    // The human's "grant the Worker another go" decision resets the dispatch
    // budget window, exactly as resuming an escalated run buys another cap's
    // worth of spec rounds.
    if to == TicketStatus::Implementing && from == TicketStatus::Escalated {
        let ticket_state = state
            .ticket_mut(ticket)
            .expect("ticket existence checked above");
        ticket_state.dispatch_budget_base = ticket_state.dispatches.len() as u64;
    }

    let ticket_state = state
        .ticket_mut(ticket)
        .expect("ticket existence checked above");
    ticket_state.status = to;
    write(worktree, state)?;
    Ok(json!({
        "command": "ticket-transition",
        "ticket": ticket,
        "from": from.as_str(),
        "to": to.as_str(),
        "revision": state.revision,
    }))
}

/// Open a review round for one gate layer. Opening a round beyond the cap is
/// impossible: the state machine escalates the layer instead.
pub(crate) fn open_round(
    worktree: &Path,
    state: &mut RunState,
    layer: GateLayer,
) -> Result<Value, String> {
    match layer {
        GateLayer::Ticket(ticket) => {
            let ticket_state = state
                .ticket_mut(ticket)
                .ok_or_else(|| format!("ticket #{ticket} is not registered"))?;
            if let Some(open) = ticket_state.open_round() {
                return Err(format!(
                    "round {} is still open for ticket #{ticket}; close it first",
                    open.round
                ));
            }
            match ticket_state.status {
                TicketStatus::Gating | TicketStatus::Fixing => {}
                other => {
                    return Err(format!(
                        "cannot open a review round for ticket #{ticket} while it is `{}`; expected `gating` or `fixing`",
                        other.as_str()
                    ));
                }
            }
            if ticket_state.gate_is_zero() {
                return Err(format!(
                    "ticket #{ticket} is already at zero; transition it to `done` instead of reviewing again"
                ));
            }
            if ticket_state.rounds.len() as u64 >= ticket_state.round_cap {
                let round_cap = ticket_state.round_cap;
                ticket_state.status = TicketStatus::Escalated;
                write(worktree, state)?;
                return Err(format!(
                    "review round cap exhausted for ticket #{ticket} ({} rounds without zero); the ticket is now `escalated` and resumes only on an explicit human decision",
                    round_cap
                ));
            }
            let number = ticket_state
                .rounds
                .last()
                .map(|round| round.round + 1)
                .unwrap_or(1);
            ticket_state.rounds.push(ReviewRound {
                round: number,
                status: RoundStatus::Reviewing,
                findings: Vec::new(),
            });
            ticket_state.status = TicketStatus::Reviewing;
            let rounds_used = ticket_state.rounds.len();
            let round_cap = ticket_state.round_cap;
            write(worktree, state)?;
            Ok(json!({
                "command": "round-open",
                "layer": "ticket",
                "ticket": ticket,
                "round": number,
                "rounds_used": rounds_used,
                "round_cap": round_cap,
                "revision": state.revision,
            }))
        }
        GateLayer::Spec => {
            if state.status != RunStatus::SpecGating {
                return Err(format!(
                    "cannot open a spec review round while the run is `{}`; expected `spec-gating`",
                    state.status.as_str()
                ));
            }
            if let Some(open) = state.spec_gate.open_round() {
                return Err(format!(
                    "spec round {} is still open; close it first",
                    open.round
                ));
            }
            if state.spec_gate.is_zero() {
                return Err(
                    "the spec gate is already at zero; open the Spec PR instead of reviewing again"
                        .to_string(),
                );
            }
            if state.spec_gate.rounds.len() as u64 >= state.spec_gate.round_cap {
                let round_cap = state.spec_gate.round_cap;
                state.status = RunStatus::Escalated;
                write(worktree, state)?;
                return Err(format!(
                    "spec review round cap exhausted ({} rounds without zero); the run is now `escalated` and resumes only on an explicit human decision",
                    round_cap
                ));
            }
            let number = state
                .spec_gate
                .rounds
                .last()
                .map(|round| round.round + 1)
                .unwrap_or(1);
            state.spec_gate.rounds.push(ReviewRound {
                round: number,
                status: RoundStatus::Reviewing,
                findings: Vec::new(),
            });
            let rounds_used = state.spec_gate.rounds.len();
            let round_cap = state.spec_gate.round_cap;
            write(worktree, state)?;
            Ok(json!({
                "command": "round-open",
                "layer": "spec",
                "ticket": Value::Null,
                "round": number,
                "rounds_used": rounds_used,
                "round_cap": round_cap,
                "revision": state.revision,
            }))
        }
    }
}

/// Close one open review round and report its verdict.
pub(crate) fn close_round(
    worktree: &Path,
    state: &mut RunState,
    layer: GateLayer,
    number: u64,
) -> Result<Value, String> {
    let round = round_mut(state, layer, number, "close", true)?;
    round.status = RoundStatus::Complete;
    write(worktree, state)?;
    let verdict = verdict_for(state, layer).expect("layer existence checked above");
    Ok(json!({
        "command": "round-close",
        "round": number,
        "verdict": verdict.to_json(),
        "revision": state.revision,
    }))
}

/// Record one finding on an open round. The disposition may be supplied right
/// away (`--fixed` / `--rejected`), or later through `dispose_finding`.
pub(crate) fn record_finding(
    worktree: &Path,
    state: &mut RunState,
    layer: GateLayer,
    number: u64,
    finding: NewFinding,
) -> Result<Value, String> {
    // Identity is what makes a finding checkable: an unnamed or unhashed
    // finding could never be matched against a re-issued review, so it is
    // refused rather than recorded as an anonymous row.
    require_text(&finding.id, "finding id")?;
    require_text(&finding.hash, "finding hash")?;
    require_text(&finding.summary, "finding summary")?;

    let round = round_mut(state, layer, number, "record a finding on", true)?;
    if round
        .findings
        .iter()
        .any(|existing| existing.id == finding.id)
    {
        return Err(format!(
            "finding {:?} is already recorded on round {number}",
            finding.id
        ));
    }
    let disposition_json = finding.disposition.as_ref().map(FindingDisposition::as_str);
    let axis_json = finding.axis.as_str();
    round.findings.push(ReviewFinding {
        id: finding.id.clone(),
        axis: finding.axis,
        hash: finding.hash,
        summary: finding.summary,
        disposition: finding.disposition,
    });
    write(worktree, state)?;
    let verdict = verdict_for(state, layer).expect("layer existence checked above");
    Ok(json!({
        "command": "finding-record",
        "round": number,
        "finding": finding.id,
        "axis": axis_json,
        "disposition": disposition_json,
        "verdict": verdict.to_json(),
        "revision": state.revision,
    }))
}

/// Record the Director's disposition for one already-recorded finding.
pub(crate) fn dispose_finding(
    worktree: &Path,
    state: &mut RunState,
    layer: GateLayer,
    number: u64,
    id: &str,
    disposition: FindingDisposition,
) -> Result<Value, String> {
    // Dispositions land after the review round closed: the Director records
    // the adjudication once the fixes are in, which is exactly when the round
    // is no longer collecting findings.
    let round = round_mut(state, layer, number, "dispose a finding on", false)?;
    if !disposition.is_recorded() {
        return Err(
            "a rejected disposition needs a written reason; `rejected` with an empty reason does not count"
                .to_string(),
        );
    }
    let finding = round
        .findings
        .iter_mut()
        .find(|finding| finding.id == id)
        .ok_or_else(|| format!("finding {id:?} is not recorded on round {number}"))?;
    if finding.disposition.is_some() {
        return Err(format!(
            "finding {id:?} already carries a disposition; dispositions are recorded once"
        ));
    }
    finding.disposition = Some(disposition);
    write(worktree, state)?;
    let verdict = verdict_for(state, layer).expect("layer existence checked above");
    Ok(json!({
        "command": "finding-dispose",
        "round": number,
        "finding": id,
        "verdict": verdict.to_json(),
        "revision": state.revision,
    }))
}

// ── dispatch bookkeeping ──

/// Register the start of one Worker dispatch attempt. The retry budget — a
/// failed attempt plus exactly one same-Worker retry — is enforced here, so
/// "the Worker gets another go" is a fact about state, not a sentence in a
/// prompt (ADR 0048).
pub(crate) fn begin_dispatch(
    worktree: &Path,
    state: &mut RunState,
    ticket: u64,
    worker: &str,
) -> Result<Value, String> {
    let worker = worker.trim();
    require_text(worker, "--worker")?;

    let ticket_state = state
        .ticket_mut(ticket)
        .ok_or_else(|| format!("ticket #{ticket} is not registered"))?;
    if ticket_state.status != TicketStatus::Implementing {
        return Err(format!(
            "cannot dispatch a Worker for ticket #{ticket} while it is `{}`; dispatches begin in `implementing`",
            ticket_state.status.as_str()
        ));
    }
    if let Some(open) = ticket_state.open_dispatch() {
        return Err(format!(
            "dispatch attempt {} for ticket #{ticket} is still open; finish it before starting another",
            open.attempt
        ));
    }
    let failures = ticket_state.failed_dispatches_in_streak();
    if failures > DISPATCH_RETRY_BUDGET {
        return Err(format!(
            "the dispatch retry budget for ticket #{ticket} is exhausted ({failures} failed attempts, {DISPATCH_RETRY_BUDGET} retry allowed); the ticket resumes only on an explicit human decision"
        ));
    }
    if failures == DISPATCH_RETRY_BUDGET {
        let failed_worker = ticket_state
            .last_failed_dispatch_in_streak()
            .map(|record| record.worker.as_str())
            .unwrap_or_default();
        if failed_worker != worker {
            return Err(format!(
                "the retry for ticket #{ticket} must go to the same Worker that failed ({failed_worker:?}), not {worker:?}"
            ));
        }
    }

    let attempt = ticket_state.dispatches.len() as u64 + 1;
    ticket_state.dispatches.push(DispatchRecord {
        worker: worker.to_string(),
        attempt,
        status: DispatchStatus::Started,
        reason: None,
        report: None,
    });
    write(worktree, state)?;
    Ok(json!({
        "command": "dispatch-begin",
        "ticket": ticket,
        "worker": worker,
        "attempt": attempt,
        "retry": failures > 0,
        "revision": state.revision,
    }))
}

/// Validate and attach the Worker's `WORKER_REPORT` envelope to the open
/// dispatch. A malformed envelope is a recorded dispatch failure, never a
/// silent pass; the retry budget then decides whether another attempt is
/// sanctioned or the ticket escalates.
pub(crate) fn record_worker_report(
    worktree: &Path,
    state: &mut RunState,
    ticket: u64,
    raw: &str,
) -> Result<Value, String> {
    let (worker, attempt) = open_dispatch_identity(state, ticket)?;
    match report::parse_envelope(raw) {
        Ok(worker_report) => {
            let summary = worker_report.summary();
            let ticket_state = state
                .ticket_mut(ticket)
                .expect("ticket existence checked above");
            let open = ticket_state
                .open_dispatch_mut()
                .expect("open dispatch checked above");
            open.report = Some(worker_report);
            write(worktree, state)?;
            Ok(json!({
                "command": "report-validate",
                "ticket": ticket,
                "worker": worker,
                "attempt": attempt,
                "report": summary,
                "revision": state.revision,
            }))
        }
        Err(detail) => {
            let reason = format!("malformed WORKER_REPORT: {detail}");
            let failure = fail_open_dispatch(worktree, state, ticket, &reason)?;
            Err(format!("{reason}\n{}", failure.tail(ticket)))
        }
    }
}

/// Finish the open dispatch as successful. Only a validated `done` report
/// counts: a `blocked` self-report is a failed dispatch by definition, and a
/// dispatch with no validated report has no evidence at all.
pub(crate) fn finish_dispatch_ok(
    worktree: &Path,
    state: &mut RunState,
    ticket: u64,
) -> Result<Value, String> {
    let (worker, attempt) = open_dispatch_identity(state, ticket)?;
    let report_status = state
        .ticket(ticket)
        .and_then(TicketState::open_dispatch)
        .and_then(|open| open.report.as_ref())
        .map(|worker_report| worker_report.status);
    let Some(report_status) = report_status else {
        return Err(format!(
            "no validated WORKER_REPORT is attached to the open dispatch for ticket #{ticket}; run `director report validate` before finishing it"
        ));
    };
    if report_status == report::ReportStatus::Blocked {
        return Err(format!(
            "the attached WORKER_REPORT for ticket #{ticket} reports `blocked`; finish the dispatch with `--outcome failed --reason <blockers>` so the retry budget applies"
        ));
    }

    let ticket_state = state
        .ticket_mut(ticket)
        .expect("ticket existence checked above");
    let open = ticket_state
        .open_dispatch_mut()
        .expect("open dispatch checked above");
    open.status = DispatchStatus::Ok;
    write(worktree, state)?;
    Ok(json!({
        "command": "dispatch-finish",
        "ticket": ticket,
        "worker": worker,
        "attempt": attempt,
        "outcome": "ok",
        "revision": state.revision,
    }))
}

/// Finish the open dispatch as failed. The budget decides the ending: the
/// first failure records a sanctioned retry, the one past the budget escalates
/// the ticket and refuses to be recorded as another retry.
pub(crate) fn finish_dispatch_failed(
    worktree: &Path,
    state: &mut RunState,
    ticket: u64,
    reason: &str,
) -> Result<Value, String> {
    let reason = reason.trim();
    require_text(reason, "--reason")?;

    let (worker, attempt) = open_dispatch_identity(state, ticket)?;
    let failure = fail_open_dispatch(worktree, state, ticket, reason)?;
    if failure.escalated {
        return Err(format!(
            "dispatch attempt {attempt} for ticket #{ticket} failed: {reason}\n{}",
            failure.tail(ticket)
        ));
    }
    Ok(json!({
        "command": "dispatch-finish",
        "ticket": ticket,
        "worker": worker,
        "attempt": attempt,
        "outcome": "failed",
        "reason": reason,
        "retry_remaining": true,
        "revision": state.revision,
    }))
}

/// What a recorded dispatch failure means for the ticket.
struct DispatchFailure {
    failures: u64,
    escalated: bool,
}

impl DispatchFailure {
    fn tail(&self, ticket: u64) -> String {
        if self.escalated {
            format!(
                "ticket #{ticket} has {} failed dispatch attempt(s) and the retry budget allows {DISPATCH_RETRY_BUDGET} retry, so it is now `escalated` and resumes only on an explicit human decision",
                self.failures
            )
        } else {
            format!(
                "ticket #{ticket} has {} failed dispatch attempt(s); one same-Worker retry remains",
                self.failures
            )
        }
    }
}

fn open_dispatch_identity(state: &RunState, ticket: u64) -> Result<(String, u64), String> {
    let ticket_state = state
        .ticket(ticket)
        .ok_or_else(|| format!("ticket #{ticket} is not registered"))?;
    let open = ticket_state.open_dispatch().ok_or_else(|| {
        format!("ticket #{ticket} has no open dispatch; run `director dispatch begin` first")
    })?;
    Ok((open.worker.clone(), open.attempt))
}

/// Close the open dispatch as failed and apply the retry budget. The state is
/// written either way: the failure is a fact, and only the *next* attempt is
/// refused.
fn fail_open_dispatch(
    worktree: &Path,
    state: &mut RunState,
    ticket: u64,
    reason: &str,
) -> Result<DispatchFailure, String> {
    let ticket_state = state
        .ticket_mut(ticket)
        .ok_or_else(|| format!("ticket #{ticket} is not registered"))?;
    let open = ticket_state
        .open_dispatch_mut()
        .ok_or_else(|| format!("ticket #{ticket} has no open dispatch"))?;
    open.status = DispatchStatus::Failed;
    open.reason = Some(reason.to_string());

    let failures = ticket_state.failed_dispatches_in_streak();
    let escalated = failures > DISPATCH_RETRY_BUDGET;
    if escalated {
        ticket_state.status = TicketStatus::Escalated;
    }
    write(worktree, state)?;
    Ok(DispatchFailure {
        failures,
        escalated,
    })
}

fn require_text(value: &str, field: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("{field} must not be empty"));
    }
    Ok(())
}

// ── helpers ──

/// A finding about to be recorded.
pub(crate) struct NewFinding {
    pub(crate) id: String,
    pub(crate) axis: ReviewAxis,
    pub(crate) hash: String,
    pub(crate) summary: String,
    pub(crate) disposition: Option<FindingDisposition>,
}

fn round_mut<'a>(
    state: &'a mut RunState,
    layer: GateLayer,
    number: u64,
    action: &str,
    open_only: bool,
) -> Result<&'a mut ReviewRound, String> {
    let round = match layer {
        GateLayer::Ticket(ticket) => {
            let ticket_state = state
                .ticket_mut(ticket)
                .ok_or_else(|| format!("ticket #{ticket} is not registered"))?;
            match ticket_state.status {
                TicketStatus::Reviewing | TicketStatus::Fixing => {}
                other => {
                    return Err(format!(
                        "cannot {action} ticket #{ticket} while it is `{}`; review rounds live in `reviewing`",
                        other.as_str()
                    ));
                }
            }
            ticket_state.round_mut(number)
        }
        GateLayer::Spec => state.spec_gate.round_mut(number),
    };
    let round = round.ok_or_else(|| {
        format!(
            "round {number} is not recorded for the {} layer",
            layer.as_str()
        )
    })?;
    if open_only && round.status != RoundStatus::Reviewing {
        return Err(format!(
            "round {number} is already `{}`; only an open round can be written",
            round.status.as_str()
        ));
    }
    Ok(round)
}

fn verdict_for(state: &RunState, layer: GateLayer) -> Option<gate::GateVerdict> {
    match layer {
        GateLayer::Ticket(ticket) => state.ticket(ticket).map(gate::ticket_verdict),
        GateLayer::Spec => Some(gate::spec_verdict(state)),
    }
}

/// Bump the revision and persist: every state transition is revisioned
/// (ADR 0018 pattern), so a stale session can detect that the run moved.
fn write(worktree: &Path, state: &mut RunState) -> Result<(), String> {
    state.revision += 1;
    state::write_state(worktree, state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_edges_follow_the_documented_chain() {
        assert_eq!(
            run_edges(RunStatus::Init),
            &[RunStatus::Running, RunStatus::Escalated]
        );
        assert_eq!(
            run_edges(RunStatus::Running),
            &[RunStatus::SpecGating, RunStatus::Escalated]
        );
        assert_eq!(
            run_edges(RunStatus::SpecGating),
            &[RunStatus::PrOpen, RunStatus::Escalated]
        );
        assert_eq!(
            run_edges(RunStatus::PrOpen),
            &[RunStatus::Done, RunStatus::Escalated]
        );
        assert!(run_edges(RunStatus::Done).is_empty());
        assert_eq!(
            run_edges(RunStatus::Escalated),
            &[RunStatus::Running, RunStatus::SpecGating]
        );
    }

    #[test]
    fn ticket_edges_cycle_through_fixing_back_to_reviewing() {
        assert_eq!(
            ticket_edges(TicketStatus::Pending),
            &[TicketStatus::Implementing]
        );
        assert!(ticket_edges(TicketStatus::Implementing).contains(&TicketStatus::Gating));
        assert!(ticket_edges(TicketStatus::Gating).contains(&TicketStatus::Reviewing));
        // The test gate can fail back to the Worker.
        assert!(ticket_edges(TicketStatus::Gating).contains(&TicketStatus::Implementing));
        assert!(ticket_edges(TicketStatus::Reviewing).contains(&TicketStatus::Fixing));
        assert!(ticket_edges(TicketStatus::Reviewing).contains(&TicketStatus::Done));
        assert_eq!(
            ticket_edges(TicketStatus::Fixing),
            &[
                TicketStatus::Reviewing,
                TicketStatus::Done,
                TicketStatus::Escalated
            ]
        );
        assert!(ticket_edges(TicketStatus::Done).is_empty());
        assert_eq!(
            ticket_edges(TicketStatus::Escalated),
            &[TicketStatus::Implementing, TicketStatus::Reviewing]
        );
    }
}
