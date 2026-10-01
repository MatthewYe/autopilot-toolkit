//! The state-machine records: every mutation as a pure method on the record it
//! belongs to, plus the two persistence helpers the thin command layer needs.
//!
//! Each method owns its guards, mutates in memory, and returns an [`Outcome`]
//! instead of writing anything. Nothing here touches the filesystem, a path, or
//! a subprocess: `transition.rs` loads the state, calls one method, and lets the
//! single `commit` seam decide whether the result has to be persisted.
//!
//! The three-state outcome exists because four refusals are *not* refusals of
//! the incident itself: a review round that exhausts its cap escalates the layer
//! first, and a dispatch that fails past its retry budget escalates the ticket.
//! Those paths mutate and persist, then exit non-zero — the type says so.

use std::path::Path;

use serde_json::{json, Map, Value};

use crate::gate::{self, GateLayer};
use crate::report;
use crate::state::{
    DispatchRecord, DispatchStatus, FindingDisposition, ReviewAxis, ReviewFinding, ReviewRound,
    RoundStatus, RunState, RunStatus, TicketState, TicketStatus,
};
use crate::storage::{WorktreeFingerprint, DEFAULT_ROUND_CAP, DISPATCH_RETRY_BUDGET};
use crate::util::require_text;

use super::Outcome;

// ── persistence seam ──

/// Move to the revision the caller is about to write.
pub(crate) fn bump(state: &mut RunState) {
    state.revision += 1;
}

/// Build an envelope from `(key, value)` pairs.
///
/// Keys are emitted in sorted order, whether they arrive sorted or not, and
/// `revision` is appended last — so every envelope stays byte-identical to the
/// inline `json!` objects it replaced, which serde_json sorted the same way.
pub(crate) fn envelope<'a>(
    command: &str,
    revision: u64,
    fields: impl IntoIterator<Item = (&'a str, Value)>,
) -> Value {
    let mut object = Map::new();
    object.insert("command".to_string(), Value::from(command));
    for (key, value) in fields {
        object.insert(key.to_string(), value);
    }
    object.insert("revision".to_string(), Value::from(revision));
    debug_assert!(
        object.keys().is_sorted(),
        "envelope keys must be emitted in sorted order for byte-identity"
    );
    Value::Object(object)
}

/// A finding about to be recorded.
pub(crate) struct NewFinding {
    pub(crate) id: String,
    pub(crate) axis: ReviewAxis,
    pub(crate) hash: String,
    pub(crate) summary: String,
    pub(crate) disposition: Option<FindingDisposition>,
}

// ── run records ──

impl RunState {
    /// Register a ticket of the spec under a fresh `pending` state.
    pub(crate) fn record_add_ticket(
        &mut self,
        ticket: u64,
        title: &str,
        blocked_by: &[u64],
    ) -> Result<Outcome, String> {
        if self.ticket(ticket).is_some() {
            return Ok(Outcome::Refused(format!(
                "ticket #{ticket} is already registered"
            )));
        }
        let title = title.trim();
        if title.is_empty() {
            return Ok(Outcome::Refused("--title must not be empty".to_string()));
        }
        if blocked_by.contains(&ticket) {
            return Ok(Outcome::Refused(format!(
                "ticket #{ticket} cannot be blocked by itself"
            )));
        }
        let mut blocked_by = blocked_by.to_vec();
        blocked_by.sort_unstable();
        blocked_by.dedup();

        self.tickets.push(TicketState {
            ticket,
            title: title.to_string(),
            status: TicketStatus::Pending,
            round_cap: DEFAULT_ROUND_CAP,
            blocked_by: blocked_by.clone(),
            rounds: Vec::new(),
            dispatches: Vec::new(),
            dispatch_budget_base: 0,
        });
        bump(self);
        Ok(Outcome::Applied(envelope(
            "ticket-add",
            self.revision,
            [
                ("ticket", json!(ticket)),
                ("title", json!(title)),
                ("status", json!(TicketStatus::Pending.as_str())),
                ("blocked_by", json!(blocked_by)),
            ],
        )))
    }

    /// Move the run along its legal edges.
    pub(crate) fn record_transition_run(&mut self, to: RunStatus) -> Result<Outcome, String> {
        let from = self.status;
        if !run_edges(from).contains(&to) {
            return Ok(Outcome::Refused(format!(
                "illegal transition: run cannot move from `{}` to `{}`; legal targets: {}",
                from.as_str(),
                to.as_str(),
                run_targets(from)
            )));
        }

        if to == RunStatus::SpecGating && from == RunStatus::Running {
            let blocked = self
                .tickets
                .iter()
                .filter(|ticket| ticket.status != TicketStatus::Done)
                .map(|ticket| format!("#{} ({})", ticket.ticket, ticket.status.as_str()))
                .collect::<Vec<_>>();
            if !blocked.is_empty() {
                return Ok(Outcome::Refused(format!(
                    "cannot enter `spec-gating`: tickets not done: {}",
                    blocked.join(", ")
                )));
            }
        }
        if to == RunStatus::PrOpen || to == RunStatus::Done {
            let verdict = gate::spec_verdict(self);
            if !verdict.zero {
                if let Some(open) = verdict.open_round {
                    return Ok(Outcome::Refused(format!(
                        "cannot move the run to `{}`: spec round {open} is still open; close it first",
                        to.as_str()
                    )));
                }
                return Ok(Outcome::Refused(format!(
                    "cannot move the run to `{}`: the spec gate is not at zero ({} undispositioned finding(s) across {} round(s))",
                    to.as_str(),
                    verdict.undispositioned,
                    verdict.rounds_used
                )));
            }
        }
        // Resuming an escalated run on the human's "grant more rounds" decision
        // buys another cap's worth of spec rounds; the decision itself stays human.
        if to == RunStatus::SpecGating && from == RunStatus::Escalated {
            self.spec_gate.round_cap += DEFAULT_ROUND_CAP;
        }

        self.status = to;
        bump(self);
        Ok(Outcome::Applied(envelope(
            "run-transition",
            self.revision,
            [("from", json!(from.as_str())), ("to", json!(to.as_str()))],
        )))
    }

    /// Revalidate the worktree the run state was last written against.
    ///
    /// A matching fingerprint resumes without touching the revision: resume is a
    /// read, and a read that bumps state would move the run out from under a
    /// stale session for no reason. A mismatch fails closed with both sides named
    /// (ADR 0035 pattern); `--accept-drift` is the explicit human acknowledgement
    /// that re-baselines the fingerprint, and only that path mutates.
    pub(crate) fn record_resume(
        &mut self,
        path: &Path,
        live: WorktreeFingerprint,
        accept_drift: bool,
    ) -> Result<Outcome, String> {
        let recorded = self.worktree.clone();
        let drift = match &recorded {
            Some(recorded) => recorded.drift(&live),
            None => vec![
                "the state file records no worktree fingerprint (it predates schema 2, or was never written)"
                    .to_string(),
            ],
        };

        if drift.is_empty() {
            return Ok(Outcome::Applied(envelope(
                "resume",
                self.revision,
                [
                    ("run_id", json!(self.run_id)),
                    ("spec_issue", json!(self.spec_issue)),
                    ("branch", json!(self.branch)),
                    ("status", json!(self.status.as_str())),
                    ("worktree", json!(recorded)),
                    ("drift", json!(Vec::<String>::new())),
                    ("rebaselined", json!(false)),
                ],
            )));
        }

        if !accept_drift {
            return Ok(Outcome::Refused(format!(
                "worktree drift detected in {}: {}; settle the worktree back to the recorded state, or re-run with --accept-drift to re-baseline",
                path.display(),
                drift.join("; ")
            )));
        }

        self.worktree = Some(live.clone());
        bump(self);
        Ok(Outcome::Applied(envelope(
            "resume",
            self.revision,
            [
                ("run_id", json!(self.run_id)),
                ("spec_issue", json!(self.spec_issue)),
                ("branch", json!(self.branch)),
                ("status", json!(self.status.as_str())),
                ("worktree", json!(live)),
                ("drift", json!(drift)),
                ("rebaselined", json!(true)),
            ],
        )))
    }
}

