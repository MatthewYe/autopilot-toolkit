use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::env;
use std::fs;
use std::fs::OpenOptions;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::storage;
use crate::CURRENT_SCHEMA_VERSION;
use crate::RunLock;


// ── typed run state schema ──
//
// `RunState` is the typed owner of the schema-1 state.json shape. The load
// path is `read Value → migrate_state(Value) → deserialize RunState`; the
// write path is `RunState → to_value → to_vec_pretty`. Unknown top-level
// keys survive via the `extra` catch-all; nested structs intentionally stay
// permissive (no `deny_unknown_fields`). The `implementation_started` dead
// field is not modeled — it round-trips through `extra`.

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum RunLifecycle {
    Active,
    Blocked,
    Completed,
    Purged,
    Superseded,
    Aborted,
    SupersessionPending,
}

impl RunLifecycle {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Blocked => "blocked",
            Self::Completed => "completed",
            Self::Purged => "purged",
            Self::Superseded => "superseded",
            Self::Aborted => "aborted",
            Self::SupersessionPending => "supersession-pending",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum StageState {
    Pending,
    Active,
    Completed,
    Waiting,
    Blocked,
    NeedsReconciliation,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum PurgeCleanupState {
    Pending,
    Completed,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum PublicationStatus {
    Confirmed,
    NeedsReconciliation,
    Pending,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum BoundaryState {
    Waiting,
    Blocked,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct SessionBinding {
    pub(crate) runtime: String,
    pub(crate) session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) released: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) released_revision: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct WorkflowSnapshot {
    pub(crate) version: String,
    pub(crate) source: String,
    pub(crate) stages: Vec<crate::workflow::WorkflowStage>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct Requirement {
    pub(crate) source: String,
    pub(crate) text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) supersession_reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct StageStateEntry {
    pub(crate) id: String,
    pub(crate) state: StageState,
    pub(crate) revision: u64,
    pub(crate) authorized_action: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct EvidenceAdapter {
    pub(crate) executor: String,
    pub(crate) skill: Option<String>,
    pub(crate) invocation: String,
}

/// Completion evidence recorded by a workflow stage (carries an `adapter`).
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct StageCompletionEvidence {
    pub(crate) stage: String,
    pub(crate) completed_revision: u64,
    pub(crate) accepted_user_checkpoint: String,
    pub(crate) summary: String,
    pub(crate) adapter: EvidenceAdapter,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) clarification: Option<Value>,
}

/// Completion evidence recorded by intake (no `adapter` key on disk).
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct IntakeCompletionEvidence {
    pub(crate) stage: String,
    pub(crate) completed_revision: u64,
    pub(crate) accepted_user_checkpoint: String,
    pub(crate) summary: String,
    /// Catch-all so a stage entry that fails the `Stage` variant (e.g. a
    /// malformed or newer `adapter`) reclassifies as `Intake` WITHOUT
    /// silently dropping its `adapter`/other keys on re-serialization.
    #[serde(flatten)]
    pub(crate) extra: Map<String, Value>,
}

/// Heterogeneous `completion_evidence[]` entry. The two on-disk shapes stay
// distinct by design; unifying them would require a migration.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub(crate) enum CompletionEvidence {
    Stage(StageCompletionEvidence),
    Intake(IntakeCompletionEvidence),
}

/// A confirmed (or needs-reconciliation) publication record.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct PublicationRecord {
    pub(crate) operation_id: String,
    pub(crate) artifact_id: String,
    pub(crate) path: String,
    pub(crate) payload_path: String,
    pub(crate) payload_hash: String,
    pub(crate) dependency_artifact_ids: Vec<String>,
    pub(crate) tracker: String,
    pub(crate) status: PublicationStatus,
    pub(crate) title: Option<String>,
}

/// The narrow placeholder written for issues skipped after a blocked publish.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct PendingIssuePublication {
    pub(crate) operation_id: String,
    pub(crate) title: String,
    pub(crate) status: PublicationStatus,
    /// Catch-all so a published entry that fails the `Published` variant
    /// (e.g. a missing required field) reclassifies as `Pending` WITHOUT
    /// silently dropping its remaining keys on re-serialization.
    #[serde(flatten)]
    pub(crate) extra: Map<String, Value>,
}

impl CompletionEvidence {
    pub(crate) fn stage(&self) -> &str {
        match self {
            Self::Stage(entry) => &entry.stage,
            Self::Intake(entry) => &entry.stage,
        }
    }
}

/// Heterogeneous `publications.issues[]` entry. The two on-disk shapes stay
// distinct by design; unifying them would require a migration.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub(crate) enum IssuePublication {
    Published(PublicationRecord),
    Pending(PendingIssuePublication),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct Publications {
    pub(crate) prd: Option<PublicationRecord>,
    pub(crate) issues: Vec<IssuePublication>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct StageBoundary {
    pub(crate) stage: String,
    pub(crate) state: BoundaryState,
    pub(crate) reason: String,
    pub(crate) required_next_action: String,
    pub(crate) revision: u64,
}

/// Heterogeneous `boundaries[]` entry. Legacy `record_stage_boundary`
/// appended without validating prior entries, so a non-conforming entry
/// must keep loading and round-trip verbatim instead of failing the whole
/// load.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub(crate) enum BoundaryEntry {
    Stage(StageBoundary),
    Other(Value),
}

impl BoundaryEntry {
    /// The typed entry, when this one conforms to the current shape.
    #[allow(dead_code)]
    pub(crate) fn as_typed(&self) -> Option<&StageBoundary> {
        match self {
            Self::Stage(entry) => Some(entry),
            Self::Other(_) => None,
        }
    }

    /// The stage this entry refers to, for either shape (matching how the
    /// legacy Value projection read it).
    pub(crate) fn stage(&self) -> Option<&str> {
        match self {
            Self::Stage(entry) => Some(&entry.stage),
            Self::Other(value) => value.get("stage").and_then(Value::as_str),
        }
    }

    pub(crate) fn required_next_action(&self) -> Value {
        match self {
            Self::Stage(entry) => json!(entry.required_next_action),
            Self::Other(value) => value
                .get("required_next_action")
                .cloned()
                .unwrap_or(Value::Null),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct Handoff {
    pub(crate) from_session: String,
    pub(crate) to_session: String,
    pub(crate) reason: String,
    pub(crate) revision: u64,
    pub(crate) invalidates_previous_session: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct DriftAcknowledgment {
    pub(crate) stage: String,
    pub(crate) material: bool,
    pub(crate) reason: String,
    pub(crate) detected: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct MigrationEvent {
    pub(crate) from_schema: u64,
    pub(crate) to_schema: u64,
    pub(crate) summary: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct AbortRecord {
    pub(crate) reason: String,
    pub(crate) revision: u64,
    pub(crate) user_authorized: bool,
    pub(crate) domain_document_artifacts: Vec<Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct PurgeState {
    pub(crate) cleanup_state: PurgeCleanupState,
    pub(crate) source_revision: u64,
    pub(crate) revision: u64,
    pub(crate) user_authorized: bool,
    pub(crate) tombstone: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct Supersession {
    pub(crate) reason: String,
    pub(crate) revision: u64,
    pub(crate) successor_run_id: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct RunState {
    pub(crate) schema_version: u64,
    pub(crate) run_id: String,
    pub(crate) state: RunLifecycle,
    pub(crate) revision: u64,
    pub(crate) current_stage: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) predecessor_run_id: Option<String>,
    pub(crate) session_binding: SessionBinding,
    pub(crate) workflow: WorkflowSnapshot,
    pub(crate) requirement: Option<Requirement>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) requirement_snapshot: Option<Value>,
    #[serde(default)]
    pub(crate) clarification: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) storage: Option<Value>,
    pub(crate) stages: Vec<StageStateEntry>,
    pub(crate) completion_evidence: Vec<CompletionEvidence>,
    pub(crate) publications: Publications,
    pub(crate) report: Value,
    #[serde(default)]
    pub(crate) context_baseline: Value,
    #[serde(default)]
    pub(crate) drift_acknowledgments: Vec<DriftAcknowledgment>,
    #[serde(default)]
    pub(crate) boundaries: Vec<BoundaryEntry>,
    pub(crate) abort: Option<AbortRecord>,
    #[serde(default)]
    pub(crate) handoffs: Vec<Handoff>,
    #[serde(default)]
    pub(crate) migration_events: Vec<MigrationEvent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) purge: Option<PurgeState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) superseded_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) supersession: Option<Supersession>,
    #[serde(flatten)]
    pub(crate) extra: Map<String, Value>,
}

/// Everything `start_distill` and `supersede_run` need to build the initial
/// run state; the two call sites differ only in these inputs.
pub(crate) struct NewRunState {
    pub(crate) run_id: String,
    pub(crate) lifecycle: RunLifecycle,
    pub(crate) current_stage: String,
    pub(crate) predecessor_run_id: Option<String>,
    pub(crate) session_binding: SessionBinding,
    pub(crate) workflow: WorkflowSnapshot,
    pub(crate) requirement: Requirement,
    pub(crate) requirement_snapshot: Option<Value>,
    pub(crate) storage: Option<Value>,
    pub(crate) stage_states: Vec<Value>,
    pub(crate) intake_summary: String,
    pub(crate) context_baseline: Value,
}

impl RunState {
    /// The single constructor for a fresh run's initial state, shared by
    /// `start_distill` and the `supersede` successor.
    pub(crate) fn new(params: NewRunState) -> Result<Self, String> {
        let stages = params
            .stage_states
            .into_iter()
            .map(serde_json::from_value)
            .collect::<Result<Vec<StageStateEntry>, _>>()
            .map_err(|err| format!("initial stage states do not match schema: {err}"))?;
        // The `implementation_started` dead field is deliberately not modeled;
        // writers keep emitting it through the catch-all so on-disk bytes are
        // unchanged.
        let mut extra = Map::new();
        extra.insert("implementation_started".to_string(), Value::Bool(false));
        Ok(Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            run_id: params.run_id,
            state: params.lifecycle,
            revision: 1,
            current_stage: Some(params.current_stage),
            predecessor_run_id: params.predecessor_run_id,
            session_binding: params.session_binding,
            workflow: params.workflow,
            requirement: Some(params.requirement),
            requirement_snapshot: params.requirement_snapshot,
            clarification: Value::Null,
            storage: params.storage,
            stages,
            completion_evidence: vec![CompletionEvidence::Intake(IntakeCompletionEvidence {
                stage: "intake".to_string(),
                completed_revision: 1,
                accepted_user_checkpoint: "explicit-text-captured".to_string(),
                summary: params.intake_summary,
                extra: Map::new(),
            })],
            publications: Publications {
                prd: None,
                issues: Vec::new(),
            },
            report: Value::Null,
            context_baseline: params.context_baseline,
            drift_acknowledgments: Vec::new(),
            boundaries: Vec::new(),
            abort: None,
            handoffs: Vec::new(),
            migration_events: Vec::new(),
            purge: None,
            superseded_by: None,
            supersession: None,
            extra,
        })
    }
}


// ── transitions ──
//
// Every legal state transition is a method on `RunState`; guards live inside
// the methods and illegal calls return a structured `StateError` carrying
// expected/actual. `Display` renders the exact CLI error text the handlers
// produced before the guards were pulled in, so messages stay byte-identical.

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum StateError {
    PendingPurge,
    IllegalRunState { expected: String, actual: String },
    MissingCurrentStage,
    SessionMismatch { expected: String, actual: String },
    StageAlreadyCompleted { stage: String },
    StageNotAuthorized { expected: String, actual: String },
    StageNotFound { stage: String },
    WorkflowStageNotFound { stage: String },
    SessionAlreadyReleased,
    EmptyBoundaryReason,
    EmptyBoundaryRequiredNextAction,
    RunBindingSessionMismatch { expected: String, actual: String },
    FromSessionMismatch { expected: String, actual: String },
    AlreadyPurged,
    NoPendingPurge,
}

impl std::fmt::Display for StateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PendingPurge => write!(
                formatter,
                "purge cleanup is pending; resume purge before any other transition"
            ),
            Self::IllegalRunState { .. } => write!(formatter, "run is not active or blocked"),
            Self::MissingCurrentStage => write!(formatter, "state is missing current_stage"),
            Self::SessionMismatch { .. } => {
                write!(formatter, "session id does not match active run binding")
            }
            Self::StageAlreadyCompleted { stage } => {
                write!(formatter, "stage {stage} is already completed")
            }
            Self::StageNotAuthorized { expected, actual } => {
                write!(formatter, "stage {actual} is not authorized; expected {expected}")
            }
            Self::StageNotFound { stage } => write!(formatter, "stage state not found: {stage}"),
            Self::WorkflowStageNotFound { stage } => {
                write!(formatter, "workflow stage not found: {stage}")
            }
            Self::SessionAlreadyReleased => write!(formatter, "session is already released"),
            Self::EmptyBoundaryReason => {
                write!(formatter, "boundary evidence.reason must not be empty")
            }
            Self::EmptyBoundaryRequiredNextAction => write!(
                formatter,
                "boundary evidence.required_next_action must not be empty"
            ),
            Self::RunBindingSessionMismatch { .. } => {
                write!(formatter, "session id does not match run binding")
            }
            Self::FromSessionMismatch { .. } => {
                write!(formatter, "from-session does not match active run binding")
            }
            Self::AlreadyPurged => write!(formatter, "run is already purged"),
            Self::NoPendingPurge => write!(formatter, "purge cleanup is not pending"),
        }
    }
}

impl From<StateError> for String {
    fn from(error: StateError) -> String {
        error.to_string()
    }
}

/// What happened after `complete_stage` recorded the completion: the run
/// advanced to the next stage, or the final stage completed the run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum StageCompletion {
    Advanced { next_stage: String },
    RunCompleted,
}

