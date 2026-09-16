//! Typed owner of the `.director/state.json` schema.
//!
//! The load path is `read → migrate_to_current → RunState`; the write path is
//! `RunState → to_vec_pretty`. Every persisted key is modeled: unknown keys and
//! unknown enum values fail closed rather than round-tripping as free-form
//! JSON, so the schema stays a typed contract instead of a bag of values.
//!
//! This module owns the schema and the per-record predicates that read it
//! (`ReviewRound::is_zero`, `FindingDisposition::is_recorded`). The gate
//! verdict itself — the absolute-zero rule, its round accounting and its
//! escalation arithmetic — lives in [`crate::gate`], the single owner of that
//! rule, so the two layers can never disagree about when a gate passes.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::report::WorkerReport;
use crate::storage;
use crate::CURRENT_SCHEMA_VERSION;

// ── enums ──

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum RunStatus {
    Init,
    Running,
    SpecGating,
    PrOpen,
    Done,
    Escalated,
}

impl RunStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Init => "init",
            Self::Running => "running",
            Self::SpecGating => "spec-gating",
            Self::PrOpen => "pr-open",
            Self::Done => "done",
            Self::Escalated => "escalated",
        }
    }

    /// Parse a run status from its persisted spelling.
    pub(crate) fn parse(raw: &str) -> Result<Self, String> {
        match raw {
            "init" => Ok(Self::Init),
            "running" => Ok(Self::Running),
            "spec-gating" => Ok(Self::SpecGating),
            "pr-open" => Ok(Self::PrOpen),
            "done" => Ok(Self::Done),
            "escalated" => Ok(Self::Escalated),
            other => Err(format!(
                "unknown run status {other:?}; expected one of: init, running, spec-gating, pr-open, done, escalated"
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum TicketStatus {
    Pending,
    Implementing,
    Gating,
    Reviewing,
    Fixing,
    Done,
    Escalated,
}

impl TicketStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Implementing => "implementing",
            Self::Gating => "gating",
            Self::Reviewing => "reviewing",
            Self::Fixing => "fixing",
            Self::Done => "done",
            Self::Escalated => "escalated",
        }
    }

    /// Parse a ticket status from its persisted spelling.
    pub(crate) fn parse(raw: &str) -> Result<Self, String> {
        match raw {
            "pending" => Ok(Self::Pending),
            "implementing" => Ok(Self::Implementing),
            "gating" => Ok(Self::Gating),
            "reviewing" => Ok(Self::Reviewing),
            "fixing" => Ok(Self::Fixing),
            "done" => Ok(Self::Done),
            "escalated" => Ok(Self::Escalated),
            other => Err(format!(
                "unknown ticket status {other:?}; expected one of: pending, implementing, gating, reviewing, fixing, done, escalated"
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum RoundStatus {
    Reviewing,
    Complete,
}

impl RoundStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Reviewing => "reviewing",
            Self::Complete => "complete",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ReviewAxis {
    Standards,
    Spec,
}

impl ReviewAxis {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Standards => "standards",
            Self::Spec => "spec",
        }
    }

    pub(crate) fn parse(raw: &str) -> Result<Self, String> {
        match raw {
            "standards" => Ok(Self::Standards),
            "spec" => Ok(Self::Spec),
            other => Err(format!(
                "unknown review axis {other:?}; expected `standards` or `spec`"
            )),
        }
    }
}

/// How one Worker dispatch attempt ended. `started` is the open attempt; the
/// other two are terminal.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum DispatchStatus {
    Started,
    Ok,
    Failed,
}

impl DispatchStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Started => "started",
            Self::Ok => "ok",
            Self::Failed => "failed",
        }
    }
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
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Fixed { .. } => "fixed",
            Self::Rejected { .. } => "rejected",
        }
    }

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

