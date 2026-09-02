use serde_json::{json, Value};
use std::env;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process;

use crate::util::{create_run_id, current_timestamp_millis, sha256_hex, stable_hash};
mod context;
mod intake;
mod publication;
mod storage;
mod util;
mod workflow;
mod args;
mod state;
mod event;
mod report;
mod transition;

pub(crate) const CURRENT_SCHEMA_VERSION: u64 = 1;
pub(crate) const WORKFLOW_SOURCE: &str = "embedded:distill.v1.json";
pub(crate) const WORKFLOW_JSON: &str = include_str!("../workflows/distill.v1.json");
pub(crate) struct RunLock {
    pub(crate) path: PathBuf,
}

impl Drop for RunLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn main() {
    if let Err(err) = run() {
        eprintln!("ERROR: {err}");
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("start") => {
            let response = start_distill(args::parse_start_args(args.collect())?)?;
            print_json(&response)
        }
        Some("submit-evidence") => {
            let response = submit_evidence(args::parse_submit_args(args.collect())?)?;
            print_json(&response)
        }
        Some("takeover") => {
            let response = takeover_run(args::parse_takeover_args(args.collect())?)?;
            print_json(&response)
        }
        Some("supersede") => {
            let response = supersede_run(args::parse_supersede_args(args.collect())?)?;
            print_json(&response)
        }
        Some("set-project-quota") => {
            let response = set_project_quota(args::parse_quota_args(args.collect())?)?;
            print_json(&response)
        }
        Some("purge") => {
            let response = purge_run(args::parse_purge_args(args.collect())?)?;
            print_json(&response)
        }
        Some("abort") => {
            let response = abort_run(args::parse_abort_args(args.collect())?)?;
            print_json(&response)
        }
        Some("inspect") => {
            let response = inspect_run(args::parse_inspect_args(args.collect())?)?;
            print_json(&response)
        }
        Some("events") => {
            let response = events_run(args::parse_events_args(args.collect())?)?;
            print_json(&response)
        }
        Some("render-report") => {
            let response = render_report_run(args::parse_render_report_args(args.collect())?)?;
            print_json(&response)
        }
        Some("--help") | Some("-h") | None => {
            print_usage();
            Ok(())
        }
        Some(other) => Err(format!("unknown command: {other}")),
    }
}

fn print_usage() {
    println!(
        "Usage: distill start --json --runtime <codex|kimi|reasonix> --session-id <id> --worktree <path> (--requirement <text>|--intake-json <json>)\n       distill submit-evidence --json --worktree <path> --run-id <id> --session-id <id> --expected-revision <n> --stage <stage> --evidence <json>\n       distill set-project-quota --json --worktree <path> --bytes <n>\n       distill purge --json --user-authorized --worktree <path> --run-id <id> --session-id <id> --expected-revision <n>\n       distill abort --json --user-authorized --worktree <path> --run-id <id> --session-id <id> --expected-revision <n> --reason <reason>\n       distill inspect --json --worktree <path> --run-id <id> --session-id <id> --expected-revision <n>"
    );
}

fn print_json(value: &Value) -> Result<(), String> {
    println!(
        "{}",
        serde_json::to_string_pretty(value)
            .map_err(|err| format!("cannot serialize response: {err}"))?
    );
    Ok(())
}