/// What `begin_purge` did: started a fresh two-phase purge, or found the
/// durable pending purge of an interrupted one to recover.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PurgeBegin {
    Started,
    Recovering,
}

impl RunState {
    /// The shared precondition of every stage submission transition: no
    /// pending purge, run active/blocked, session match, stage not already
    /// completed, stage is the current stage. The transition methods enforce
    /// it internally; handlers may also call it upfront to preserve the
    /// check-before-side-effects ordering of the pre-typed handlers.
    pub(crate) fn guard_stage_submission(
        &self,
        session_id: &str,
        stage_id: &str,
    ) -> Result<(), StateError> {
        if self
            .purge
            .as_ref()
            .is_some_and(|purge| purge.cleanup_state == PurgeCleanupState::Pending)
        {
            return Err(StateError::PendingPurge);
        }
        if !matches!(self.state, RunLifecycle::Active | RunLifecycle::Blocked) {
            return Err(StateError::IllegalRunState {
                expected: "active or blocked".to_string(),
                actual: self.state.as_str().to_string(),
            });
        }
        if self.session_binding.session_id != session_id {
            return Err(StateError::SessionMismatch {
                expected: self.session_binding.session_id.clone(),
                actual: session_id.to_string(),
            });
        }
        let expected_stage = self
            .current_stage
            .as_deref()
            .ok_or(StateError::MissingCurrentStage)?;
        if self
            .completion_evidence
            .iter()
            .any(|entry| entry.stage() == stage_id)
        {
            return Err(StateError::StageAlreadyCompleted {
                stage: stage_id.to_string(),
            });
        }
        if stage_id != expected_stage {
            return Err(StateError::StageNotAuthorized {
                expected: expected_stage.to_string(),
                actual: stage_id.to_string(),
            });
        }
        Ok(())
    }

    /// Record a stage completion at the next revision, then advance to the
    /// next workflow stage or complete the run (releasing the session).
    pub(crate) fn complete_stage(
        &mut self,
        session_id: &str,
        stage: &crate::workflow::WorkflowStage,
        summary: &str,
    ) -> Result<StageCompletion, StateError> {
        self.guard_stage_submission(session_id, &stage.id)?;
        let revision = self.revision + 1;
        self.state = RunLifecycle::Active;
        self.revision = revision;
        self.completion_evidence
            .push(CompletionEvidence::Stage(StageCompletionEvidence {
                stage: stage.id.clone(),
                completed_revision: revision,
                accepted_user_checkpoint: stage.checkpoint.clone(),
                summary: summary.to_string(),
                adapter: EvidenceAdapter {
                    executor: stage.executor.clone(),
                    skill: stage.skill.clone(),
                    invocation: "unmodified-skill".to_string(),
                },
                clarification: (stage.id == "clarification").then(|| self.clarification.clone()),
            }));
        self.mark_stage(&stage.id, StageState::Completed, revision)?;
        let index = self
            .workflow
            .stages
            .iter()
            .position(|entry| entry.id == stage.id)
            .ok_or_else(|| StateError::WorkflowStageNotFound {
                stage: stage.id.clone(),
            })?;
        match self.workflow.stages.get(index + 1) {
            Some(next_stage) => {
                let next_stage = next_stage.id.clone();
                self.current_stage = Some(next_stage.clone());
                self.mark_stage(&next_stage, StageState::Active, revision)?;
                Ok(StageCompletion::Advanced {
                    next_stage,
                })
            }
            None => {
                self.state = RunLifecycle::Completed;
                self.current_stage = None;
                self.release_session()?;
                Ok(StageCompletion::RunCompleted)
            }
        }
    }

    /// Defer the current stage to a waiting/blocked boundary at the next
    /// revision, recording the boundary entry.
    pub(crate) fn defer_stage(
        &mut self,
        session_id: &str,
        stage_id: &str,
        boundary: BoundaryState,
        reason: &str,
        required_next_action: &str,
    ) -> Result<(), StateError> {
        self.guard_stage_submission(session_id, stage_id)?;
        self.record_boundary(stage_id, boundary, reason, required_next_action)?;
        let revision = self.revision;
        let stage_state = match boundary {
            BoundaryState::Waiting => StageState::Waiting,
            BoundaryState::Blocked => StageState::Blocked,
        };
        self.mark_stage(stage_id, stage_state, revision)?;
        self.state = match boundary {
            BoundaryState::Waiting => RunLifecycle::Active,
            BoundaryState::Blocked => RunLifecycle::Blocked,
        };
        Ok(())
    }

    /// Append a boundary entry at the next revision. This method owns the
    /// revision bump, so the entry always lands at the new revision and a
    /// rejected call leaves revision and boundaries untouched.
    ///
    /// Behavior preserved from the pre-typed-schema handler: malformed
    /// *entries* in `boundaries` are tolerated — they load as
    /// `BoundaryEntry::Other`, round-trip verbatim, and appends proceed
    /// past them. (Non-array `boundaries` was already rejected at load by
    /// `ensure_array_field` before the typed schema, so the old silent
    /// reset-to-`[]` branch was unreachable and has no equivalent here.)
    pub(crate) fn record_boundary(
        &mut self,
        stage_id: &str,
        boundary: BoundaryState,
        reason: &str,
        required_next_action: &str,
    ) -> Result<(), StateError> {
        if reason.is_empty() {
            return Err(StateError::EmptyBoundaryReason);
        }
        if required_next_action.is_empty() {
            return Err(StateError::EmptyBoundaryRequiredNextAction);
        }
        self.revision += 1;
        self.boundaries.push(BoundaryEntry::Stage(StageBoundary {
            stage: stage_id.to_string(),
            state: boundary,
            reason: reason.to_string(),
            required_next_action: required_next_action.to_string(),
            revision: self.revision,
        }));
        Ok(())
    }

    /// Mark the current stage needs-reconciliation after an uncertain
    /// publication outcome, at the next revision.
    pub(crate) fn mark_stage_needs_reconciliation(
        &mut self,
        session_id: &str,
        stage_id: &str,
    ) -> Result<(), StateError> {
        self.guard_stage_submission(session_id, stage_id)?;
        self.revision += 1;
        let revision = self.revision;
        self.mark_stage(stage_id, StageState::NeedsReconciliation, revision)?;
        self.state = RunLifecycle::Active;
        Ok(())
    }