// ── ticket records ──

impl RunState {
    /// Move one ticket along its legal edges.
    pub(crate) fn record_transition_ticket(
        &mut self,
        ticket: u64,
        to: TicketStatus,
    ) -> Result<Outcome, String> {
        let current = self
            .ticket(ticket)
            .ok_or_else(|| format!("ticket #{ticket} is not registered"))?;
        let from = current.status;
        if !ticket_edges(from).contains(&to) {
            return Ok(Outcome::Refused(format!(
                "illegal transition: ticket #{ticket} cannot move from `{}` to `{}`; legal targets: {}",
                from.as_str(),
                to.as_str(),
                ticket_targets(from)
            )));
        }

        match to {
            TicketStatus::Done => {
                let verdict = gate::ticket_verdict(current);
                if !verdict.zero {
                    if let Some(open) = verdict.open_round {
                        return Ok(Outcome::Refused(format!(
                            "cannot mark ticket #{ticket} `done`: round {open} is still open; close it first"
                        )));
                    }
                    return Ok(Outcome::Refused(format!(
                        "cannot mark ticket #{ticket} `done`: its gate is not at zero ({} undispositioned finding(s) across {} round(s))",
                        verdict.undispositioned, verdict.rounds_used
                    )));
                }
            }
            TicketStatus::Fixing => {
                let open = current
                    .latest_round()
                    .map(|round| !round.is_zero())
                    .unwrap_or(false);
                if !open {
                    return Ok(Outcome::Refused(format!(
                        "cannot move ticket #{ticket} to `fixing`: there is no round with open findings (the latest round is absent or already at zero)"
                    )));
                }
            }
            _ => {}
        }

        if to == TicketStatus::Reviewing && from == TicketStatus::Escalated {
            let ticket_state = self
                .ticket_mut(ticket)
                .expect("ticket existence checked above");
            ticket_state.round_cap += DEFAULT_ROUND_CAP;
        }
        // The human's "grant the Worker another go" decision resets the dispatch
        // budget window, exactly as resuming an escalated run buys another cap's
        // worth of spec rounds.
        if to == TicketStatus::Implementing && from == TicketStatus::Escalated {
            let ticket_state = self
                .ticket_mut(ticket)
                .expect("ticket existence checked above");
            ticket_state.dispatch_budget_base = ticket_state.dispatches.len() as u64;
        }

        let ticket_state = self
            .ticket_mut(ticket)
            .expect("ticket existence checked above");
        ticket_state.status = to;
        bump(self);
        Ok(Outcome::Applied(envelope(
            "ticket-transition",
            self.revision,
            [
                ("ticket", json!(ticket)),
                ("from", json!(from.as_str())),
                ("to", json!(to.as_str())),
            ],
        )))
    }

