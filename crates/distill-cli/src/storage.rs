use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const DISTILL_IGNORE_RULE: &str = "/.distill/";
pub const PER_SOURCE_BYTES: u64 = 50 * 1024 * 1024;
pub const RUN_BYTES: u64 = 256 * 1024 * 1024;
pub const PROJECT_BYTES: u64 = 2 * 1024 * 1024 * 1024;
pub const EVENT_BYTES: usize = 64 * 1024;
pub const RUN_EVENT_LOG_BYTES: u64 = 32 * 1024 * 1024;

pub(crate) struct PlannedFile {
    pub(crate) path: PathBuf,
    pub(crate) bytes: Vec<u8>,
    pub(crate) counts_against_quota: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct StorageLimits {
    pub per_source_bytes: u64,
    pub run_bytes: u64,
    pub project_bytes: u64,
    pub event_bytes: usize,
    pub run_event_log_bytes: u64,
}

pub struct RunFile {
    pub relative_path: PathBuf,
    pub bytes: Vec<u8>,
}

impl StorageLimits {
    pub fn load(worktree: &Path) -> Result<Self, String> {
        let mut limits = Self::default();
        let path = worktree.join(".distill/quota.json");
        if path.exists() {
            let value: Value = serde_json::from_str(
                &fs::read_to_string(&path)
                    .map_err(|err| format!("cannot read quota config: {err}"))?,
            )
            .map_err(|err| format!("cannot parse quota config: {err}"))?;
            if let Some(project_bytes) = value["project_bytes"].as_u64() {
                limits.project_bytes = project_bytes;
            }
        }
        Ok(limits)
    }
}

impl Default for StorageLimits {
    fn default() -> Self {
        Self {
            per_source_bytes: PER_SOURCE_BYTES,
            run_bytes: RUN_BYTES,
            project_bytes: PROJECT_BYTES,
            event_bytes: EVENT_BYTES,
            run_event_log_bytes: RUN_EVENT_LOG_BYTES,
        }
    }
}

pub fn validate_run_id(run_id: &str) -> Result<(), String> {
    if run_id.is_empty()
        || run_id.len() > 120
        || !run_id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        return Err("run id is unsafe".to_string());
    }
    Ok(())
}

pub fn run_dir(worktree: &Path, run_id: &str) -> Result<PathBuf, String> {
    validate_run_id(run_id)?;
    Ok(worktree.join(".distill/runs").join(run_id))
}

pub fn ensure_distill_ignored(worktree: &Path) -> Result<(), String> {
    let gitignore = worktree.join(".gitignore");
    if let Ok(meta) = fs::symlink_metadata(&gitignore) {
        if meta.file_type().is_symlink() {
            return Err(".gitignore must not be a symlink".to_string());
        }
    }

    let content = match fs::read_to_string(&gitignore) {
        Ok(content) => content,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(err) => return Err(format!("cannot read .gitignore safely: {err}")),
    };
    let ignored = content
        .lines()
        .any(|line| line.trim() == DISTILL_IGNORE_RULE);
    if ignored {
        ensure_distill_path_safe(worktree)?;
        return Ok(());
    }

    if worktree.join(".distill").exists() {
        return Err(".distill already exists before ignore is effective".to_string());
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&gitignore)
        .map_err(|err| format!("cannot establish /.distill/ gitignore: {err}"))?;
    if !content.is_empty() && !content.ends_with('\n') {
        file.write_all(b"\n")
            .map_err(|err| format!("cannot update .gitignore: {err}"))?;
    }
    file.write_all(format!("{DISTILL_IGNORE_RULE}\n").as_bytes())
        .map_err(|err| format!("cannot update .gitignore: {err}"))?;
    ensure_distill_path_safe(worktree)
}

pub fn ensure_distill_path_safe(worktree: &Path) -> Result<(), String> {
    let distill = worktree.join(".distill");
    let meta = match fs::symlink_metadata(&distill) {
        Ok(meta) => meta,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(format!("cannot inspect .distill safely: {err}")),
    };
    if meta.file_type().is_symlink() {
        return Err(".distill must not be a symlink".to_string());
    }
    if !meta.is_dir() {
        return Err(".distill must be a directory".to_string());
    }
    let worktree =
        fs::canonicalize(worktree).map_err(|err| format!("cannot canonicalize worktree: {err}"))?;
    let distill =
        fs::canonicalize(&distill).map_err(|err| format!("cannot canonicalize .distill: {err}"))?;
    if !distill.starts_with(&worktree) {
        return Err(".distill must stay inside the worktree".to_string());
    }
    Ok(())
}