fn start_distill(args: args::StartArgs) -> Result<Value, String> {
    if !args.json {
        return Err("distill requires --json".to_string());
    }
    state::ensure_worktree(&args.worktree)?;
    state::validate_required_project_configuration(&args.worktree)?;
    publication::validate_tracker_config(&args.worktree)?;
    let limits = storage::StorageLimits::load(&args.worktree)?;
    let run_id = create_run_id(&args.session_id);
    let intake_json = match args.intake_json.as_deref() {
        Some(bundle) => bundle.to_string(),
        None => intake::bundle_from_text(
            args.requirement
                .as_deref()
                .ok_or("--requirement or --intake-json is required")?,
        ),
    };
    let mut intake_snapshot = intake::prepare_intake(&run_id, &intake_json, &limits)?;

    storage::ensure_distill_ignored(&args.worktree)?;
    let _start_lock = state::acquire_project_start_lock(&args.worktree)?;

    if let Some(existing) = workflow::active_run_for_session(&args.worktree, &args.session_id)? {
        let state = state::read_state(&args.worktree, &existing)?;
        return Ok(report::response_from_state(&state));
    }

    let workflow = workflow::load_workflow()?;
    let first_stage = workflow::next_stage_after(&workflow, "intake")?;
    let requirement_text = intake_snapshot.manifest["sources"]
        .as_array()
        .map(|sources| {
            sources
                .iter()
                .filter_map(|source| source["normalized_text"].as_str())
                .collect::<Vec<_>>()
                .join("\n\n")
        })
        .unwrap_or_default();

    let state = json!({
        "schema_version": CURRENT_SCHEMA_VERSION,
        "run_id": run_id,
        "state": "active",
        "revision": 1,
        "current_stage": first_stage.id,
        "session_binding": {
            "runtime": args.runtime,
            "session_id": args.session_id,
        },
        "workflow": {
            "version": workflow.version,
            "source": WORKFLOW_SOURCE,
            "stages": workflow.stages,
        },
        "requirement": {
            "source": if args.intake_json.is_some() { "runtime-intake-json" } else { "explicit-text" },
            "text": requirement_text,
        },
        "requirement_snapshot": intake_snapshot.manifest.clone(),
        "clarification": Value::Null,
        "storage": storage::storage_summary(&args.worktree, &limits)?,
        "stages": workflow::initial_stage_states(&workflow)?,
        "completion_evidence": [{
            "stage": "intake",
            "completed_revision": 1,
            "accepted_user_checkpoint": "explicit-text-captured",
            "summary": "Captured the supplied requirement text.",
        }],
        "publications": {
            "prd": Value::Null,
            "issues": [],
        },
        "report": Value::Null,
        "context_baseline": context::capture_context_baseline(&args.worktree)?,
        "drift_acknowledgments": [],
        "boundaries": [],
        "abort": Value::Null,
        "handoffs": [],
        "migration_events": [],
        "implementation_started": false,
    });

    let session_event = event::run_event(
        &run_id,
        1,
        1,
        "session-bound",
        json!({
            "runtime": state["session_binding"]["runtime"],
            "session_id": state["session_binding"]["session_id"],
        }),
        &limits,
    )?;
    let intake_event = event::run_event(
        &run_id,
        2,
        1,
        "intake-completed",
        json!({
            "source_count": intake_snapshot.source_count,
            "raw_bytes": intake_snapshot.total_raw_bytes,
        }),
        &limits,
    )?;
    let event_line = format!("{session_event}\n{intake_event}");
    intake_snapshot.files.push(storage::RunFile {
        relative_path: PathBuf::from("events.jsonl"),
        bytes: format!("{event_line}\n").into_bytes(),
    });
    let state_bytes =
        serde_json::to_vec_pretty(&state).map_err(|err| format!("json error: {err}"))?;
    intake_snapshot.files.push(storage::RunFile {
        relative_path: PathBuf::from("state.json"),
        bytes: state_bytes,
    });
    storage::commit_new_run(&args.worktree, &run_id, intake_snapshot.files, &limits)?;
    let mut response = report::response_from_state(&state);
    response["intake"] = json!({ "source_count": intake_snapshot.source_count });
    response["storage"] = storage::storage_summary(&args.worktree, &limits)?;
    response["storage"]["usage"]["run_raw_source_bytes"] = json!(intake_snapshot.total_raw_bytes);
    Ok(response)
}

