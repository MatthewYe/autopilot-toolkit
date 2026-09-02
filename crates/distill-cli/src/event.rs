use std::fs;
use std::path::Path;

use serde_json::{json, Value};

use crate::storage::{self, PlannedFile, PlannedWrite};
use crate::util::{current_timestamp_millis, sha256_hex};
use crate::CURRENT_SCHEMA_VERSION;

pub(crate) fn plan_evidence_artifact(
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
        write: PlannedWrite::RunArtifact,
    });
    Ok(json!({
        "schema_version": CURRENT_SCHEMA_VERSION,
        "evidence_version": "distill.evidence.v1",
        "artifact_path": rel,
        "sha256": hash,
        "bytes": planned_files.last().map(|file| file.bytes.len()).unwrap_or(0),
    }))
}

pub(crate) fn build_transition_events(
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

pub(crate) fn event_log_bytes(event_lines: &[String]) -> u64 {
    event_lines
        .iter()
        .map(|line| line.len() as u64 + 1)
        .fold(0_u64, u64::saturating_add)
}

pub(crate) fn append_event_lines(
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

pub(crate) fn append_audit_event(
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

pub(crate) fn run_event(
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

pub(crate) fn next_event_sequence(worktree: &Path, run_id: &str) -> Result<u64, String> {
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

pub(crate) fn read_events_after(worktree: &Path, run_id: &str, after: u64) -> Result<Vec<Value>, String> {
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

pub(crate) fn event_type_exists(worktree: &Path, run_id: &str, event_type: &str) -> Result<bool, String> {
    Ok(read_events_after(worktree, run_id, 0)?
        .iter()
        .any(|event| event["type"] == event_type))
}