pub fn preflight_quota(
    worktree: &Path,
    run_bytes: u64,
    limits: &StorageLimits,
) -> Result<(), String> {
    if run_bytes > limits.run_bytes {
        return Err(format!(
            "run quota exceeded: {run_bytes} > {}",
            limits.run_bytes
        ));
    }
    let current = project_usage(worktree)?;
    if current.saturating_add(run_bytes) > limits.project_bytes {
        return Err(format!(
            "project quota exceeded: {} > {}",
            current.saturating_add(run_bytes),
            limits.project_bytes
        ));
    }
    Ok(())
}

pub fn preflight_additional_run_bytes(
    worktree: &Path,
    run_id: &str,
    additional_bytes: u64,
    limits: &StorageLimits,
) -> Result<(), String> {
    let run_usage = dir_usage(&run_dir(worktree, run_id)?)?;
    let next_run_usage = run_usage.saturating_add(additional_bytes);
    if next_run_usage > limits.run_bytes {
        return Err(format!(
            "run quota exceeded: {next_run_usage} > {}",
            limits.run_bytes
        ));
    }
    let project_usage = project_usage(worktree)?;
    let next_project_usage = project_usage.saturating_add(additional_bytes);
    if next_project_usage > limits.project_bytes {
        return Err(format!(
            "project quota exceeded: {next_project_usage} > {}",
            limits.project_bytes
        ));
    }
    Ok(())
}

pub fn commit_new_run(
    worktree: &Path,
    run_id: &str,
    files: Vec<RunFile>,
    limits: &StorageLimits,
) -> Result<(), String> {
    validate_run_id(run_id)?;
    let total_bytes = files.iter().try_fold(0_u64, |total, file| {
        validate_relative_run_file(&file.relative_path)?;
        Ok::<u64, String>(total.saturating_add(file.bytes.len() as u64))
    })?;
    preflight_quota(worktree, total_bytes, limits)?;

    let runs_dir = worktree.join(".distill/runs");
    let final_dir = run_dir(worktree, run_id)?;
    if final_dir.exists() {
        return Err("run already exists".to_string());
    }
    let staging_dir = worktree
        .join(".distill/staging")
        .join(format!("{run_id}.{}", now_millis()));
    let result = (|| {
        for file in &files {
            let path = staging_dir.join(&file.relative_path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|err| format!("cannot create staged run dir: {err}"))?;
            }
            fs::write(&path, &file.bytes)
                .map_err(|err| format!("cannot write staged run file {}: {err}", path.display()))?;
        }
        fs::create_dir_all(&runs_dir).map_err(|err| format!("cannot create runs dir: {err}"))?;
        fs::rename(&staging_dir, &final_dir)
            .map_err(|err| format!("cannot commit staged run: {err}"))
    })();
    if result.is_err() {
        fs::remove_dir_all(&staging_dir).ok();
    }
    result
}

pub fn set_project_quota(worktree: &Path, bytes: u64) -> Result<Value, String> {
    ensure_distill_ignored(worktree)?;
    let usage = project_usage(worktree)?;
    if bytes < usage {
        return Err(format!(
            "cannot set project quota below current usage: {bytes} < {usage}"
        ));
    }
    let path = worktree.join(".distill/quota.json");
    atomic_write_json(&path, &json!({ "project_bytes": bytes }), false)?;
    let event = json!({
        "schema_version": 1,
        "event_version": 1,
        "type": "project-quota-changed",
        "new_project_bytes": bytes,
        "usage_bytes": usage,
        "recorded_at": now_millis(),
    });
    append_line(
        &worktree.join(".distill/quota-events.jsonl"),
        &event.to_string(),
    )?;
    let limits = StorageLimits::load(worktree)?;
    Ok(json!({
        "status": "ok",
        "storage": storage_summary(worktree, &limits)?,
    }))
}