    /// Open a review round for one gate layer. Opening a round beyond the cap is
    /// impossible: the state machine escalates the layer instead.
    pub(crate) fn record_open_round(&mut self, layer: GateLayer) -> Result<Outcome, String> {
        match layer {
            GateLayer::Ticket(ticket) => {
                // The zero check reads the gate verdict, the single owner of the
                // absolute-zero rule, before the ticket is borrowed mutably.
                let already_zero = self
                    .ticket(ticket)
                    .map(|ticket_state| gate::ticket_verdict(ticket_state).zero)
                    .unwrap_or(false);
                let ticket_state = self
                    .ticket_mut(ticket)
                    .ok_or_else(|| format!("ticket #{ticket} is not registered"))?;
                if let Some(open) = ticket_state.open_round() {
                    return Ok(Outcome::Refused(format!(
                        "round {} is still open for ticket #{ticket}; close it first",
                        open.round
                    )));
                }
                match ticket_state.status {
                    TicketStatus::Gating | TicketStatus::Fixing => {}
                    other => {
                        return Ok(Outcome::Refused(format!(
                            "cannot open a review round for ticket #{ticket} while it is `{}`; expected `gating` or `fixing`",
                            other.as_str()
                        )));
                    }
                }
                if already_zero {
                    return Ok(Outcome::Refused(format!(
                        "ticket #{ticket} is already at zero; transition it to `done` instead of reviewing again"
                    )));
                }
                if ticket_state.rounds.len() as u64 >= ticket_state.round_cap {
                    let round_cap = ticket_state.round_cap;
                    ticket_state.status = TicketStatus::Escalated;
                    bump(self);
                    return Ok(Outcome::RefusedWithMutation(format!(
                        "review round cap exhausted for ticket #{ticket} ({} rounds without zero); the ticket is now `escalated` and resumes only on an explicit human decision",
                        round_cap
                    )));
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
                bump(self);
                Ok(Outcome::Applied(envelope(
                    "round-open",
                    self.revision,
                    [
                        ("layer", json!("ticket")),
                        ("ticket", json!(ticket)),
                        ("round", json!(number)),
                        ("rounds_used", json!(rounds_used)),
                        ("round_cap", json!(round_cap)),
                    ],
                )))
            }
            GateLayer::Spec => {
                if self.status != RunStatus::SpecGating {
                    return Ok(Outcome::Refused(format!(
                        "cannot open a spec review round while the run is `{}`; expected `spec-gating`",
                        self.status.as_str()
                    )));
                }
                if let Some(open) = self.spec_gate.open_round() {
                    return Ok(Outcome::Refused(format!(
                        "spec round {} is still open; close it first",
                        open.round
                    )));
                }
                if gate::spec_verdict(self).zero {
                    return Ok(Outcome::Refused(
                        "the spec gate is already at zero; open the Spec PR instead of reviewing again"
                            .to_string(),
                    ));
                }
                if self.spec_gate.rounds.len() as u64 >= self.spec_gate.round_cap {
                    let round_cap = self.spec_gate.round_cap;
                    self.status = RunStatus::Escalated;
                    bump(self);
                    return Ok(Outcome::RefusedWithMutation(format!(
                        "spec review round cap exhausted ({} rounds without zero); the run is now `escalated` and resumes only on an explicit human decision",
                        round_cap
                    )));
                }
                let number = self
                    .spec_gate
                    .rounds
                    .last()
                    .map(|round| round.round + 1)
                    .unwrap_or(1);
                self.spec_gate.rounds.push(ReviewRound {
                    round: number,
                    status: RoundStatus::Reviewing,
                    findings: Vec::new(),
                });
                let rounds_used = self.spec_gate.rounds.len();
                let round_cap = self.spec_gate.round_cap;
                bump(self);
                Ok(Outcome::Applied(envelope(
                    "round-open",
                    self.revision,
                    [
                        ("layer", json!("spec")),
                        ("ticket", Value::Null),
                        ("round", json!(number)),
                        ("rounds_used", json!(rounds_used)),
                        ("round_cap", json!(round_cap)),
                    ],
                )))
            }
        }
    }

    /// Close one open review round and report its verdict.
    pub(crate) fn record_close_round(
        &mut self,
        layer: GateLayer,
        number: u64,
    ) -> Result<Outcome, String> {
        let round = round_mut(self, layer, number, "close", true)?;
        round.status = RoundStatus::Complete;
        bump(self);
        let verdict = verdict_for(self, layer).expect("layer existence checked above");
        Ok(Outcome::Applied(envelope(
            "round-close",
            self.revision,
            [("round", json!(number)), ("verdict", verdict.to_json())],
        )))
    }

    /// Record one finding on an open round. The disposition may be supplied right
    /// away (`--fixed` / `--rejected`), or later through `dispose_finding`.
    pub(crate) fn record_finding(
        &mut self,
        layer: GateLayer,
        number: u64,
        finding: NewFinding,
    ) -> Result<Outcome, String> {
        // Identity is what makes a finding checkable: an unnamed or unhashed
        // finding could never be matched against a re-issued review, so it is
        // refused rather than recorded as an anonymous row.
        require_text(&finding.id, "finding id")?;
        require_text(&finding.hash, "finding hash")?;
        require_text(&finding.summary, "finding summary")?;

        let round = round_mut(self, layer, number, "record a finding on", true)?;
        if round
            .findings
            .iter()
            .any(|existing| existing.id == finding.id)
        {
            return Ok(Outcome::Refused(format!(
                "finding {:?} is already recorded on round {number}",
                finding.id
            )));
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
        bump(self);
        let verdict = verdict_for(self, layer).expect("layer existence checked above");
        Ok(Outcome::Applied(envelope(
            "finding-record",
            self.revision,
            [
                ("round", json!(number)),
                ("finding", json!(finding.id)),
                ("axis", json!(axis_json)),
                ("disposition", json!(disposition_json)),
                ("verdict", verdict.to_json()),
            ],
        )))
    }

    /// Record the Director's disposition for one already-recorded finding.
    pub(crate) fn record_dispose_finding(
        &mut self,
        layer: GateLayer,
        number: u64,
        id: &str,
        disposition: FindingDisposition,
    ) -> Result<Outcome, String> {
        // Dispositions land after the review round closed: the Director records
        // the adjudication once the fixes are in, which is exactly when the round
        // is no longer collecting findings.
        let round = round_mut(self, layer, number, "dispose a finding on", false)?;
        if !disposition.is_recorded() {
            return Ok(Outcome::Refused(
                "a rejected disposition needs a written reason; `rejected` with an empty reason does not count"
                    .to_string(),
            ));
        }
        let finding = round
            .findings
            .iter_mut()
            .find(|finding| finding.id == id)
            .ok_or_else(|| format!("finding {id:?} is not recorded on round {number}"))?;
        if finding.disposition.is_some() {
            return Ok(Outcome::Refused(format!(
                "finding {id:?} already carries a disposition; dispositions are recorded once"
            )));
        }
        finding.disposition = Some(disposition);
        bump(self);
        let verdict = verdict_for(self, layer).expect("layer existence checked above");
        Ok(Outcome::Applied(envelope(
            "finding-dispose",
            self.revision,
            [
                ("round", json!(number)),
                ("finding", json!(id)),
                ("verdict", verdict.to_json()),
            ],
        )))
    }
}

// ── dispatch records ──

impl RunState {
    /// Register the start of one Worker dispatch attempt. The retry budget — a
    /// failed attempt plus exactly one same-Worker retry — is enforced here, so
    /// "the Worker gets another go" is a fact about state, not a sentence in a
    /// prompt (ADR 0048).
    pub(crate) fn record_begin_dispatch(
        &mut self,
        ticket: u64,
        worker: &str,
    ) -> Result<Outcome, String> {
        let worker = worker.trim();
        require_text(worker, "--worker")?;

        let ticket_state = self
            .ticket_mut(ticket)
            .ok_or_else(|| format!("ticket #{ticket} is not registered"))?;
        if ticket_state.status != TicketStatus::Implementing {
            return Ok(Outcome::Refused(format!(
                "cannot dispatch a Worker for ticket #{ticket} while it is `{}`; dispatches begin in `implementing`",
                ticket_state.status.as_str()
            )));
        }
        if let Some(open) = ticket_state.open_dispatch() {
            return Ok(Outcome::Refused(format!(
                "dispatch attempt {} for ticket #{ticket} is still open; finish it before starting another",
                open.attempt
            )));
        }
        let failures = ticket_state.failed_dispatches_in_streak();
        if failures > DISPATCH_RETRY_BUDGET {
            return Ok(Outcome::Refused(format!(
                "the dispatch retry budget for ticket #{ticket} is exhausted ({failures} failed attempts, {DISPATCH_RETRY_BUDGET} retry allowed); the ticket resumes only on an explicit human decision"
            )));
        }
        if failures == DISPATCH_RETRY_BUDGET {
            let failed_worker = ticket_state
                .last_failed_dispatch_in_streak()
                .map(|record| record.worker.as_str())
                .unwrap_or_default();
            if failed_worker != worker {
                return Ok(Outcome::Refused(format!(
                    "the retry for ticket #{ticket} must go to the same Worker that failed ({failed_worker:?}), not {worker:?}"
                )));
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
        bump(self);
        Ok(Outcome::Applied(envelope(
            "dispatch-begin",
            self.revision,
            [
                ("ticket", json!(ticket)),
                ("worker", json!(worker)),
                ("attempt", json!(attempt)),
                ("retry", json!(failures > 0)),
            ],
        )))
    }

    /// Validate and attach the Worker's `WORKER_REPORT` envelope to the open
    /// dispatch. A malformed envelope is a recorded dispatch failure, never a
    /// silent pass; the retry budget then decides whether another attempt is
    /// sanctioned or the ticket escalates.
    pub(crate) fn record_worker_report(
        &mut self,
        ticket: u64,
        raw: &str,
    ) -> Result<Outcome, String> {
        let (worker, attempt) = open_dispatch_identity(self, ticket)?;
        match report::parse_envelope(raw) {
            Ok(worker_report) => {
                let summary = worker_report.summary();
                let ticket_state = self
                    .ticket_mut(ticket)
                    .expect("ticket existence checked above");
                let open = ticket_state
                    .open_dispatch_mut()
                    .expect("open dispatch checked above");
                open.report = Some(worker_report);
                bump(self);
                Ok(Outcome::Applied(envelope(
                    "report-validate",
                    self.revision,
                    [
                        ("ticket", json!(ticket)),
                        ("worker", json!(worker)),
                        ("attempt", json!(attempt)),
                        ("report", summary),
                    ],
                )))
            }
            Err(detail) => {
                let reason = format!("malformed WORKER_REPORT: {detail}");
                let failure = self.record_failed_dispatch(ticket, &reason)?;
                Ok(Outcome::RefusedWithMutation(format!(
                    "{reason}\n{}",
                    failure.tail(ticket)
                )))
            }
        }
    }

    /// Finish the open dispatch as successful. Only a validated `done` report
    /// counts: a `blocked` self-report is a failed dispatch by definition, and a
    /// dispatch with no validated report has no evidence at all.
    pub(crate) fn record_finish_dispatch_ok(&mut self, ticket: u64) -> Result<Outcome, String> {
        let (worker, attempt) = open_dispatch_identity(self, ticket)?;
        let report_status = self
            .ticket(ticket)
            .and_then(TicketState::open_dispatch)
            .and_then(|open| open.report.as_ref())
            .map(|worker_report| worker_report.status);
        let Some(report_status) = report_status else {
            return Ok(Outcome::Refused(format!(
                "no validated WORKER_REPORT is attached to the open dispatch for ticket #{ticket}; run `director report validate` before finishing it"
            )));
        };
        if report_status == report::ReportStatus::Blocked {
            return Ok(Outcome::Refused(format!(
                "the attached WORKER_REPORT for ticket #{ticket} reports `blocked`; finish the dispatch with `--outcome failed --reason <blockers>` so the retry budget applies"
            )));
        }

        let ticket_state = self
            .ticket_mut(ticket)
            .expect("ticket existence checked above");
        let open = ticket_state
            .open_dispatch_mut()
            .expect("open dispatch checked above");
        open.status = DispatchStatus::Ok;
        bump(self);
        Ok(Outcome::Applied(envelope(
            "dispatch-finish",
            self.revision,
            [
                ("ticket", json!(ticket)),
                ("worker", json!(worker)),
                ("attempt", json!(attempt)),
                ("outcome", json!("ok")),
            ],
        )))
    }

    /// Finish the open dispatch as failed. The budget decides the ending: the
    /// first failure records a sanctioned retry, the one past the budget escalates
    /// the ticket and refuses to be recorded as another retry.
    pub(crate) fn record_finish_dispatch_failed(
        &mut self,
        ticket: u64,
        reason: &str,
    ) -> Result<Outcome, String> {
        let reason = reason.trim();
        require_text(reason, "--reason")?;

        let (worker, attempt) = open_dispatch_identity(self, ticket)?;
        let failure = self.record_failed_dispatch(ticket, reason)?;
        if failure.escalated {
            return Ok(Outcome::RefusedWithMutation(format!(
                "dispatch attempt {attempt} for ticket #{ticket} failed: {reason}\n{}",
                failure.tail(ticket)
            )));
        }
        Ok(Outcome::Applied(envelope(
            "dispatch-finish",
            self.revision,
            [
                ("ticket", json!(ticket)),
                ("worker", json!(worker)),
                ("attempt", json!(attempt)),
                ("outcome", json!("failed")),
                ("reason", json!(reason)),
                ("retry_remaining", json!(true)),
            ],
        )))
    }

    /// Close the open dispatch as failed and apply the retry budget. The state is
    /// written either way: the failure is a fact, and only the *next* attempt is
    /// refused.
    fn record_failed_dispatch(
        &mut self,
        ticket: u64,
        reason: &str,
    ) -> Result<DispatchFailure, String> {
        let ticket_state = self
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
        bump(self);
        Ok(DispatchFailure {
            failures,
            escalated,
        })
    }
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

// ── helpers ──

fn open_dispatch_identity(state: &RunState, ticket: u64) -> Result<(String, u64), String> {
    let ticket_state = state
        .ticket(ticket)
        .ok_or_else(|| format!("ticket #{ticket} is not registered"))?;
    let open = ticket_state.open_dispatch().ok_or_else(|| {
        format!("ticket #{ticket} has no open dispatch; run `director dispatch begin` first")
    })?;
    Ok((open.worker.clone(), open.attempt))
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

#[cfg(test)]
mod tests {
    use super::*;

    // ── fixtures: hand-built records, no filesystem, no subprocess ──

    fn finding(id: &str, disposition: Option<FindingDisposition>) -> ReviewFinding {
        ReviewFinding {
            id: id.to_string(),
            axis: ReviewAxis::Standards,
            hash: format!("hash-{id}"),
            summary: "note".to_string(),
            disposition,
        }
    }

    fn round(number: u64, status: RoundStatus, findings: Vec<ReviewFinding>) -> ReviewRound {
        ReviewRound {
            round: number,
            status,
            findings,
        }
    }

    fn fresh() -> RunState {
        RunState::new(
            "spec-128".to_string(),
            128,
            "codex/spec-128-autopilot-director".to_string(),
        )
    }

    fn ticket(ticket: u64, status: TicketStatus) -> TicketState {
        TicketState {
            ticket,
            title: "a ticket".to_string(),
            status,
            round_cap: DEFAULT_ROUND_CAP,
            blocked_by: Vec::new(),
            rounds: Vec::new(),
            dispatches: Vec::new(),
            dispatch_budget_base: 0,
        }
    }

    fn with_ticket(mut state: RunState, status: TicketStatus) -> RunState {
        state.tickets.push(ticket(200, status));
        state
    }

    /// The message of a hard structural error (`Result::Err`), which the CLI
    /// turns into the same non-zero exit as a refusal but which is not an
    /// `Outcome` at all.
    #[track_caller]
    fn hard_error<T>(result: Result<T, String>) -> String {
        result.err().expect("expected a hard error")
    }

    fn new_finding(id: &str, disposition: Option<FindingDisposition>) -> NewFinding {
        NewFinding {
            id: id.to_string(),
            axis: ReviewAxis::Standards,
            hash: format!("hash-{id}"),
            summary: "note".to_string(),
            disposition,
        }
    }

    /// A `done` envelope with a real commit, exactly as a Worker would send it.
    fn done_report() -> crate::report::WorkerReport {
        crate::report::parse_envelope(
            r#"WORKER_REPORT:
{
  "status": "done",
  "branch": "codex/132-worker-report",
  "commits": [{ "sha": "abc1234", "subject": "feat: envelope" }],
  "tests": [{ "command": "cargo test", "outcome": "pass", "evidence": "71 passed" }],
  "acceptance": [{ "criterion": "validated", "evidence": "tests/dispatch.rs" }],
  "blockers": []
}"#,
        )
        .expect("the fixture envelope parses")
    }

    /// A `blocked` envelope that names at least one blocker, as the schema needs.
    fn blocked_report() -> crate::report::WorkerReport {
        crate::report::parse_envelope(
            r#"WORKER_REPORT:
{
  "status": "blocked",
  "branch": "codex/132-worker-report",
  "commits": [],
  "tests": [],
  "acceptance": [],
  "blockers": ["the seam I need is not on the branch"]
}"#,
        )
        .expect("the fixture envelope parses")
    }

    fn fingerprint(seed: &str) -> WorktreeFingerprint {
        WorktreeFingerprint {
            head: format!("head-{seed}"),
            tree: format!("tree-{seed}"),
            branch: format!("branch-{seed}"),
        }
    }

    #[track_caller]
    fn refused(outcome: Outcome) -> String {
        match outcome {
            Outcome::Refused(reason) => reason,
            other => panic!("expected a clean refusal, got {other:?}"),
        }
    }

    #[track_caller]
    fn applied(outcome: Outcome) -> Value {
        match outcome {
            Outcome::Applied(envelope) => envelope,
            other => panic!("expected an applied outcome, got {other:?}"),
        }
    }

    #[track_caller]
    fn refused_with_mutation(outcome: Outcome) -> String {
        match outcome {
            Outcome::RefusedWithMutation(reason) => reason,
            other => panic!("expected a refusal that already mutated state, got {other:?}"),
        }
    }

    /// A ticket in `reviewing` with one open round.
    fn reviewing() -> RunState {
        let mut state = with_ticket(fresh(), TicketStatus::Reviewing);
        state.tickets[0]
            .rounds
            .push(round(1, RoundStatus::Reviewing, Vec::new()));
        state
    }

    /// A ticket in `implementing` ready to dispatch.
    fn implementing() -> RunState {
        with_ticket(fresh(), TicketStatus::Implementing)
    }

    fn escalated() -> RunState {
        with_ticket(fresh(), TicketStatus::Escalated)
    }

    fn spec_gating(rounds: Vec<ReviewRound>, round_cap: u64) -> RunState {
        let mut state = fresh();
        state.status = RunStatus::SpecGating;
        state.spec_gate.round_cap = round_cap;
        state.spec_gate.rounds = rounds;
        state
    }

    // ── add_ticket ──

    #[test]
    fn adding_a_ticket_registers_it_pending_and_bumps_the_revision() {
        let mut state = fresh();
        let envelope = applied(
            state
                .record_add_ticket(200, "  one  ", &[300, 300])
                .unwrap(),
        );
        assert_eq!(state.revision, 1);
        assert_eq!(state.tickets.len(), 1);
        assert_eq!(state.tickets[0].title, "one", "the title is trimmed");
        assert_eq!(
            state.tickets[0].blocked_by,
            vec![300],
            "blockers are sorted and deduped"
        );
        assert_eq!(envelope["command"], "ticket-add");
        assert_eq!(envelope["revision"], 1);
    }

    #[test]
    fn adding_an_already_registered_ticket_is_refused() {
        let mut state = with_ticket(fresh(), TicketStatus::Pending);
        assert_eq!(
            refused(state.record_add_ticket(200, "one", &[]).unwrap()),
            "ticket #200 is already registered"
        );
        assert_eq!(state.revision, 0, "a refusal never moves the revision");
    }

    #[test]
    fn adding_a_ticket_with_a_blank_title_is_refused() {
        let mut state = fresh();
        assert_eq!(
            refused(state.record_add_ticket(200, "   ", &[]).unwrap()),
            "--title must not be empty"
        );
        assert!(state.tickets.is_empty());
    }

    #[test]
    fn a_ticket_cannot_block_itself() {
        let mut state = fresh();
        assert_eq!(
            refused(state.record_add_ticket(200, "one", &[200, 300]).unwrap()),
            "ticket #200 cannot be blocked by itself"
        );
        assert!(state.tickets.is_empty());
    }

    // ── transition_run ──

    #[test]
    fn a_run_only_moves_along_its_legal_edges() {
        let mut state = fresh();
        let reason = refused(state.record_transition_run(RunStatus::Done).unwrap());
        assert!(
            reason.starts_with("illegal transition: run cannot move"),
            "{reason}"
        );
        assert!(reason.contains("legal targets:"), "{reason}");
        assert_eq!(state.revision, 0);

        let envelope = applied(state.record_transition_run(RunStatus::Running).unwrap());
        assert_eq!(state.status, RunStatus::Running);
        assert_eq!(envelope["command"], "run-transition");
        assert_eq!(envelope["from"], "init");
        assert_eq!(envelope["to"], "running");
        assert_eq!(envelope["revision"], 1);
    }

    #[test]
    fn entering_spec_gating_needs_every_ticket_done() {
        let mut state = with_ticket(fresh(), TicketStatus::Reviewing);
        state.status = RunStatus::Running;
        let reason = refused(state.record_transition_run(RunStatus::SpecGating).unwrap());
        assert!(
            reason.contains("tickets not done: #200 (reviewing)"),
            "{reason}"
        );
        assert_eq!(state.status, RunStatus::Running);
    }

    #[test]
    fn the_spec_pr_needs_a_zero_spec_gate() {
        // No round has run: the gate is not at zero.
        let mut state = fresh();
        state.status = RunStatus::SpecGating;
        let reason = refused(state.record_transition_run(RunStatus::PrOpen).unwrap());
        assert!(reason.contains("spec gate is not at zero"), "{reason}");

        // An open round blocks the move and says so first.
        let mut state = spec_gating(vec![round(1, RoundStatus::Reviewing, Vec::new())], 3);
        let reason = refused(state.record_transition_run(RunStatus::PrOpen).unwrap());
        assert!(reason.contains("spec round 1 is still open"), "{reason}");
    }

    #[test]
    fn resuming_an_escalated_run_buys_another_spec_round_cap() {
        let mut state = fresh();
        state.status = RunStatus::Escalated;
        applied(state.record_transition_run(RunStatus::SpecGating).unwrap());
        assert_eq!(state.spec_gate.round_cap, DEFAULT_ROUND_CAP * 2);
    }

    // ── resume ──

    #[test]
    fn a_resume_with_drift_refuses_and_names_both_sides() {
        let mut state = fresh();
        let path = std::path::Path::new("/tmp/worktree");
        state.worktree = Some(fingerprint("recorded"));
        let reason = refused(
            state
                .record_resume(path, fingerprint("live"), false)
                .unwrap(),
        );
        assert!(
            reason.contains("worktree drift detected in /tmp/worktree"),
            "{reason}"
        );
        assert!(reason.contains("head-recorded"), "{reason}");
        assert!(reason.contains("head-live"), "{reason}");
        assert!(reason.contains("--accept-drift"), "{reason}");
        assert_eq!(
            state.revision, 0,
            "a blocked resume leaves the revision alone"
        );
    }

    #[test]
    fn a_state_with_no_recorded_fingerprint_blocks_until_drift_is_accepted() {
        let mut state = fresh();
        let path = std::path::Path::new("/tmp/worktree");
        let reason = refused(
            state
                .record_resume(path, fingerprint("live"), false)
                .unwrap(),
        );
        assert!(reason.contains("no worktree fingerprint"), "{reason}");

        let envelope = applied(
            state
                .record_resume(path, fingerprint("live"), true)
                .unwrap(),
        );
        assert_eq!(state.worktree, Some(fingerprint("live")));
        assert_eq!(envelope["rebaselined"], true);
        assert_eq!(envelope["revision"], 1);
    }

    #[test]
    fn a_clean_resume_reports_the_run_without_touching_it() {
        let mut state = fresh();
        let path = std::path::Path::new("/tmp/worktree");
        state.worktree = Some(fingerprint("same"));
        let envelope = applied(
            state
                .record_resume(path, fingerprint("same"), false)
                .unwrap(),
        );
        assert_eq!(envelope["command"], "resume");
        assert_eq!(envelope["revision"], 0, "a clean resume never bumps");
        assert_eq!(envelope["rebaselined"], false);
        assert_eq!(envelope["drift"].as_array().unwrap().len(), 0);
    }

    // ── transition_ticket ──

    #[test]
    fn an_unregistered_ticket_is_a_hard_error() {
        let mut state = fresh();
        let err = state
            .record_transition_ticket(200, TicketStatus::Implementing)
            .unwrap_err();
        assert_eq!(err, "ticket #200 is not registered");
    }

    #[test]
    fn a_ticket_only_moves_along_its_legal_edges() {
        let mut state = with_ticket(fresh(), TicketStatus::Pending);
        let reason = refused(
            state
                .record_transition_ticket(200, TicketStatus::Done)
                .unwrap(),
        );
        assert!(
            reason.starts_with("illegal transition: ticket #200"),
            "{reason}"
        );
        assert_eq!(state.tickets[0].status, TicketStatus::Pending);
        assert_eq!(state.revision, 0);
    }

    #[test]
    fn done_needs_a_closed_zero_gate() {
        let mut state = reviewing();
        let reason = refused(
            state
                .record_transition_ticket(200, TicketStatus::Done)
                .unwrap(),
        );
        assert!(reason.contains("round 1 is still open"), "{reason}");

        state.tickets[0].rounds[0].status = RoundStatus::Complete;
        state.tickets[0].rounds[0].findings = vec![finding("f1", None)];
        let reason = refused(
            state
                .record_transition_ticket(200, TicketStatus::Done)
                .unwrap(),
        );
        assert!(
            reason.contains(
                "its gate is not at zero (1 undispositioned finding(s) across 1 round(s))"
            ),
            "{reason}"
        );

        state.tickets[0].rounds[0].findings = vec![finding(
            "f1",
            Some(FindingDisposition::Fixed { commit: None }),
        )];
        let envelope = applied(
            state
                .record_transition_ticket(200, TicketStatus::Done)
                .unwrap(),
        );
        assert_eq!(state.tickets[0].status, TicketStatus::Done);
        assert_eq!(envelope["from"], "reviewing");
        assert_eq!(envelope["to"], "done");
    }

    #[test]
    fn fixing_needs_a_latest_round_with_open_findings() {
        let mut state = reviewing();
        state.tickets[0].rounds[0].status = RoundStatus::Complete;
        state.tickets[0].rounds[0].findings = vec![finding(
            "f1",
            Some(FindingDisposition::Fixed { commit: None }),
        )];
        let reason = refused(
            state
                .record_transition_ticket(200, TicketStatus::Fixing)
                .unwrap(),
        );
        assert!(
            reason.contains("there is no round with open findings"),
            "{reason}"
        );

        state.tickets[0].rounds[0].findings[0].disposition = None;
        applied(
            state
                .record_transition_ticket(200, TicketStatus::Fixing)
                .unwrap(),
        );
        assert_eq!(state.tickets[0].status, TicketStatus::Fixing);
    }

    #[test]
    fn resuming_an_escalated_ticket_grants_a_cap_and_a_dispatch_window() {
        let mut state = escalated();
        state.tickets[0].dispatches.push(DispatchRecord {
            worker: "worker-a".to_string(),
            attempt: 1,
            status: DispatchStatus::Failed,
            reason: Some("boom".to_string()),
            report: None,
        });
        applied(
            state
                .record_transition_ticket(200, TicketStatus::Reviewing)
                .unwrap(),
        );
        assert_eq!(state.tickets[0].round_cap, DEFAULT_ROUND_CAP * 2);

        let mut state = escalated();
        state.tickets[0].dispatches.push(DispatchRecord {
            worker: "worker-a".to_string(),
            attempt: 1,
            status: DispatchStatus::Failed,
            reason: Some("boom".to_string()),
            report: None,
        });
        applied(
            state
                .record_transition_ticket(200, TicketStatus::Implementing)
                .unwrap(),
        );
        assert_eq!(
            state.tickets[0].dispatch_budget_base, 1,
            "the new budget window starts after the recorded failure"
        );
    }

    // ── open_round ──

    #[test]
    fn an_unregistered_ticket_cannot_open_a_round() {
        let mut state = fresh();
        let err = state.record_open_round(GateLayer::Ticket(200)).unwrap_err();
        assert_eq!(err, "ticket #200 is not registered");
    }

    #[test]
    fn a_round_cannot_open_outside_gating_or_fixing() {
        for status in [
            TicketStatus::Pending,
            TicketStatus::Implementing,
            TicketStatus::Reviewing,
        ] {
            let mut state = with_ticket(fresh(), status);
            let reason = refused(state.record_open_round(GateLayer::Ticket(200)).unwrap());
            assert!(
                reason.contains("expected `gating` or `fixing`"),
                "{status:?}: {reason}"
            );
        }
        for status in [TicketStatus::Gating, TicketStatus::Fixing] {
            let mut state = with_ticket(fresh(), status);
            applied(state.record_open_round(GateLayer::Ticket(200)).unwrap());
            assert_eq!(state.tickets[0].rounds.len(), 1);
            assert_eq!(state.tickets[0].status, TicketStatus::Reviewing);
        }
    }

    #[test]
    fn the_same_round_cannot_open_twice() {
        let mut state = reviewing();
        let reason = refused(state.record_open_round(GateLayer::Ticket(200)).unwrap());
        assert_eq!(
            reason,
            "round 1 is still open for ticket #200; close it first"
        );
        assert_eq!(state.tickets[0].rounds.len(), 1);
    }

    #[test]
    fn a_ticket_already_at_zero_cannot_open_another_round() {
        let mut state = with_ticket(fresh(), TicketStatus::Fixing);
        state.tickets[0].rounds.push(round(
            1,
            RoundStatus::Complete,
            vec![finding(
                "f1",
                Some(FindingDisposition::Fixed { commit: None }),
            )],
        ));
        let reason = refused(state.record_open_round(GateLayer::Ticket(200)).unwrap());
        assert!(reason.contains("is already at zero"), "{reason}");
    }

    #[test]
    fn the_exhausted_ticket_round_cap_escalates_and_refuses_with_a_mutation() {
        let mut state = with_ticket(fresh(), TicketStatus::Gating);
        state.tickets[0].round_cap = 1;
        state.tickets[0]
            .rounds
            .push(round(1, RoundStatus::Complete, vec![finding("f1", None)]));

        let reason =
            refused_with_mutation(state.record_open_round(GateLayer::Ticket(200)).unwrap());
        assert!(
            reason.contains("review round cap exhausted for ticket #200"),
            "{reason}"
        );
        assert!(reason.contains("now `escalated`"), "{reason}");
        assert_eq!(state.tickets[0].status, TicketStatus::Escalated);
        assert_eq!(
            state.tickets[0].rounds.len(),
            1,
            "no extra round was recorded"
        );
        assert_eq!(
            state.revision, 1,
            "the incident is already staged for commit"
        );
    }

    #[test]
    fn a_spec_round_only_opens_while_the_run_is_spec_gating() {
        let mut state = fresh();
        let reason = refused(state.record_open_round(GateLayer::Spec).unwrap());
        assert!(reason.contains("expected `spec-gating`"), "{reason}");
    }

    #[test]
    fn the_same_spec_round_cannot_open_twice_and_zero_blocks_a_new_one() {
        let mut state = spec_gating(vec![round(1, RoundStatus::Reviewing, Vec::new())], 3);
        let reason = refused(state.record_open_round(GateLayer::Spec).unwrap());
        assert_eq!(reason, "spec round 1 is still open; close it first");

        let mut state = spec_gating(vec![round(1, RoundStatus::Complete, Vec::new())], 3);
        let reason = refused(state.record_open_round(GateLayer::Spec).unwrap());
        assert!(reason.contains("already at zero"), "{reason}");
    }

    #[test]
    fn the_exhausted_spec_round_cap_escalates_the_run_and_refuses_with_a_mutation() {
        let rounds = (1..=3)
            .map(|number| round(number, RoundStatus::Complete, vec![finding("f", None)]))
            .collect();
        let mut state = spec_gating(rounds, 3);

        let reason = refused_with_mutation(state.record_open_round(GateLayer::Spec).unwrap());
        assert!(
            reason.contains("spec review round cap exhausted"),
            "{reason}"
        );
        assert!(reason.contains("the run is now `escalated`"), "{reason}");
        assert_eq!(state.status, RunStatus::Escalated);
        assert_eq!(state.spec_gate.rounds.len(), 3);
        assert_eq!(state.revision, 1);
    }

    #[test]
    fn an_open_spec_round_still_numbers_its_envelope_from_the_previous_one() {
        let mut state = spec_gating(
            vec![round(1, RoundStatus::Complete, vec![finding("f", None)])],
            3,
        );
        let envelope = applied(state.record_open_round(GateLayer::Spec).unwrap());
        assert_eq!(envelope["round"], 2);
        assert_eq!(envelope["rounds_used"], 2);
        assert!(
            envelope["ticket"].is_null(),
            "the spec layer keeps its null ticket"
        );
        assert_eq!(state.spec_gate.rounds.len(), 2);
    }

    // ── close_round ──

    #[test]
    fn closing_needs_a_recorded_open_round() {
        let mut state = reviewing();
        assert_eq!(
            hard_error(state.record_close_round(GateLayer::Ticket(200), 2)),
            "round 2 is not recorded for the ticket layer"
        );

        state.tickets[0].rounds[0].status = RoundStatus::Complete;
        let error = hard_error(state.record_close_round(GateLayer::Ticket(200), 1));
        assert!(
            error.contains("is already `complete`; only an open round can be written"),
            "{error}"
        );
    }

    #[test]
    fn closing_an_open_round_reports_the_verdict_it_produced() {
        let mut state = reviewing();
        let envelope = applied(state.record_close_round(GateLayer::Ticket(200), 1).unwrap());
        assert_eq!(state.tickets[0].rounds[0].status, RoundStatus::Complete);
        assert_eq!(envelope["command"], "round-close");
        assert_eq!(
            envelope["verdict"]["zero"], true,
            "a closed empty round did run"
        );
        assert_eq!(envelope["revision"], 1);
    }

    // ── record_finding ──

    #[test]
    fn a_finding_needs_an_identity() {
        let mut state = reviewing();
        for (finding, expected) in [
            (new_finding("  ", None), "finding id must not be empty"),
            (
                NewFinding {
                    id: "f1".to_string(),
                    axis: ReviewAxis::Standards,
                    hash: "   ".to_string(),
                    summary: "note".to_string(),
                    disposition: None,
                },
                "finding hash must not be empty",
            ),
            (
                NewFinding {
                    id: "f1".to_string(),
                    axis: ReviewAxis::Standards,
                    hash: "hash-f1".to_string(),
                    summary: "   ".to_string(),
                    disposition: None,
                },
                "finding summary must not be empty",
            ),
        ] {
            let err = state
                .record_finding(GateLayer::Ticket(200), 1, finding)
                .unwrap_err();
            assert_eq!(err, expected);
        }
        assert!(state.tickets[0].rounds[0].findings.is_empty());
    }

    #[test]
    fn a_finding_is_recorded_once_per_round() {
        let mut state = reviewing();
        applied(
            state
                .record_finding(GateLayer::Ticket(200), 1, new_finding("f1", None))
                .unwrap(),
        );
        let reason = refused(
            state
                .record_finding(GateLayer::Ticket(200), 1, new_finding("f1", None))
                .unwrap(),
        );
        assert_eq!(reason, "finding \"f1\" is already recorded on round 1");
        assert_eq!(state.tickets[0].rounds[0].findings.len(), 1);
        assert_eq!(state.revision, 1);
    }

    #[test]
    fn a_finding_cannot_land_on_a_closed_round() {
        let mut state = reviewing();
        state.tickets[0].rounds[0].status = RoundStatus::Complete;
        let error =
            hard_error(state.record_finding(GateLayer::Ticket(200), 1, new_finding("f1", None)));
        assert!(error.contains("already `complete`"), "{error}");
    }

    #[test]
    fn a_recorded_finding_echoes_its_axis_and_disposition() {
        let mut state = reviewing();
        let envelope = applied(
            state
                .record_finding(
                    GateLayer::Ticket(200),
                    1,
                    new_finding(
                        "f1",
                        Some(FindingDisposition::Rejected {
                            reason: "out of scope".to_string(),
                        }),
                    ),
                )
                .unwrap(),
        );
        assert_eq!(envelope["axis"], "standards");
        assert_eq!(envelope["disposition"], "rejected");
        assert_eq!(envelope["finding"], "f1");
        assert_eq!(state.tickets[0].rounds[0].findings.len(), 1);
    }

    #[test]
    fn recording_a_finding_needs_the_ticket_to_be_reviewing() {
        let mut state = with_ticket(fresh(), TicketStatus::Gating);
        state.tickets[0]
            .rounds
            .push(round(1, RoundStatus::Reviewing, Vec::new()));
        let error =
            hard_error(state.record_finding(GateLayer::Ticket(200), 1, new_finding("f1", None)));
        assert!(
            error.contains("review rounds live in `reviewing`"),
            "{error}"
        );
    }

    // ── dispose_finding ──

    #[test]
    fn a_disposition_needs_something_recorded() {
        let mut state = reviewing();
        state.tickets[0].rounds[0].status = RoundStatus::Complete;
        state.tickets[0].rounds[0].findings = vec![finding("f1", None)];

        let reason = refused(
            state
                .record_dispose_finding(
                    GateLayer::Ticket(200),
                    1,
                    "f1",
                    FindingDisposition::Rejected {
                        reason: "   ".to_string(),
                    },
                )
                .unwrap(),
        );
        assert!(reason.contains("needs a written reason"), "{reason}");
    }

    #[test]
    fn a_disposition_lands_exactly_once() {
        let mut state = reviewing();
        state.tickets[0].rounds[0].status = RoundStatus::Complete;
        state.tickets[0].rounds[0].findings = vec![finding("f1", None)];

        let envelope = applied(
            state
                .record_dispose_finding(
                    GateLayer::Ticket(200),
                    1,
                    "f1",
                    FindingDisposition::Fixed {
                        commit: Some("abc1234".to_string()),
                    },
                )
                .unwrap(),
        );
        assert_eq!(envelope["command"], "finding-dispose");
        assert_eq!(envelope["finding"], "f1");
        assert_eq!(envelope["verdict"]["undispositioned"], 0);

        let reason = refused(
            state
                .record_dispose_finding(
                    GateLayer::Ticket(200),
                    1,
                    "f1",
                    FindingDisposition::Fixed { commit: None },
                )
                .unwrap(),
        );
        assert!(reason.contains("already carries a disposition"), "{reason}");
    }

    #[test]
    fn an_unknown_finding_cannot_be_disposed() {
        let mut state = reviewing();
        state.tickets[0].rounds[0].status = RoundStatus::Complete;
        let err = state
            .record_dispose_finding(
                GateLayer::Ticket(200),
                1,
                "ghost",
                FindingDisposition::Fixed { commit: None },
            )
            .unwrap_err();
        assert_eq!(err, "finding \"ghost\" is not recorded on round 1");
    }

    // ── begin_dispatch ──

    #[test]
    fn a_dispatch_needs_a_worker_and_an_implementing_ticket() {
        let mut state = implementing();
        let err = state.record_begin_dispatch(200, "   ").unwrap_err();
        assert_eq!(err, "--worker must not be empty");

        let mut state = with_ticket(fresh(), TicketStatus::Gating);
        let reason = refused(state.record_begin_dispatch(200, "worker-a").unwrap());
        assert!(
            reason.contains("dispatches begin in `implementing`"),
            "{reason}"
        );
        assert!(state.tickets[0].dispatches.is_empty());
    }

    #[test]
    fn an_open_dispatch_must_finish_before_another_begins() {
        let mut state = implementing();
        applied(state.record_begin_dispatch(200, "worker-a").unwrap());
        let reason = refused(state.record_begin_dispatch(200, "worker-a").unwrap());
        assert_eq!(
            reason,
            "dispatch attempt 1 for ticket #200 is still open; finish it before starting another"
        );
    }

    #[test]
    fn the_retry_budget_refuses_an_attempt_after_escalation() {
        // A ticket that is `implementing` again — the human decision edge — but
        // whose budget window already holds two failures: past the budget.
        let mut state = implementing();
        let ticket_state = state.ticket_mut(200).unwrap();
        for attempt in 1..=2 {
            ticket_state.dispatches.push(DispatchRecord {
                worker: "worker-a".to_string(),
                attempt,
                status: DispatchStatus::Failed,
                reason: Some("boom".to_string()),
                report: None,
            });
        }

        let reason = refused(state.record_begin_dispatch(200, "worker-a").unwrap());
        assert!(
            reason.contains("retry budget for ticket #200 is exhausted"),
            "{reason}"
        );
        assert!(
            reason.contains("(2 failed attempts, 1 retry allowed)"),
            "{reason}"
        );
        assert_eq!(state.tickets[0].dispatches.len(), 2);
    }

    #[test]
    fn the_sanctioned_retry_must_reuse_the_failed_worker() {
        let mut state = implementing();
        state.tickets[0].dispatches.push(DispatchRecord {
            worker: "worker-a".to_string(),
            attempt: 1,
            status: DispatchStatus::Failed,
            reason: Some("boom".to_string()),
            report: None,
        });

        let reason = refused(state.record_begin_dispatch(200, "worker-b").unwrap());
        assert_eq!(
            reason,
            "the retry for ticket #200 must go to the same Worker that failed (\"worker-a\"), not \"worker-b\""
        );

        let envelope = applied(state.record_begin_dispatch(200, "worker-a").unwrap());
        assert_eq!(envelope["attempt"], 2);
        assert_eq!(envelope["retry"], true);
        assert_eq!(state.tickets[0].dispatches.len(), 2);
    }

    #[test]
    fn the_first_dispatch_reports_no_retry() {
        let mut state = implementing();
        let envelope = applied(state.record_begin_dispatch(200, " worker-a ").unwrap());
        assert_eq!(envelope["worker"], "worker-a");
        assert_eq!(envelope["attempt"], 1);
        assert_eq!(envelope["retry"], false);
        assert_eq!(
            state.tickets[0].dispatches[0].status,
            DispatchStatus::Started
        );
    }

    // ── record_worker_report ──

    #[test]
    fn a_report_needs_an_open_dispatch() {
        let mut state = implementing();
        let err = state.record_worker_report(200, "junk").unwrap_err();
        assert_eq!(
            err,
            "ticket #200 has no open dispatch; run `director dispatch begin` first"
        );
    }

    #[test]
    fn a_malformed_report_is_a_refusal_that_already_mutated_state() {
        let mut state = implementing();
        applied(state.record_begin_dispatch(200, "worker-a").unwrap());

        let reason =
            refused_with_mutation(state.record_worker_report(200, "no envelope here").unwrap());
        assert!(reason.contains("malformed WORKER_REPORT"), "{reason}");
        assert!(reason.contains("one same-Worker retry remains"), "{reason}");
        assert_eq!(
            state.tickets[0].dispatches[0].status,
            DispatchStatus::Failed,
            "the failure is the recorded fact"
        );
        assert_eq!(state.tickets[0].status, TicketStatus::Implementing);
    }

    #[test]
    fn a_second_malformed_report_escalates_through_the_same_path() {
        let mut state = implementing();
        state.tickets[0].dispatches.push(DispatchRecord {
            worker: "worker-a".to_string(),
            attempt: 1,
            status: DispatchStatus::Failed,
            reason: Some("boom".to_string()),
            report: None,
        });
        applied(state.record_begin_dispatch(200, "worker-a").unwrap());

        let reason = refused_with_mutation(state.record_worker_report(200, "still junk").unwrap());
        assert!(reason.contains("now `escalated`"), "{reason}");
        assert_eq!(state.tickets[0].status, TicketStatus::Escalated);
        assert_eq!(
            state.tickets[0].dispatches[1].status,
            DispatchStatus::Failed
        );
    }

    #[test]
    fn a_validated_report_is_attached_and_summarized() {
        let mut state = implementing();
        applied(state.record_begin_dispatch(200, "worker-a").unwrap());

        let raw = r#"WORKER_REPORT:
{
  "status": "done",
  "branch": "codex/132-worker-report",
  "commits": [{ "sha": "abc1234", "subject": "feat: envelope" }],
  "tests": [{ "command": "cargo test", "outcome": "pass", "evidence": "71 passed" }],
  "acceptance": [{ "criterion": "validated", "evidence": "tests/dispatch.rs" }],
  "blockers": []
}"#;
        let envelope = applied(state.record_worker_report(200, raw).unwrap());
        assert_eq!(envelope["command"], "report-validate");
        assert_eq!(envelope["report"]["status"], "done");
        assert_eq!(envelope["report"]["commits"], 1);
        assert_eq!(
            state.tickets[0].dispatches[0].status,
            DispatchStatus::Started
        );
        assert!(state.tickets[0].dispatches[0].report.is_some());
    }

    // ── finish_dispatch_ok ──

    #[test]
    fn finishing_ok_needs_a_validated_report() {
        let mut state = implementing();
        applied(state.record_begin_dispatch(200, "worker-a").unwrap());
        let reason = refused(state.record_finish_dispatch_ok(200).unwrap());
        assert!(reason.contains("no validated WORKER_REPORT"), "{reason}");
        assert_eq!(
            state.tickets[0].dispatches[0].status,
            DispatchStatus::Started
        );
    }

    #[test]
    fn a_blocked_report_cannot_finish_ok() {
        let mut state = implementing();
        state.tickets[0].dispatches.push(DispatchRecord {
            worker: "worker-a".to_string(),
            attempt: 1,
            status: DispatchStatus::Started,
            reason: None,
            report: Some(blocked_report()),
        });
        let reason = refused(state.record_finish_dispatch_ok(200).unwrap());
        assert!(reason.contains("reports `blocked`"), "{reason}");
    }

    #[test]
    fn a_validated_done_report_finishes_the_dispatch_ok() {
        let mut state = implementing();
        state.tickets[0].dispatches.push(DispatchRecord {
            worker: "worker-a".to_string(),
            attempt: 1,
            status: DispatchStatus::Started,
            reason: None,
            report: Some(done_report()),
        });
        let envelope = applied(state.record_finish_dispatch_ok(200).unwrap());
        assert_eq!(envelope["outcome"], "ok");
        assert_eq!(envelope["attempt"], 1);
        assert_eq!(state.tickets[0].dispatches[0].status, DispatchStatus::Ok);
    }

    // ── finish_dispatch_failed ──

    #[test]
    fn a_failed_finish_needs_a_written_reason() {
        let mut state = implementing();
        applied(state.record_begin_dispatch(200, "worker-a").unwrap());
        let err = state.record_finish_dispatch_failed(200, "   ").unwrap_err();
        assert_eq!(err, "--reason must not be empty");
        assert_eq!(
            state.tickets[0].dispatches[0].status,
            DispatchStatus::Started
        );
    }

    #[test]
    fn the_first_failure_is_applied_and_keeps_one_retry() {
        let mut state = implementing();
        applied(state.record_begin_dispatch(200, "worker-a").unwrap());
        let envelope = applied(
            state
                .record_finish_dispatch_failed(200, "cargo cannot run offline")
                .unwrap(),
        );
        assert_eq!(envelope["outcome"], "failed");
        assert_eq!(envelope["reason"], "cargo cannot run offline");
        assert_eq!(envelope["retry_remaining"], true);
        assert_eq!(
            state.tickets[0].dispatches[0].status,
            DispatchStatus::Failed
        );
        assert_eq!(
            state.tickets[0].dispatches[0].reason.as_deref(),
            Some("cargo cannot run offline")
        );
        assert_eq!(state.tickets[0].status, TicketStatus::Implementing);
    }

    #[test]
    fn a_failure_past_the_budget_escalates_and_refuses_with_a_mutation() {
        let mut state = implementing();
        state.tickets[0].dispatches.push(DispatchRecord {
            worker: "worker-a".to_string(),
            attempt: 1,
            status: DispatchStatus::Failed,
            reason: Some("boom".to_string()),
            report: None,
        });
        applied(state.record_begin_dispatch(200, "worker-a").unwrap());

        let reason = refused_with_mutation(
            state
                .record_finish_dispatch_failed(200, "still failing")
                .unwrap(),
        );
        assert!(
            reason.starts_with("dispatch attempt 2 for ticket #200 failed: still failing"),
            "{reason}"
        );
        assert!(reason.contains("now `escalated`"), "{reason}");
        assert_eq!(state.tickets[0].status, TicketStatus::Escalated);
        assert_eq!(
            state.tickets[0].dispatches[1].status,
            DispatchStatus::Failed
        );
        assert_eq!(state.revision, 2);
    }

    #[test]
    fn a_failure_without_an_open_dispatch_is_refused() {
        let mut state = implementing();
        let err = state
            .record_finish_dispatch_failed(200, "boom")
            .unwrap_err();
        assert_eq!(
            err,
            "ticket #200 has no open dispatch; run `director dispatch begin` first"
        );
    }

    // ── the envelope builder ──

    #[test]
    fn the_envelope_builder_sorts_keys_and_appends_the_revision() {
        let envelope = envelope(
            "probe",
            7,
            [
                ("zeta", json!(1)),
                ("alpha", json!("two")),
                ("mid", Value::Null),
            ],
        );
        assert_eq!(
            serde_json::to_string(&envelope).unwrap(),
            r#"{"alpha":"two","command":"probe","mid":null,"revision":7,"zeta":1}"#
        );
    }

    #[test]
    fn the_envelope_builder_keeps_unknown_insertion_order_out_of_the_output() {
        let forward = envelope("probe", 1, [("b", json!(1)), ("a", json!(2))]);
        let backward = envelope("probe", 1, [("a", json!(2)), ("b", json!(1))]);
        assert_eq!(forward, backward);
    }
}