    /// Release the session binding at the current revision. One-shot.
    pub(crate) fn release_session(&mut self) -> Result<(), StateError> {
        if self.session_binding.released == Some(true) {
            return Err(StateError::SessionAlreadyReleased);
        }
        self.session_binding.released = Some(true);
        self.session_binding.released_revision = Some(self.revision);
        Ok(())
    }

    /// The shared precondition of the lifecycle mutations (abort, takeover,
    /// supersede): no pending purge and the run is active or blocked.
    fn guard_lifecycle_mutation(&self) -> Result<(), StateError> {
        if self
            .purge
            .as_ref()
            .is_some_and(|purge| purge.cleanup_state == PurgeCleanupState::Pending)
        {
            return Err(StateError::PendingPurge);
        }
        if !matches!(self.state, RunLifecycle::Active | RunLifecycle::Blocked) {
            return Err(StateError::IllegalRunState {
                expected: "active or blocked".to_string(),
                actual: self.state.as_str().to_string(),
            });
        }
        Ok(())
    }

    /// Abort the run as a user-authorized terminal transition at the next
    /// revision, releasing the session.
    pub(crate) fn abort(
        &mut self,
        session_id: &str,
        reason: &str,
        domain_document_artifacts: Vec<Value>,
    ) -> Result<(), StateError> {
        self.guard_lifecycle_mutation()?;
        if self.session_binding.session_id != session_id {
            return Err(StateError::RunBindingSessionMismatch {
                expected: self.session_binding.session_id.clone(),
                actual: session_id.to_string(),
            });
        }
        self.revision += 1;
        let revision = self.revision;
        self.state = RunLifecycle::Aborted;
        self.current_stage = None;
        self.session_binding.released = Some(true);
        self.session_binding.released_revision = Some(revision);
        self.abort = Some(AbortRecord {
            reason: reason.to_string(),
            revision,
            user_authorized: true,
            domain_document_artifacts,
        });
        Ok(())
    }

    /// Hand the run to a replacement session at the next revision, recording
    /// the handoff.
    pub(crate) fn takeover(
        &mut self,
        from_session: &str,
        to_session: &str,
        reason: &str,
    ) -> Result<(), StateError> {
        self.guard_lifecycle_mutation()?;
        if self.session_binding.session_id != from_session {
            return Err(StateError::FromSessionMismatch {
                expected: self.session_binding.session_id.clone(),
                actual: from_session.to_string(),
            });
        }
        self.revision += 1;
        let revision = self.revision;
        self.session_binding.session_id = to_session.to_string();
        self.handoffs.push(Handoff {
            from_session: from_session.to_string(),
            to_session: to_session.to_string(),
            reason: reason.to_string(),
            revision,
            invalidates_previous_session: true,
        });
        Ok(())
    }

    /// Begin the two-phase purge: mark the purge pending at the next
    /// revision. A run whose purge is already pending is an interrupted
    /// purge to recover — a no-op so recovery resumes at the same revision.
    pub(crate) fn begin_purge(
        &mut self,
        session_id: &str,
        tombstone: Value,
    ) -> Result<PurgeBegin, StateError> {
        if self.session_binding.session_id != session_id {
            return Err(StateError::RunBindingSessionMismatch {
                expected: self.session_binding.session_id.clone(),
                actual: session_id.to_string(),
            });
        }
        if self
            .purge
            .as_ref()
            .is_some_and(|purge| purge.cleanup_state == PurgeCleanupState::Pending)
        {
            return Ok(PurgeBegin::Recovering);
        }
        if self.state == RunLifecycle::Purged {
            return Err(StateError::AlreadyPurged);
        }
        let source_revision = self.revision;
        self.revision += 1;
        self.purge = Some(PurgeState {
            cleanup_state: PurgeCleanupState::Pending,
            source_revision,
            revision: self.revision,
            user_authorized: true,
            tombstone,
        });
        Ok(PurgeBegin::Started)
    }

    /// Complete the two-phase purge at the current (pending) revision: the
    /// run goes terminal and the session is released.
    pub(crate) fn complete_purge(&mut self, session_id: &str) -> Result<(), StateError> {
        if self.session_binding.session_id != session_id {
            return Err(StateError::RunBindingSessionMismatch {
                expected: self.session_binding.session_id.clone(),
                actual: session_id.to_string(),
            });
        }
        let purge = self.purge.as_mut().ok_or(StateError::NoPendingPurge)?;
        if purge.cleanup_state != PurgeCleanupState::Pending {
            return Err(StateError::NoPendingPurge);
        }
        purge.cleanup_state = PurgeCleanupState::Completed;
        let revision = self.revision;
        self.state = RunLifecycle::Purged;
        self.current_stage = None;
        // Purging a completed run re-releases an already-released session,
        // so set the fields directly instead of the one-shot
        // `release_session`.
        self.session_binding.released = Some(true);
        self.session_binding.released_revision = Some(revision);
        self.requirement = None;
        self.requirement_snapshot = Some(json!({
            "purged": true,
            "tombstone_path": format!(".distill/runs/{}/tombstone.json", self.run_id),
        }));
        Ok(())
    }

    /// Mark the run superseded by its successor at the next revision. The
    /// cross-run two-write + rollback orchestration stays in the handler.
    pub(crate) fn mark_superseded(
        &mut self,
        session_id: &str,
        reason: &str,
        successor_run_id: &str,
    ) -> Result<(), StateError> {
        self.guard_lifecycle_mutation()?;
        if self.session_binding.session_id != session_id {
            return Err(StateError::SessionMismatch {
                expected: self.session_binding.session_id.clone(),
                actual: session_id.to_string(),
            });
        }
        self.revision += 1;
        let revision = self.revision;
        self.state = RunLifecycle::Superseded;
        self.current_stage = None;
        self.superseded_by = Some(successor_run_id.to_string());
        self.supersession = Some(Supersession {
            reason: reason.to_string(),
            revision,
            successor_run_id: successor_run_id.to_string(),
        });
        Ok(())
    }

    fn mark_stage(
        &mut self,
        stage_id: &str,
        stage_state: StageState,
        revision: u64,
    ) -> Result<(), StateError> {
        let stage = self
            .stages
            .iter_mut()
            .find(|stage| stage.id == stage_id)
            .ok_or_else(|| StateError::StageNotFound {
                stage: stage_id.to_string(),
            })?;
        stage.state = stage_state;
        stage.revision = revision;
        Ok(())
    }
}

/// The single serializer for state.json: `Value → RunState → Value →
/// to_vec_pretty`. The commit quota preflight and every state write go
/// through this function, so preflight byte counts always match the bytes
/// that land on disk.
pub(crate) fn serialize_state(state: &Value) -> Result<Vec<u8>, String> {
    let value = canonicalize_state(state.clone())?;
    serde_json::to_vec_pretty(&value).map_err(|err| format!("json error: {err}"))
}

/// The typed gate at the end of every load path: deserialize `RunState` and
/// hand handlers the canonical Value projection.
fn canonicalize_state(state: Value) -> Result<Value, String> {
    let run_state: RunState = serde_json::from_value(state)
        .map_err(|err| format!("state does not match the run state schema: {err}"))?;
    serde_json::to_value(&run_state).map_err(|err| format!("json error: {err}"))
}

pub(crate) fn validate_required_project_configuration(worktree: &Path) -> Result<(), String> {
    let canonical_worktree =
        fs::canonicalize(worktree).map_err(|err| format!("cannot canonicalize worktree: {err}"))?;
    for rel in [
        "docs/agents/issue-tracker.md",
        "docs/agents/triage-labels.md",
        "docs/agents/domain.md",
    ] {
        let relative = Path::new(rel);
        let mut current = worktree.to_path_buf();
        for component in relative.components() {
            let std::path::Component::Normal(component) = component else {
                return Err(format!("invalid project configuration path: {rel}"));
            };
            current.push(component);
            let metadata = fs::symlink_metadata(&current)
                .map_err(|_| format!("{rel} is required project configuration"))?;
            if metadata.file_type().is_symlink() {
                return Err(format!(
                    "project configuration must not be a symlink: {rel}"
                ));
            }
        }
        let metadata = fs::metadata(&current)
            .map_err(|_| format!("{rel} is required project configuration"))?;
        if !metadata.is_file() {
            return Err(format!(
                "project configuration must be a regular file: {rel}"
            ));
        }
        let canonical = fs::canonicalize(&current)
            .map_err(|err| format!("cannot canonicalize project configuration {rel}: {err}"))?;
        if !canonical.starts_with(&canonical_worktree) {
            return Err(format!("project configuration escapes worktree: {rel}"));
        }
        let content = fs::read_to_string(&canonical)
            .map_err(|err| format!("cannot read project configuration {rel}: {err}"))?;
        if content.trim().is_empty() {
            return Err(format!("project configuration must not be empty: {rel}"));
        }
    }
    Ok(())
}

pub(crate) fn rollback_successor(worktree: &Path, successor_id: &str) -> Result<(), String> {
    storage::remove_dir_if_exists(&storage::run_dir(worktree, successor_id)?)
        .map_err(|err| format!("cannot roll back successor run: {err}"))
}

pub(crate) fn ensure_worktree(worktree: &Path) -> Result<(), String> {
    if !worktree.is_dir() {
        return Err(format!("worktree does not exist: {}", worktree.display()));
    }
    storage::ensure_distill_path_safe(worktree)?;
    Ok(())
}

pub(crate) fn acquire_project_start_lock(worktree: &Path) -> Result<RunLock, String> {
    acquire_lock_file(
        worktree.join(".distill/start.lock"),
        "project start is locked by another writer",
    )
}

pub(crate) fn acquire_run_lock(worktree: &Path, run_id: &str) -> Result<RunLock, String> {
    acquire_lock_file(
        storage::run_dir(worktree, run_id)?.join("state.lock"),
        "run is locked by another writer",
    )
}

fn acquire_lock_file(path: PathBuf, busy_message: &str) -> Result<RunLock, String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("cannot create lock dir: {err}"))?;
    }
    match OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(_) => Ok(RunLock { path }),
        Err(err) if err.kind() == ErrorKind::AlreadyExists => {
            let content = fs::read_to_string(&path).unwrap_or_default();
            if content.contains("\"stale\":true") {
                fs::remove_file(&path)
                    .map_err(|err| format!("cannot recover stale run lock: {err}"))?;
                OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)
                    .map_err(|err| format!("cannot acquire recovered run lock: {err}"))?;
                Ok(RunLock { path })
            } else {
                Err(busy_message.to_string())
            }
        }
        Err(err) => Err(format!("cannot acquire run lock: {err}")),
    }
}