fn submit_evidence(args: args::SubmitArgs) -> Result<Value, String> {
    if !args.json {
        return Err("distill requires --json".to_string());
    }
    state::ensure_worktree(&args.worktree)?;

    let _lock = state::acquire_run_lock(&args.worktree, &args.run_id)?;
    let state =
        state::read_state_for_update_at_revision(&args.worktree, &args.run_id, args.expected_revision)?;
    state::ensure_no_pending_purge(&state)?;
    if state["state"] != "active" && state["state"] != "blocked" {
        return Err("run is not active or blocked".to_string());
    }
    if state["session_binding"]["session_id"] != args.session_id {
        return Err("session id does not match active run binding".to_string());
    }

    let expected_stage = state["current_stage"]
        .as_str()
        .ok_or("state is missing current_stage")?;
    if state::completed_stage_ids(&state)?.contains(&args.stage.as_str()) {
        return Err(format!("stage {} is already completed", args.stage));
    }
    if args.stage != expected_stage {
        return Err(format!(
            "stage {} is not authorized; expected {expected_stage}",
            args.stage
        ));
    }

    let evidence: Value = serde_json::from_str(&args.evidence)
        .map_err(|err| format!("--evidence must be valid JSON: {err}"))?;
    let stage = workflow::workflow_stage(&state, expected_stage)?.clone();
    let checkpoint = evidence["checkpoint"]
        .as_str()
        .ok_or("evidence.checkpoint is required")?;
    if checkpoint != stage.checkpoint {
        return Err(format!(
            "checkpoint {checkpoint} is not accepted for stage {}; expected {}",
            stage.id, stage.checkpoint
        ));
    }
    let limits = storage::StorageLimits::load(&args.worktree)?;
    let mut next_state = state.clone();
    if let Some(boundary) = evidence["status"].as_str() {
        if boundary == "waiting" || boundary == "blocked" {
            return record_stage_boundary(
                &args.worktree,
                &args.run_id,
                &stage,
                &evidence,
                next_state,
                &limits,
            );
        }
        if boundary != "completed" {
            return Err("evidence.status must be completed, waiting, or blocked".to_string());
        }
    }

    let current_revision = state["revision"]
        .as_u64()
        .ok_or("state revision is invalid")?;
    let next_revision = current_revision + 1;

    let clarification = (stage.id == "clarification")
        .then(|| validate_clarification_evidence(&args.worktree, &evidence))
        .transpose()?;
    context::validate_context_drift(
        &args.worktree,
        &mut next_state,
        &stage.id,
        &evidence,
        clarification.as_ref(),
    )?;
    if let Some(clarification) = clarification {
        next_state["clarification"] = clarification;
    }
    next_state["state"] = json!("active");

    let mut planned_files = Vec::new();
    let evidence_artifact = event::plan_evidence_artifact(
        &args.worktree,
        &args.run_id,
        &stage.id,
        next_revision,
        &evidence,
        &mut planned_files,
    )?;
    let mut blocked_publication = None;
    if stage.id == "prd" {
        let outcome = publication::publish_prd(
            &args.worktree,
            &args.run_id,
            current_revision,
            &mut next_state,
            &evidence,
        )?;
        planned_files.extend(outcome.files);
        blocked_publication = outcome.blocked;
    } else if stage.id == "issues" {
        let outcome = publication::publish_issues(
            &args.worktree,
            &args.run_id,
            current_revision,
            &mut next_state,
            &evidence,
        )?;
        planned_files.extend(outcome.files);
        blocked_publication = outcome.blocked;
    }

    if let Some(blocked_reason) = blocked_publication {
        mark_stage_reconciliation(&mut next_state, &stage.id, next_revision)?;
        next_state["revision"] = json!(next_revision);
        next_state["storage"] = storage::storage_summary(&args.worktree, &limits)?;
        let event_lines = event::build_transition_events(
            &args.worktree,
            &args.run_id,
            next_revision,
            vec![(
                "publication-reconciliation",
                json!({
                    "stage": stage.id,
                    "reason": blocked_reason,
                    "evidence": evidence_artifact,
                    "publications": next_state["publications"],
                }),
            )],
            &limits,
        )?;
        transition::commit(
            &args.worktree,
            &args.run_id,
            &next_state,
            &event_lines,
            planned_files,
            &limits,
        )?;
        let mut response = report::response_from_state(&next_state);
        response["publication_blocked"] = json!(blocked_reason);
        return Ok(response);
    }

    record_stage_completion(&mut next_state, &stage, next_revision, &evidence)?;
    let mut event_payloads = vec![(
        "stage-completed",
        json!({
            "stage": stage.id,
            "checkpoint": stage.checkpoint,
            "evidence": evidence_artifact,
        }),
    )];
    if stage.id == "prd" {
        event_payloads.push((
            "publication-recorded",
            json!({
                "stage": "prd",
                "publication": next_state["publications"]["prd"],
            }),
        ));
    } else if stage.id == "issues" {
        event_payloads.push((
            "publication-recorded",
            json!({
                "stage": "issues",
                "publications": next_state["publications"]["issues"],
            }),
        ));
    }

    if let Some(next_stage) = workflow::next_stage_after_snapshot(&next_state, &stage.id)? {
        next_state["current_stage"] = json!(next_stage.id);
        mark_stage_active(&mut next_state, &next_stage.id, next_revision)?;
    } else {
        next_state["state"] = json!("completed");
        next_state["current_stage"] = json!(Value::Null);
        next_state["session_binding"]["released"] = json!(true);
        next_state["session_binding"]["released_revision"] = json!(next_revision);
        next_state["revision"] = json!(next_revision);
        next_state["storage"] = storage::storage_summary(&args.worktree, &limits)?;
        planned_files.extend(report::plan_completion_report(
            &args.worktree,
            &args.run_id,
            &mut next_state,
        )?);
        event_payloads.push((
            "session-released",
            json!({
                "session_id": next_state["session_binding"]["session_id"],
                "released_revision": next_revision,
            }),
        ));
        event_payloads.push((
            "terminal-completed",
            json!({
                "final_revision": next_revision,
                "report": next_state["report"],
            }),
        ));
    }
    next_state["revision"] = json!(next_revision);
    next_state["storage"] = storage::storage_summary(&args.worktree, &limits)?;
    let event_lines = event::build_transition_events(
        &args.worktree,
        &args.run_id,
        next_revision,
        event_payloads,
        &limits,
    )?;

    transition::commit(
        &args.worktree,
        &args.run_id,
        &next_state,
        &event_lines,
        planned_files,
        &limits,
    )?;
    Ok(report::response_from_state(&next_state))
}

fn set_project_quota(args: args::QuotaArgs) -> Result<Value, String> {
    if !args.json {
        return Err("distill requires --json".to_string());
    }
    state::ensure_worktree(&args.worktree)?;
    storage::set_project_quota(&args.worktree, args.bytes)
}

