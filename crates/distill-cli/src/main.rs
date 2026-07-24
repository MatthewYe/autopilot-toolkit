use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::fs::OpenOptions;
use std::hash::{Hash, Hasher};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::{self, Command};
use std::time::{SystemTime, UNIX_EPOCH};

mod intake;
mod publication;
mod storage;

const CURRENT_SCHEMA_VERSION: u64 = 1;
const WORKFLOW_SOURCE: &str = "embedded:distill.v1.json";
const WORKFLOW_JSON: &str = include_str!("../workflows/distill.v1.json");
const SUPPORTED_RUNTIMES: [&str; 3] = ["codex", "kimi", "reasonix"];

#[derive(Clone, Deserialize, Serialize)]
struct WorkflowDefinition {
    version: String,
    stages: Vec<WorkflowStage>,
}

#[derive(Clone, Deserialize, Serialize)]
struct WorkflowStage {
    id: String,
    executor: String,
    skill: Option<String>,
    checkpoint: String,
    next_action: String,
}

struct StartArgs {
    runtime: String,
    session_id: String,
    worktree: PathBuf,
    requirement: Option<String>,
    intake_json: Option<String>,
    json: bool,
}

struct SubmitArgs {
    run_id: String,
    session_id: String,
    expected_revision: u64,
    worktree: PathBuf,
    stage: String,
    evidence: String,
    json: bool,
}

struct TakeoverArgs {
    run_id: String,
    from_session: String,
    to_session: String,
    expected_revision: u64,
    worktree: PathBuf,
    reason: String,
    user_authorized: bool,
    json: bool,
}

struct SupersedeArgs {
    run_id: String,
    session_id: String,
    expected_revision: u64,
    worktree: PathBuf,
    reason: String,
    requirement: String,
    json: bool,
}

struct InspectArgs {
    run_id: String,
    session_id: String,
    expected_revision: u64,
    worktree: PathBuf,
    json: bool,
}

struct EventsArgs {
    run_id: String,
    worktree: PathBuf,
    after: u64,
    json: bool,
}

struct RenderReportArgs {
    run_id: String,
    worktree: PathBuf,
    renderer: String,
    json: bool,
}

struct RunLock {
    path: PathBuf,
}

pub(crate) struct PlannedFile {
    pub(crate) path: PathBuf,
    pub(crate) bytes: Vec<u8>,
    pub(crate) counts_against_quota: bool,
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
            let response = start_distill(parse_start_args(args.collect())?)?;
            print_json(&response)
        }
        Some("submit-evidence") => {
            let response = submit_evidence(parse_submit_args(args.collect())?)?;
            print_json(&response)
        }
        Some("takeover") => {
            let response = takeover_run(parse_takeover_args(args.collect())?)?;
            print_json(&response)
        }
        Some("supersede") => {
            let response = supersede_run(parse_supersede_args(args.collect())?)?;
            print_json(&response)
        }
        Some("set-project-quota") => {
            let response = set_project_quota(parse_quota_args(args.collect())?)?;
            print_json(&response)
        }
        Some("purge") => {
            let response = purge_run(parse_purge_args(args.collect())?)?;
            print_json(&response)
        }
        Some("abort") => {
            let response = abort_run(parse_abort_args(args.collect())?)?;
            print_json(&response)
        }
        Some("inspect") => {
            let response = inspect_run(parse_inspect_args(args.collect())?)?;
            print_json(&response)
        }
        Some("events") => {
            let response = events_run(parse_events_args(args.collect())?)?;
            print_json(&response)
        }
        Some("render-report") => {
            let response = render_report_run(parse_render_report_args(args.collect())?)?;
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

fn parse_start_args(args: Vec<String>) -> Result<StartArgs, String> {
    let mut runtime = None;
    let mut session_id = None;
    let mut worktree = None;
    let mut requirement = None;
    let mut intake_json = None;
    let mut json = false;

    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--json" => json = true,
            "--runtime" => runtime = iter.next(),
            "--session-id" => session_id = iter.next(),
            "--worktree" => worktree = iter.next().map(PathBuf::from),
            "--requirement" => requirement = iter.next(),
            "--intake-json" => intake_json = iter.next(),
            other => return Err(format!("unknown start argument: {other}")),
        }
    }

    let runtime = require_supported_runtime(runtime)?;
    let session_id = require_non_empty(session_id, "--session-id")?;
    if requirement.is_none() && intake_json.is_none() {
        return Err("--requirement or --intake-json is required".to_string());
    }
    if requirement
        .as_ref()
        .is_some_and(|requirement| requirement.trim().is_empty())
    {
        return Err("--requirement must not be empty".to_string());
    }
    if intake_json
        .as_ref()
        .is_some_and(|intake_json| intake_json.trim().is_empty())
    {
        return Err("--intake-json must not be empty".to_string());
    }

    Ok(StartArgs {
        runtime,
        session_id,
        worktree: worktree.ok_or("--worktree is required")?,
        requirement,
        intake_json,
        json,
    })
}

fn require_supported_runtime(runtime: Option<String>) -> Result<String, String> {
    let runtime = runtime.ok_or("--runtime is required")?;
    if SUPPORTED_RUNTIMES.contains(&runtime.as_str()) {
        Ok(runtime)
    } else {
        Err(format!(
            "--runtime must be one of: {}",
            SUPPORTED_RUNTIMES.join(", ")
        ))
    }
}

