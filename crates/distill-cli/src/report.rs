use serde_json::{json, Value};
use std::env;
use std::path::Path;

use crate::state::{PurgeCleanupState, RunLifecycle, RunState};
use crate::storage::{PlannedFile, PlannedWrite};
use crate::util::sha256_hex;
use crate::CURRENT_SCHEMA_VERSION;

// The read side of the report projection is typed: every run-state field is
// read from `RunState` (fields still `Value`-typed in the schema —
// clarification, requirement_snapshot, storage, report — are read through the
// struct's Value field). The projection still builds the same `Value` tree as
// before, so `canonical_hash` inputs and report.json/report.md bytes are
// unchanged.

pub(crate) fn response_from_state(state: &RunState) -> Value {
    let purge_pending = state
        .purge
        .as_ref()
        .is_some_and(|purge| purge.cleanup_state == PurgeCleanupState::Pending);
    let is_terminal = !purge_pending
        && matches!(
            state.state,
            RunLifecycle::Completed
                | RunLifecycle::Purged
                | RunLifecycle::Superseded
                | RunLifecycle::Aborted
        );
    let stage = if is_terminal {
        state.state.as_str().to_string()
    } else {
        state
            .current_stage
            .clone()
            .unwrap_or_else(|| "unknown".to_string())
    };
    let stage_state = if is_terminal {
        json!(state.state)
    } else {
        state
            .stages
            .iter()
            .find(|entry| entry.id == stage)
            .map(|entry| json!(entry.state))
            .unwrap_or(Value::Null)
    };
    let at_boundary = purge_pending
        || stage_state == "waiting"
        || stage_state == "blocked"
        || stage_state == "needs-reconciliation";
    let authorized = if is_terminal || at_boundary {
        Value::Null
    } else {
        state
            .workflow
            .stages
            .iter()
            .find(|entry| entry.id == stage)
            .map(crate::workflow::authorized_action)
            .unwrap_or(Value::Null)
    };
    let next_action = if purge_pending {
        "resume-purge"
    } else if is_terminal {
        "terminal"
    } else if stage_state == "waiting" {
        "wait"
    } else if stage_state == "blocked" {
        "unblock"
    } else if stage_state == "needs-reconciliation" {
        "reconcile"
    } else {
        "invoke-skill"
    };
    let required_next_action = if purge_pending {
        json!("Resume the authorized purge transaction.")
    } else if at_boundary {
        state
            .boundaries
            .iter()
            .rev()
            .find(|boundary| boundary.stage() == Some(stage.as_str()))
            .map(|boundary| boundary.required_next_action())
            .unwrap_or_else(|| {
                if stage_state == "needs-reconciliation" {
                    json!("Reconcile the pending publication before resubmitting evidence.")
                } else {
                    Value::Null
                }
            })
    } else {
        Value::Null
    };
    json!({
        "schema_version": state.schema_version,
        "status": state.state,
        "runtime": state.session_binding.runtime,
        "session_id": state.session_binding.session_id,
        "session_binding": state.session_binding,
        "run_id": state.run_id,
        "stage": stage,
        "stage_state": stage_state,
        "revision": state.revision,
        "next_action": next_action,
        "required_next_action": required_next_action,
        "authorized_action": authorized,
        "implementation_started": state
            .extra
            .get("implementation_started")
            .cloned()
            .unwrap_or(Value::Null),
        "workflow": state.workflow,
        "publications": state.publications,
        "report": state.report,
        "migration_events": state.migration_events,
        "storage": state.storage.clone().unwrap_or(Value::Null),
    })
}

pub(crate) fn plan_completion_report(
    worktree: &Path,
    run_id: &str,
    state: &mut RunState,
) -> Result<Vec<PlannedFile>, String> {
    crate::storage::validate_run_id(run_id)?;
    let json_rel = format!(".distill/runs/{run_id}/report.json");
    let markdown_rel = format!(".distill/runs/{run_id}/report.md");
    let renderer_fails = env::var("DISTILL_FAIL_RENDERER_FOR_RUN").as_deref() == Ok(run_id);
    let warnings = if renderer_fails {
        json!([{
            "type": "renderer-failed",
            "renderer": "markdown",
            "retryable": true,
            "message": "injected renderer failure",
        }])
    } else {
        json!([])
    };
    let mut report = canonical_report(run_id, state, warnings);
    let canonical_hash = canonical_hash(&report)?;
    report["canonical_hash"] = json!(canonical_hash);
    state.report = json!({
        "json_path": json_rel,
        "markdown_path": markdown_rel,
        "canonical_hash": report["canonical_hash"],
        "renderer": {
            "name": "markdown",
            "status": if renderer_fails { "failed" } else { "rendered" },
            "retryable": renderer_fails,
        },
    });
    let mut files = vec![PlannedFile {
        path: worktree.join(&json_rel),
        bytes: serde_json::to_vec_pretty(&report).map_err(|err| format!("json error: {err}"))?,
        write: PlannedWrite::RunArtifact,
    }];
    if !renderer_fails {
        files.push(PlannedFile {
            path: worktree.join(&markdown_rel),
            bytes: render_markdown_report(&report)?.into_bytes(),
            write: PlannedWrite::RunArtifact,
        });
    }
    Ok(files)
}