fn purge_run(args: args::PurgeArgs) -> Result<Value, String> {
    if !args.json {
        return Err("distill requires --json".to_string());
    }
    if !args.user_authorized {
        return Err("purge requires --user-authorized".to_string());
    }
    state::ensure_worktree(&args.worktree)?;
    storage::ensure_distill_path_safe(&args.worktree)?;
    let _lock = state::acquire_run_lock(&args.worktree, &args.run_id)?;
    let mut state =
        state::read_state_for_update_at_revision(&args.worktree, &args.run_id, args.expected_revision)?;
    if state["session_binding"]["session_id"] != args.session_id {
        return Err("session id does not match run binding".to_string());
    }
    let recovering = state["purge"]["cleanup_state"].as_str() == Some("pending");
    if state["state"] == "purged" && !recovering {
        return Err("run is already purged".to_string());
    }
    let next_revision = if recovering {
        args.expected_revision
    } else {
        args.expected_revision + 1
    };
    let limits = storage::StorageLimits::load(&args.worktree)?;
    let run_dir = storage::run_dir(&args.worktree, &args.run_id)?;
    if !recovering {
        let tombstone = json!({
            "run_id": args.run_id,
            "state": "purged",
            "revision": next_revision,
            "purged_at": current_timestamp_millis(),
            "user_authorized": true,
            "source_hashes": intake::source_hashes(&state["requirement_snapshot"]),
            "publications": state["publications"],
        });
        state["revision"] = json!(next_revision);
        state["purge"] = json!({
            "cleanup_state": "pending",
            "source_revision": args.expected_revision,
            "revision": next_revision,
            "user_authorized": true,
            "tombstone": tombstone,
        });
        state::write_state(&args.worktree, &state)?;
    }
    if env::var("DISTILL_FAIL_PURGE_BEFORE_AUTH_EVENT").as_deref() == Ok(args.run_id.as_str()) {
        return Err("injected purge interruption before authorization event".to_string());
    }
    if !event::event_type_exists(&args.worktree, &args.run_id, "run-purge-authorized")? {
        event::append_audit_event(
            &args.worktree,
            &args.run_id,
            next_revision,
            "run-purge-authorized",
            json!({
                "expected_revision": state["purge"]["source_revision"],
                "next_revision": state["purge"]["revision"],
                "user_authorized": true,
            }),
            &limits,
        )?;
    }
    if env::var("DISTILL_FAIL_PURGE_AFTER_PENDING").as_deref() == Ok(args.run_id.as_str()) {
        return Err("injected purge interruption after durable pending state".to_string());
    }
    let tombstone = state["purge"]["tombstone"].clone();
    storage::remove_dir_if_exists(&run_dir.join("snapshots"))?;
    storage::remove_dir_if_exists(&run_dir.join("artifacts"))?;
    storage::atomic_write_json(&run_dir.join("tombstone.json"), &tombstone, false)?;
    state["state"] = json!("purged");
    state["revision"] = json!(next_revision);
    state["current_stage"] = json!(Value::Null);
    state["session_binding"]["released"] = json!(true);
    state["session_binding"]["released_revision"] = json!(next_revision);
    state["requirement"] = json!(Value::Null);
    state["requirement_snapshot"] = json!({
        "purged": true,
        "tombstone_path": format!(".distill/runs/{}/tombstone.json", args.run_id),
    });
    state["purge"]["cleanup_state"] = json!("completed");
    if !event::event_type_exists(&args.worktree, &args.run_id, "run-purged")? {
        event::append_audit_event(
            &args.worktree,
            &args.run_id,
            next_revision,
            "run-purged",
            json!({
                "tombstone_path": format!(".distill/runs/{}/tombstone.json", args.run_id),
                "user_authorized": true,
            }),
            &limits,
        )?;
    }
    state::write_state(&args.worktree, &state)?;
    Ok(report::response_from_state(&state))
}

fn abort_run(args: args::AbortArgs) -> Result<Value, String> {
    if !args.json {
        return Err("distill requires --json".to_string());
    }
    if !args.user_authorized {
        return Err("abort requires --user-authorized".to_string());
    }
    state::ensure_worktree(&args.worktree)?;
    let _lock = state::acquire_run_lock(&args.worktree, &args.run_id)?;
    let mut state =
        state::read_state_for_update_at_revision(&args.worktree, &args.run_id, args.expected_revision)?;
    state::ensure_no_pending_purge(&state)?;
    if state["state"] != "active" && state["state"] != "blocked" {
        return Err("run is not active or blocked".to_string());
    }
    if state["session_binding"]["session_id"] != args.session_id {
        return Err("session id does not match run binding".to_string());
    }
    let next_revision = args.expected_revision + 1;
    state["state"] = json!("aborted");
    state["revision"] = json!(next_revision);
    state["current_stage"] = json!(Value::Null);
    state["session_binding"]["released"] = json!(true);
    state["session_binding"]["released_revision"] = json!(next_revision);
    state["abort"] = json!({
        "reason": args.reason,
        "revision": next_revision,
        "user_authorized": true,
        "domain_document_artifacts": changed_domain_document_artifacts(
            &args.worktree,
            &state["context_baseline"],
        )?,
    });
    state::write_state(&args.worktree, &state)?;
    let limits = storage::StorageLimits::load(&args.worktree)?;
    event::append_audit_event(
        &args.worktree,
        &args.run_id,
        next_revision,
        "run-aborted",
        json!({
            "reason": state["abort"]["reason"],
            "user_authorized": true,
            "session_released": true,
        }),
        &limits,
    )?;
    Ok(report::response_from_state(&state))
}

