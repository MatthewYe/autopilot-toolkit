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

    if let Some(existing) = state::active_run_for_session(&args.worktree, &args.session_id)? {
        let state = state::read_state(&args.worktree, &existing)?;
        let run_state = state::run_state_from_value(state)?;
        return Ok(report::response_from_state(&run_state));
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

    let run_state = state::RunState::new(state::NewRunState {
        run_id: run_id.clone(),
        lifecycle: state::RunLifecycle::Active,
        current_stage: first_stage.id.clone(),
        predecessor_run_id: None,
        session_binding: state::SessionBinding {
            runtime: args.runtime.clone(),
            session_id: args.session_id.clone(),
            released: None,
            released_revision: None,
        },
        workflow: state::WorkflowSnapshot {
            version: workflow.version.clone(),
            source: WORKFLOW_SOURCE.to_string(),
            stages: workflow.stages.clone(),
        },
        requirement: state::Requirement {
            source: if args.intake_json.is_some() {
                "runtime-intake-json"
            } else {
                "explicit-text"
            }
            .to_string(),
            text: requirement_text,
            supersession_reason: None,
        },
        requirement_snapshot: Some(intake_snapshot.manifest.clone()),
        storage: Some(storage::storage_summary(&args.worktree, &limits)?),
        stage_states: workflow::initial_stage_states(&workflow)?,
        intake_summary: "Captured the supplied requirement text.".to_string(),
        context_baseline: context::capture_context_baseline(&args.worktree)?,
    })?;
    let state = state::to_state_value(&run_state)?;

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
    let state_bytes = state::serialize_state(&state)?;
    intake_snapshot.files.push(storage::RunFile {
        relative_path: PathBuf::from("state.json"),
        bytes: state_bytes,
    });
    storage::commit_new_run(&args.worktree, &run_id, intake_snapshot.files, &limits)?;
    let mut response = report::response_from_state(&run_state);
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
    let mut run_state = state::read_run_state_for_update_at_revision(
        &args.worktree,
        &args.run_id,
        args.expected_revision,
    )?;
    // Upfront guard, before any publication side effects; the transition
    // methods re-enforce the same guard internally.
    run_state.guard_stage_submission(&args.session_id, &args.stage)?;

    let evidence: Value = serde_json::from_str(&args.evidence)
        .map_err(|err| format!("--evidence must be valid JSON: {err}"))?;
    let expected_stage = run_state
        .current_stage
        .clone()
        .ok_or("state is missing current_stage")?;
    let stage = run_state
        .workflow
        .stages
        .iter()
        .find(|stage| stage.id == expected_stage)
        .cloned()
        .ok_or(format!("workflow stage not found: {expected_stage}"))?;
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
    if let Some(boundary) = evidence["status"].as_str() {
        if boundary == "waiting" || boundary == "blocked" {
            return record_stage_boundary(
                &args.worktree,
                &args.run_id,
                &args.session_id,
                &stage,
                &evidence,
                run_state,
                &limits,
            );
        }
        if boundary != "completed" {
            return Err("evidence.status must be completed, waiting, or blocked".to_string());
        }
    }

    let current_revision = run_state.revision;
    let next_revision = current_revision + 1;

    let clarification = (stage.id == "clarification")
        .then(|| validate_clarification_evidence(&args.worktree, &evidence))
        .transpose()?;
    // `context` and `publication` still mutate untyped state Values (their
    // typed migration is a later ticket); bridge through the canonical
    // Value projection, which the typed round-trip keeps lossless.
    state::with_value_projection(&mut run_state, |projection| {
        context::validate_context_drift(
            &args.worktree,
            projection,
            &stage.id,
            &evidence,
            clarification.as_ref(),
        )
    })?;
    if let Some(clarification) = clarification {
        run_state.clarification = clarification;
    }

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
        let outcome = state::with_value_projection(&mut run_state, |projection| {
            publication::publish_prd(
                &args.worktree,
                &args.run_id,
                current_revision,
                projection,
                &evidence,
            )
        })?;
        planned_files.extend(outcome.files);
        blocked_publication = outcome.blocked;
    } else if stage.id == "issues" {
        let outcome = state::with_value_projection(&mut run_state, |projection| {
            publication::publish_issues(
                &args.worktree,
                &args.run_id,
                current_revision,
                projection,
                &evidence,
            )
        })?;
        planned_files.extend(outcome.files);
        blocked_publication = outcome.blocked;
    }

    if let Some(blocked_reason) = blocked_publication {
        run_state.mark_stage_needs_reconciliation(&args.session_id, &stage.id)?;
        run_state.storage = Some(storage::storage_summary(&args.worktree, &limits)?);
        let event_lines = event::build_transition_events(
            &args.worktree,
            &args.run_id,
            run_state.revision,
            vec![(
                "publication-reconciliation",
                json!({
                    "stage": stage.id,
                    "reason": blocked_reason,
                    "evidence": evidence_artifact,
                    "publications": serde_json::to_value(&run_state.publications)
                        .map_err(|err| format!("json error: {err}"))?,
                }),
            )],
            &limits,
        )?;
        let next_state =
            state::to_state_value(&run_state)?;
        transition::commit(
            &args.worktree,
            &args.run_id,
            &next_state,
            &event_lines,
            planned_files,
            &limits,
        )?;
        let mut response = report::response_from_state(&run_state);
        response["publication_blocked"] = json!(blocked_reason);
        return Ok(response);
    }

    let summary = evidence["summary"]
        .as_str()
        .unwrap_or("Completion evidence accepted.");
    let completion = run_state.complete_stage(&args.session_id, &stage, summary)?;
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
                "publication": serde_json::to_value(&run_state.publications.prd)
                    .map_err(|err| format!("json error: {err}"))?,
            }),
        ));
    } else if stage.id == "issues" {
        event_payloads.push((
            "publication-recorded",
            json!({
                "stage": "issues",
                "publications": serde_json::to_value(&run_state.publications.issues)
                    .map_err(|err| format!("json error: {err}"))?,
            }),
        ));
    }

    if completion == state::StageCompletion::RunCompleted {
        run_state.storage = Some(storage::storage_summary(&args.worktree, &limits)?);
        planned_files.extend(report::plan_completion_report(
            &args.worktree,
            &args.run_id,
            &mut run_state,
        )?);
        event_payloads.push((
            "session-released",
            json!({
                "session_id": run_state.session_binding.session_id,
                "released_revision": run_state.revision,
            }),
        ));
        event_payloads.push((
            "terminal-completed",
            json!({
                "final_revision": run_state.revision,
                "report": run_state.report,
            }),
        ));
    }
    run_state.storage = Some(storage::storage_summary(&args.worktree, &limits)?);
    let event_lines = event::build_transition_events(
        &args.worktree,
        &args.run_id,
        run_state.revision,
        event_payloads,
        &limits,
    )?;

    let next_state = state::to_state_value(&run_state)?;
    transition::commit(
        &args.worktree,
        &args.run_id,
        &next_state,
        &event_lines,
        planned_files,
        &limits,
    )?;
    Ok(report::response_from_state(&run_state))
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
    let mut run_state = state::read_run_state_for_update_at_revision(
        &args.worktree,
        &args.run_id,
        args.expected_revision,
    )?;
    // The tombstone is priced for a fresh purge; recovery keeps the durable
    // one from the interrupted attempt instead.
    let empty_snapshot = Value::Null;
    let requirement_snapshot = run_state
        .requirement_snapshot
        .as_ref()
        .unwrap_or(&empty_snapshot);
    let tombstone = json!({
        "run_id": args.run_id,
        "state": "purged",
        "revision": run_state.revision + 1,
        "purged_at": current_timestamp_millis(),
        "user_authorized": true,
        "source_hashes": intake::source_hashes(requirement_snapshot),
        "publications": run_state.publications,
    });
    let begin = run_state.begin_purge(&args.session_id, tombstone)?;
    let limits = storage::StorageLimits::load(&args.worktree)?;
    let run_dir = storage::run_dir(&args.worktree, &args.run_id)?;
    if begin == state::PurgeBegin::Started {
        // The durable pending state must precede the authorization event so
        // an interrupted purge stays recoverable — the sanctioned
        // state-before-event exception to the commit protocol.
        let state_value =
            state::to_state_value(&run_state)?;
        state::write_state(&args.worktree, &state_value)?;
    }
    if env::var("DISTILL_FAIL_PURGE_BEFORE_AUTH_EVENT").as_deref() == Ok(args.run_id.as_str()) {
        return Err("injected purge interruption before authorization event".to_string());
    }
    if !event::event_type_exists(&args.worktree, &args.run_id, "run-purge-authorized")? {
        let purge = run_state
            .purge
            .as_ref()
            .expect("begin_purge guarantees a purge state");
        event::append_audit_event(
            &args.worktree,
            &args.run_id,
            run_state.revision,
            "run-purge-authorized",
            json!({
                "expected_revision": purge.source_revision,
                "next_revision": purge.revision,
                "user_authorized": true,
            }),
            &limits,
        )?;
    }
    if env::var("DISTILL_FAIL_PURGE_AFTER_PENDING").as_deref() == Ok(args.run_id.as_str()) {
        return Err("injected purge interruption after durable pending state".to_string());
    }
    let tombstone = run_state
        .purge
        .as_ref()
        .expect("begin_purge guarantees a purge state")
        .tombstone
        .clone();
    storage::remove_dir_if_exists(&run_dir.join("snapshots"))?;
    storage::remove_dir_if_exists(&run_dir.join("artifacts"))?;
    storage::atomic_write_json(&run_dir.join("tombstone.json"), &tombstone, false)?;
    run_state.complete_purge(&args.session_id)?;
    let event_lines = if event::event_type_exists(&args.worktree, &args.run_id, "run-purged")? {
        // Recovery of an attempt that appended the event but failed the
        // state write: do not double-append.
        Vec::new()
    } else {
        event::build_transition_events(
            &args.worktree,
            &args.run_id,
            run_state.revision,
            vec![(
                "run-purged",
                json!({
                    "tombstone_path": format!(".distill/runs/{}/tombstone.json", args.run_id),
                    "user_authorized": true,
                }),
            )],
            &limits,
        )?
    };
    let state_value =
        state::to_state_value(&run_state)?;
    transition::commit_audit_only(&args.worktree, &args.run_id, &state_value, &event_lines, &limits)?;
    Ok(report::response_from_state(&run_state))
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
    let mut run_state = state::read_run_state_for_update_at_revision(
        &args.worktree,
        &args.run_id,
        args.expected_revision,
    )?;
    // Computed before the transition so a failure leaves the state untouched.
    let domain_document_artifacts =
        changed_domain_document_artifacts(&args.worktree, &run_state.context_baseline)?;
    run_state.abort(&args.session_id, &args.reason, domain_document_artifacts)?;
    let limits = storage::StorageLimits::load(&args.worktree)?;
    let event_lines = event::build_transition_events(
        &args.worktree,
        &args.run_id,
        run_state.revision,
        vec![(
            "run-aborted",
            json!({
                "reason": args.reason,
                "user_authorized": true,
                "session_released": true,
            }),
        )],
        &limits,
    )?;
    let state_value =
        state::to_state_value(&run_state)?;
    transition::commit_audit_only(&args.worktree, &args.run_id, &state_value, &event_lines, &limits)?;
    Ok(report::response_from_state(&run_state))
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
    let mut run_state = state::read_run_state_for_update_at_revision(
        &args.worktree,
        &args.run_id,
        args.expected_revision,
    )?;
    run_state.takeover(&args.from_session, &args.to_session, &args.reason)?;
    let limits = storage::StorageLimits::load(&args.worktree)?;
    let event_lines = event::build_transition_events(
        &args.worktree,
        &args.run_id,
        run_state.revision,
        vec![(
            "session-takeover",
            json!({
                "from_session": args.from_session,
                "to_session": args.to_session,
                "reason": args.reason,
                "user_authorized": true,
            }),
        )],
        &limits,
    )?;
    let state_value =
        state::to_state_value(&run_state)?;
    transition::commit_audit_only(&args.worktree, &args.run_id, &state_value, &event_lines, &limits)?;
    Ok(report::response_from_state(&run_state))
}