/// One Worker dispatch attempt against one ticket. The retry budget counts
/// these records, so "the Worker gets one more try" is a fact about state, not
/// a sentence in a prompt.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DispatchRecord {
    pub(crate) worker: String,
    pub(crate) attempt: u64,
    pub(crate) status: DispatchStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) reason: Option<String>,
    /// The validated `WORKER_REPORT` the Worker handed back, kept verbatim so
    /// the escalation report can show what was claimed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) report: Option<WorkerReport>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReviewFinding {
    pub(crate) id: String,
    pub(crate) axis: ReviewAxis,
    /// Stable identity of the finding's text, so a re-issued review can be
    /// matched against the recorded one.
    pub(crate) hash: String,
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
    /// Worker dispatch attempts, oldest first. Additive since schema v1: an
    /// existing state file loads with an empty ledger and budget at zero.
    #[serde(default)]
    pub(crate) dispatches: Vec<DispatchRecord>,
    /// Index into `dispatches` where the current retry budget starts. A human
    /// decision that resumes an `escalated` ticket moves this to the end of the
    /// ledger, granting a fresh attempt budget — the same way resuming the run
    /// grants a fresh round cap.
    #[serde(default)]
    pub(crate) dispatch_budget_base: u64,
}

impl TicketState {
    /// The open review round, if a round is still collecting findings.
    pub(crate) fn open_round(&self) -> Option<&ReviewRound> {
        self.rounds
            .iter()
            .find(|round| round.status == RoundStatus::Reviewing)
    }

    /// The most recently recorded round.
    pub(crate) fn latest_round(&self) -> Option<&ReviewRound> {
        self.rounds.last()
    }

    pub(crate) fn round_mut(&mut self, number: u64) -> Option<&mut ReviewRound> {
        self.rounds.iter_mut().find(|round| round.round == number)
    }

    /// The dispatch attempt still in flight, if any.
    pub(crate) fn open_dispatch(&self) -> Option<&DispatchRecord> {
        self.dispatches
            .iter()
            .find(|record| record.status == DispatchStatus::Started)
    }

    pub(crate) fn open_dispatch_mut(&mut self) -> Option<&mut DispatchRecord> {
        self.dispatches
            .iter_mut()
            .find(|record| record.status == DispatchStatus::Started)
    }

    /// Failed attempts inside the current budget window.
    pub(crate) fn failed_dispatches_in_streak(&self) -> u64 {
        self.dispatches_in_streak()
            .filter(|record| record.status == DispatchStatus::Failed)
            .count() as u64
    }

    /// The most recent failed attempt of the current budget window, which is
    /// the Worker the sanctioned retry has to reuse.
    pub(crate) fn last_failed_dispatch_in_streak(&self) -> Option<&DispatchRecord> {
        self.dispatches_in_streak()
            .rev()
            .find(|record| record.status == DispatchStatus::Failed)
    }

    fn dispatches_in_streak(&self) -> impl DoubleEndedIterator<Item = &DispatchRecord> {
        let base = self.dispatch_budget_base as usize;
        self.dispatches.iter().skip(base.min(self.dispatches.len()))
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
    pub(crate) fn open_round(&self) -> Option<&ReviewRound> {
        self.rounds
            .iter()
            .find(|round| round.status == RoundStatus::Reviewing)
    }

    pub(crate) fn round_mut(&mut self, number: u64) -> Option<&mut ReviewRound> {
        self.rounds.iter_mut().find(|round| round.round == number)
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
    /// The worktree this run was last written against. Absent in a state file
    /// that predates schema 2 and until the first write records it.
    #[serde(default)]
    pub(crate) worktree: Option<storage::WorktreeFingerprint>,
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
            status: RunStatus::Init,
            revision: 0,
            worktree: None,
            tickets: Vec::new(),
            spec_gate: SpecGate {
                round_cap: storage::DEFAULT_ROUND_CAP,
                rounds: Vec::new(),
            },
        }
    }

    pub(crate) fn ticket(&self, ticket: u64) -> Option<&TicketState> {
        self.tickets.iter().find(|state| state.ticket == ticket)
    }