fn parse_submit_args(args: Vec<String>) -> Result<SubmitArgs, String> {
    let mut run_id = None;
    let mut session_id = None;
    let mut expected_revision = None;
    let mut worktree = None;
    let mut stage = None;
    let mut evidence = None;
    let mut json = false;

    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--json" => json = true,
            "--run-id" => run_id = iter.next(),
            "--session-id" => session_id = iter.next(),
            "--expected-revision" => expected_revision = iter.next(),
            "--worktree" => worktree = iter.next().map(PathBuf::from),
            "--stage" => stage = iter.next(),
            "--evidence" => evidence = iter.next(),
            other => return Err(format!("unknown submit-evidence argument: {other}")),
        }
    }

    Ok(SubmitArgs {
        run_id: require_run_id(run_id, "--run-id")?,
        session_id: require_non_empty(session_id, "--session-id")?,
        expected_revision: parse_revision(expected_revision, "--expected-revision")?,
        worktree: worktree.ok_or("--worktree is required")?,
        stage: require_non_empty(stage, "--stage")?,
        evidence: require_non_empty(evidence, "--evidence")?,
        json,
    })
}

struct QuotaArgs {
    worktree: PathBuf,
    bytes: u64,
    json: bool,
}

struct PurgeArgs {
    worktree: PathBuf,
    run_id: String,
    session_id: String,
    expected_revision: u64,
    user_authorized: bool,
    json: bool,
}

struct AbortArgs {
    worktree: PathBuf,
    run_id: String,
    session_id: String,
    expected_revision: u64,
    reason: String,
    user_authorized: bool,
    json: bool,
}

fn parse_quota_args(args: Vec<String>) -> Result<QuotaArgs, String> {
    let mut worktree = None;
    let mut bytes = None;
    let mut json = false;
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--json" => json = true,
            "--worktree" => worktree = iter.next().map(PathBuf::from),
            "--bytes" => bytes = iter.next(),
            other => return Err(format!("unknown set-project-quota argument: {other}")),
        }
    }
    Ok(QuotaArgs {
        worktree: worktree.ok_or("--worktree is required")?,
        bytes: parse_revision(bytes, "--bytes")?,
        json,
    })
}

fn parse_purge_args(args: Vec<String>) -> Result<PurgeArgs, String> {
    let mut worktree = None;
    let mut run_id = None;
    let mut session_id = None;
    let mut expected_revision = None;
    let mut user_authorized = false;
    let mut json = false;
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--json" => json = true,
            "--worktree" => worktree = iter.next().map(PathBuf::from),
            "--run-id" => run_id = iter.next(),
            "--session-id" => session_id = iter.next(),
            "--expected-revision" => expected_revision = iter.next(),
            "--user-authorized" => user_authorized = true,
            other => return Err(format!("unknown purge argument: {other}")),
        }
    }
    Ok(PurgeArgs {
        worktree: worktree.ok_or("--worktree is required")?,
        run_id: require_run_id(run_id, "--run-id")?,
        session_id: require_non_empty(session_id, "--session-id")?,
        expected_revision: parse_revision(expected_revision, "--expected-revision")?,
        user_authorized,
        json,
    })
}

fn parse_abort_args(args: Vec<String>) -> Result<AbortArgs, String> {
    let mut worktree = None;
    let mut run_id = None;
    let mut session_id = None;
    let mut expected_revision = None;
    let mut reason = None;
    let mut user_authorized = false;
    let mut json = false;
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--json" => json = true,
            "--worktree" => worktree = iter.next().map(PathBuf::from),
            "--run-id" => run_id = iter.next(),
            "--session-id" => session_id = iter.next(),
            "--expected-revision" => expected_revision = iter.next(),
            "--reason" => reason = iter.next(),
            "--user-authorized" => user_authorized = true,
            other => return Err(format!("unknown abort argument: {other}")),
        }
    }
    Ok(AbortArgs {
        worktree: worktree.ok_or("--worktree is required")?,
        run_id: require_run_id(run_id, "--run-id")?,
        session_id: require_non_empty(session_id, "--session-id")?,
        expected_revision: parse_revision(expected_revision, "--expected-revision")?,
        reason: require_non_empty(reason, "--reason")?,
        user_authorized,
        json,
    })
}

fn parse_takeover_args(args: Vec<String>) -> Result<TakeoverArgs, String> {
    let mut run_id = None;
    let mut from_session = None;
    let mut to_session = None;
    let mut expected_revision = None;
    let mut worktree = None;
    let mut reason = None;
    let mut user_authorized = false;
    let mut json = false;

    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--json" => json = true,
            "--run-id" => run_id = iter.next(),
            "--from-session" => from_session = iter.next(),
            "--to-session" => to_session = iter.next(),
            "--expected-revision" => expected_revision = iter.next(),
            "--worktree" => worktree = iter.next().map(PathBuf::from),
            "--reason" => reason = iter.next(),
            "--user-authorized" => user_authorized = true,
            other => return Err(format!("unknown takeover argument: {other}")),
        }
    }

    Ok(TakeoverArgs {
        run_id: require_run_id(run_id, "--run-id")?,
        from_session: require_non_empty(from_session, "--from-session")?,
        to_session: require_non_empty(to_session, "--to-session")?,
        expected_revision: parse_revision(expected_revision, "--expected-revision")?,
        worktree: worktree.ok_or("--worktree is required")?,
        reason: require_non_empty(reason, "--reason")?,
        user_authorized,
        json,
    })
}

fn parse_supersede_args(args: Vec<String>) -> Result<SupersedeArgs, String> {
    let mut run_id = None;
    let mut session_id = None;
    let mut expected_revision = None;
    let mut worktree = None;
    let mut reason = None;
    let mut requirement = None;
    let mut json = false;

    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--json" => json = true,
            "--run-id" => run_id = iter.next(),
            "--session-id" => session_id = iter.next(),
            "--expected-revision" => expected_revision = iter.next(),
            "--worktree" => worktree = iter.next().map(PathBuf::from),
            "--reason" => reason = iter.next(),
            "--requirement" => requirement = iter.next(),
            other => return Err(format!("unknown supersede argument: {other}")),
        }
    }

    Ok(SupersedeArgs {
        run_id: require_run_id(run_id, "--run-id")?,
        session_id: require_non_empty(session_id, "--session-id")?,
        expected_revision: parse_revision(expected_revision, "--expected-revision")?,
        worktree: worktree.ok_or("--worktree is required")?,
        reason: require_non_empty(reason, "--reason")?,
        requirement: require_non_empty(requirement, "--requirement")?,
        json,
    })
}