fn validate_expected_revision(state: &Value, expected_revision: u64) -> Result<(), String> {
    let current = state["revision"]
        .as_u64()
        .ok_or("state revision is invalid")?;
    if current != expected_revision {
        return Err(format!(
            "expected revision {expected_revision} is stale; current revision is {current}"
        ));
    }
    Ok(())
}

fn read_state_for_update_at_revision(
    worktree: &Path,
    run_id: &str,
    expected_revision: u64,
) -> Result<Value, String> {
    read_state_for_update_checked(worktree, run_id, Some(expected_revision))
}

fn read_state_for_update_checked(
    worktree: &Path,
    run_id: &str,
    expected_revision: Option<u64>,
) -> Result<Value, String> {
    let path = state_path(worktree, run_id)?;
    let original = fs::read_to_string(&path).map_err(|err| format!("cannot read state: {err}"))?;
    let mut state: Value =
        serde_json::from_str(&original).map_err(|err| format!("cannot parse state: {err}"))?;
    if let Some(expected_revision) = expected_revision {
        validate_expected_revision(&state, expected_revision)?;
    }
    let schema_version = state["schema_version"].as_u64().unwrap_or(0);
    let backup_path = (schema_version < CURRENT_SCHEMA_VERSION)
        .then(|| path.with_file_name(format!("state.schema-{schema_version}.backup.json")));
    migrate_to_current(&mut state, &original, backup_path)?;
    canonicalize_state(state)
}

/// The Value layer of every load path: schema gate → (optional backup) →
/// `migrate_state` → ensure current fields. `backup_path` is written before
/// migrating, but only by callers that will persist the migrated state.
fn migrate_to_current(
    state: &mut Value,
    original: &str,
    backup_path: Option<PathBuf>,
) -> Result<(), String> {
    let schema_version = state["schema_version"].as_u64().unwrap_or(0);
    if schema_version > CURRENT_SCHEMA_VERSION {
        return Err(format!(
            "state schema {schema_version} is newer than this CLI supports"
        ));
    }
    if schema_version < CURRENT_SCHEMA_VERSION {
        if let Some(backup_path) = backup_path {
            fs::write(backup_path, original)
                .map_err(|err| format!("cannot write migration backup: {err}"))?;
        }
        migrate_state(state, schema_version)?;
    }
    ensure_current_state_fields(state)
}

fn migrate_state(state: &mut Value, from_schema: u64) -> Result<(), String> {
    if from_schema != 0 {
        return Err(format!("unknown state schema {from_schema}"));
    }
    state["schema_version"] = json!(CURRENT_SCHEMA_VERSION);
    ensure_array_field(state, "handoffs")?;
    ensure_array_field(state, "drift_acknowledgments")?;
    state["context_baseline"] = state
        .get("context_baseline")
        .cloned()
        .unwrap_or_else(|| json!({"captured": false, "dirty": false, "domain_documents": []}));
    state["migration_events"] = json!([{
        "from_schema": from_schema,
        "to_schema": CURRENT_SCHEMA_VERSION,
        "summary": "Added lifecycle fields required by schema 1."
    }]);
    Ok(())
}

fn ensure_current_state_fields(state: &mut Value) -> Result<(), String> {
    ensure_array_field(state, "handoffs")?;
    ensure_array_field(state, "drift_acknowledgments")?;
    ensure_array_field(state, "boundaries")?;
    if state.get("clarification").is_none() {
        state["clarification"] = Value::Null;
    }
    if state.get("abort").is_none() {
        state["abort"] = Value::Null;
    }
    Ok(())
}

fn ensure_array_field(state: &mut Value, name: &str) -> Result<(), String> {
    if state.get(name).is_none() || state[name].is_null() {
        state[name] = json!([]);
    }
    if !state[name].is_array() {
        return Err(format!("state {name} is invalid"));
    }
    Ok(())
}

/// Typed variant of `read_state_for_update_at_revision` for handlers that
/// drive `RunState` transition methods.
/// Serialize the typed run state into the canonical Value projection — the
/// same shape `write_state` persists and `transition::commit` preflights.
pub(crate) fn to_state_value(run_state: &RunState) -> Result<Value, String> {
    serde_json::to_value(run_state).map_err(|err| format!("json error: {err}"))
}

/// Parse a run state Value into the typed schema.
pub(crate) fn run_state_from_value(state: Value) -> Result<RunState, String> {
    serde_json::from_value(state)
        .map_err(|err| format!("state does not match the run state schema: {err}"))
}

/// Bridge for the not-yet-typed `context`/`publication` modules: hand them
/// the canonical Value projection, then re-parse the mutated result back
/// into the typed schema.
pub(crate) fn with_value_projection<T>(
    run_state: &mut RunState,
    mutation: impl FnOnce(&mut Value) -> Result<T, String>,
) -> Result<T, String> {
    let mut projection = to_state_value(run_state)?;
    let outcome = mutation(&mut projection)?;
    *run_state = run_state_from_value(projection)?;
    Ok(outcome)
}

pub(crate) fn read_run_state_for_update_at_revision(
    worktree: &Path,
    run_id: &str,
    expected_revision: u64,
) -> Result<RunState, String> {
    let value = read_state_for_update_at_revision(worktree, run_id, expected_revision)?;
    serde_json::from_value(value)
        .map_err(|err| format!("state does not match the run state schema: {err}"))
}

/// Typed load for `inspect`: the canonical run state plus the migration
/// backfill the caller persists and audits. The `Option` is `Some` whenever
/// the state carries migration events, mirroring the legacy non-empty check.
pub(crate) fn read_run_state_for_inspect_at_revision(
    worktree: &Path,
    run_id: &str,
    expected_revision: u64,
) -> Result<(RunState, Option<Vec<MigrationEvent>>), String> {
    let run_state = read_run_state_for_update_at_revision(worktree, run_id, expected_revision)?;
    let migration_backfill =
        (!run_state.migration_events.is_empty()).then(|| run_state.migration_events.clone());
    Ok((run_state, migration_backfill))
}

pub(crate) fn read_state(worktree: &Path, run_id: &str) -> Result<Value, String> {
    let path = state_path(worktree, run_id)?;
    let original =
        fs::read_to_string(&path).map_err(|err| format!("cannot read state: {err}"))?;
    let mut state: Value =
        serde_json::from_str(&original).map_err(|err| format!("cannot parse state: {err}"))?;
    // Read-only path: migrate in memory. No backup is written here — the
    // update path writes it before persisting the migrated state.
    migrate_to_current(&mut state, &original, None)?;
    canonicalize_state(state)
}

pub(crate) fn active_run_for_session(worktree: &Path, session_id: &str) -> Result<Option<String>, String> {
    let runs_dir = worktree.join(".distill/runs");
    if !runs_dir.exists() {
        return Ok(None);
    }
    let mut matches = Vec::new();
    for entry in fs::read_dir(&runs_dir)
        .map_err(|err| format!("cannot list runs in {}: {err}", runs_dir.display()))?
    {
        let entry = entry.map_err(|err| format!("cannot inspect run entry: {err}"))?;
        let state_path = entry.path().join("state.json");
        if !state_path.is_file() {
            continue;
        }
        let state: Value = serde_json::from_str(
            &fs::read_to_string(&state_path)
                .map_err(|err| format!("cannot read {}: {err}", state_path.display()))?,
        )
        .map_err(|err| format!("cannot parse {}: {err}", state_path.display()))?;
        if (state["state"] == "active" || state["state"] == "blocked")
            && state["session_binding"]["session_id"] == session_id
        {
            if let Some(run_id) = state["run_id"].as_str() {
                matches.push(run_id.to_string());
            }
        }
    }
    match matches.len() {
        0 => Ok(None),
        1 => Ok(matches.pop()),
        _ => Err(format!(
            "multiple unfinished runs are bound to session {session_id}"
        )),
    }
}

pub(crate) fn write_state(worktree: &Path, state: &Value) -> Result<(), String> {
    let run_id = state["run_id"].as_str().ok_or("state is missing run_id")?;
    if env::var("DISTILL_FAIL_WRITE_STATE_FOR_RUN").as_deref() == Ok(run_id) {
        return Err(format!("injected state write failure for {run_id}"));
    }
    let bytes = serialize_state(state)?;
    storage::atomic_write(&state_path(worktree, run_id)?, &bytes, false)
}

pub(crate) fn state_path(worktree: &Path, run_id: &str) -> Result<PathBuf, String> {
    Ok(storage::run_dir(worktree, run_id)?.join("state.json"))
}