fn takeover_run(args: args::TakeoverArgs) -> Result<Value, String> {
    if !args.json {
        return Err("distill requires --json".to_string());
    }
    if !args.user_authorized {
        return Err("takeover requires --user-authorized".to_string());
    }
    state::ensure_worktree(&args.worktree)?;
    let _lock = state::acquire_run_lock(&args.worktree, &args.run_id)?;
    let mut state =
        state::read_state_for_update_at_revision(&args.worktree, &args.run_id, args.expected_revision)?;
    state::ensure_no_pending_purge(&state)?;
    if state["state"] != "active" && state["state"] != "blocked" {
        return Err("run is not active or blocked".to_string());
    }
    if state["session_binding"]["session_id"] != args.from_session {
        return Err("from-session does not match active run binding".to_string());
    }
    let next_revision = args.expected_revision + 1;
    let from_session = args.from_session.clone();
    let to_session = args.to_session.clone();
    let reason = args.reason.clone();
    state["session_binding"]["session_id"] = json!(args.to_session);
    state["revision"] = json!(next_revision);
    state["handoffs"]
        .as_array_mut()
        .ok_or("state handoffs are invalid")?
        .push(json!({
            "from_session": args.from_session,
            "to_session": to_session,
            "reason": args.reason,
            "revision": next_revision,
            "invalidates_previous_session": true,
        }));
    state::write_state(&args.worktree, &state)?;
    let limits = storage::StorageLimits::load(&args.worktree)?;
    event::append_audit_event(
        &args.worktree,
        &args.run_id,
        next_revision,
        "session-takeover",
        json!({
            "from_session": from_session,
            "to_session": to_session,
            "reason": reason,
            "user_authorized": true,
        }),
        &limits,
    )?;
    Ok(report::response_from_state(&state))
}

fn supersede_run(args: args::SupersedeArgs) -> Result<Value, String> {
    if !args.json {
        return Err("distill requires --json".to_string());
    }
    state::ensure_worktree(&args.worktree)?;
    let _lock = state::acquire_run_lock(&args.worktree, &args.run_id)?;
    let mut state =
        state::read_state_for_update_at_revision(&args.worktree, &args.run_id, args.expected_revision)?;
    state::ensure_no_pending_purge(&state)?;
    if state["state"] != "active" && state["state"] != "blocked" {
        return Err("run is not active or blocked".to_string());
    }
    if state["session_binding"]["session_id"] != args.session_id {
        return Err("session id does not match active run binding".to_string());
    }

    let predecessor_before_supersession = state.clone();
    let workflow = workflow::load_workflow()?;
    let successor_id = create_run_id(&args.session_id);
    let first_stage = workflow::next_stage_after(&workflow, "intake")?;
    let mut successor = json!({
        "schema_version": CURRENT_SCHEMA_VERSION,
        "run_id": successor_id,
        "state": "supersession-pending",
        "revision": 1,
        "current_stage": first_stage.id,
        "predecessor_run_id": args.run_id,
        "session_binding": state["session_binding"],
        "workflow": {
            "version": workflow.version,
            "source": WORKFLOW_SOURCE,
            "stages": workflow.stages,
        },
        "requirement": {
            "source": "explicit-text",
            "text": args.requirement,
            "supersession_reason": args.reason,
        },
        "clarification": Value::Null,
        "stages": workflow::initial_stage_states(&workflow)?,
        "completion_evidence": [{
            "stage": "intake",
            "completed_revision": 1,
            "accepted_user_checkpoint": "explicit-text-captured",
            "summary": "Captured the superseding requirement text.",
        }],
        "publications": {
            "prd": Value::Null,
            "issues": [],
        },
        "report": Value::Null,
        "context_baseline": context::capture_context_baseline(&args.worktree)?,
        "drift_acknowledgments": [],
        "boundaries": [],
        "abort": Value::Null,
        "handoffs": [],
        "migration_events": [],
        "implementation_started": false,
    });
    let successor_bytes = serde_json::to_vec_pretty(&successor)
        .map_err(|err| format!("cannot serialize successor run: {err}"))?;
    storage::commit_new_run(
        &args.worktree,
        successor_id.as_str(),
        vec![storage::RunFile {
            relative_path: PathBuf::from("state.json"),
            bytes: successor_bytes,
        }],
        &storage::StorageLimits::load(&args.worktree)?,
    )
    .map_err(|err| format!("cannot create successor run: {err}"))?;

    state["state"] = json!("superseded");
    state["current_stage"] = json!(Value::Null);
    state["superseded_by"] = json!(successor_id);
    state["supersession"] = json!({
        "reason": args.reason,
        "revision": args.expected_revision + 1,
        "successor_run_id": successor["run_id"],
    });
    state["revision"] = json!(args.expected_revision + 1);
    if let Err(err) = state::write_state(&args.worktree, &state) {
        state::rollback_successor(&args.worktree, successor_id.as_str())?;
        return Err(format!("cannot supersede predecessor run: {err}"));
    }

    successor["state"] = json!("active");
    if let Err(err) = state::write_state(&args.worktree, &successor) {
        let rollback_result = state::rollback_successor(&args.worktree, successor_id.as_str());
        let restore_result = state::write_state(&args.worktree, &predecessor_before_supersession);
        if let Err(rollback_err) = rollback_result {
            return Err(format!(
                "cannot activate successor run: {err}; cannot roll back successor run: {rollback_err}"
            ));
        }
        if let Err(restore_err) = restore_result {
            return Err(format!(
                "cannot activate successor run: {err}; cannot restore predecessor run: {restore_err}"
            ));
        }
        return Err(format!("cannot activate successor run: {err}"));
    }
    let limits = storage::StorageLimits::load(&args.worktree)?;
    event::append_audit_event(
        &args.worktree,
        &args.run_id,
        state["revision"]
            .as_u64()
            .unwrap_or(args.expected_revision + 1),
        "run-superseded",
        json!({
            "successor_run_id": successor["run_id"],
            "reason": state["supersession"]["reason"],
        }),
        &limits,
    )?;
    Ok(json!({
        "status": "superseded",
        "run_id": args.run_id,
        "successor_run_id": successor["run_id"],
        "revision": state["revision"],
    }))
}