pub fn storage_summary(worktree: &Path, limits: &StorageLimits) -> Result<Value, String> {
    Ok(json!({
        "limits": limits,
        "usage": {
            "project_bytes": project_usage(worktree)?,
        }
    }))
}

pub fn append_run_event(
    worktree: &Path,
    run_id: &str,
    event: &Value,
    limits: &StorageLimits,
) -> Result<(), String> {
    validate_run_id(run_id)?;
    let line = event.to_string();
    if line.len() > limits.event_bytes {
        return Err(format!("event exceeds {} bytes", limits.event_bytes));
    }
    let path = worktree
        .join(".distill/runs")
        .join(run_id)
        .join("events.jsonl");
    let existing = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    let next = existing.saturating_add(line.len() as u64 + 1);
    if next > limits.run_event_log_bytes {
        return Err(format!(
            "run event log quota exceeded: {next} > {}",
            limits.run_event_log_bytes
        ));
    }
    append_line(&path, &line)
}

pub fn atomic_write_json(path: &Path, value: &Value, create_new: bool) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|err| format!("json error: {err}"))?;
    atomic_write(path, &bytes, create_new)
}

pub fn atomic_write(path: &Path, bytes: &[u8], create_new: bool) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("cannot create {}: {err}", parent.display()))?;
    }
    if create_new && path.exists() {
        return Err(format!("immutable file already exists: {}", path.display()));
    }
    let tmp = tmp_path(path);
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    let mut file = options
        .open(&tmp)
        .map_err(|err| format!("cannot create temp file {}: {err}", tmp.display()))?;
    file.write_all(bytes)
        .map_err(|err| format!("cannot write temp file {}: {err}", tmp.display()))?;
    file.sync_all()
        .map_err(|err| format!("cannot sync temp file {}: {err}", tmp.display()))?;
    if create_new {
        match fs::hard_link(&tmp, path) {
            Ok(()) => {
                fs::remove_file(&tmp).ok();
                Ok(())
            }
            Err(err) => {
                fs::remove_file(&tmp).ok();
                Err(format!(
                    "cannot create immutable file {}: {err}",
                    path.display()
                ))
            }
        }
    } else {
        fs::rename(&tmp, path).map_err(|err| format!("cannot replace {}: {err}", path.display()))
    }
}

pub fn gzip_bytes(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder
        .write_all(bytes)
        .map_err(|err| format!("cannot compress source bytes: {err}"))?;
    encoder
        .finish()
        .map_err(|err| format!("cannot finish source compression: {err}"))
}

pub fn remove_dir_if_exists(path: &Path) -> Result<(), String> {
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(format!("cannot remove {}: {err}", path.display())),
    }
}

fn validate_relative_run_file(path: &Path) -> Result<(), String> {
    if path.as_os_str().is_empty() || path.is_absolute() {
        return Err("run file path is unsafe".to_string());
    }
    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            _ => return Err("run file path is unsafe".to_string()),
        }
    }
    Ok(())
}

fn append_line(path: &Path, line: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("cannot create {}: {err}", parent.display()))?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|err| format!("cannot open {}: {err}", path.display()))?;
    file.write_all(line.as_bytes())
        .and_then(|_| file.write_all(b"\n"))
        .map_err(|err| format!("cannot append {}: {err}", path.display()))
}

fn project_usage(worktree: &Path) -> Result<u64, String> {
    dir_usage(&worktree.join(".distill"))
}

fn dir_usage(path: &Path) -> Result<u64, String> {
    let meta = match fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(err) => return Err(format!("cannot inspect {}: {err}", path.display())),
    };
    if meta.file_type().is_symlink() {
        return Err(format!("refusing to measure symlink {}", path.display()));
    }
    if meta.is_file() {
        return Ok(meta.len());
    }
    let mut total = 0_u64;
    for entry in
        fs::read_dir(path).map_err(|err| format!("cannot list {}: {err}", path.display()))?
    {
        let entry = entry.map_err(|err| format!("cannot inspect usage entry: {err}"))?;
        total = total.saturating_add(dir_usage(&entry.path())?);
    }
    Ok(total)
}

fn tmp_path(path: &Path) -> PathBuf {
    let file = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("distill");
    path.with_file_name(format!(".{file}.{}.tmp", now_millis()))
}

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}
