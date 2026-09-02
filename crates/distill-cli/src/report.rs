use serde_json::{json, Value};
use std::env;
use std::path::Path;

use crate::storage::{PlannedFile, PlannedWrite};
use crate::util::sha256_hex;
use crate::CURRENT_SCHEMA_VERSION;

pub(crate) fn response_from_state(state: &Value) -> Value {
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

pub(crate) fn plan_completion_report(
    worktree: &Path,
    run_id: &str,
    state: &mut Value,
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
    state["report"] = json!({
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

fn canonical_report(run_id: &str, state: &Value, warnings: Value) -> Value {
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

fn canonical_hash(report: &Value) -> Result<String, String> {
    let mut canonical = report.clone();
    canonical["canonical_hash"] = json!(Value::Null);
    let bytes = serde_json::to_vec(&canonical).map_err(|err| format!("json error: {err}"))?;
    Ok(sha256_hex(&bytes))
}
