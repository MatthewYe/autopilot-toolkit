//! Typed owner of the `.director/state.json` schema.
//!
//! The load path is `read → migrate_to_current → RunState`; the write path is
//! `RunState → to_vec_pretty`. Every persisted key is modeled: unknown keys and
//! unknown enum values fail closed rather than round-tripping as free-form
//! JSON, so the schema stays a typed contract instead of a bag of values.
//!
//! This module owns the schema and the gate arithmetic that reads it. The
//! arithmetic (`is_recorded` / `is_zero` / `cap_exhausted`) is exercised by the
//! unit tests here and consumed by the transition commands that follow; those
//! call sites are still unwired, so the lint is allowed rather than dodged by
//! deleting the seam the tests exist to pin.

#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::storage;
use crate::CURRENT_SCHEMA_VERSION;

// ── enums ──

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum RunStatus {
    Active,
    Escalated,
    SpecPrOpen,
    Completed,
    Aborted,
}

impl RunStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Escalated => "escalated",
            Self::SpecPrOpen => "spec-pr-open",
            Self::Completed => "completed",
            Self::Aborted => "aborted",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum TicketStatus {
    Pending,
    Implementing,
    Reviewing,
    Done,
    Escalated,
}

impl TicketStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Implementing => "implementing",
            Self::Reviewing => "reviewing",
            Self::Done => "done",
            Self::Escalated => "escalated",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum RoundStatus {
    Reviewing,
    Complete,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ReviewAxis {
    Standards,
    Spec,
}

/// Internally tagged so the two dispositions serialize as
/// `{"status": "fixed", ...}` / `{"status": "rejected", "reason": "..."}`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case", tag = "status")]
pub(crate) enum FindingDisposition {
    Fixed { commit: Option<String> },
    Rejected { reason: String },
}

impl FindingDisposition {
    /// The absolute-zero bar counts only recorded dispositions; a finding
    /// without one keeps its gate open.
    pub(crate) fn is_recorded(&self) -> bool {
        match self {
            Self::Fixed { .. } => true,
            Self::Rejected { reason } => !reason.trim().is_empty(),
        }
    }
}

// ── records ──

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReviewFinding {
    pub(crate) id: String,
    pub(crate) axis: ReviewAxis,
    pub(crate) summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) disposition: Option<FindingDisposition>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReviewRound {
    pub(crate) round: u64,
    pub(crate) status: RoundStatus,
    #[serde(default)]
    pub(crate) findings: Vec<ReviewFinding>,
}

impl ReviewRound {
    /// A round reaches zero only when every finding carries a recorded
    /// disposition (fixed, or rejected with a written reason).
    pub(crate) fn is_zero(&self) -> bool {
        self.findings.iter().all(|finding| {
            finding
                .disposition
                .as_ref()
                .is_some_and(|d| d.is_recorded())
        })
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TicketState {
    pub(crate) ticket: u64,
    pub(crate) title: String,
    pub(crate) status: TicketStatus,
    pub(crate) round_cap: u64,
    #[serde(default)]
    pub(crate) blocked_by: Vec<u64>,
    #[serde(default)]
    pub(crate) rounds: Vec<ReviewRound>,
}

impl TicketState {
    /// The absolute-zero gate at the ticket layer: every round recorded so far
    /// is at zero, and at least one round has run.
    pub(crate) fn gate_is_zero(&self) -> bool {
        !self.rounds.is_empty() && self.rounds.iter().all(ReviewRound::is_zero)
    }

    /// The layer has run out of rounds without reaching zero.
    pub(crate) fn cap_exhausted(&self) -> bool {
        self.rounds.len() as u64 >= self.round_cap && !self.gate_is_zero()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SpecGate {
    pub(crate) round_cap: u64,
    #[serde(default)]
    pub(crate) rounds: Vec<ReviewRound>,
}

impl SpecGate {
    pub(crate) fn is_zero(&self) -> bool {
        !self.rounds.is_empty() && self.rounds.iter().all(ReviewRound::is_zero)
    }

    pub(crate) fn cap_exhausted(&self) -> bool {
        self.rounds.len() as u64 >= self.round_cap && !self.is_zero()
    }
}

// ── run state ──

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RunState {
    pub(crate) schema_version: u64,
    /// Harness session that opened the run; the identity a resumed session
    /// must match.
    pub(crate) run_id: String,
    pub(crate) spec_issue: u64,
    /// Spec branch carrying the run's Ticket boundary commits.
    pub(crate) branch: String,
    pub(crate) status: RunStatus,
    pub(crate) revision: u64,
    #[serde(default)]
    pub(crate) tickets: Vec<TicketState>,
    pub(crate) spec_gate: SpecGate,
}

impl RunState {
    /// The initial state of a fresh Spec run.
    pub(crate) fn new(run_id: String, spec_issue: u64, branch: String) -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            run_id,
            spec_issue,
            branch,
            status: RunStatus::Active,
            revision: 0,
            tickets: Vec::new(),
            spec_gate: SpecGate {
                round_cap: storage::DEFAULT_ROUND_CAP,
                rounds: Vec::new(),
            },
        }
    }
}

// ── paths ──

pub(crate) fn state_path(worktree: &Path) -> PathBuf {
    worktree.join(".director/state.json")
}

/// Read the run state, applying forward-only migrations before the typed gate.
pub(crate) fn read_state(worktree: &Path) -> Result<RunState, String> {
    let path = state_path(worktree);
    let original = fs::read_to_string(&path).map_err(|err| format!("cannot read state: {err}"))?;
    let mut value: serde_json::Value =
        serde_json::from_str(&original).map_err(|err| format!("cannot parse state: {err}"))?;
    migrate_to_current(&mut value)?;
    deserialize_state(value)
}

pub(crate) fn write_state(worktree: &Path, state: &RunState) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(state).map_err(|err| format!("json error: {err}"))?;
    storage::atomic_write(&state_path(worktree), &bytes)
}

/// The typed gate: state that does not match the schema fails closed.
pub(crate) fn deserialize_state(value: serde_json::Value) -> Result<RunState, String> {
    serde_json::from_value(value)
        .map_err(|err| format!("state does not match the run state schema: {err}"))
}

/// Forward-only migration to [`CURRENT_SCHEMA_VERSION`]. Pre-release schema 0
/// (no persisted specimen yet) differs from schema 1 by carrying neither the
/// ticket ledger nor the spec gate; it is upgraded by re-deriving both from the
/// run-level fields it does carry. Unknown versions stop closed.
pub(crate) fn migrate_to_current(value: &mut serde_json::Value) -> Result<(), String> {
    let from_schema = value
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| "state is missing schema_version".to_string())?;
    if from_schema == CURRENT_SCHEMA_VERSION {
        return Ok(());
    }
    if from_schema != 0 {
        return Err(format!("unknown state schema {from_schema}"));
    }
    if let Some(object) = value.as_object_mut() {
        object.insert(
            "schema_version".to_string(),
            serde_json::Value::from(CURRENT_SCHEMA_VERSION),
        );
        object
            .entry("tickets".to_string())
            .or_insert_with(|| serde_json::Value::Array(Vec::new()));
        object.entry("spec_gate".to_string()).or_insert_with(
            || serde_json::json!({ "round_cap": storage::DEFAULT_ROUND_CAP, "rounds": [] }),
        );
    }
    Ok(())
}

/// Reject a second `init` against an existing run instead of silently
/// re-creating state.
pub(crate) fn ensure_no_existing_run(worktree: &Path) -> Result<(), String> {
    let path = state_path(worktree);
    if path.exists() {
        return Err(format!(
            "a Spec run already exists at {}; resume it instead of re-initializing",
            path.display()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn populated_state() -> RunState {
        let mut state = RunState::new(
            "spec-128".to_string(),
            128,
            "codex/spec-128-autopilot-director".to_string(),
        );
        state.revision = 4;
        state.tickets.push(TicketState {
            ticket: 130,
            title: "director-cli skeleton".to_string(),
            status: TicketStatus::Reviewing,
            round_cap: 3,
            blocked_by: vec![129],
            rounds: vec![ReviewRound {
                round: 1,
                status: RoundStatus::Complete,
                findings: vec![
                    ReviewFinding {
                        id: "f1".to_string(),
                        axis: ReviewAxis::Standards,
                        summary: "duplicated load path".to_string(),
                        disposition: Some(FindingDisposition::Fixed {
                            commit: Some("abc1234".to_string()),
                        }),
                    },
                    ReviewFinding {
                        id: "f2".to_string(),
                        axis: ReviewAxis::Spec,
                        summary: "acceptance criterion unverified".to_string(),
                        disposition: Some(FindingDisposition::Rejected {
                            reason: "criterion is out of scope for this ticket".to_string(),
                        }),
                    },
                    ReviewFinding {
                        id: "f3".to_string(),
                        axis: ReviewAxis::Spec,
                        summary: "still open".to_string(),
                        disposition: None,
                    },
                ],
            }],
        });
        state
    }

    #[test]
    fn schema_round_trips_without_loss() {
        let state = populated_state();
        let bytes = serde_json::to_vec_pretty(&state).unwrap();
        let reloaded: RunState = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(state, reloaded);
    }

    #[test]
    fn fresh_state_starts_at_revision_zero() {
        let state = RunState::new("spec-128".to_string(), 128, "codex/spec-128".to_string());
        assert_eq!(state.revision, 0);
        assert_eq!(state.status, RunStatus::Active);
        assert_eq!(state.schema_version, CURRENT_SCHEMA_VERSION);
        assert!(state.tickets.is_empty());
        assert!(state.spec_gate.rounds.is_empty());
    }

    #[test]
    fn disposition_requires_a_recorded_resolution() {
        assert!(FindingDisposition::Fixed { commit: None }.is_recorded());
        assert!(FindingDisposition::Rejected {
            reason: "out of scope".to_string()
        }
        .is_recorded());
        assert!(!FindingDisposition::Rejected {
            reason: "   ".to_string()
        }
        .is_recorded());
    }

    #[test]
    fn round_reaches_zero_only_when_every_finding_is_dispositioned() {
        let state = populated_state();
        assert!(!state.tickets[0].rounds[0].is_zero());
        assert!(!state.tickets[0].gate_is_zero());
    }

    #[test]
    fn round_is_zero_once_all_findings_are_dispositioned() {
        let mut round = populated_state().tickets[0].rounds[0].clone();
        round.findings[2].disposition = Some(FindingDisposition::Fixed { commit: None });
        assert!(round.is_zero());
    }

    #[test]
    fn gate_is_not_zero_before_any_round_runs() {
        let state = RunState::new("spec-128".to_string(), 128, "codex/spec-128".to_string());
        assert!(!state.spec_gate.is_zero());
        assert!(!state.spec_gate.cap_exhausted());
    }

    #[test]
    fn layer_escalates_only_after_the_cap_is_exhausted_without_zero() {
        let mut spec_gate = SpecGate {
            round_cap: 3,
            rounds: Vec::new(),
        };
        for round in 1..=3 {
            spec_gate.rounds.push(round_with_open_finding(round));
        }
        assert!(spec_gate.cap_exhausted());

        // Dispositioning only the latest round is not enough: an earlier
        // round's open finding keeps the whole layer's gate open.
        spec_gate.rounds[2].findings[0].disposition =
            Some(FindingDisposition::Fixed { commit: None });
        assert!(!spec_gate.is_zero());

        for round in spec_gate.rounds.iter_mut() {
            round.findings[0].disposition = Some(FindingDisposition::Fixed { commit: None });
        }
        assert!(spec_gate.is_zero());
        assert!(!spec_gate.cap_exhausted());
    }

    fn round_with_open_finding(round: u64) -> ReviewRound {
        ReviewRound {
            round,
            status: RoundStatus::Complete,
            findings: vec![ReviewFinding {
                id: format!("f{round}"),
                axis: ReviewAxis::Standards,
                summary: "open".to_string(),
                disposition: None,
            }],
        }
    }

    #[test]
    fn unknown_keys_fail_closed() {
        let mut value = serde_json::to_value(populated_state()).unwrap();
        value["unexpected"] = serde_json::json!("field");
        let error = deserialize_state(value).unwrap_err();
        assert!(error.contains("unknown field"), "got: {error}");
    }

    #[test]
    fn unknown_run_status_fails_closed() {
        let mut value = serde_json::to_value(populated_state()).unwrap();
        value["status"] = serde_json::json!("paused");
        let error = deserialize_state(value).unwrap_err();
        assert!(error.contains("state does not match"), "got: {error}");
    }

    #[test]
    fn schema_zero_migrates_forward() {
        let mut value = serde_json::json!({
            "schema_version": 0,
            "run_id": "spec-128",
            "spec_issue": 128,
            "branch": "codex/spec-128",
            "status": "active",
            "revision": 0
        });
        migrate_to_current(&mut value).unwrap();
        let state = deserialize_state(value).unwrap();
        assert_eq!(state.schema_version, CURRENT_SCHEMA_VERSION);
        assert!(state.tickets.is_empty());
        assert_eq!(state.spec_gate.round_cap, storage::DEFAULT_ROUND_CAP);
    }

    #[test]
    fn newer_schema_stops_closed() {
        let mut value = serde_json::to_value(populated_state()).unwrap();
        value["schema_version"] = serde_json::json!(CURRENT_SCHEMA_VERSION + 1);
        let error = migrate_to_current(&mut value).unwrap_err();
        assert!(error.contains("unknown state schema"), "got: {error}");
    }

    #[test]
    fn missing_schema_version_stops_closed() {
        let mut value = serde_json::json!({ "run_id": "spec-128" });
        let error = migrate_to_current(&mut value).unwrap_err();
        assert!(error.contains("missing schema_version"), "got: {error}");
    }
}