fn supersede_run(args: args::SupersedeArgs) -> Result<Value, String> {
    if !args.json {
        return Err("distill requires --json".to_string());
    }
    state::ensure_worktree(&args.worktree)?;
    let _lock = state::acquire_run_lock(&args.worktree, &args.run_id)?;
    let mut run_state = state::read_run_state_for_update_at_revision(
        &args.worktree,
        &args.run_id,
        args.expected_revision,
    )?;

    let predecessor_before_supersession = run_state.clone();
    let workflow = workflow::load_workflow()?;
    let successor_id = create_run_id(&args.session_id);
    let first_stage = workflow::next_stage_after(&workflow, "intake")?;
    let session_binding = run_state.session_binding.clone();
    // Guards + single-run mutation; the cross-run two-write + rollback
    // transaction below stays orchestrated here.
    run_state.mark_superseded(&args.session_id, &args.reason, &successor_id)?;
    let mut successor_state = state::RunState::new(state::NewRunState {
        run_id: successor_id.clone(),
        lifecycle: state::RunLifecycle::SupersessionPending,
        current_stage: first_stage.id.clone(),
        predecessor_run_id: Some(args.run_id.clone()),
        session_binding,
        workflow: state::WorkflowSnapshot {
            version: workflow.version.clone(),
            source: WORKFLOW_SOURCE.to_string(),
            stages: workflow.stages.clone(),
        },
        requirement: state::Requirement {
            source: "explicit-text".to_string(),
            text: args.requirement.clone(),
            supersession_reason: Some(args.reason.clone()),
        },
        requirement_snapshot: None,
        storage: None,
        stage_states: workflow::initial_stage_states(&workflow)?,
        intake_summary: "Captured the superseding requirement text.".to_string(),
        context_baseline: context::capture_context_baseline(&args.worktree)?,
    })?;
    let successor_value =
        serde_json::to_value(&successor_state).map_err(|err| format!("json error: {err}"))?;
    let successor_bytes = state::serialize_state(&successor_value)?;
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

    let predecessor_value =
        state::to_state_value(&run_state)?;
    if let Err(err) = state::write_state(&args.worktree, &predecessor_value) {
        state::rollback_successor(&args.worktree, successor_id.as_str())?;
        return Err(format!("cannot supersede predecessor run: {err}"));
    }

    successor_state.state = state::RunLifecycle::Active;
    let successor_value =
        serde_json::to_value(&successor_state).map_err(|err| format!("json error: {err}"))?;
    if let Err(err) = state::write_state(&args.worktree, &successor_value) {
        let rollback_result = state::rollback_successor(&args.worktree, successor_id.as_str());
        let restore_value = serde_json::to_value(&predecessor_before_supersession)
            .map_err(|err| format!("json error: {err}"))?;
        let restore_result = state::write_state(&args.worktree, &restore_value);
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
    let event_lines = event::build_transition_events(
        &args.worktree,
        &args.run_id,
        run_state.revision,
        vec![(
            "run-superseded",
            json!({
                "successor_run_id": successor_id,
                "reason": args.reason,
            }),
        )],
        &limits,
    )?;
    transition::commit_audit_only(&args.worktree, &args.run_id, &predecessor_value, &event_lines, &limits)?;
    Ok(json!({
        "status": "superseded",
        "run_id": args.run_id,
        "successor_run_id": successor_id,
        "revision": run_state.revision,
    }))
}

fn inspect_run(args: args::InspectArgs) -> Result<Value, String> {
    if !args.json {
        return Err("distill requires --json".to_string());
    }
    state::ensure_worktree(&args.worktree)?;
    let _lock = state::acquire_run_lock(&args.worktree, &args.run_id)?;
    let (run_state, migration_backfill) = state::read_run_state_for_inspect_at_revision(
        &args.worktree,
        &args.run_id,
        args.expected_revision,
    )?;
    if run_state.session_binding.session_id != args.session_id {
        return Err("session id does not match run binding".to_string());
    }
    // Persist the (possibly migrated/backfilled) canonical state.
    let state_value =
        state::to_state_value(&run_state)?;
    state::write_state(&args.worktree, &state_value)?;
    if let Some(migration_events) = migration_backfill {
        if !event::event_type_exists(&args.worktree, &args.run_id, "state-migrated")? {
            let limits = storage::StorageLimits::load(&args.worktree)?;
            event::append_audit_event(
                &args.worktree,
                &args.run_id,
                run_state.revision,
                "state-migrated",
                json!({
                    "migration_events": migration_events,
                }),
                &limits,
            )?;
        }
    }
    Ok(report::response_from_state(&run_state))
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
    session_id: &str,
    stage: &workflow::WorkflowStage,
    evidence: &Value,
    mut run_state: state::RunState,
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
    let boundary_state = if boundary == "blocked" {
        state::BoundaryState::Blocked
    } else {
        state::BoundaryState::Waiting
    };
    // Guards (including the empty reason/next-action checks) live inside the
    // transition methods.
    run_state.defer_stage(session_id, &stage.id, boundary_state, reason, required_next_action)?;
    let next_revision = run_state.revision;

    let mut planned_files = Vec::new();
    let evidence_artifact = event::plan_evidence_artifact(
        worktree,
        run_id,
        &stage.id,
        next_revision,
        evidence,
        &mut planned_files,
    )?;
    run_state.storage = Some(storage::storage_summary(worktree, limits)?);
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
    let next_state = state::to_state_value(&run_state)?;
    transition::commit(worktree, run_id, &next_state, &event_lines, planned_files, limits)?;
    Ok(report::response_from_state(&run_state))
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


