use serde_json::{json, Value};
use std::env;
use std::fs;
use std::fs::OpenOptions;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::storage;
use crate::CURRENT_SCHEMA_VERSION;
use crate::RunLock;

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

pub(crate) fn acquire_lock_file(path: PathBuf, busy_message: &str) -> Result<RunLock, String> {
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

pub(crate) fn validate_expected_revision(state: &Value, expected_revision: u64) -> Result<(), String> {
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

pub(crate) fn ensure_no_pending_purge(state: &Value) -> Result<(), String> {
    if state["purge"]["cleanup_state"] == "pending" {
        Err("purge cleanup is pending; resume purge before any other transition".to_string())
    } else {
        Ok(())
    }
}

pub(crate) fn completed_stage_ids(state: &Value) -> Result<Vec<&str>, String> {
    let evidence = state["completion_evidence"]
        .as_array()
        .ok_or("state completion_evidence is invalid")?;
    Ok(evidence
        .iter()
        .filter_map(|entry| entry["stage"].as_str())
        .collect())
}

pub(crate) fn read_state_for_update_at_revision(
    worktree: &Path,
    run_id: &str,
    expected_revision: u64,
) -> Result<Value, String> {
    read_state_for_update_checked(worktree, run_id, Some(expected_revision))
}

pub(crate) fn read_state_for_update_checked(
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

pub(crate) fn migrate_state(state: &mut Value, from_schema: u64) -> Result<(), String> {
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

pub(crate) fn ensure_current_state_fields(state: &mut Value) -> Result<(), String> {
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

pub(crate) fn ensure_array_field(state: &mut Value, name: &str) -> Result<(), String> {
    if state.get(name).is_none() || state[name].is_null() {
        state[name] = json!([]);
    }
    if !state[name].is_array() {
        return Err(format!("state {name} is invalid"));
    }
    Ok(())
}

pub(crate) fn read_state(worktree: &Path, run_id: &str) -> Result<Value, String> {
    let path = state_path(worktree, run_id)?;
    serde_json::from_str(
        &fs::read_to_string(&path).map_err(|err| format!("cannot read state: {err}"))?,
    )
    .map_err(|err| format!("cannot parse state: {err}"))
}

pub(crate) fn write_state(worktree: &Path, state: &Value) -> Result<(), String> {
    let run_id = state["run_id"].as_str().ok_or("state is missing run_id")?;
    if env::var("DISTILL_FAIL_WRITE_STATE_FOR_RUN").as_deref() == Ok(run_id) {
        return Err(format!("injected state write failure for {run_id}"));
    }
    write_json(&state_path(worktree, run_id)?, state)
}

pub(crate) fn state_path(worktree: &Path, run_id: &str) -> Result<PathBuf, String> {
    Ok(storage::run_dir(worktree, run_id)?.join("state.json"))
}

pub(crate) fn write_json(path: &Path, value: &Value) -> Result<(), String> {
    storage::atomic_write_json(path, value, false)
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