pub(crate) fn render_markdown_report(report: &Value) -> Result<String, String> {
    let run_id = report["run_id"]
        .as_str()
        .ok_or("report is missing run_id")?;
    let status = report["status"].as_str().unwrap_or("unknown");
    let issue_count = report["publications"]["issues"]
        .as_array()
        .map_or(0, Vec::len);
    let prd_path = report["publications"]["prd"]["artifact_path"]
        .as_str()
        .or_else(|| report["publications"]["prd"]["path"].as_str())
        .unwrap_or("none");
    Ok(format!(
        "# Distill Completion Report\n\nRun: {run_id}\n\nState: {status}\n\nCanonical hash: {}\n\nPublished PRD: {prd_path}\n\nPublished issues: {issue_count}\n",
        report["canonical_hash"].as_str().unwrap_or("unknown"),
    ))
}

fn canonical_report(run_id: &str, state: &RunState, warnings: Value) -> Value {
    let clarification = &state.clarification;
    let requirement = if clarification["clarified_requirement"].is_string() {
        json!({
            "source": state.requirement.as_ref().map(|requirement| requirement.source.clone()),
            "text": clarification["clarified_requirement"],
            "original_text": state.requirement.as_ref().map(|requirement| requirement.text.clone()),
        })
    } else {
        json!(state.requirement)
    };
    json!({
        "schema_version": CURRENT_SCHEMA_VERSION,
        "report_version": "distill.report.v1",
        "canonical_hash": Value::Null,
        "run_id": run_id,
        "status": state.state,
        "completion": {
            "state": state.state,
            "revision": state.revision,
        },
        "final_revision": state.revision,
        "sources": state
            .requirement_snapshot
            .as_ref()
            .map(|snapshot| snapshot["sources"].clone())
            .unwrap_or(Value::Null),
        "requirement": requirement,
        "decisions": clarification["decisions"],
        "assumptions": clarification["accepted_assumptions"],
        "material_unknowns": clarification["material_unknowns"],
        "domain_changes": clarification["domain_document_artifacts"],
        "drift_acknowledgments": state.drift_acknowledgments,
        "publications": state.publications,
        "warnings": warnings,
        "versions": {
            "state_schema": state.schema_version,
            "workflow": state.workflow.version,
            "events": 1,
            "evidence": "distill.evidence.v1",
            "report": "distill.report.v1",
            "cli": env!("CARGO_PKG_VERSION"),
        },
        "session": {
            "runtime": state.session_binding.runtime,
            "session_id": state.session_binding.session_id,
            "released": state.session_binding.released.unwrap_or(false),
            "released_revision": state.session_binding.released_revision,
        },
        "completion_evidence": state.completion_evidence,
        "workflow": state.workflow,
        "storage": state.storage.clone().unwrap_or(Value::Null),
    })
}