pub(crate) fn write_bytes(path: &Path, bytes: &[u8], atomic: bool) -> Result<(), String> {
    if atomic {
        return storage::atomic_write(path, bytes, false);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("cannot create dir: {err}"))?;
    }
    fs::write(path, bytes).map_err(|err| format!("cannot write {}: {err}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Synthetic full-schema state: every key the current binary can write,
    /// plus an unknown top-level key a newer binary might have added.
    fn full_schema_state() -> Value {
        json!({
            "schema_version": 1,
            "run_id": "run-20260903-abcd",
            "state": "active",
            "revision": 7,
            "current_stage": "issues",
            "predecessor_run_id": "run-previous",
            "session_binding": {
                "runtime": "codex",
                "session_id": "sess-1",
                "released": true,
                "released_revision": 6
            },
            "workflow": {
                "version": "distill.v1",
                "source": "embedded:distill.v1.json",
                "stages": [
                    {"id": "intake", "executor": "runner", "skill": null, "checkpoint": "explicit-text-captured", "next_action": "capture-text-requirement"},
                    {"id": "clarification", "executor": "skill", "skill": "grill-with-docs", "checkpoint": "clarification-complete", "next_action": "invoke-skill"}
                ]
            },
            "requirement": {
                "source": "explicit-text",
                "text": "Build a release-readiness dashboard.",
                "supersession_reason": "scope changed"
            },
            "requirement_snapshot": {"sources": [], "total_raw_bytes": 0},
            "clarification": {
                "clarified_requirement": "Build a dashboard with a JSON CLI seam.",
                "decisions": ["Expose readiness through the existing CLI."],
                "accepted_assumptions": ["The local tracker is authoritative."],
                "material_unknowns": [{"description": "Which runtime drives the smoke test?", "material": true, "resolved": true, "resolution": "Codex."}],
                "domain_document_artifacts": [{"path": "CONTEXT.md", "sha256": "abc123"}]
            },
            "storage": {
                "limits": {"per_source_bytes": 1, "run_bytes": 2, "project_bytes": 3, "event_bytes": 4, "run_event_log_bytes": 5},
                "usage": {"project_bytes": 123}
            },
            "stages": [
                {"id": "intake", "state": "completed", "revision": 1, "authorized_action": {"type": "capture-text-requirement"}},
                {"id": "clarification", "state": "completed", "revision": 3, "authorized_action": {"type": "invoke-skill"}},
                {"id": "prd", "state": "waiting", "revision": 4, "authorized_action": {"type": "wait"}},
                {"id": "issues", "state": "blocked", "revision": 5, "authorized_action": {"type": "unblock"}},
                {"id": "publish", "state": "needs-reconciliation", "revision": 6, "authorized_action": {"type": "reconcile"}},
                {"id": "report", "state": "pending", "revision": 0, "authorized_action": {"type": "invoke-skill"}},
                {"id": "done", "state": "active", "revision": 6, "authorized_action": {"type": "invoke-skill"}}
            ],
            "completion_evidence": [
                {"stage": "intake", "completed_revision": 1, "accepted_user_checkpoint": "explicit-text-captured", "summary": "Captured the supplied requirement text."},
                {"stage": "clarification", "completed_revision": 3, "accepted_user_checkpoint": "clarification-complete", "summary": "Clarified.", "adapter": {"executor": "skill", "skill": "grill-with-docs", "invocation": "unmodified-skill"}, "clarification": {"clarified_requirement": "x"}},
                {"stage": "prd", "completed_revision": 4, "accepted_user_checkpoint": "prd-approved", "summary": "PRD done.", "adapter": {"executor": "skill", "skill": null, "invocation": "unmodified-skill"}}
            ],
            "publications": {
                "prd": {"operation_id": "run-r4-prd", "artifact_id": "PRD", "path": "docs/prd/PRD.md", "payload_path": ".distill/runs/run/publication/payloads/x.payload", "payload_hash": "deadbeef", "dependency_artifact_ids": [], "tracker": "local-markdown", "status": "confirmed", "title": null},
                "issues": [
                    {"operation_id": "run-r5-issue-01", "artifact_id": "01-foo", "path": "docs/issues/01-foo.md", "payload_path": ".distill/runs/run/publication/payloads/y.payload", "payload_hash": "beef", "dependency_artifact_ids": ["01-foo"], "tracker": "local-markdown", "status": "needs-reconciliation", "title": "Foo"},
                    {"operation_id": "run-r5-issue-02", "title": "Bar", "status": "pending"}
                ]
            },
            "report": {"json_path": ".distill/runs/run/report.json", "markdown_path": ".distill/runs/run/report.md", "canonical_hash": "cafe", "renderer": {"name": "markdown", "status": "rendered", "retryable": false}},
            "context_baseline": {"captured": true, "worktree": "/tmp/w", "git_head": "abc", "git_branch": "main", "git_status": "", "dirty": false, "domain_documents": [{"path": "CONTEXT.md", "hash": "h"}]},
            "drift_acknowledgments": [{"stage": "prd", "material": false, "reason": "typo fix", "detected": {"domain_documents": ["CONTEXT.md"]}}],
            "boundaries": [{"stage": "prd", "state": "waiting", "reason": "awaiting review", "required_next_action": "wait", "revision": 4}],
            "abort": null,
            "handoffs": [{"from_session": "sess-0", "to_session": "sess-1", "reason": "laptop sleep", "revision": 2, "invalidates_previous_session": true}],
            "migration_events": [{"from_schema": 0, "to_schema": 1, "summary": "Added lifecycle fields required by schema 1."}],
            "purge": {"cleanup_state": "pending", "source_revision": 6, "revision": 7, "user_authorized": true, "tombstone": {"run_id": "run", "state": "purged", "revision": 7, "purged_at": 1725000000000i64, "user_authorized": true, "source_hashes": {}, "publications": {"prd": null, "issues": []}}},
            "superseded_by": "run-next",
            "supersession": {"reason": "scope changed", "revision": 7, "successor_run_id": "run-next"},
            "implementation_started": false,
            "future_extension_key": {"nested": [1, 2, 3], "note": "written by a newer binary"}
        })
    }

    #[test]
    fn full_schema_state_round_trips_byte_for_byte() {
        let state = full_schema_state();
        let expected = serde_json::to_vec_pretty(&state).expect("baseline bytes");

        let run_state: RunState = serde_json::from_value(state).expect("full schema deserializes");
        let value = serde_json::to_value(&run_state).expect("serialize to value");
        let actual = serde_json::to_vec_pretty(&value).expect("pretty bytes");

        assert_eq!(
            actual,
            expected,
            "deserialize → serialize must be byte-identical (sorted-key pretty); diff:\n{}",
            String::from_utf8_lossy(&actual)
        );
    }

    #[test]
    fn unknown_top_level_key_survives_read_modify_write() {
        let temp = tempfile::tempdir().expect("tempdir");
        let state = full_schema_state();
        write_state(temp.path(), &state).expect("write state");

        // read-modify-write through the typed load/save path
        let mut loaded = read_state_for_update_at_revision(temp.path(), "run-20260903-abcd", 7)
            .expect("load state");
        loaded["revision"] = json!(8);
        write_state(temp.path(), &loaded).expect("rewrite state");

        let persisted = read_state(temp.path(), "run-20260903-abcd").expect("re-read state");
        assert_eq!(
            persisted["future_extension_key"],
            json!({"nested": [1, 2, 3], "note": "written by a newer binary"}),
            "unknown top-level key must survive a read-modify-write round trip"
        );
        assert_eq!(persisted["revision"], json!(8));
    }

    #[test]
    fn legacy_schema_zero_state_deserializes_after_migration() {
        let mut state = json!({
            "schema_version": 0,
            "run_id": "legacy",
            "state": "active",
            "revision": 1,
            "current_stage": "clarification",
            "session_binding": {"runtime": "codex", "session_id": "legacy-session"},
            "workflow": {"version": "distill.v1", "source": "embedded:distill.v1.json", "stages": []},
            "requirement": {"source": "explicit-text", "text": "legacy requirement"},
            "stages": [],
            "completion_evidence": [{"stage": "intake", "completed_revision": 1, "accepted_user_checkpoint": "explicit-text-captured", "summary": "Captured."}],
            "publications": {"prd": Value::Null, "issues": []},
            "report": Value::Null,
            "implementation_started": false
        });
        migrate_state(&mut state, 0).expect("migration succeeds");
        ensure_current_state_fields(&mut state).expect("current fields ensured");

        let run_state: RunState =
            serde_json::from_value(state).expect("legacy state deserializes after migration");
        assert_eq!(run_state.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(run_state.migration_events.len(), 1);
        // ...and serializes back through the typed write path.
        serde_json::to_value(&run_state).expect("legacy state serializes");
    }

    #[test]
    fn write_path_uses_the_same_serializer_as_quota_preflight() {
        let temp = tempfile::tempdir().expect("tempdir");
        let state = full_schema_state();

        // `serialize_state` is the serializer the commit quota preflight uses
        // to price the state write; the bytes on disk must match it exactly.
        let preflight_bytes = serialize_state(&state).expect("preflight bytes");
        write_state(temp.path(), &state).expect("write state");
        let written = std::fs::read(state_path(temp.path(), "run-20260903-abcd").expect("path"))
            .expect("read written state");

        assert_eq!(written, preflight_bytes);
    }

    #[test]
    fn run_state_new_covers_start_and_supersede_shapes() {
        let workflow = crate::workflow::load_workflow().expect("workflow");
        let stage_states = crate::workflow::initial_stage_states(&workflow).expect("stage states");
        let session_binding = SessionBinding {
            runtime: "codex".to_string(),
            session_id: "sess-1".to_string(),
            released: None,
            released_revision: None,
        };
        let workflow_snapshot = WorkflowSnapshot {
            version: workflow.version.clone(),
            source: crate::WORKFLOW_SOURCE.to_string(),
            stages: workflow.stages.clone(),
        };

        // start shape: snapshot + storage present, no predecessor.
        let started = RunState::new(NewRunState {
            run_id: "run-a".to_string(),
            lifecycle: RunLifecycle::Active,
            current_stage: "clarification".to_string(),
            predecessor_run_id: None,
            session_binding: session_binding.clone(),
            workflow: workflow_snapshot.clone(),
            requirement: Requirement {
                source: "explicit-text".to_string(),
                text: "Build a dashboard.".to_string(),
                supersession_reason: None,
            },
            requirement_snapshot: Some(json!({"sources": []})),
            storage: Some(json!({"limits": {}, "usage": {}})),
            stage_states: stage_states.clone(),
            intake_summary: "Captured the supplied requirement text.".to_string(),
            context_baseline: json!({"captured": false}),
        })
        .expect("start state");
        let started = serde_json::to_value(&started).expect("start value");
        assert_eq!(started["state"], json!("active"));
        assert!(started.get("requirement_snapshot").is_some());
        assert!(started.get("storage").is_some());
        assert!(started.get("predecessor_run_id").is_none());
        assert_eq!(started["implementation_started"], json!(false));
        assert_eq!(started["revision"], json!(1));
        assert_eq!(started["completion_evidence"][0]["stage"], json!("intake"));
        assert!(started["completion_evidence"][0].get("adapter").is_none());

        // supersede shape: predecessor present, snapshot + storage absent.
        let successor = RunState::new(NewRunState {
            run_id: "run-b".to_string(),
            lifecycle: RunLifecycle::SupersessionPending,
            current_stage: "clarification".to_string(),
            predecessor_run_id: Some("run-a".to_string()),
            session_binding,
            workflow: workflow_snapshot,
            requirement: Requirement {
                source: "explicit-text".to_string(),
                text: "Build something else.".to_string(),
                supersession_reason: Some("scope changed".to_string()),
            },
            requirement_snapshot: None,
            storage: None,
            stage_states,
            intake_summary: "Captured the superseding requirement text.".to_string(),
            context_baseline: json!({"captured": false}),
        })
        .expect("successor state");
        let successor = serde_json::to_value(&successor).expect("successor value");
        assert_eq!(successor["state"], json!("supersession-pending"));
        assert_eq!(successor["predecessor_run_id"], json!("run-a"));
        assert_eq!(
            successor["requirement"]["supersession_reason"],
            json!("scope changed")
        );
        assert!(successor.get("requirement_snapshot").is_none());
        assert!(successor.get("storage").is_none());
        assert_eq!(successor["implementation_started"], json!(false));
    }

    #[test]
    fn read_state_loads_legacy_state_through_migration() {
        let temp = tempfile::tempdir().expect("tempdir");
        let legacy = json!({
            "schema_version": 0,
            "run_id": "legacy",
            "state": "active",
            "revision": 1,
            "current_stage": "clarification",
            "session_binding": {"runtime": "codex", "session_id": "legacy-session"},
            "workflow": {"version": "distill.v1", "source": "embedded:distill.v1.json", "stages": []},
            "requirement": {"source": "explicit-text", "text": "legacy requirement"},
            "stages": [],
            "completion_evidence": [{"stage": "intake", "completed_revision": 1, "accepted_user_checkpoint": "explicit-text-captured", "summary": "Captured."}],
            "publications": {"prd": Value::Null, "issues": []},
            "report": Value::Null,
            "implementation_started": false
        });
        let path = state_path(temp.path(), "legacy").expect("path");
        std::fs::create_dir_all(path.parent().expect("parent")).expect("run dir");
        std::fs::write(&path, serde_json::to_vec_pretty(&legacy).expect("bytes"))
            .expect("write legacy state");

        // The read-only load path is the same `read Value → migrate_state →
        // deserialize RunState` pipeline as the update path (minus the
        // on-disk migration backup, which the update path writes before it
        // persists the migrated state).
        let state = read_state(temp.path(), "legacy").expect("legacy state loads via read_state");
        assert_eq!(state["schema_version"], json!(CURRENT_SCHEMA_VERSION));
        assert_eq!(state["migration_events"].as_array().expect("array").len(), 1);
        assert_eq!(state["implementation_started"], json!(false));
    }

    #[test]
    fn stage_evidence_failing_the_stage_variant_reclassifies_without_data_loss() {
        // A stage entry whose `adapter` is present but malformed fails the
        // `Stage` variant and falls through to `Intake`; the catch-all must
        // carry `adapter` along so re-serialization loses nothing.
        let entry = json!({
            "stage": "prd",
            "completed_revision": 4,
            "accepted_user_checkpoint": "prd-approved",
            "summary": "PRD done.",
            "adapter": {"executor": "skill"}
        });
        let evidence: CompletionEvidence =
            serde_json::from_value(entry.clone()).expect("entry deserializes");
        let value = serde_json::to_value(&evidence).expect("serializes");
        assert_eq!(value, entry, "reclassified entry must not lose any keys");
    }

    #[test]
    fn published_issue_failing_the_full_variant_reclassifies_without_data_loss() {
        // A published issue entry missing a required field (here `path`)
        // falls through to the `Pending` variant; the catch-all must carry
        // the remaining keys along so re-serialization loses nothing.
        let entry = json!({
            "operation_id": "run-r5-issue-01",
            "title": "Foo",
            "status": "confirmed",
            "artifact_id": "01-foo",
            "payload_path": ".distill/runs/run/publication/payloads/y.payload",
            "payload_hash": "beef",
            "dependency_artifact_ids": [],
            "tracker": "local-markdown"
        });
        let publication: IssuePublication =
            serde_json::from_value(entry.clone()).expect("entry deserializes");
        let value = serde_json::to_value(&publication).expect("serializes");
        assert_eq!(value, entry, "reclassified entry must not lose any keys");
    }

    // ── transition table tests ──

    fn active_run_state() -> RunState {
        let workflow = crate::workflow::load_workflow().expect("workflow");
        RunState::new(NewRunState {
            run_id: "run-t".to_string(),
            lifecycle: RunLifecycle::Active,
            current_stage: "clarification".to_string(),
            predecessor_run_id: None,
            session_binding: SessionBinding {
                runtime: "codex".to_string(),
                session_id: "sess-1".to_string(),
                released: None,
                released_revision: None,
            },
            workflow: WorkflowSnapshot {
                version: workflow.version.clone(),
                source: crate::WORKFLOW_SOURCE.to_string(),
                stages: workflow.stages.clone(),
            },
            requirement: Requirement {
                source: "explicit-text".to_string(),
                text: "req".to_string(),
                supersession_reason: None,
            },
            requirement_snapshot: None,
            storage: None,
            stage_states: crate::workflow::initial_stage_states(&workflow).expect("stages"),
            intake_summary: "Captured.".to_string(),
            context_baseline: json!({"captured": false}),
        })
        .expect("run state")
    }

    fn workflow_stage(run_state: &RunState, id: &str) -> crate::workflow::WorkflowStage {
        run_state
            .workflow
            .stages
            .iter()
            .find(|stage| stage.id == id)
            .cloned()
            .expect("workflow stage")
    }

    #[test]
    fn complete_stage_advances_to_next_stage() {
        let mut run_state = active_run_state();
        let stage = workflow_stage(&run_state, "clarification");

        let outcome = run_state
            .complete_stage("sess-1", &stage, "Clarified.")
            .expect("legal completion");

        assert_eq!(
            outcome,
            StageCompletion::Advanced {
                next_stage: "prd".to_string()
            }
        );
        assert_eq!(run_state.revision, 2, "revision increments by one");
        assert_eq!(run_state.state, RunLifecycle::Active);
        assert_eq!(run_state.current_stage.as_deref(), Some("prd"));
        let clarification = run_state
            .stages
            .iter()
            .find(|entry| entry.id == "clarification")
            .expect("stage");
        assert_eq!(clarification.state, StageState::Completed);
        assert_eq!(clarification.revision, 2);
        let prd = run_state
            .stages
            .iter()
            .find(|entry| entry.id == "prd")
            .expect("stage");
        assert_eq!(prd.state, StageState::Active);
        assert_eq!(prd.revision, 2);
        let evidence = run_state.completion_evidence.last().expect("evidence");
        assert_eq!(evidence.stage(), "clarification");
        let CompletionEvidence::Stage(evidence) = evidence else {
            panic!("stage completion must record the adapter-carrying variant");
        };
        assert_eq!(evidence.completed_revision, 2);
        assert_eq!(evidence.accepted_user_checkpoint, "clarification-complete");
        assert_eq!(evidence.adapter.invocation, "unmodified-skill");
        assert_eq!(evidence.adapter.skill.as_deref(), Some("grill-with-docs"));
    }

    #[test]
    fn complete_stage_final_stage_completes_run_and_releases_session() {
        let mut run_state = active_run_state();
        run_state.current_stage = Some("issues".to_string());
        let stage = workflow_stage(&run_state, "issues");

        let outcome = run_state
            .complete_stage("sess-1", &stage, "Issues sliced.")
            .expect("legal terminal completion");

        assert_eq!(outcome, StageCompletion::RunCompleted);
        assert_eq!(run_state.revision, 2, "revision increments by one");
        assert_eq!(run_state.state, RunLifecycle::Completed);
        assert_eq!(run_state.current_stage, None);
        assert_eq!(run_state.session_binding.released, Some(true));
        assert_eq!(run_state.session_binding.released_revision, Some(2));
        let issues = run_state
            .stages
            .iter()
            .find(|entry| entry.id == "issues")
            .expect("stage");
        assert_eq!(issues.state, StageState::Completed);
        assert_eq!(issues.revision, 2);
    }

    #[test]
    fn complete_stage_rejects_wrong_session() {
        let mut run_state = active_run_state();
        let stage = workflow_stage(&run_state, "clarification");
        let err = run_state
            .complete_stage("sess-other", &stage, "x")
            .expect_err("session mismatch is illegal");
        assert_eq!(
            err,
            StateError::SessionMismatch {
                expected: "sess-1".to_string(),
                actual: "sess-other".to_string()
            }
        );
        assert_eq!(
            err.to_string(),
            "session id does not match active run binding"
        );
    }

    #[test]
    fn complete_stage_rejects_unauthorized_stage() {
        let mut run_state = active_run_state();
        let stage = workflow_stage(&run_state, "prd");
        let err = run_state
            .complete_stage("sess-1", &stage, "x")
            .expect_err("out-of-turn stage is illegal");
        assert_eq!(
            err,
            StateError::StageNotAuthorized {
                expected: "clarification".to_string(),
                actual: "prd".to_string()
            }
        );
        assert_eq!(
            err.to_string(),
            "stage prd is not authorized; expected clarification"
        );
    }

    #[test]
    fn complete_stage_rejects_already_completed_stage() {
        let mut run_state = active_run_state();
        let stage = workflow_stage(&run_state, "intake");
        let err = run_state
            .complete_stage("sess-1", &stage, "x")
            .expect_err("re-completing a stage is illegal");
        assert_eq!(
            err,
            StateError::StageAlreadyCompleted {
                stage: "intake".to_string()
            }
        );
        assert_eq!(err.to_string(), "stage intake is already completed");
    }

    #[test]
    fn complete_stage_rejects_non_active_run() {
        let mut run_state = active_run_state();
        run_state.state = RunLifecycle::Completed;
        let stage = workflow_stage(&run_state, "clarification");
        let err = run_state
            .complete_stage("sess-1", &stage, "x")
            .expect_err("terminal run is illegal");
        assert_eq!(
            err,
            StateError::IllegalRunState {
                expected: "active or blocked".to_string(),
                actual: "completed".to_string()
            }
        );
        assert_eq!(err.to_string(), "run is not active or blocked");
    }

    #[test]
    fn guard_rejects_missing_current_stage() {
        let run_state = RunState {
            current_stage: None,
            ..active_run_state()
        };
        let err = run_state
            .guard_stage_submission("sess-1", "clarification")
            .expect_err("missing current stage is illegal");
        assert_eq!(err, StateError::MissingCurrentStage);
        assert_eq!(err.to_string(), "state is missing current_stage");
    }

    #[test]
    fn defer_stage_waiting_records_boundary_and_bumps_revision() {
        let mut run_state = active_run_state();

        run_state
            .defer_stage(
                "sess-1",
                "clarification",
                BoundaryState::Waiting,
                "awaiting review",
                "wait",
            )
            .expect("legal defer");

        assert_eq!(run_state.revision, 2, "revision increments by one");
        assert_eq!(run_state.state, RunLifecycle::Active);
        let stage = run_state
            .stages
            .iter()
            .find(|entry| entry.id == "clarification")
            .expect("stage");
        assert_eq!(stage.state, StageState::Waiting);
        assert_eq!(stage.revision, 2);
        assert_eq!(run_state.boundaries.len(), 1);
        let boundary = run_state.boundaries[0].as_typed().expect("typed boundary");
        assert_eq!(boundary.stage, "clarification");
        assert_eq!(boundary.state, BoundaryState::Waiting);
        assert_eq!(boundary.reason, "awaiting review");
        assert_eq!(boundary.required_next_action, "wait");
        assert_eq!(boundary.revision, 2);
    }

    #[test]
    fn defer_stage_blocked_marks_run_blocked() {
        let mut run_state = active_run_state();

        run_state
            .defer_stage(
                "sess-1",
                "clarification",
                BoundaryState::Blocked,
                "missing dependency",
                "unblock",
            )
            .expect("legal block");

        assert_eq!(run_state.revision, 2);
        assert_eq!(run_state.state, RunLifecycle::Blocked);
        let stage = run_state
            .stages
            .iter()
            .find(|entry| entry.id == "clarification")
            .expect("stage");
        assert_eq!(stage.state, StageState::Blocked);
        assert_eq!(stage.revision, 2);
        assert_eq!(
            run_state.boundaries[0].as_typed().expect("typed boundary").state,
            BoundaryState::Blocked
        );
    }

    #[test]
    fn defer_stage_rejects_pending_purge() {
        let mut run_state = active_run_state();
        run_state.purge = Some(PurgeState {
            cleanup_state: PurgeCleanupState::Pending,
            source_revision: 1,
            revision: 2,
            user_authorized: true,
            tombstone: json!({}),
        });
        let err = run_state
            .defer_stage(
                "sess-1",
                "clarification",
                BoundaryState::Waiting,
                "reason",
                "wait",
            )
            .expect_err("pending purge is illegal");
        assert_eq!(err, StateError::PendingPurge);
        assert_eq!(
            err.to_string(),
            "purge cleanup is pending; resume purge before any other transition"
        );
    }

    #[test]
    fn record_boundary_rejects_empty_fields() {
        let mut run_state = active_run_state();
        let err = run_state
            .record_boundary("clarification", BoundaryState::Waiting, "", "wait")
            .expect_err("empty reason is illegal");
        assert_eq!(err, StateError::EmptyBoundaryReason);
        assert_eq!(
            err.to_string(),
            "boundary evidence.reason must not be empty"
        );
        let err = run_state
            .record_boundary("clarification", BoundaryState::Waiting, "reason", "")
            .expect_err("empty required next action is illegal");
        assert_eq!(err, StateError::EmptyBoundaryRequiredNextAction);
        assert_eq!(
            err.to_string(),
            "boundary evidence.required_next_action must not be empty"
        );
    }

    #[test]
    fn release_session_is_one_shot() {
        let mut run_state = active_run_state();
        run_state.release_session().expect("legal release");
        assert_eq!(run_state.session_binding.released, Some(true));
        assert_eq!(run_state.session_binding.released_revision, Some(1));
        let err = run_state
            .release_session()
            .expect_err("double release is illegal");
        assert_eq!(err, StateError::SessionAlreadyReleased);
    }

    #[test]
    fn mark_stage_needs_reconciliation_flips_stage_and_bumps_revision() {
        let mut run_state = active_run_state();

        run_state
            .mark_stage_needs_reconciliation("sess-1", "clarification")
            .expect("legal reconciliation mark");

        assert_eq!(run_state.revision, 2, "revision increments by one");
        assert_eq!(run_state.state, RunLifecycle::Active);
        let stage = run_state
            .stages
            .iter()
            .find(|entry| entry.id == "clarification")
            .expect("stage");
        assert_eq!(stage.state, StageState::NeedsReconciliation);
        assert_eq!(stage.revision, 2);
        let err = run_state
            .mark_stage_needs_reconciliation("sess-other", "clarification")
            .expect_err("session mismatch is illegal");
        assert_eq!(
            err,
            StateError::SessionMismatch {
                expected: "sess-1".to_string(),
                actual: "sess-other".to_string()
            }
        );
    }

    #[test]
    fn complete_stage_rejects_stage_missing_from_stage_states() {
        // Guard passes (stage is current and not completed), but stages[]
        // has no entry for it.
        let mut run_state = active_run_state();
        run_state.current_stage = Some("ghost".to_string());
        let ghost = crate::workflow::WorkflowStage {
            id: "ghost".to_string(),
            executor: "skill".to_string(),
            skill: None,
            checkpoint: "ghost-checkpoint".to_string(),
            next_action: "invoke-skill".to_string(),
        };
        let err = run_state
            .complete_stage("sess-1", &ghost, "x")
            .expect_err("stage with no stage-state entry is illegal");
        assert_eq!(
            err,
            StateError::StageNotFound {
                stage: "ghost".to_string()
            }
        );
        assert_eq!(err.to_string(), "stage state not found: ghost");
    }

    #[test]
    fn complete_stage_rejects_stage_missing_from_workflow_snapshot() {
        // stages[] has an entry and the stage is current, but the workflow
        // snapshot does not define it.
        let mut run_state = active_run_state();
        run_state.current_stage = Some("ghost".to_string());
        run_state.stages.push(StageStateEntry {
            id: "ghost".to_string(),
            state: StageState::Active,
            revision: 1,
            authorized_action: json!({"type": "invoke-skill"}),
        });
        let ghost = crate::workflow::WorkflowStage {
            id: "ghost".to_string(),
            executor: "skill".to_string(),
            skill: None,
            checkpoint: "ghost-checkpoint".to_string(),
            next_action: "invoke-skill".to_string(),
        };
        let err = run_state
            .complete_stage("sess-1", &ghost, "x")
            .expect_err("stage outside the workflow snapshot is illegal");
        assert_eq!(
            err,
            StateError::WorkflowStageNotFound {
                stage: "ghost".to_string()
            }
        );
        assert_eq!(err.to_string(), "workflow stage not found: ghost");
    }

    #[test]
    fn mark_stage_needs_reconciliation_rejects_unauthorized_stage() {
        let mut run_state = active_run_state();
        let err = run_state
            .mark_stage_needs_reconciliation("sess-1", "issues")
            .expect_err("out-of-turn stage is illegal");
        assert_eq!(
            err,
            StateError::StageNotAuthorized {
                expected: "clarification".to_string(),
                actual: "issues".to_string()
            }
        );
        assert_eq!(
            err.to_string(),
            "stage issues is not authorized; expected clarification"
        );
    }

    #[test]
    fn abort_flips_terminal_state_and_releases_session() {
        let mut run_state = active_run_state();

        run_state
            .abort("sess-1", "no longer needed", vec![json!({"path": "docs/adr/0001.md"})])
            .expect("legal abort");

        assert_eq!(run_state.revision, 2, "revision increments by one");
        assert_eq!(run_state.state, RunLifecycle::Aborted);
        assert_eq!(run_state.current_stage, None);
        assert_eq!(run_state.session_binding.released, Some(true));
        assert_eq!(run_state.session_binding.released_revision, Some(2));
        let abort = run_state.abort.as_ref().expect("abort record");
        assert_eq!(abort.reason, "no longer needed");
        assert_eq!(abort.revision, 2);
        assert!(abort.user_authorized);
        assert_eq!(
            abort.domain_document_artifacts,
            vec![json!({"path": "docs/adr/0001.md"})]
        );
    }

    #[test]
    fn abort_rejects_illegal_calls() {
        let mut run_state = active_run_state();
        run_state.purge = Some(PurgeState {
            cleanup_state: PurgeCleanupState::Pending,
            source_revision: 1,
            revision: 2,
            user_authorized: true,
            tombstone: json!({}),
        });
        let err = run_state
            .abort("sess-1", "x", vec![])
            .expect_err("pending purge is illegal");
        assert_eq!(err, StateError::PendingPurge);
        assert_eq!(
            err.to_string(),
            "purge cleanup is pending; resume purge before any other transition"
        );

        let mut run_state = active_run_state();
        run_state.state = RunLifecycle::Completed;
        let err = run_state
            .abort("sess-1", "x", vec![])
            .expect_err("completed run is illegal");
        assert_eq!(
            err,
            StateError::IllegalRunState {
                expected: "active or blocked".to_string(),
                actual: "completed".to_string()
            }
        );
        assert_eq!(err.to_string(), "run is not active or blocked");

        let mut run_state = active_run_state();
        let err = run_state
            .abort("sess-other", "x", vec![])
            .expect_err("session mismatch is illegal");
        assert_eq!(
            err,
            StateError::RunBindingSessionMismatch {
                expected: "sess-1".to_string(),
                actual: "sess-other".to_string()
            }
        );
        assert_eq!(err.to_string(), "session id does not match run binding");
        assert_eq!(run_state.revision, 1, "rejected abort leaves revision untouched");
    }

    #[test]
    fn takeover_switches_session_and_records_handoff() {
        let mut run_state = active_run_state();

        run_state
            .takeover("sess-1", "sess-2", "thread interrupted")
            .expect("legal takeover");

        assert_eq!(run_state.revision, 2, "revision increments by one");
        assert_eq!(run_state.session_binding.session_id, "sess-2");
        assert_eq!(run_state.handoffs.len(), 1);
        let handoff = &run_state.handoffs[0];
        assert_eq!(handoff.from_session, "sess-1");
        assert_eq!(handoff.to_session, "sess-2");
        assert_eq!(handoff.reason, "thread interrupted");
        assert_eq!(handoff.revision, 2);
        assert!(handoff.invalidates_previous_session);
    }

    #[test]
    fn takeover_rejects_illegal_calls() {
        let mut run_state = active_run_state();
        run_state.purge = Some(PurgeState {
            cleanup_state: PurgeCleanupState::Pending,
            source_revision: 1,
            revision: 2,
            user_authorized: true,
            tombstone: json!({}),
        });
        let err = run_state
            .takeover("sess-1", "sess-2", "x")
            .expect_err("pending purge is illegal");
        assert_eq!(err, StateError::PendingPurge);

        let mut run_state = active_run_state();
        run_state.state = RunLifecycle::Superseded;
        let err = run_state
            .takeover("sess-1", "sess-2", "x")
            .expect_err("superseded run is illegal");
        assert!(matches!(err, StateError::IllegalRunState { .. }));

        let mut run_state = active_run_state();
        let err = run_state
            .takeover("sess-other", "sess-2", "x")
            .expect_err("from-session mismatch is illegal");
        assert_eq!(
            err,
            StateError::FromSessionMismatch {
                expected: "sess-1".to_string(),
                actual: "sess-other".to_string()
            }
        );
        assert_eq!(
            err.to_string(),
            "from-session does not match active run binding"
        );
        assert_eq!(run_state.revision, 1);
        assert!(run_state.handoffs.is_empty());
    }

    #[test]
    fn begin_purge_marks_pending_and_recovers_without_a_second_bump() {
        let mut run_state = active_run_state();
        let tombstone = json!({"run_id": "run-t"});

        let begin = run_state
            .begin_purge("sess-1", tombstone.clone())
            .expect("legal begin");
        assert_eq!(begin, PurgeBegin::Started);
        assert_eq!(run_state.revision, 2, "revision increments by one");
        assert_eq!(
            run_state.state,
            RunLifecycle::Active,
            "begin_purge leaves the run lifecycle untouched"
        );
        let purge = run_state.purge.as_ref().expect("purge state");
        assert_eq!(purge.cleanup_state, PurgeCleanupState::Pending);
        assert_eq!(purge.source_revision, 1);
        assert_eq!(purge.revision, 2);
        assert!(purge.user_authorized);
        assert_eq!(purge.tombstone, tombstone);

        let begin = run_state
            .begin_purge("sess-1", json!({}))
            .expect("recovery begin");
        assert_eq!(begin, PurgeBegin::Recovering);
        assert_eq!(run_state.revision, 2, "recovery does not bump again");
        assert_eq!(
            run_state.purge.as_ref().expect("purge").tombstone,
            tombstone,
            "recovery keeps the durable tombstone"
        );
    }

    #[test]
    fn begin_purge_rejects_wrong_session_and_purged_run() {
        let mut run_state = active_run_state();
        let err = run_state
            .begin_purge("sess-other", json!({}))
            .expect_err("session mismatch is illegal");
        assert_eq!(
            err,
            StateError::RunBindingSessionMismatch {
                expected: "sess-1".to_string(),
                actual: "sess-other".to_string()
            }
        );
        assert_eq!(err.to_string(), "session id does not match run binding");

        let mut run_state = active_run_state();
        run_state.state = RunLifecycle::Purged;
        let err = run_state
            .begin_purge("sess-1", json!({}))
            .expect_err("already purged is illegal");
        assert_eq!(err, StateError::AlreadyPurged);
        assert_eq!(err.to_string(), "run is already purged");
        assert_eq!(run_state.revision, 1);
    }

    #[test]
    fn complete_purge_finalizes_at_the_same_revision() {
        let mut run_state = active_run_state();
        run_state
            .begin_purge("sess-1", json!({"run_id": "run-t"}))
            .expect("begin");

        run_state.complete_purge("sess-1").expect("legal complete");

        assert_eq!(run_state.revision, 2, "complete_purge does not bump");
        assert_eq!(run_state.state, RunLifecycle::Purged);
        assert_eq!(run_state.current_stage, None);
        assert_eq!(run_state.session_binding.released, Some(true));
        assert_eq!(run_state.session_binding.released_revision, Some(2));
        assert!(run_state.requirement.is_none());
        assert_eq!(
            run_state.requirement_snapshot,
            Some(json!({
                "purged": true,
                "tombstone_path": ".distill/runs/run-t/tombstone.json"
            }))
        );
        assert_eq!(
            run_state.purge.as_ref().expect("purge").cleanup_state,
            PurgeCleanupState::Completed
        );
    }

    #[test]
    fn complete_purge_re_releases_an_already_released_session() {
        // Purging a completed run: the session was released at completion and
        // the purge overwrites the release marker idempotently.
        let mut run_state = active_run_state();
        run_state.state = RunLifecycle::Completed;
        run_state.current_stage = None;
        run_state.session_binding.released = Some(true);
        run_state.session_binding.released_revision = Some(1);

        run_state.begin_purge("sess-1", json!({})).expect("begin");
        run_state.complete_purge("sess-1").expect("complete");

        assert_eq!(run_state.session_binding.released, Some(true));
        assert_eq!(run_state.session_binding.released_revision, Some(2));
    }

    #[test]
    fn complete_purge_requires_a_pending_purge() {
        let mut run_state = active_run_state();
        let err = run_state
            .complete_purge("sess-1")
            .expect_err("no pending purge is illegal");
        assert_eq!(err, StateError::NoPendingPurge);

        let mut run_state = active_run_state();
        run_state.begin_purge("sess-1", json!({})).expect("begin");
        let err = run_state
            .complete_purge("sess-other")
            .expect_err("session mismatch is illegal");
        assert_eq!(
            err,
            StateError::RunBindingSessionMismatch {
                expected: "sess-1".to_string(),
                actual: "sess-other".to_string()
            }
        );
        assert_eq!(
            run_state.state,
            RunLifecycle::Active,
            "rejected complete leaves state untouched"
        );
    }

    #[test]
    fn mark_superseded_flips_state_and_links_successor() {
        let mut run_state = active_run_state();

        run_state
            .mark_superseded("sess-1", "scope changed", "run-next")
            .expect("legal supersede");

        assert_eq!(run_state.revision, 2, "revision increments by one");
        assert_eq!(run_state.state, RunLifecycle::Superseded);
        assert_eq!(run_state.current_stage, None);
        assert_eq!(run_state.superseded_by.as_deref(), Some("run-next"));
        let supersession = run_state.supersession.as_ref().expect("supersession");
        assert_eq!(supersession.reason, "scope changed");
        assert_eq!(supersession.revision, 2);
        assert_eq!(supersession.successor_run_id, "run-next");
    }

    #[test]
    fn mark_superseded_rejects_illegal_calls() {
        let mut run_state = active_run_state();
        run_state.purge = Some(PurgeState {
            cleanup_state: PurgeCleanupState::Pending,
            source_revision: 1,
            revision: 2,
            user_authorized: true,
            tombstone: json!({}),
        });
        let err = run_state
            .mark_superseded("sess-1", "x", "run-next")
            .expect_err("pending purge is illegal");
        assert_eq!(err, StateError::PendingPurge);

        let mut run_state = active_run_state();
        run_state.state = RunLifecycle::Aborted;
        let err = run_state
            .mark_superseded("sess-1", "x", "run-next")
            .expect_err("aborted run is illegal");
        assert!(matches!(err, StateError::IllegalRunState { .. }));

        let mut run_state = active_run_state();
        let err = run_state
            .mark_superseded("sess-other", "x", "run-next")
            .expect_err("session mismatch is illegal");
        assert_eq!(
            err,
            StateError::SessionMismatch {
                expected: "sess-1".to_string(),
                actual: "sess-other".to_string()
            }
        );
        assert_eq!(
            err.to_string(),
            "session id does not match active run binding"
        );
        assert_eq!(run_state.revision, 1);
        assert_eq!(run_state.state, RunLifecycle::Active);
    }

    #[test]
    fn record_boundary_bumps_revision_and_appends() {
        let mut run_state = active_run_state();
        run_state
            .record_boundary("clarification", BoundaryState::Waiting, "awaiting review", "wait")
            .expect("legal boundary");
        assert_eq!(run_state.revision, 2, "record_boundary owns the revision bump");
        assert_eq!(run_state.boundaries.len(), 1);
        assert_eq!(
            run_state.boundaries[0].as_typed().expect("typed boundary").revision,
            2
        );
        // Failed calls leave revision and boundaries untouched.
        let before = run_state.revision;
        let _ = run_state
            .record_boundary("clarification", BoundaryState::Waiting, "", "wait")
            .expect_err("empty reason is illegal");
        assert_eq!(run_state.revision, before);
        assert_eq!(run_state.boundaries.len(), 1);
    }

    #[test]
    fn malformed_boundary_entries_are_tolerated_and_preserved() {
        // Behavior preservation: legacy `record_stage_boundary` appended
        // without validating prior entries, so non-conforming entries must
        // keep loading, round-trip verbatim, and not block appends.
        let mut value = full_schema_state();
        value["boundaries"] = json!([
            {"stage": "prd", "unexpected": true},
            "not-even-an-object"
        ]);
        let mut run_state: RunState =
            serde_json::from_value(value).expect("malformed entries must not fail the load");

        run_state
            .record_boundary("clarification", BoundaryState::Waiting, "awaiting review", "wait")
            .expect("append after malformed entries");

        let round_tripped = serde_json::to_value(&run_state).expect("serialize");
        let boundaries = round_tripped["boundaries"].as_array().expect("array");
        assert_eq!(boundaries[0], json!({"stage": "prd", "unexpected": true}));
        assert_eq!(boundaries[1], json!("not-even-an-object"));
        assert_eq!(boundaries.len(), 3);
        assert_eq!(boundaries[2]["stage"], json!("clarification"));
        assert_eq!(boundaries[2]["state"], json!("waiting"));
    }
}
