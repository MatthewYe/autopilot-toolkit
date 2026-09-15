//! Gate verdicts — the absolute-zero arithmetic (ADR 0048).
//!
//! A gate layer passes only at absolute zero: at least one review round has
//! run, and every finding recorded in every round carries a disposition that
//! counts (fixed, or rejected with a written reason). The verdict is computed
//! from the recorded Finding dispositions, never asserted by prose.

use serde_json::{json, Value};

use crate::state::{ReviewRound, RoundStatus, RunState, TicketState};

/// One of the two gate layers: a single ticket, or the aggregate spec diff.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GateLayer {
    Ticket(u64),
    Spec,
}

impl GateLayer {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Ticket(_) => "ticket",
            Self::Spec => "spec",
        }
    }

    pub(crate) fn ticket(self) -> Option<u64> {
        match self {
            Self::Ticket(ticket) => Some(ticket),
            Self::Spec => None,
        }
    }
}

/// The computed verdict for one gate layer.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GateVerdict {
    pub(crate) layer: GateLayer,
    pub(crate) rounds_used: u64,
    pub(crate) round_cap: u64,
    pub(crate) open_round: Option<u64>,
    pub(crate) findings_total: u64,
    pub(crate) undispositioned: u64,
    pub(crate) zero: bool,
    pub(crate) cap_exhausted: bool,
}

impl GateVerdict {
    pub(crate) fn compute(layer: GateLayer, rounds: &[ReviewRound], round_cap: u64) -> Self {
        let findings_total = rounds
            .iter()
            .map(|round| round.findings.len() as u64)
            .sum::<u64>();
        let undispositioned = rounds
            .iter()
            .flat_map(|round| round.findings.iter())
            .filter(|finding| {
                !finding
                    .disposition
                    .as_ref()
                    .is_some_and(|disposition| disposition.is_recorded())
            })
            .count() as u64;
        let rounds_used = rounds.len() as u64;
        let zero = rounds_used > 0 && undispositioned == 0;
        Self {
            layer,
            rounds_used,
            round_cap,
            open_round: rounds
                .iter()
                .find(|round| round.status == RoundStatus::Reviewing)
                .map(|round| round.round),
            findings_total,
            undispositioned,
            zero,
            cap_exhausted: rounds_used >= round_cap && !zero,
        }
    }

    pub(crate) fn to_json(&self) -> Value {
        json!({
            "layer": self.layer.as_str(),
            "ticket": self.layer.ticket(),
            "rounds_used": self.rounds_used,
            "round_cap": self.round_cap,
            "open_round": self.open_round,
            "findings_total": self.findings_total,
            "undispositioned": self.undispositioned,
            "zero": self.zero,
            "cap_exhausted": self.cap_exhausted,
        })
    }
}

pub(crate) fn ticket_verdict(ticket: &TicketState) -> GateVerdict {
    GateVerdict::compute(
        GateLayer::Ticket(ticket.ticket),
        &ticket.rounds,
        ticket.round_cap,
    )
}

pub(crate) fn spec_verdict(run: &RunState) -> GateVerdict {
    GateVerdict::compute(
        GateLayer::Spec,
        &run.spec_gate.rounds,
        run.spec_gate.round_cap,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{FindingDisposition, ReviewAxis, ReviewFinding};

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

    #[test]
    fn a_layer_without_rounds_is_not_zero() {
        let verdict = GateVerdict::compute(GateLayer::Ticket(200), &[], 3);
        assert!(!verdict.zero);
        assert_eq!(verdict.rounds_used, 0);
        assert!(!verdict.cap_exhausted);
    }

    #[test]
    fn an_undispositioned_finding_keeps_the_gate_open() {
        let rounds = vec![round(
            1,
            RoundStatus::Complete,
            vec![
                finding("f1", Some(FindingDisposition::Fixed { commit: None })),
                finding("f2", None),
            ],
        )];
        let verdict = GateVerdict::compute(GateLayer::Ticket(200), &rounds, 3);
        assert!(!verdict.zero);
        assert_eq!(verdict.findings_total, 2);
        assert_eq!(verdict.undispositioned, 1);
    }

    #[test]
    fn a_rejection_needs_a_written_reason_to_count() {
        let rounds = vec![round(
            1,
            RoundStatus::Complete,
            vec![finding(
                "f1",
                Some(FindingDisposition::Rejected {
                    reason: "   ".to_string(),
                }),
            )],
        )];
        let verdict = GateVerdict::compute(GateLayer::Spec, &rounds, 3);
        assert!(!verdict.zero);
        assert_eq!(verdict.undispositioned, 1);
    }

    #[test]
    fn every_finding_dispositioned_is_absolute_zero() {
        let rounds = vec![
            round(
                1,
                RoundStatus::Complete,
                vec![finding(
                    "f1",
                    Some(FindingDisposition::Rejected {
                        reason: "out of scope for this ticket".to_string(),
                    }),
                )],
            ),
            round(
                2,
                RoundStatus::Reviewing,
                vec![finding(
                    "f2",
                    Some(FindingDisposition::Fixed {
                        commit: Some("abc1234".to_string()),
                    }),
                )],
            ),
        ];
        let verdict = GateVerdict::compute(GateLayer::Spec, &rounds, 3);
        assert!(verdict.zero);
        assert_eq!(verdict.rounds_used, 2);
        assert_eq!(verdict.open_round, Some(2));
        assert!(!verdict.cap_exhausted);
    }

    #[test]
    fn the_cap_exhausts_only_without_zero() {
        let open = || vec![round(1, RoundStatus::Complete, vec![finding("f1", None)])];
        let exhausted = GateVerdict::compute(GateLayer::Ticket(200), &open(), 1);
        assert!(exhausted.cap_exhausted);

        let resolved = vec![round(
            1,
            RoundStatus::Complete,
            vec![finding(
                "f1",
                Some(FindingDisposition::Fixed { commit: None }),
            )],
        )];
        let zero = GateVerdict::compute(GateLayer::Ticket(200), &resolved, 1);
        assert!(zero.zero);
        assert!(!zero.cap_exhausted);
    }
}