fn canonical_hash(report: &Value) -> Result<String, String> {
    let mut canonical = report.clone();
    canonical["canonical_hash"] = json!(Value::Null);
    let bytes = serde_json::to_vec(&canonical).map_err(|err| format!("json error: {err}"))?;
    Ok(sha256_hex(&bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::RunState;
    use serde_json::json;

    /// Completed-run state covering every field the report projection reads.
    fn completed_run_state() -> Value {
        json!({
            "schema_version": 1,
            "run_id": "run-20260903-done",
            "state": "completed",
            "revision": 6,
            "current_stage": null,
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
                "text": "Build a release-readiness dashboard."
            },
            "requirement_snapshot": {"sources": [{"normalized_text": "Build a dashboard."}], "total_raw_bytes": 42},
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
                {"id": "prd", "state": "completed", "revision": 4, "authorized_action": {"type": "invoke-skill"}},
                {"id": "issues", "state": "completed", "revision": 6, "authorized_action": {"type": "invoke-skill"}}
            ],
            "completion_evidence": [
                {"stage": "intake", "completed_revision": 1, "accepted_user_checkpoint": "explicit-text-captured", "summary": "Captured the supplied requirement text."},
                {"stage": "clarification", "completed_revision": 3, "accepted_user_checkpoint": "clarification-complete", "summary": "Clarified.", "adapter": {"executor": "skill", "skill": "grill-with-docs", "invocation": "unmodified-skill"}, "clarification": {"clarified_requirement": "x"}},
                {"stage": "prd", "completed_revision": 4, "accepted_user_checkpoint": "prd-approved", "summary": "PRD published.", "adapter": {"executor": "skill", "skill": "to-prd", "invocation": "unmodified-skill"}}
            ],
            "publications": {
                "prd": {
                    "operation_id": "run-r4-prd",
                    "artifact_id": "prd-1",
                    "path": "docs/prd/PRD-0001.md",
                    "payload_path": ".distill/runs/run/artifacts/prd.md",
                    "payload_hash": "deadbeef",
                    "dependency_artifact_ids": [],
                    "tracker": "local-markdown",
                    "status": "confirmed",
                    "title": "PRD-0001"
                },
                "issues": [
                    {"operation_id": "run-r5-issue-01", "artifact_id": "issue-1", "path": "docs/issues/0001.md", "payload_path": ".distill/runs/run/artifacts/issue-1.md", "payload_hash": "beef", "dependency_artifact_ids": ["prd-1"], "tracker": "local-markdown", "status": "needs-reconciliation", "title": "Foo"},
                    {"operation_id": "run-r5-issue-02", "title": "Bar", "status": "pending"}
                ]
            },
            "report": {"json_path": ".distill/runs/run/report.json", "markdown_path": ".distill/runs/run/report.md", "canonical_hash": "cafe", "renderer": {"name": "markdown", "status": "rendered", "retryable": false}},
            "context_baseline": {"captured": true},
            "drift_acknowledgments": [{"stage": "prd", "material": false, "reason": "typo fix", "detected": {"domain_documents": ["CONTEXT.md"]}}],
            "boundaries": [{"stage": "prd", "state": "waiting", "reason": "awaiting review", "required_next_action": "wait", "revision": 4}],
            "abort": null,
            "handoffs": [{"from_session": "sess-0", "to_session": "sess-1", "reason": "laptop sleep", "revision": 2, "invalidates_previous_session": true}],
            "migration_events": [{"from_schema": 0, "to_schema": 1, "summary": "Added lifecycle fields required by schema 1."}],
            "implementation_started": false
        })
    }

    /// Active run mid-stage, not at a boundary: exercises the authorized
    /// action projection and the Null `required_next_action` branch.
    fn active_mid_run_state() -> Value {
        json!({
            "schema_version": 1,
            "run_id": "run-20260903-mid",
            "state": "active",
            "revision": 2,
            "current_stage": "clarification",
            "session_binding": {"runtime": "kimi", "session_id": "sess-9"},
            "workflow": {
                "version": "distill.v1",
                "source": "embedded:distill.v1.json",
                "stages": [
                    {"id": "intake", "executor": "runner", "skill": null, "checkpoint": "explicit-text-captured", "next_action": "capture-text-requirement"},
                    {"id": "clarification", "executor": "skill", "skill": "grill-with-docs", "checkpoint": "clarification-complete", "next_action": "invoke-skill"}
                ]
            },
            "requirement": {"source": "explicit-text", "text": "Anything."},
            "stages": [
                {"id": "intake", "state": "completed", "revision": 1, "authorized_action": {"type": "capture-text-requirement"}},
                {"id": "clarification", "state": "active", "revision": 1, "authorized_action": {"type": "invoke-skill"}}
            ],
            "completion_evidence": [
                {"stage": "intake", "completed_revision": 1, "accepted_user_checkpoint": "explicit-text-captured", "summary": "Captured."}
            ],
            "publications": {"prd": null, "issues": []},
            "report": null,
            "implementation_started": true
        })
    }

    /// Active run waiting at a boundary: exercises the boundary lookup for
    /// `required_next_action`. Omits `storage` and `implementation_started`
    /// to cover the absent-key branches.
    fn waiting_run_state() -> Value {
        let mut state = active_mid_run_state();
        state["run_id"] = json!("run-20260903-wait");
        state["revision"] = json!(4);
        state["current_stage"] = json!("prd");
        state["stages"].as_array_mut().expect("stages").push(json!(
            {"id": "prd", "state": "waiting", "revision": 4, "authorized_action": {"type": "wait"}}
        ));
        state["boundaries"] = json!([
            {"stage": "prd", "state": "waiting", "reason": "awaiting review", "required_next_action": "wait for review", "revision": 4}
        ]);
        state.as_object_mut().expect("object").remove("implementation_started");
        state
    }

    /// Active run whose current stage needs reconciliation with no boundary
    /// entry: exercises the reconciliation fallback message.
    fn reconciliation_run_state() -> Value {
        let mut state = active_mid_run_state();
        state["run_id"] = json!("run-20260903-recon");
        state["current_stage"] = json!("issues");
        state["stages"].as_array_mut().expect("stages").push(json!(
            {"id": "issues", "state": "needs-reconciliation", "revision": 5, "authorized_action": {"type": "reconcile"}}
        ));
        state
    }

    /// Run with a pending purge cleanup: exercises the purge short-circuit.
    fn purge_pending_run_state() -> Value {
        let mut state = active_mid_run_state();
        state["run_id"] = json!("run-20260903-purge");
        state["purge"] = json!({
            "cleanup_state": "pending",
            "source_revision": 2,
            "revision": 3,
            "user_authorized": true,
            "tombstone": {"run_id": "run-20260903-purge"}
        });
        state
    }

    /// The pre-migration string-key response projection, kept verbatim as
    /// the golden reference for the typed read side.
    fn legacy_response_from_state(state: &Value) -> Value {
        let purge_pending = state["purge"]["cleanup_state"] == "pending";
        let is_terminal = !purge_pending
            && (state["state"] == "completed"
                || state["state"] == "purged"
                || state["state"] == "superseded"
                || state["state"] == "aborted");
        let stage = if is_terminal {
            state["state"].as_str().unwrap_or("terminal").to_string()
        } else {
            state["current_stage"]
                .as_str()
                .unwrap_or("unknown")
                .to_string()
        };
        let stage_state = if is_terminal {
            state["state"].clone()
        } else {
            state["stages"]
                .as_array()
                .and_then(|stages| stages.iter().find(|item| item["id"] == stage))
                .map(|item| item["state"].clone())
                .unwrap_or(Value::Null)
        };
        let at_boundary = purge_pending
            || stage_state == "waiting"
            || stage_state == "blocked"
            || stage_state == "needs-reconciliation";
        let authorized = if is_terminal || at_boundary {
            Value::Null
        } else {
            crate::workflow::workflow_stage(state, &stage)
                .map(|stage| crate::workflow::authorized_action(&stage))
                .unwrap_or(Value::Null)
        };
        let next_action = if purge_pending {
            "resume-purge"
        } else if is_terminal {
            "terminal"
        } else if stage_state == "waiting" {
            "wait"
        } else if stage_state == "blocked" {
            "unblock"
        } else if stage_state == "needs-reconciliation" {
            "reconcile"
        } else {
            "invoke-skill"
        };
        let required_next_action = if purge_pending {
            json!("Resume the authorized purge transaction.")
        } else if at_boundary {
            state["boundaries"]
                .as_array()
                .and_then(|boundaries| {
                    boundaries
                        .iter()
                        .rev()
                        .find(|boundary| boundary["stage"] == stage)
                })
                .and_then(|boundary| boundary["required_next_action"].as_str())
                .map(|action| json!(action))
                .unwrap_or_else(|| {
                    if stage_state == "needs-reconciliation" {
                        json!("Reconcile the pending publication before resubmitting evidence.")
                    } else {
                        Value::Null
                    }
                })
        } else {
            Value::Null
        };
        json!({
            "schema_version": state["schema_version"],
            "status": state["state"],
            "runtime": state["session_binding"]["runtime"],
            "session_id": state["session_binding"]["session_id"],
            "session_binding": state["session_binding"],
            "run_id": state["run_id"],
            "stage": stage,
            "stage_state": stage_state,
            "revision": state["revision"],
            "next_action": next_action,
            "required_next_action": required_next_action,
            "authorized_action": authorized,
            "implementation_started": state["implementation_started"],
            "workflow": state["workflow"],
            "publications": state["publications"],
            "report": state["report"],
            "migration_events": state["migration_events"],
            "storage": state["storage"],
        })
    }

    /// The pre-migration string-key canonical report projection, kept
    /// verbatim as the golden reference for the typed read side.
    fn legacy_canonical_report(run_id: &str, state: &Value, warnings: Value) -> Value {
        let clarification = &state["clarification"];
        let requirement = if clarification["clarified_requirement"].is_string() {
            json!({
                "source": state["requirement"]["source"],
                "text": clarification["clarified_requirement"],
                "original_text": state["requirement"]["text"],
            })
        } else {
            state["requirement"].clone()
        };
        json!({
            "schema_version": CURRENT_SCHEMA_VERSION,
            "report_version": "distill.report.v1",
            "canonical_hash": Value::Null,
            "run_id": run_id,
            "status": state["state"],
            "completion": {
                "state": state["state"],
                "revision": state["revision"],
            },
            "final_revision": state["revision"],
            "sources": state["requirement_snapshot"]["sources"],
            "requirement": requirement,
            "decisions": clarification["decisions"],
            "assumptions": clarification["accepted_assumptions"],
            "material_unknowns": clarification["material_unknowns"],
            "domain_changes": clarification["domain_document_artifacts"],
            "drift_acknowledgments": state["drift_acknowledgments"],
            "publications": state["publications"],
            "warnings": warnings,
            "versions": {
                "state_schema": state["schema_version"],
                "workflow": state["workflow"]["version"],
                "events": 1,
                "evidence": "distill.evidence.v1",
                "report": "distill.report.v1",
                "cli": env!("CARGO_PKG_VERSION"),
            },
            "session": {
                "runtime": state["session_binding"]["runtime"],
                "session_id": state["session_binding"]["session_id"],
                "released": state["session_binding"]["released"].as_bool().unwrap_or(false),
                "released_revision": state["session_binding"]["released_revision"],
            },
            "completion_evidence": state["completion_evidence"],
            "workflow": state["workflow"],
            "storage": state["storage"],
        })
    }

    #[test]
    fn response_projection_matches_legacy_value_projection_byte_for_byte() {
        for state in [
            completed_run_state(),
            active_mid_run_state(),
            waiting_run_state(),
            reconciliation_run_state(),
            purge_pending_run_state(),
        ] {
            let run_state: RunState =
                serde_json::from_value(state).expect("fixture deserializes");
            // Production always projects from the canonical form (post-load
            // canonicalize or `to_value` of a `RunState`).
            let canonical = serde_json::to_value(&run_state).expect("canonical value");
            let expected = legacy_response_from_state(&canonical);
            let actual = response_from_state(&run_state);
            assert_eq!(
                serde_json::to_vec(&actual).expect("actual bytes"),
                serde_json::to_vec(&expected).expect("expected bytes"),
                "typed response projection diverged from legacy for {canonical}"
            );
        }
    }

    #[test]
    fn canonical_report_matches_legacy_projection_bytes_and_hash() {
        let mut fixtures = vec![completed_run_state()];
        let mut no_clarification = completed_run_state();
        no_clarification["clarification"] = Value::Null;
        fixtures.push(no_clarification);
        let mut null_requirement = completed_run_state();
        null_requirement["clarification"] = Value::Null;
        null_requirement["requirement"] = Value::Null;
        fixtures.push(null_requirement);
        for state in fixtures {
            let run_state: RunState =
                serde_json::from_value(state).expect("fixture deserializes");
            let canonical = serde_json::to_value(&run_state).expect("canonical value");
            for warnings in [
                json!([]),
                json!([{
                    "type": "renderer-failed",
                    "renderer": "markdown",
                    "retryable": true,
                    "message": "injected renderer failure",
                }]),
            ] {
                let expected =
                    legacy_canonical_report("run-20260903-done", &canonical, warnings.clone());
                let actual = canonical_report("run-20260903-done", &run_state, warnings);
                assert_eq!(
                    serde_json::to_vec(&actual).expect("actual bytes"),
                    serde_json::to_vec(&expected).expect("expected bytes"),
                    "typed canonical report diverged from legacy for {canonical}"
                );
                assert_eq!(
                    canonical_hash(&actual).expect("actual hash"),
                    canonical_hash(&expected).expect("expected hash"),
                    "canonical_hash input must be byte-identical"
                );
            }
        }
    }
}