fn parse_inspect_args(args: Vec<String>) -> Result<InspectArgs, String> {
    let mut run_id = None;
    let mut session_id = None;
    let mut expected_revision = None;
    let mut worktree = None;
    let mut json = false;

    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--json" => json = true,
            "--run-id" => run_id = iter.next(),
            "--session-id" => session_id = iter.next(),
            "--expected-revision" => expected_revision = iter.next(),
            "--worktree" => worktree = iter.next().map(PathBuf::from),
            other => return Err(format!("unknown inspect argument: {other}")),
        }
    }

    Ok(InspectArgs {
        run_id: require_run_id(run_id, "--run-id")?,
        session_id: require_non_empty(session_id, "--session-id")?,
        expected_revision: parse_revision(expected_revision, "--expected-revision")?,
        worktree: worktree.ok_or("--worktree is required")?,
        json,
    })
}

fn parse_events_args(args: Vec<String>) -> Result<EventsArgs, String> {
    let mut run_id = None;
    let mut worktree = None;
    let mut after = None;
    let mut json = false;

    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--json" => json = true,
            "--run-id" => run_id = iter.next(),
            "--worktree" => worktree = iter.next().map(PathBuf::from),
            "--after" => after = iter.next(),
            other => return Err(format!("unknown events argument: {other}")),
        }
    }

    Ok(EventsArgs {
        run_id: require_run_id(run_id, "--run-id")?,
        worktree: worktree.ok_or("--worktree is required")?,
        after: parse_revision(Some(after.unwrap_or_else(|| "0".to_string())), "--after")?,
        json,
    })
}

fn parse_render_report_args(args: Vec<String>) -> Result<RenderReportArgs, String> {
    let mut run_id = None;
    let mut worktree = None;
    let mut renderer = None;
    let mut json = false;

    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--json" => json = true,
            "--run-id" => run_id = iter.next(),
            "--worktree" => worktree = iter.next().map(PathBuf::from),
            "--renderer" => renderer = iter.next(),
            other => return Err(format!("unknown render-report argument: {other}")),
        }
    }

    Ok(RenderReportArgs {
        run_id: require_run_id(run_id, "--run-id")?,
        worktree: worktree.ok_or("--worktree is required")?,
        renderer: require_non_empty(renderer, "--renderer")?,
        json,
    })
}

fn require_non_empty(value: Option<String>, name: &str) -> Result<String, String> {
    let value = value.ok_or_else(|| format!("{name} is required"))?;
    if value.trim().is_empty() {
        return Err(format!("{name} must not be empty"));
    }
    Ok(value)
}

fn require_run_id(value: Option<String>, name: &str) -> Result<String, String> {
    let value = require_non_empty(value, name)?;
    storage::validate_run_id(&value)?;
    Ok(value)
}

fn parse_revision(value: Option<String>, name: &str) -> Result<u64, String> {
    require_non_empty(value, name)?
        .parse()
        .map_err(|_| format!("{name} must be an integer"))
}