    pub(crate) fn ticket_mut(&mut self, ticket: u64) -> Option<&mut TicketState> {
        self.tickets.iter_mut().find(|state| state.ticket == ticket)
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

/// Write the state, re-capturing the worktree fingerprint first. Every write
/// refreshes it: the Director makes boundary commits during a run, and a
/// frozen capture would report those legitimate commits as drift.
pub(crate) fn write_state(worktree: &Path, state: &mut RunState) -> Result<(), String> {
    state.schema_version = CURRENT_SCHEMA_VERSION;
    state.worktree = Some(storage::capture_fingerprint(worktree)?);
    let bytes = serde_json::to_vec_pretty(state).map_err(|err| format!("json error: {err}"))?;
    storage::atomic_write(&state_path(worktree), &bytes)
}

/// The typed gate: state that does not match the schema fails closed.
pub(crate) fn deserialize_state(value: serde_json::Value) -> Result<RunState, String> {
    serde_json::from_value(value)
        .map_err(|err| format!("state does not match the run state schema: {err}"))
}

/// Forward-only migration to [`CURRENT_SCHEMA_VERSION`].
///
/// The chain is a list of single-step migrations applied in order, so every old
/// state file reaches the current schema the same way and no step has to know
/// about a version it was not written for. Unknown newer versions stop closed,
/// and so does a state with no version at all.
pub(crate) fn migrate_to_current(value: &mut serde_json::Value) -> Result<(), String> {
    let mut version = value
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| "state is missing schema_version".to_string())?;
    if version > CURRENT_SCHEMA_VERSION {
        return Err(format!("unknown state schema {version}"));
    }
    while version < CURRENT_SCHEMA_VERSION {
        version = match version {
            0 => migrate_v0_to_v1(value)?,
            1 => migrate_v1_to_v2(value)?,
            other => return Err(format!("no migration is defined from state schema {other}")),
        };
    }
    Ok(())
}

/// Schema 0 → 1: the pre-release shape carried neither the ticket ledger nor
/// the spec gate; both are re-derived from the run-level fields it does carry.
fn migrate_v0_to_v1(value: &mut serde_json::Value) -> Result<u64, String> {
    let object = object_mut(value)?;
    object
        .entry("tickets".to_string())
        .or_insert_with(|| serde_json::Value::Array(Vec::new()));
    object.entry("spec_gate".to_string()).or_insert_with(
        || serde_json::json!({ "round_cap": storage::DEFAULT_ROUND_CAP, "rounds": [] }),
    );
    object.insert("schema_version".to_string(), serde_json::Value::from(1u64));
    Ok(1)
}

/// Schema 1 → 2: the worktree fingerprint. A schema-1 state has no baseline —
/// nothing was recorded to compare against — so the key is established as an
/// explicit null, and the first write (or the first resume) records the live
/// worktree.
fn migrate_v1_to_v2(value: &mut serde_json::Value) -> Result<u64, String> {
    let object = object_mut(value)?;
    object
        .entry("worktree".to_string())
        .or_insert(serde_json::Value::Null);
    object.insert("schema_version".to_string(), serde_json::Value::from(2u64));
    Ok(2)
}

fn object_mut(
    value: &mut serde_json::Value,
) -> Result<&mut serde_json::Map<String, serde_json::Value>, String> {
    value
        .as_object_mut()
        .ok_or_else(|| "state is not a JSON object".to_string())
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
                        hash: "hash-f1".to_string(),
                        summary: "duplicated load path".to_string(),
                        disposition: Some(FindingDisposition::Fixed {
                            commit: Some("abc1234".to_string()),
                        }),
                    },
                    ReviewFinding {
                        id: "f2".to_string(),
                        axis: ReviewAxis::Spec,
                        hash: "hash-f2".to_string(),
                        summary: "acceptance criterion unverified".to_string(),
                        disposition: Some(FindingDisposition::Rejected {
                            reason: "criterion is out of scope for this ticket".to_string(),
                        }),
                    },
                    ReviewFinding {
                        id: "f3".to_string(),
                        axis: ReviewAxis::Spec,
                        hash: "hash-f3".to_string(),
                        summary: "still open".to_string(),
                        disposition: None,
                    },
                ],
            }],
            dispatches: Vec::new(),
            dispatch_budget_base: 0,
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
        assert_eq!(state.status, RunStatus::Init);
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
        assert!(!crate::gate::ticket_verdict(&state.tickets[0]).zero);
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
        assert!(!crate::gate::spec_verdict(&state).zero);
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
            "status": "init",
            "revision": 0
        });
        migrate_to_current(&mut value).unwrap();
        let state = deserialize_state(value).unwrap();
        assert_eq!(state.schema_version, CURRENT_SCHEMA_VERSION);
        assert!(state.tickets.is_empty());
        assert_eq!(state.spec_gate.round_cap, storage::DEFAULT_ROUND_CAP);
        assert_eq!(state.worktree, None);
    }

    /// The schema as ticket #131 shipped it: a ticket ledger, a spec gate, and
    /// no worktree fingerprint. It has to keep loading — and has to come out at
    /// the current version with its rounds intact.
    fn schema_one_fixture() -> serde_json::Value {
        serde_json::from_str(include_str!("../tests/fixtures/state-v1.json")).unwrap()
    }

    #[test]
    fn schema_one_migrates_forward_through_the_chain() {
        let mut value = schema_one_fixture();
        assert_eq!(value["schema_version"], 1);
        migrate_to_current(&mut value).unwrap();
        assert_eq!(value["schema_version"], CURRENT_SCHEMA_VERSION);
        assert!(value["worktree"].is_null());

        let state = deserialize_state(value).unwrap();
        assert_eq!(state.run_id, "spec-128");
        assert_eq!(state.tickets.len(), 2);
        assert_eq!(state.tickets[0].rounds.len(), 1);
        assert_eq!(state.tickets[0].rounds[0].findings.len(), 2);
        assert!(state.tickets[0].dispatches.is_empty());
        assert!(state.spec_gate.rounds.is_empty());
        assert_eq!(state.worktree, None);
    }

    #[test]
    fn migration_is_forward_only_and_idempotent() {
        let mut value = schema_one_fixture();
        migrate_to_current(&mut value).unwrap();
        let once = value.clone();
        migrate_to_current(&mut value).unwrap();
        assert_eq!(value, once, "a migrated state must not move again");
    }

    #[test]
    fn a_write_records_the_worktree_it_belongs_to() {
        let dir = tempfile::tempdir().unwrap();
        let worktree = dir.path();
        git(worktree, &["init"]);
        git(
            worktree,
            &[
                "-c",
                "user.email=test@example.com",
                "-c",
                "user.name=Test",
                "commit",
                "--allow-empty",
                "-m",
                "init",
            ],
        );

        let mut state = RunState::new("spec-128".to_string(), 128, "codex/spec-128".to_string());
        write_state(worktree, &mut state).unwrap();
        let recorded = state
            .worktree
            .clone()
            .expect("a write records the worktree fingerprint");
        assert_eq!(recorded.head.len(), 40);
        assert_eq!(recorded.tree.len(), 40);
        assert!(!recorded.branch.is_empty());

        let reloaded = read_state(worktree).unwrap();
        assert_eq!(reloaded.worktree, Some(recorded));
    }

    #[test]
    fn a_write_outside_a_git_worktree_stops_closed() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = RunState::new("spec-128".to_string(), 128, "codex/spec-128".to_string());
        let error = write_state(dir.path(), &mut state).unwrap_err();
        assert!(error.contains("git rev-parse"), "got: {error}");
    }

    fn git(worktree: &Path, args: &[&str]) {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(worktree)
            .output()
            .expect("git should run");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn newer_schema_stops_closed() {
        let mut value = serde_json::to_value(populated_state()).unwrap();
        value["schema_version"] = serde_json::json!(CURRENT_SCHEMA_VERSION + 1);
        let error = migrate_to_current(&mut value).unwrap_err();
        assert!(error.contains("unknown state schema"), "got: {error}");
    }

    #[test]
    fn statuses_round_trip_through_their_persisted_spellings() {
        for status in [
            RunStatus::Init,
            RunStatus::Running,
            RunStatus::SpecGating,
            RunStatus::PrOpen,
            RunStatus::Done,
            RunStatus::Escalated,
        ] {
            assert_eq!(RunStatus::parse(status.as_str()).unwrap(), status);
        }
        for status in [
            TicketStatus::Pending,
            TicketStatus::Implementing,
            TicketStatus::Gating,
            TicketStatus::Reviewing,
            TicketStatus::Fixing,
            TicketStatus::Done,
            TicketStatus::Escalated,
        ] {
            assert_eq!(TicketStatus::parse(status.as_str()).unwrap(), status);
        }
        assert!(RunStatus::parse("active").is_err());
        assert!(TicketStatus::parse("active").is_err());
    }

    #[test]
    fn missing_schema_version_stops_closed() {
        let mut value = serde_json::json!({ "run_id": "spec-128" });
        let error = migrate_to_current(&mut value).unwrap_err();
        assert!(error.contains("missing schema_version"), "got: {error}");
    }
}