fn inspect_run(args: args::InspectArgs) -> Result<Value, String> {
    if !args.json {
        return Err("distill requires --json".to_string());
    }
    state::ensure_worktree(&args.worktree)?;
    let _lock = state::acquire_run_lock(&args.worktree, &args.run_id)?;
    let state =
        state::read_state_for_update_at_revision(&args.worktree, &args.run_id, args.expected_revision)?;
    if state["session_binding"]["session_id"] != args.session_id {
        return Err("session id does not match run binding".to_string());
    }
    state::write_state(&args.worktree, &state)?;
    if state["migration_events"]
        .as_array()
        .is_some_and(|events| !events.is_empty())
        && !event::event_type_exists(&args.worktree, &args.run_id, "state-migrated")?
    {
        let limits = storage::StorageLimits::load(&args.worktree)?;
        event::append_audit_event(
            &args.worktree,
            &args.run_id,
            state["revision"].as_u64().unwrap_or(args.expected_revision),
            "state-migrated",
            json!({
                "migration_events": state["migration_events"],
            }),
            &limits,
        )?;
    }
    Ok(report::response_from_state(&state))
}

fn events_run(args: args::EventsArgs) -> Result<Value, String> {
    if !args.json {
        return Err("distill requires --json".to_string());
    }
    state::ensure_worktree(&args.worktree)?;
    let events = event::read_events_after(&args.worktree, &args.run_id, args.after)?;
    let next_after = events
        .last()
        .and_then(|event| event["sequence"].as_u64())
        .unwrap_or(args.after);
    Ok(json!({
        "schema_version": CURRENT_SCHEMA_VERSION,
        "event_version": 1,
        "run_id": args.run_id,
        "after": args.after,
        "next_after": next_after,
        "events": events,
    }))
}

fn render_report_run(args: args::RenderReportArgs) -> Result<Value, String> {
    if !args.json {
        return Err("distill requires --json".to_string());
    }
    if args.renderer != "markdown" {
        return Err("only --renderer markdown is supported".to_string());
    }
    state::ensure_worktree(&args.worktree)?;
    let report_rel = format!(".distill/runs/{}/report.json", args.run_id);
    let report_path = args.worktree.join(&report_rel);
    let report: Value = serde_json::from_str(
        &fs::read_to_string(&report_path)
            .map_err(|err| format!("cannot read canonical report: {err}"))?,
    )
    .map_err(|err| format!("cannot parse canonical report: {err}"))?;
    if report["run_id"] != args.run_id || report["status"] != "completed" {
        return Err("canonical report is not a completed report for this run".to_string());
    }
    let markdown_rel = format!(".distill/runs/{}/report.md", args.run_id);
    let markdown = report::render_markdown_report(&report)?;
    state::write_bytes(
        &args.worktree.join(&markdown_rel),
        markdown.as_bytes(),
        true,
    )?;
    Ok(json!({
        "schema_version": CURRENT_SCHEMA_VERSION,
        "status": "rendered",
        "renderer": "markdown",
        "run_id": args.run_id,
        "canonical_hash": report["canonical_hash"],
        "markdown_path": markdown_rel,
    }))
}

fn record_stage_boundary(
    worktree: &Path,
    run_id: &str,
    stage: &workflow::WorkflowStage,
    evidence: &Value,
    mut state: Value,
    limits: &storage::StorageLimits,
) -> Result<Value, String> {
    let boundary = evidence["status"]
        .as_str()
        .ok_or("evidence.status is required for a stage boundary")?;
    let reason = evidence["reason"].as_str().unwrap_or("").trim();
    let required_next_action = evidence["required_next_action"]
        .as_str()
        .unwrap_or("")
        .trim();
    if reason.is_empty() {
        return Err("boundary evidence.reason must not be empty".to_string());
    }
    if required_next_action.is_empty() {
        return Err("boundary evidence.required_next_action must not be empty".to_string());
    }
    let next_revision = state["revision"]
        .as_u64()
        .ok_or("state revision is invalid")?
        + 1;
    mark_stage_state(&mut state, &stage.id, boundary, next_revision)?;
    state["state"] = json!(if boundary == "blocked" {
        "blocked"
    } else {
        "active"
    });
    state["revision"] = json!(next_revision);
    if !state["boundaries"].is_array() {
        state["boundaries"] = json!([]);
    }
    state["boundaries"]
        .as_array_mut()
        .ok_or("state boundaries are invalid")?
        .push(json!({
            "stage": stage.id,
            "state": boundary,
            "reason": reason,
            "required_next_action": required_next_action,
            "revision": next_revision,
        }));

    let mut planned_files = Vec::new();
    let evidence_artifact = event::plan_evidence_artifact(
        worktree,
        run_id,
        &stage.id,
        next_revision,
        evidence,
        &mut planned_files,
    )?;
    state["storage"] = storage::storage_summary(worktree, limits)?;
    let event_lines = event::build_transition_events(
        worktree,
        run_id,
        next_revision,
        vec![(
            if boundary == "blocked" {
                "stage-blocked"
            } else {
                "stage-waiting"
            },
            json!({
                "stage": stage.id,
                "reason": reason,
                "required_next_action": required_next_action,
                "evidence": evidence_artifact,
            }),
        )],
        limits,
    )?;
    transition::commit(worktree, run_id, &state, &event_lines, planned_files, limits)?;
    Ok(report::response_from_state(&state))
}