fn start_distill(args: StartArgs) -> Result<Value, String> {
    if !args.json {
        return Err("distill requires --json".to_string());
    }
    ensure_worktree(&args.worktree)?;
    validate_required_project_configuration(&args.worktree)?;
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
    let _start_lock = acquire_project_start_lock(&args.worktree)?;

    if let Some(existing) = active_run_for_session(&args.worktree, &args.session_id)? {
        let state = read_state(&args.worktree, &existing)?;
        return Ok(response_from_state(&state));
    }

    let workflow = load_workflow()?;
    let first_stage = next_stage_after(&workflow, "intake")?;
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
        "stages": initial_stage_states(&workflow)?,
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
        "context_baseline": capture_context_baseline(&args.worktree)?,
        "drift_acknowledgments": [],
        "boundaries": [],
        "abort": Value::Null,
        "handoffs": [],
        "migration_events": [],
        "implementation_started": false,
    });

    let session_event = run_event(
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
    let intake_event = run_event(
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
    let mut response = response_from_state(&state);
    response["intake"] = json!({ "source_count": intake_snapshot.source_count });
    response["storage"] = storage::storage_summary(&args.worktree, &limits)?;
    response["storage"]["usage"]["run_raw_source_bytes"] = json!(intake_snapshot.total_raw_bytes);
    Ok(response)
}

fn validate_required_project_configuration(worktree: &Path) -> Result<(), String> {
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

fn submit_evidence(args: SubmitArgs) -> Result<Value, String> {
    if !args.json {
        return Err("distill requires --json".to_string());
    }
    ensure_worktree(&args.worktree)?;

    let _lock = acquire_run_lock(&args.worktree, &args.run_id)?;
    let state =
        read_state_for_update_at_revision(&args.worktree, &args.run_id, args.expected_revision)?;
    ensure_no_pending_purge(&state)?;
    if state["state"] != "active" && state["state"] != "blocked" {
        return Err("run is not active or blocked".to_string());
    }
    if state["session_binding"]["session_id"] != args.session_id {
        return Err("session id does not match active run binding".to_string());
    }

    let expected_stage = state["current_stage"]
        .as_str()
        .ok_or("state is missing current_stage")?;
    if completed_stage_ids(&state)?.contains(&args.stage.as_str()) {
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
    let stage = workflow_stage(&state, expected_stage)?.clone();
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
    validate_context_drift(
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
    let evidence_artifact = plan_evidence_artifact(
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
        let event_lines = build_transition_events(
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
        let state_bytes =
            serde_json::to_vec_pretty(&next_state).map_err(|err| format!("json error: {err}"))?;
        let current_state_bytes = fs::metadata(state_path(&args.worktree, &args.run_id)?)
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        let additional_distill_bytes = event_log_bytes(&event_lines)
            .saturating_add(
                planned_files
                    .iter()
                    .filter(|file| file.counts_against_quota)
                    .map(|file| file.bytes.len() as u64)
                    .fold(0_u64, u64::saturating_add),
            )
            .saturating_add((state_bytes.len() as u64).saturating_sub(current_state_bytes));
        storage::preflight_additional_run_bytes(
            &args.worktree,
            &args.run_id,
            additional_distill_bytes,
            &limits,
        )?;
        append_event_lines(&args.worktree, &args.run_id, &event_lines, &limits)?;
        for file in planned_files {
            write_bytes(&file.path, &file.bytes, file.counts_against_quota)?;
        }
        write_state(&args.worktree, &next_state)?;
        let mut response = response_from_state(&next_state);
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

    if let Some(next_stage) = next_stage_after_snapshot(&next_state, &stage.id)? {
        next_state["current_stage"] = json!(next_stage.id);
        mark_stage_active(&mut next_state, &next_stage.id, next_revision)?;
    } else {
        next_state["state"] = json!("completed");
        next_state["current_stage"] = json!(Value::Null);
        next_state["session_binding"]["released"] = json!(true);
        next_state["session_binding"]["released_revision"] = json!(next_revision);
        next_state["revision"] = json!(next_revision);
        next_state["storage"] = storage::storage_summary(&args.worktree, &limits)?;
        planned_files.extend(plan_completion_report(
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
    let event_lines = build_transition_events(
        &args.worktree,
        &args.run_id,
        next_revision,
        event_payloads,
        &limits,
    )?;

    let state_bytes =
        serde_json::to_vec_pretty(&next_state).map_err(|err| format!("json error: {err}"))?;
    let current_state_bytes = fs::metadata(state_path(&args.worktree, &args.run_id)?)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    let additional_distill_bytes = event_log_bytes(&event_lines)
        .saturating_add(
            planned_files
                .iter()
                .filter(|file| file.counts_against_quota)
                .map(|file| file.bytes.len() as u64)
                .fold(0_u64, u64::saturating_add),
        )
        .saturating_add((state_bytes.len() as u64).saturating_sub(current_state_bytes));
    storage::preflight_additional_run_bytes(
        &args.worktree,
        &args.run_id,
        additional_distill_bytes,
        &limits,
    )?;

    append_event_lines(&args.worktree, &args.run_id, &event_lines, &limits)?;
    for file in planned_files {
        write_bytes(&file.path, &file.bytes, file.counts_against_quota)?;
    }
    write_state(&args.worktree, &next_state)?;
    Ok(response_from_state(&next_state))
}

fn set_project_quota(args: QuotaArgs) -> Result<Value, String> {
    if !args.json {
        return Err("distill requires --json".to_string());
    }
    ensure_worktree(&args.worktree)?;
    storage::set_project_quota(&args.worktree, args.bytes)
}

fn purge_run(args: PurgeArgs) -> Result<Value, String> {
    if !args.json {
        return Err("distill requires --json".to_string());
    }
    if !args.user_authorized {
        return Err("purge requires --user-authorized".to_string());
    }
    ensure_worktree(&args.worktree)?;
    storage::ensure_distill_path_safe(&args.worktree)?;
    let _lock = acquire_run_lock(&args.worktree, &args.run_id)?;
    let mut state =
        read_state_for_update_at_revision(&args.worktree, &args.run_id, args.expected_revision)?;
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
        write_state(&args.worktree, &state)?;
    }
    if env::var("DISTILL_FAIL_PURGE_BEFORE_AUTH_EVENT").as_deref() == Ok(args.run_id.as_str()) {
        return Err("injected purge interruption before authorization event".to_string());
    }
    if !event_type_exists(&args.worktree, &args.run_id, "run-purge-authorized")? {
        append_audit_event(
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
    if !event_type_exists(&args.worktree, &args.run_id, "run-purged")? {
        append_audit_event(
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
    write_state(&args.worktree, &state)?;
    Ok(response_from_state(&state))
}

fn abort_run(args: AbortArgs) -> Result<Value, String> {
    if !args.json {
        return Err("distill requires --json".to_string());
    }
    if !args.user_authorized {
        return Err("abort requires --user-authorized".to_string());
    }
    ensure_worktree(&args.worktree)?;
    let _lock = acquire_run_lock(&args.worktree, &args.run_id)?;
    let mut state =
        read_state_for_update_at_revision(&args.worktree, &args.run_id, args.expected_revision)?;
    ensure_no_pending_purge(&state)?;
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
    write_state(&args.worktree, &state)?;
    let limits = storage::StorageLimits::load(&args.worktree)?;
    append_audit_event(
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
    Ok(response_from_state(&state))
}

fn takeover_run(args: TakeoverArgs) -> Result<Value, String> {
    if !args.json {
        return Err("distill requires --json".to_string());
    }
    if !args.user_authorized {
        return Err("takeover requires --user-authorized".to_string());
    }
    ensure_worktree(&args.worktree)?;
    let _lock = acquire_run_lock(&args.worktree, &args.run_id)?;
    let mut state =
        read_state_for_update_at_revision(&args.worktree, &args.run_id, args.expected_revision)?;
    ensure_no_pending_purge(&state)?;
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
    write_state(&args.worktree, &state)?;
    let limits = storage::StorageLimits::load(&args.worktree)?;
    append_audit_event(
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
    Ok(response_from_state(&state))
}

fn supersede_run(args: SupersedeArgs) -> Result<Value, String> {
    if !args.json {
        return Err("distill requires --json".to_string());
    }
    ensure_worktree(&args.worktree)?;
    let _lock = acquire_run_lock(&args.worktree, &args.run_id)?;
    let mut state =
        read_state_for_update_at_revision(&args.worktree, &args.run_id, args.expected_revision)?;
    ensure_no_pending_purge(&state)?;
    if state["state"] != "active" && state["state"] != "blocked" {
        return Err("run is not active or blocked".to_string());
    }
    if state["session_binding"]["session_id"] != args.session_id {
        return Err("session id does not match active run binding".to_string());
    }

    let predecessor_before_supersession = state.clone();
    let workflow = load_workflow()?;
    let successor_id = create_run_id(&args.session_id);
    let first_stage = next_stage_after(&workflow, "intake")?;
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
        "stages": initial_stage_states(&workflow)?,
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
        "context_baseline": capture_context_baseline(&args.worktree)?,
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
    if let Err(err) = write_state(&args.worktree, &state) {
        rollback_successor(&args.worktree, successor_id.as_str())?;
        return Err(format!("cannot supersede predecessor run: {err}"));
    }

    successor["state"] = json!("active");
    if let Err(err) = write_state(&args.worktree, &successor) {
        let rollback_result = rollback_successor(&args.worktree, successor_id.as_str());
        let restore_result = write_state(&args.worktree, &predecessor_before_supersession);
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
    append_audit_event(
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

fn rollback_successor(worktree: &Path, successor_id: &str) -> Result<(), String> {
    storage::remove_dir_if_exists(&storage::run_dir(worktree, successor_id)?)
        .map_err(|err| format!("cannot roll back successor run: {err}"))
}

fn inspect_run(args: InspectArgs) -> Result<Value, String> {
    if !args.json {
        return Err("distill requires --json".to_string());
    }
    ensure_worktree(&args.worktree)?;
    let _lock = acquire_run_lock(&args.worktree, &args.run_id)?;
    let state =
        read_state_for_update_at_revision(&args.worktree, &args.run_id, args.expected_revision)?;
    if state["session_binding"]["session_id"] != args.session_id {
        return Err("session id does not match run binding".to_string());
    }
    write_state(&args.worktree, &state)?;
    if state["migration_events"]
        .as_array()
        .is_some_and(|events| !events.is_empty())
        && !event_type_exists(&args.worktree, &args.run_id, "state-migrated")?
    {
        let limits = storage::StorageLimits::load(&args.worktree)?;
        append_audit_event(
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
    Ok(response_from_state(&state))
}

fn events_run(args: EventsArgs) -> Result<Value, String> {
    if !args.json {
        return Err("distill requires --json".to_string());
    }
    ensure_worktree(&args.worktree)?;
    let events = read_events_after(&args.worktree, &args.run_id, args.after)?;
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

fn render_report_run(args: RenderReportArgs) -> Result<Value, String> {
    if !args.json {
        return Err("distill requires --json".to_string());
    }
    if args.renderer != "markdown" {
        return Err("only --renderer markdown is supported".to_string());
    }
    ensure_worktree(&args.worktree)?;
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
    let markdown = render_markdown_report(&report)?;
    write_bytes(
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

fn ensure_worktree(worktree: &Path) -> Result<(), String> {
    if !worktree.is_dir() {
        return Err(format!("worktree does not exist: {}", worktree.display()));
    }
    storage::ensure_distill_path_safe(worktree)?;
    Ok(())
}

fn acquire_project_start_lock(worktree: &Path) -> Result<RunLock, String> {
    acquire_lock_file(
        worktree.join(".distill/start.lock"),
        "project start is locked by another writer",
    )
}

fn acquire_run_lock(worktree: &Path, run_id: &str) -> Result<RunLock, String> {
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

fn ensure_no_pending_purge(state: &Value) -> Result<(), String> {
    if state["purge"]["cleanup_state"] == "pending" {
        Err("purge cleanup is pending; resume purge before any other transition".to_string())
    } else {
        Ok(())
    }
}

fn completed_stage_ids(state: &Value) -> Result<Vec<&str>, String> {
    let evidence = state["completion_evidence"]
        .as_array()
        .ok_or("state completion_evidence is invalid")?;
    Ok(evidence
        .iter()
        .filter_map(|entry| entry["stage"].as_str())
        .collect())
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
    if schema_version > CURRENT_SCHEMA_VERSION {
        return Err(format!(
            "state schema {schema_version} is newer than this CLI supports"
        ));
    }
    if schema_version < CURRENT_SCHEMA_VERSION {
        fs::write(
            path.with_file_name(format!("state.schema-{schema_version}.backup.json")),
            original,
        )
        .map_err(|err| format!("cannot write migration backup: {err}"))?;
        migrate_state(&mut state, schema_version)?;
    }
    ensure_current_state_fields(&mut state)?;
    Ok(state)
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

fn capture_context_baseline(worktree: &Path) -> Result<Value, String> {
    let status = git_output(worktree, &["status", "--short"]).unwrap_or_default();
    Ok(json!({
        "captured": true,
        "worktree": worktree.to_string_lossy(),
        "git_head": git_output(worktree, &["rev-parse", "HEAD"]).ok(),
        "git_branch": git_output(worktree, &["branch", "--show-current"]).ok(),
        "git_status": status,
        "dirty": !status.trim().is_empty(),
        "domain_documents": domain_document_hashes(worktree)?,
    }))
}

fn git_output(worktree: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(worktree)
        .output()
        .map_err(|err| format!("cannot run git: {err}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn domain_document_hashes(worktree: &Path) -> Result<Vec<Value>, String> {
    let mut docs = Vec::new();
    for rel in domain_document_paths(worktree)? {
        let path = worktree.join(&rel);
        if path.is_file() {
            reject_symlink_components(worktree, Path::new(&rel))?;
            let content =
                fs::read_to_string(&path).map_err(|err| format!("cannot read {rel}: {err}"))?;
            docs.push(json!({
                "path": rel,
                "hash": stable_hash(&content),
            }));
        }
    }
    Ok(docs)
}

fn domain_document_paths(worktree: &Path) -> Result<Vec<String>, String> {
    let mut paths = vec![
        "CONTEXT.md".to_string(),
        "docs/agents/domain.md".to_string(),
    ];
    let adr_dir = worktree.join("docs/adr");
    if adr_dir.is_dir() {
        for entry in
            fs::read_dir(&adr_dir).map_err(|err| format!("cannot list domain ADRs: {err}"))?
        {
            let entry = entry.map_err(|err| format!("cannot inspect domain ADR: {err}"))?;
            if entry.path().extension().and_then(|ext| ext.to_str()) == Some("md") {
                let rel = format!("docs/adr/{}", entry.file_name().to_string_lossy());
                reject_symlink_components(worktree, Path::new(&rel))?;
                paths.push(rel);
            }
        }
    }
    paths.sort();
    Ok(paths)
}

fn stable_hash(content: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    content.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn validate_context_drift(
    worktree: &Path,
    state: &mut Value,
    stage_id: &str,
    evidence: &Value,
    clarification: Option<&Value>,
) -> Result<(), String> {
    let mut detected = context_drift_details(worktree, &state["context_baseline"])?;
    if stage_id == "clarification" {
        if let Some(clarification) = clarification {
            exclude_owned_domain_drift(&mut detected, &clarification["domain_document_artifacts"])?;
        }
    }
    if detected.as_object().is_some_and(|object| object.is_empty()) {
        return Ok(());
    }

    let ack = &evidence["drift_acknowledgment"];
    let material = ack["material"].as_bool();
    let reason = ack["reason"].as_str().unwrap_or("").trim();
    if material == Some(false) && !reason.is_empty() {
        state["drift_acknowledgments"]
            .as_array_mut()
            .ok_or("state drift_acknowledgments are invalid")?
            .push(json!({
                "stage": stage_id,
                "material": false,
                "reason": reason,
                "detected": detected,
            }));
        return Ok(());
    }
    if material == Some(true) {
        return Err("material context drift requires supersession".to_string());
    }
    Err(format!(
        "context drift detected; submit a reasoned immaterial acknowledgment or supersede the run: {detected}"
    ))
}

fn exclude_owned_domain_drift(detected: &mut Value, artifacts: &Value) -> Result<(), String> {
    let claimed = artifacts
        .as_array()
        .ok_or("clarification domain artifacts are invalid")?
        .iter()
        .filter_map(|artifact| artifact["path"].as_str())
        .collect::<std::collections::HashSet<_>>();
    let object = detected
        .as_object_mut()
        .ok_or("context drift details are invalid")?;
    if let Some(domain) = object.get("domain_documents") {
        let changed_paths = changed_domain_paths(&domain["baseline"], &domain["current"])?;
        if !changed_paths.is_empty()
            && changed_paths
                .iter()
                .all(|path| claimed.contains(path.as_str()))
        {
            object.remove("domain_documents");
        }
    }
    if let Some(status) = object.get("git_status") {
        let baseline = status["baseline"].as_str().unwrap_or("");
        let current = status["current"].as_str().unwrap_or("");
        let baseline_lines = baseline.lines().collect::<std::collections::HashSet<_>>();
        let new_paths = current
            .lines()
            .filter(|line| !baseline_lines.contains(line))
            .filter_map(git_status_path)
            .collect::<Vec<_>>();
        if !new_paths.is_empty() && new_paths.iter().all(|path| claimed.contains(path.as_str())) {
            object.remove("git_status");
        }
    }
    Ok(())
}

fn changed_domain_paths(baseline: &Value, current: &Value) -> Result<Vec<String>, String> {
    let entries = |value: &Value| -> Result<std::collections::HashMap<String, String>, String> {
        Ok(value
            .as_array()
            .ok_or("domain document drift is invalid")?
            .iter()
            .filter_map(|entry| {
                Some((
                    entry["path"].as_str()?.to_string(),
                    entry["hash"].as_str()?.to_string(),
                ))
            })
            .collect())
    };
    let baseline = entries(baseline)?;
    let current = entries(current)?;
    let mut paths = baseline
        .keys()
        .chain(current.keys())
        .filter(|path| baseline.get(*path) != current.get(*path))
        .cloned()
        .collect::<Vec<_>>();
    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn git_status_path(line: &str) -> Option<String> {
    let (_, path) = line.split_once(char::is_whitespace)?;
    let path = path.trim();
    let path = path.rsplit(" -> ").next().unwrap_or(path);
    Some(path.to_string())
}

fn context_drift_details(worktree: &Path, baseline: &Value) -> Result<Value, String> {
    let current = capture_context_baseline(worktree)?;
    let mut drift = serde_json::Map::new();
    for key in ["worktree", "git_head", "git_branch", "git_status"] {
        if baseline[key] != current[key] {
            drift.insert(
                key.to_string(),
                json!({
                    "baseline": baseline[key],
                    "current": current[key],
                }),
            );
        }
    }
    if baseline["domain_documents"] != current["domain_documents"] {
        drift.insert(
            "domain_documents".to_string(),
            json!({
                "baseline": baseline["domain_documents"],
                "current": current["domain_documents"],
            }),
        );
    }
    Ok(Value::Object(drift))
}

fn load_workflow() -> Result<WorkflowDefinition, String> {
    serde_json::from_str(WORKFLOW_JSON).map_err(|err| format!("cannot parse workflow: {err}"))
}

fn initial_stage_states(workflow: &WorkflowDefinition) -> Result<Vec<Value>, String> {
    workflow
        .stages
        .iter()
        .map(|stage| {
            let state = if stage.id == "intake" {
                "completed"
            } else if stage.id == "clarification" {
                "active"
            } else {
                "pending"
            };
            Ok(json!({
                "id": stage.id,
                "state": state,
                "revision": if stage.id == "intake" { 1 } else { 0 },
                "authorized_action": authorized_action(stage),
            }))
        })
        .collect()
}

fn active_run_for_session(worktree: &Path, session_id: &str) -> Result<Option<String>, String> {
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

fn workflow_stage(state: &Value, id: &str) -> Result<WorkflowStage, String> {
    let stages = state["workflow"]["stages"]
        .as_array()
        .ok_or("state workflow stages are invalid")?;
    for stage in stages {
        let parsed: WorkflowStage = serde_json::from_value(stage.clone())
            .map_err(|err| format!("invalid workflow stage snapshot: {err}"))?;
        if parsed.id == id {
            return Ok(parsed);
        }
    }
    Err(format!("workflow stage not found: {id}"))
}

fn next_stage_after(workflow: &WorkflowDefinition, id: &str) -> Result<WorkflowStage, String> {
    let index = workflow
        .stages
        .iter()
        .position(|stage| stage.id == id)
        .ok_or_else(|| format!("workflow stage not found: {id}"))?;
    workflow
        .stages
        .get(index + 1)
        .cloned()
        .ok_or_else(|| format!("workflow has no stage after {id}"))
}

fn next_stage_after_snapshot(state: &Value, id: &str) -> Result<Option<WorkflowStage>, String> {
    let stages = state["workflow"]["stages"]
        .as_array()
        .ok_or("state workflow stages are invalid")?;
    let index = stages
        .iter()
        .position(|stage| stage["id"] == id)
        .ok_or_else(|| format!("workflow stage not found: {id}"))?;
    stages
        .get(index + 1)
        .map(|stage| {
            serde_json::from_value(stage.clone())
                .map_err(|err| format!("invalid workflow stage snapshot: {err}"))
        })
        .transpose()
}

fn authorized_action(stage: &WorkflowStage) -> Value {
    json!({
        "type": stage.next_action,
        "executor": stage.executor,
        "skill": stage.skill,
        "stage": stage.id,
        "expected_checkpoint": stage.checkpoint,
    })
}

fn record_stage_boundary(
    worktree: &Path,
    run_id: &str,
    stage: &WorkflowStage,
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
    let evidence_artifact = plan_evidence_artifact(
        worktree,
        run_id,
        &stage.id,
        next_revision,
        evidence,
        &mut planned_files,
    )?;
    state["storage"] = storage::storage_summary(worktree, limits)?;
    let event_lines = build_transition_events(
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
    let state_bytes =
        serde_json::to_vec_pretty(&state).map_err(|err| format!("json error: {err}"))?;
    let current_state_bytes = fs::metadata(state_path(worktree, run_id)?)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    let additional_distill_bytes = event_log_bytes(&event_lines)
        .saturating_add(
            planned_files
                .iter()
                .map(|file| file.bytes.len() as u64)
                .sum::<u64>(),
        )
        .saturating_add((state_bytes.len() as u64).saturating_sub(current_state_bytes));
    storage::preflight_additional_run_bytes(worktree, run_id, additional_distill_bytes, limits)?;
    append_event_lines(worktree, run_id, &event_lines, limits)?;
    for file in planned_files {
        write_bytes(&file.path, &file.bytes, file.counts_against_quota)?;
    }
    write_state(worktree, &state)?;
    Ok(response_from_state(&state))
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

fn reject_symlink_components(worktree: &Path, relative: &Path) -> Result<(), String> {
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
    let candidates = domain_document_paths(worktree)?;
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
    stage: &WorkflowStage,
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

fn plan_evidence_artifact(
    worktree: &Path,
    run_id: &str,
    stage_id: &str,
    revision: u64,
    evidence: &Value,
    planned_files: &mut Vec<PlannedFile>,
) -> Result<Value, String> {
    storage::validate_run_id(run_id)?;
    let rel = format!(".distill/runs/{run_id}/artifacts/evidence/{stage_id}-r{revision}.json");
    let artifact = json!({
        "schema_version": CURRENT_SCHEMA_VERSION,
        "evidence_version": "distill.evidence.v1",
        "run_id": run_id,
        "stage": stage_id,
        "revision": revision,
        "evidence": evidence,
    });
    let bytes = serde_json::to_vec_pretty(&artifact).map_err(|err| format!("json error: {err}"))?;
    let hash = sha256_hex(&bytes);
    planned_files.push(PlannedFile {
        path: worktree.join(&rel),
        bytes,
        counts_against_quota: true,
    });
    Ok(json!({
        "schema_version": CURRENT_SCHEMA_VERSION,
        "evidence_version": "distill.evidence.v1",
        "artifact_path": rel,
        "sha256": hash,
        "bytes": planned_files.last().map(|file| file.bytes.len()).unwrap_or(0),
    }))
}

fn build_transition_events(
    worktree: &Path,
    run_id: &str,
    revision: u64,
    payloads: Vec<(&'static str, Value)>,
    limits: &storage::StorageLimits,
) -> Result<Vec<String>, String> {
    let first_sequence = next_event_sequence(worktree, run_id)?;
    payloads
        .into_iter()
        .enumerate()
        .map(|(index, (event_type, payload))| {
            run_event(
                run_id,
                first_sequence + index as u64,
                revision,
                event_type,
                payload,
                limits,
            )
        })
        .collect()
}

fn event_log_bytes(event_lines: &[String]) -> u64 {
    event_lines
        .iter()
        .map(|line| line.len() as u64 + 1)
        .fold(0_u64, u64::saturating_add)
}

fn append_event_lines(
    worktree: &Path,
    run_id: &str,
    event_lines: &[String],
    limits: &storage::StorageLimits,
) -> Result<(), String> {
    for line in event_lines {
        let event: Value =
            serde_json::from_str(line).map_err(|err| format!("cannot parse event: {err}"))?;
        storage::append_run_event(worktree, run_id, &event, limits)?;
    }
    Ok(())
}

fn append_audit_event(
    worktree: &Path,
    run_id: &str,
    revision: u64,
    event_type: &str,
    payload: Value,
    limits: &storage::StorageLimits,
) -> Result<(), String> {
    let sequence = next_event_sequence(worktree, run_id)?;
    let line = run_event(run_id, sequence, revision, event_type, payload, limits)?;
    append_event_lines(worktree, run_id, &[line], limits)
}

fn run_event(
    run_id: &str,
    sequence: u64,
    revision: u64,
    event_type: &str,
    payload: Value,
    limits: &storage::StorageLimits,
) -> Result<String, String> {
    let event = json!({
        "schema_version": CURRENT_SCHEMA_VERSION,
        "event_version": 1,
        "sequence": sequence,
        "type": event_type,
        "run_id": run_id,
        "revision": revision,
        "recorded_at": current_timestamp_millis(),
        "payload": payload,
    });
    let line = event.to_string();
    if line.len() > limits.event_bytes {
        return Err(format!("event exceeds {} bytes", limits.event_bytes));
    }
    Ok(line)
}

fn next_event_sequence(worktree: &Path, run_id: &str) -> Result<u64, String> {
    let path = storage::run_dir(worktree, run_id)?.join("events.jsonl");
    let content = fs::read_to_string(&path).unwrap_or_default();
    let mut max_sequence = 0;
    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let event: Value =
            serde_json::from_str(line).map_err(|err| format!("cannot parse event log: {err}"))?;
        max_sequence = max_sequence.max(event["sequence"].as_u64().unwrap_or(max_sequence + 1));
    }
    Ok(max_sequence + 1)
}

fn read_events_after(worktree: &Path, run_id: &str, after: u64) -> Result<Vec<Value>, String> {
    let path = storage::run_dir(worktree, run_id)?.join("events.jsonl");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(&path).map_err(|err| format!("cannot read events: {err}"))?;
    let mut events = Vec::new();
    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let event: Value =
            serde_json::from_str(line).map_err(|err| format!("cannot parse event log: {err}"))?;
        if event["sequence"].as_u64().unwrap_or(0) > after {
            events.push(event);
        }
    }
    Ok(events)
}

fn event_type_exists(worktree: &Path, run_id: &str, event_type: &str) -> Result<bool, String> {
    Ok(read_events_after(worktree, run_id, 0)?
        .iter()
        .any(|event| event["type"] == event_type))
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

fn render_markdown_report(report: &Value) -> Result<String, String> {
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

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn plan_completion_report(
    worktree: &Path,
    run_id: &str,
    state: &mut Value,
) -> Result<Vec<PlannedFile>, String> {
    storage::validate_run_id(run_id)?;
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
        counts_against_quota: true,
    }];
    if !renderer_fails {
        files.push(PlannedFile {
            path: worktree.join(&markdown_rel),
            bytes: render_markdown_report(&report)?.into_bytes(),
            counts_against_quota: true,
        });
    }
    Ok(files)
}

fn response_from_state(state: &Value) -> Value {
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
        workflow_stage(state, &stage)
            .map(|stage| authorized_action(&stage))
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

fn read_state(worktree: &Path, run_id: &str) -> Result<Value, String> {
    let path = state_path(worktree, run_id)?;
    serde_json::from_str(
        &fs::read_to_string(&path).map_err(|err| format!("cannot read state: {err}"))?,
    )
    .map_err(|err| format!("cannot parse state: {err}"))
}

fn write_state(worktree: &Path, state: &Value) -> Result<(), String> {
    let run_id = state["run_id"].as_str().ok_or("state is missing run_id")?;
    if env::var("DISTILL_FAIL_WRITE_STATE_FOR_RUN").as_deref() == Ok(run_id) {
        return Err(format!("injected state write failure for {run_id}"));
    }
    write_json(&state_path(worktree, run_id)?, state)
}

fn state_path(worktree: &Path, run_id: &str) -> Result<PathBuf, String> {
    Ok(storage::run_dir(worktree, run_id)?.join("state.json"))
}

fn write_json(path: &Path, value: &Value) -> Result<(), String> {
    storage::atomic_write_json(path, value, false)
}

fn write_bytes(path: &Path, bytes: &[u8], atomic: bool) -> Result<(), String> {
    if atomic {
        return storage::atomic_write(path, bytes, false);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("cannot create dir: {err}"))?;
    }
    fs::write(path, bytes).map_err(|err| format!("cannot write {}: {err}", path.display()))
}

fn create_run_id(session_id: &str) -> String {
    let millis = current_timestamp_millis();
    let slug: String = session_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .take(24)
        .collect();
    format!("run-{slug}-{millis}")
}

fn current_timestamp_millis() -> u128 {
    if let Ok(value) = env::var("DISTILL_FIXED_TIMESTAMP_MILLIS") {
        if let Ok(parsed) = value.parse() {
            return parsed;
        }
    }
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

fn slugify(value: &str) -> String {
    let mut slug = String::new();
    let mut last_dash = false;
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            slug.push('-');
            last_dash = true;
        }
    }
    slug.trim_matches('-').to_string()
}
