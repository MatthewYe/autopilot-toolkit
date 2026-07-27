use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use std::process::Command;

use crate::util::stable_hash;

pub(crate) fn capture_context_baseline(worktree: &Path) -> Result<Value, String> {
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
            crate::reject_symlink_components(worktree, Path::new(&rel))?;
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

pub(crate) fn domain_document_paths(worktree: &Path) -> Result<Vec<String>, String> {
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
                crate::reject_symlink_components(worktree, Path::new(&rel))?;
                paths.push(rel);
            }
        }
    }
    paths.sort();
    Ok(paths)
}

pub(crate) fn validate_context_drift(
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