fn validate_clarification_evidence(worktree: &Path, evidence: &Value) -> Result<Value, String> {
    let clarified_requirement = evidence
        .get("clarified_requirement")
        .ok_or("evidence.clarified_requirement is required")?
        .as_str()
        .ok_or("evidence.clarified_requirement must be a string")?
        .trim();
    if clarified_requirement.is_empty() {
        return Err("evidence.clarified_requirement must not be empty".to_string());
    }
    let decisions = string_array(
        evidence
            .get("decisions")
            .ok_or("evidence.decisions is required")?,
        "evidence.decisions",
    )?;
    let assumptions_value = evidence
        .get("accepted_assumptions")
        .ok_or("evidence.accepted_assumptions is required")?;
    let accepted_assumptions = string_array(assumptions_value, "evidence.accepted_assumptions")?;
    let material_unknowns = object_array(
        evidence
            .get("material_unknowns")
            .ok_or("evidence.material_unknowns is required")?,
        "evidence.material_unknowns",
    )?;
    for unknown in &material_unknowns {
        let description = unknown["description"].as_str().unwrap_or("").trim();
        if description.is_empty() {
            return Err("material unknown description must not be empty".to_string());
        }
        let material = unknown["material"]
            .as_bool()
            .ok_or("material unknown material must be a boolean")?;
        let resolved = unknown["resolved"]
            .as_bool()
            .ok_or("material unknown resolved must be a boolean")?;
        if material && !resolved {
            return Err(format!(
                "unresolved material unknown prevents clarification completion: {description}"
            ));
        }
        if resolved
            && unknown["resolution"]
                .as_str()
                .unwrap_or("")
                .trim()
                .is_empty()
        {
            return Err(format!(
                "resolved material unknown requires a resolution: {description}"
            ));
        }
    }

    let domain_document_artifacts = validate_domain_document_artifacts(
        worktree,
        evidence
            .get("domain_document_artifacts")
            .ok_or("evidence.domain_document_artifacts is required")?,
    )?;
    Ok(json!({
        "clarified_requirement": clarified_requirement,
        "decisions": decisions,
        "accepted_assumptions": accepted_assumptions,
        "material_unknowns": material_unknowns,
        "domain_document_artifacts": domain_document_artifacts,
    }))
}

fn string_array(value: &Value, field: &str) -> Result<Vec<String>, String> {
    value
        .as_array()
        .ok_or_else(|| format!("{field} must be an array"))?
        .iter()
        .map(|item| {
            let text = item
                .as_str()
                .ok_or_else(|| format!("{field} entries must be strings"))?
                .trim();
            if text.is_empty() {
                Err(format!("{field} entries must not be empty"))
            } else {
                Ok(text.to_string())
            }
        })
        .collect()
}

fn object_array(value: &Value, field: &str) -> Result<Vec<Value>, String> {
    value
        .as_array()
        .ok_or_else(|| format!("{field} must be an array"))?
        .iter()
        .map(|item| {
            if item.is_object() {
                Ok(item.clone())
            } else {
                Err(format!("{field} entries must be objects"))
            }
        })
        .collect()
}

fn validate_domain_document_artifacts(
    worktree: &Path,
    value: &Value,
) -> Result<Vec<Value>, String> {
    let artifacts = value
        .as_array()
        .ok_or("evidence.domain_document_artifacts must be an array")?;
    let mut validated = Vec::new();
    for artifact in artifacts {
        let rel = artifact["path"]
            .as_str()
            .ok_or("domain document artifact path is required")?;
        let supplied_hash = artifact["sha256"]
            .as_str()
            .ok_or("domain document artifact sha256 is required")?;
        let path = Path::new(rel);
        let safe_components = path
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)));
        let allowed = rel == "CONTEXT.md"
            || rel == "docs/agents/domain.md"
            || (rel.starts_with("docs/adr/") && rel.ends_with(".md"));
        if !safe_components || !allowed {
            return Err(format!(
                "domain document artifact path is not allowed: {rel}"
            ));
        }
        let artifact_path = worktree.join(path);
        reject_symlink_components(worktree, path)?;
        let metadata = fs::symlink_metadata(&artifact_path)
            .map_err(|err| format!("cannot inspect {rel}: {err}"))?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "domain document artifact must not be a symlink: {rel}"
            ));
        }
        let canonical_worktree = fs::canonicalize(worktree)
            .map_err(|err| format!("cannot canonicalize worktree: {err}"))?;
        let canonical_artifact = fs::canonicalize(&artifact_path)
            .map_err(|err| format!("cannot canonicalize {rel}: {err}"))?;
        if !canonical_artifact.starts_with(&canonical_worktree) {
            return Err(format!("domain document artifact escapes worktree: {rel}"));
        }
        let bytes =
            fs::read(&canonical_artifact).map_err(|err| format!("cannot read {rel}: {err}"))?;
        let actual_hash = sha256_hex(&bytes);
        if supplied_hash != actual_hash {
            return Err(format!("domain document artifact hash mismatch for {rel}"));
        }
        validated.push(json!({"path": rel, "sha256": actual_hash}));
    }
    Ok(validated)
}

pub(crate) fn reject_symlink_components(worktree: &Path, relative: &Path) -> Result<(), String> {
    let mut current = worktree.to_path_buf();
    for component in relative.components() {
        let std::path::Component::Normal(component) = component else {
            return Err("domain document artifact path is invalid".to_string());
        };
        current.push(component);
        let metadata = fs::symlink_metadata(&current)
            .map_err(|err| format!("cannot inspect {}: {err}", current.display()))?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "domain document artifact must not be a symlink: {}",
                relative.display()
            ));
        }
    }
    Ok(())
}

fn changed_domain_document_artifacts(
    worktree: &Path,
    baseline: &Value,
) -> Result<Vec<Value>, String> {
    let baseline_hashes = baseline["domain_documents"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            Some((
                entry["path"].as_str()?.to_string(),
                entry["hash"].as_str()?.to_string(),
            ))
        })
        .collect::<std::collections::HashMap<_, _>>();
    let candidates = context::domain_document_paths(worktree)?;
    let mut artifacts = Vec::new();
    let canonical_worktree =
        fs::canonicalize(worktree).map_err(|err| format!("cannot canonicalize worktree: {err}"))?;
    for rel in candidates {
        let path = worktree.join(&rel);
        match fs::symlink_metadata(&path) {
            Err(err) if err.kind() == ErrorKind::NotFound => continue,
            Err(err) => return Err(format!("cannot inspect {rel}: {err}")),
            Ok(_) => {}
        }
        reject_symlink_components(worktree, Path::new(&rel))?;
        let canonical = fs::canonicalize(&path)
            .map_err(|err| format!("cannot canonicalize domain document {rel}: {err}"))?;
        if !canonical.starts_with(&canonical_worktree) {
            return Err(format!("domain document artifact escapes worktree: {rel}"));
        }
        if !canonical.is_file() {
            return Err(format!("domain document artifact is not a file: {rel}"));
        }
        let bytes = fs::read(&canonical).map_err(|err| format!("cannot read {rel}: {err}"))?;
        let content = String::from_utf8_lossy(&bytes);
        let changed = baseline_hashes
            .get(&rel)
            .is_none_or(|hash| hash != &stable_hash(&content));
        if changed {
            artifacts.push(json!({"path": rel, "sha256": sha256_hex(&bytes)}));
        }
    }
    Ok(artifacts)
}

fn record_stage_completion(
    state: &mut Value,
    stage: &workflow::WorkflowStage,
    revision: u64,
    evidence: &Value,
) -> Result<(), String> {
    let summary = evidence["summary"]
        .as_str()
        .unwrap_or("Completion evidence accepted.");
    let mut evidence_entry = json!({
        "stage": stage.id,
        "completed_revision": revision,
        "accepted_user_checkpoint": stage.checkpoint,
        "summary": summary,
        "adapter": {
            "executor": stage.executor,
            "skill": stage.skill,
            "invocation": "unmodified-skill",
        }
    });
    if stage.id == "clarification" {
        evidence_entry["clarification"] = state["clarification"].clone();
    }
    state["completion_evidence"]
        .as_array_mut()
        .ok_or("state completion_evidence is invalid")?
        .push(evidence_entry);
    mark_stage_completed(state, &stage.id, revision)
}

fn mark_stage_completed(state: &mut Value, stage_id: &str, revision: u64) -> Result<(), String> {
    mark_stage_state(state, stage_id, "completed", revision)
}

fn mark_stage_state(
    state: &mut Value,
    stage_id: &str,
    stage_state: &str,
    revision: u64,
) -> Result<(), String> {
    let stages = state["stages"]
        .as_array_mut()
        .ok_or("state stages are invalid")?;
    let stage = stages
        .iter_mut()
        .find(|stage| stage["id"] == stage_id)
        .ok_or_else(|| format!("stage state not found: {stage_id}"))?;
    stage["state"] = json!(stage_state);
    stage["revision"] = json!(revision);
    Ok(())
}

fn mark_stage_active(state: &mut Value, stage_id: &str, revision: u64) -> Result<(), String> {
    let stages = state["stages"]
        .as_array_mut()
        .ok_or("state stages are invalid")?;
    let stage = stages
        .iter_mut()
        .find(|stage| stage["id"] == stage_id)
        .ok_or_else(|| format!("stage state not found: {stage_id}"))?;
    stage["state"] = json!("active");
    stage["revision"] = json!(revision);
    Ok(())
}

fn mark_stage_reconciliation(
    state: &mut Value,
    stage_id: &str,
    revision: u64,
) -> Result<(), String> {
    let stages = state["stages"]
        .as_array_mut()
        .ok_or("state stages are invalid")?;
    let stage = stages
        .iter_mut()
        .find(|stage| stage["id"] == stage_id)
        .ok_or_else(|| format!("stage state not found: {stage_id}"))?;
    stage["state"] = json!("needs-reconciliation");
    stage["revision"] = json!(revision);
    Ok(())
}


