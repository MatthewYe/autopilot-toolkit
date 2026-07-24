use crate::{slugify, storage, PlannedFile};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Eq, PartialEq)]
enum TrackerKind {
    LocalMarkdown,
    Fake,
    GitHub { repository: String },
}

#[derive(Deserialize)]
struct IssuePayload {
    title: String,
    body: String,
    #[serde(default)]
    depends_on: Vec<usize>,
    external_publication: Option<Value>,
}

#[derive(Deserialize)]
struct GitHubPublicationEvidence {
    tracker: String,
    repository: String,
    operation_id: String,
    payload_hash: String,
    status: String,
    artifact_id: Option<u64>,
    artifact_url: Option<String>,
}

struct PublicationItem {
    operation_id: String,
    kind: String,
    title: Option<String>,
    body: String,
    fallback_path: String,
    dependency_indices: Vec<usize>,
    external_publication: Option<Value>,
}

struct AdapterResult {
    artifact_id: String,
    artifact_path: String,
}

pub(crate) struct PublicationOutcome {
    pub files: Vec<PlannedFile>,
    pub blocked: Option<String>,
}

pub(crate) fn validate_tracker_config(worktree: &Path) -> Result<(), String> {
    tracker_kind(worktree).map(|_| ())
}

pub(crate) fn publish_prd(
    worktree: &Path,
    run_id: &str,
    revision: u64,
    state: &mut Value,
    evidence: &Value,
) -> Result<PublicationOutcome, String> {
    storage::validate_run_id(run_id)?;
    let markdown = evidence["prd_markdown"]
        .as_str()
        .ok_or("prd evidence requires prd_markdown")?;
    let item = PublicationItem {
        operation_id: state["publications"]["prd"]["operation_id"]
            .as_str()
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| format!("{run_id}-r{revision}-prd")),
        kind: "prd".to_string(),
        title: None,
        body: markdown.to_string(),
        fallback_path: ".scratch/distill-tracer/PRD.md".to_string(),
        dependency_indices: Vec::new(),
        external_publication: evidence.get("external_publication").cloned(),
    };
    let mut artifact_ids = Vec::new();
    let outcome = publish_item(worktree, run_id, revision, &item, &artifact_ids)?;
    artifact_ids.push(outcome.artifact_id.clone());
    state["publications"]["prd"] = publication_state(&outcome);
    Ok(PublicationOutcome {
        files: outcome.files,
        blocked: outcome.blocked,
    })
}

pub(crate) fn publish_issues(
    worktree: &Path,
    run_id: &str,
    revision: u64,
    state: &mut Value,
    evidence: &Value,
) -> Result<PublicationOutcome, String> {
    storage::validate_run_id(run_id)?;
    let issues: Vec<IssuePayload> = serde_json::from_value(evidence["issues"].clone())
        .map_err(|err| format!("issues evidence must include issue objects: {err}"))?;
    if issues.is_empty() {
        return Err("issues evidence must include at least one issue".to_string());
    }

    let mut files = Vec::new();
    let mut publications = Vec::new();
    let mut artifact_ids: Vec<Option<String>> = vec![None; issues.len()];
    let mut blocked = None;

    for (index, issue) in issues.iter().enumerate() {
        if blocked.is_some() {
            publications.push(existing_or_pending_issue_state(
                run_id, revision, index, issue,
            ));
            continue;
        }
        let filename = format!("{:02}-{}.md", index + 1, slugify(&issue.title));
        let item = PublicationItem {
            operation_id: state["publications"]["issues"]
                .as_array()
                .and_then(|publications| publications.get(index))
                .and_then(|publication| publication["operation_id"].as_str())
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| format!("{run_id}-r{revision}-issue-{:02}", index + 1)),
            kind: "issue".to_string(),
            title: Some(issue.title.clone()),
            body: issue.body.clone(),
            fallback_path: format!(".scratch/distill-tracer/issues/{filename}"),
            dependency_indices: issue.depends_on.clone(),
            external_publication: issue.external_publication.clone(),
        };
        let known_dependencies = dependency_ids(&artifact_ids, &item.dependency_indices)?;
        let outcome = publish_item(worktree, run_id, revision, &item, &known_dependencies)?;
        let publication = publication_state(&outcome);
        files.extend(outcome.files);
        publications.push(publication);
        artifact_ids[index] = Some(outcome.artifact_id.clone());
        if let Some(reason) = outcome.blocked {
            blocked = Some(reason);
        }
    }

    state["publications"]["issues"] = json!(publications);
    Ok(PublicationOutcome { files, blocked })
}

struct PublishedItem {
    operation_id: String,
    artifact_id: String,
    artifact_path: String,
    payload_path: String,
    payload_hash: String,
    title: Option<String>,
    dependency_artifact_ids: Vec<String>,
    tracker: String,
    files: Vec<PlannedFile>,
    blocked: Option<String>,
}

fn publish_item(
    worktree: &Path,
    run_id: &str,
    revision: u64,
    item: &PublicationItem,
    dependency_artifact_ids: &[String],
) -> Result<PublishedItem, String> {
    let tracker = tracker_kind(worktree)?;
    let tracker_name = tracker_name(&tracker).to_string();
    let payload_rel = format!(
        ".distill/runs/{run_id}/publication/payloads/{}.payload",
        item.operation_id
    );
    let parsed_revision = operation_revision(&item.operation_id);
    let operation_revision = if parsed_revision == 0 {
        revision
    } else {
        parsed_revision
    };
    let payload_bytes = item.body.as_bytes().to_vec();
    let payload_hash = sha256_hex(&payload_bytes);
    let payload_path = worktree.join(&payload_rel);
    if payload_path.exists() {
        let frozen_bytes = fs::read(&payload_path)
            .map_err(|err| format!("cannot read frozen publication payload: {err}"))?;
        if sha256_hex(&frozen_bytes) != payload_hash {
            return Err("frozen publication payload changed for stable operation".to_string());
        }
    } else {
        storage::atomic_write(&payload_path, &payload_bytes, false)?;
    }

    if let Some(record) = read_record(worktree, run_id, &item.operation_id)? {
        if record["payload_path"].as_str() != Some(payload_rel.as_str()) {
            return Err(
                "publication record payload_path does not match stable operation".to_string(),
            );
        }
        return resume_record(worktree, run_id, item, &record, &payload_hash);
    }

    write_record(
        worktree,
        run_id,
        &item.operation_id,
        &json!({
            "operation_id": item.operation_id,
            "run_id": run_id,
            "revision": operation_revision,
            "kind": item.kind,
            "title": item.title,
            "payload_path": payload_rel,
            "payload_hash": payload_hash,
            "dependency_artifact_ids": dependency_artifact_ids,
            "tracker": tracker_name,
            "status": "intent",
        }),
    )?;

    match publish_via_tracker(worktree, &tracker, item, &payload_hash) {
        Ok(result) => {
            let published = PublishedItem {
                operation_id: item.operation_id.clone(),
                artifact_id: result.artifact_id,
                artifact_path: result.artifact_path,
                payload_path: payload_rel,
                payload_hash,
                title: item.title.clone(),
                dependency_artifact_ids: dependency_artifact_ids.to_vec(),
                tracker: tracker_name,
                files: local_projection_file(worktree, &tracker, item),
                blocked: None,
            };
            write_confirmed_record(worktree, run_id, &published)?;
            Ok(published)
        }
        Err(PublishFailure::Unavailable(message)) => Err(message),
        Err(PublishFailure::Uncertain(message)) => {
            write_record(
                worktree,
                run_id,
                &item.operation_id,
                &json!({
                    "operation_id": item.operation_id,
                    "run_id": run_id,
                    "revision": operation_revision,
                    "kind": item.kind,
                    "title": item.title,
                    "payload_path": payload_rel,
                    "payload_hash": payload_hash,
                    "dependency_artifact_ids": dependency_artifact_ids,
                    "tracker": tracker_name,
                    "status": "needs-reconciliation",
                    "reason": message,
                }),
            )?;
            Ok(PublishedItem {
                operation_id: item.operation_id.clone(),
                artifact_id: String::new(),
                artifact_path: String::new(),
                payload_path: payload_rel,
                payload_hash,
                title: item.title.clone(),
                dependency_artifact_ids: dependency_artifact_ids.to_vec(),
                tracker: tracker_name,
                files: Vec::new(),
                blocked: Some(message),
            })
        }
    }
}

fn resume_record(
    worktree: &Path,
    run_id: &str,
    item: &PublicationItem,
    record: &Value,
    payload_hash: &str,
) -> Result<PublishedItem, String> {
    if record["payload_hash"] != payload_hash {
        return Err("frozen publication payload changed for stable operation".to_string());
    }
    let tracker = tracker_kind(worktree)?;
    validate_record_tracker(record, &tracker)?;
    if record["status"] == "confirmed" {
        let artifact_id = record["artifact_id"]
            .as_str()
            .ok_or("confirmed publication record is missing artifact_id")?;
        let artifact_path = record["artifact_path"]
            .as_str()
            .ok_or("confirmed publication record is missing artifact_path")?;
        verify_confirmed_artifact(worktree, &tracker, artifact_id, artifact_path, payload_hash)?;
        return Ok(PublishedItem {
            operation_id: item.operation_id.clone(),
            artifact_id: artifact_id.to_string(),
            artifact_path: artifact_path.to_string(),
            payload_path: record["payload_path"].as_str().unwrap_or("").to_string(),
            payload_hash: payload_hash.to_string(),
            title: item.title.clone(),
            dependency_artifact_ids: strings_from_array(&record["dependency_artifact_ids"]),
            tracker: tracker_name(&tracker).to_string(),
            files: Vec::new(),
            blocked: None,
        });
    }

    if record["status"] == "intent" {
        let dependency_artifact_ids = strings_from_array(&record["dependency_artifact_ids"]);
        match publish_via_tracker(worktree, &tracker, item, payload_hash) {
            Ok(result) => {
                let published = PublishedItem {
                    operation_id: item.operation_id.clone(),
                    artifact_id: result.artifact_id,
                    artifact_path: result.artifact_path,
                    payload_path: record["payload_path"].as_str().unwrap_or("").to_string(),
                    payload_hash: payload_hash.to_string(),
                    title: item.title.clone(),
                    dependency_artifact_ids,
                    tracker: tracker_name(&tracker).to_string(),
                    files: local_projection_file(worktree, &tracker, item),
                    blocked: None,
                };
                write_confirmed_record(worktree, run_id, &published)?;
                return Ok(published);
            }
            Err(PublishFailure::Unavailable(message)) => return Err(message),
            Err(PublishFailure::Uncertain(message)) => {
                write_record(
                    worktree,
                    run_id,
                    &item.operation_id,
                    &json!({
                        "operation_id": item.operation_id,
                        "run_id": run_id,
                        "revision": operation_revision(&item.operation_id),
                        "kind": item.kind,
                        "title": item.title,
                        "payload_path": record["payload_path"],
                        "payload_hash": payload_hash,
                        "dependency_artifact_ids": dependency_artifact_ids,
                        "tracker": tracker_name(&tracker),
                        "status": "needs-reconciliation",
                        "reason": message,
                    }),
                )?;
                return Ok(PublishedItem {
                    operation_id: item.operation_id.clone(),
                    artifact_id: String::new(),
                    artifact_path: String::new(),
                    payload_path: record["payload_path"].as_str().unwrap_or("").to_string(),
                    payload_hash: payload_hash.to_string(),
                    title: item.title.clone(),
                    dependency_artifact_ids,
                    tracker: tracker_name(&tracker).to_string(),
                    files: Vec::new(),
                    blocked: Some(message),
                });
            }
        }
    }

    if record["status"] == "needs-reconciliation" {
        let fake_result = if tracker == TrackerKind::Fake {
            find_fake_artifact_by_operation(worktree, &item.operation_id)?
        } else {
            None
        };
        if let Some(result) = fake_result {
            verify_artifact(worktree, &result.artifact_path, payload_hash)?;
            let published = PublishedItem {
                operation_id: item.operation_id.clone(),
                artifact_id: result.artifact_id,
                artifact_path: result.artifact_path,
                payload_path: record["payload_path"].as_str().unwrap_or("").to_string(),
                payload_hash: payload_hash.to_string(),
                title: item.title.clone(),
                dependency_artifact_ids: strings_from_array(&record["dependency_artifact_ids"]),
                tracker: tracker_name(&tracker).to_string(),
                files: Vec::new(),
                blocked: None,
            };
            write_confirmed_record(worktree, run_id, &published)?;
            return Ok(published);
        }
        let dependency_artifact_ids = strings_from_array(&record["dependency_artifact_ids"]);
        match publish_via_tracker(worktree, &tracker, item, payload_hash) {
            Ok(result) => {
                let published = PublishedItem {
                    operation_id: item.operation_id.clone(),
                    artifact_id: result.artifact_id,
                    artifact_path: result.artifact_path,
                    payload_path: record["payload_path"].as_str().unwrap_or("").to_string(),
                    payload_hash: payload_hash.to_string(),
                    title: item.title.clone(),
                    dependency_artifact_ids,
                    tracker: tracker_name(&tracker).to_string(),
                    files: local_projection_file(worktree, &tracker, item),
                    blocked: None,
                };
                write_confirmed_record(worktree, run_id, &published)?;
                return Ok(published);
            }
            Err(PublishFailure::Unavailable(message)) => return Err(message),
            Err(PublishFailure::Uncertain(message)) => {
                return Ok(PublishedItem {
                    operation_id: item.operation_id.clone(),
                    artifact_id: String::new(),
                    artifact_path: String::new(),
                    payload_path: record["payload_path"].as_str().unwrap_or("").to_string(),
                    payload_hash: payload_hash.to_string(),
                    title: item.title.clone(),
                    dependency_artifact_ids,
                    tracker: tracker_name(&tracker).to_string(),
                    files: Vec::new(),
                    blocked: Some(message),
                });
            }
        }
    }

    Err("publication record has unknown status".to_string())
}

fn write_confirmed_record(
    worktree: &Path,
    run_id: &str,
    published: &PublishedItem,
) -> Result<(), String> {
    write_record(
        worktree,
        run_id,
        &published.operation_id,
        &json!({
            "operation_id": published.operation_id,
            "run_id": run_id,
            "revision": operation_revision(&published.operation_id),
            "kind": if published.title.is_some() { "issue" } else { "prd" },
            "title": published.title,
            "payload_path": published.payload_path,
            "payload_hash": published.payload_hash,
            "dependency_artifact_ids": published.dependency_artifact_ids,
            "tracker": published.tracker,
            "status": "confirmed",
            "artifact_id": published.artifact_id,
            "artifact_path": published.artifact_path,
        }),
    )
}

fn publish_via_tracker(
    worktree: &Path,
    tracker: &TrackerKind,
    item: &PublicationItem,
    payload_hash: &str,
) -> Result<AdapterResult, PublishFailure> {
    match tracker {
        TrackerKind::LocalMarkdown => {
            let path = item.fallback_path.clone();
            Ok(AdapterResult {
                artifact_id: path.clone(),
                artifact_path: path,
            })
        }
        TrackerKind::Fake => fake_publish(worktree, item, payload_hash),
        TrackerKind::GitHub { repository } => github_publish(repository, item, payload_hash),
    }
}

enum PublishFailure {
    Unavailable(String),
    Uncertain(String),
}

fn github_publish(
    configured_repository: &str,
    item: &PublicationItem,
    payload_hash: &str,
) -> Result<AdapterResult, PublishFailure> {
    let Some(raw_evidence) = item.external_publication.as_ref() else {
        return Err(PublishFailure::Uncertain(
            "github publication requires confirmed external evidence".to_string(),
        ));
    };
    let evidence: GitHubPublicationEvidence = serde_json::from_value(raw_evidence.clone())
        .map_err(|err| {
            PublishFailure::Unavailable(format!("github publication evidence is invalid: {err}"))
        })?;
    if evidence.tracker != "github" {
        return Err(PublishFailure::Unavailable(
            "github publication evidence tracker must be github".to_string(),
        ));
    }
    if evidence.repository != configured_repository {
        return Err(PublishFailure::Unavailable(
            "github publication evidence repository does not match configured tracker".to_string(),
        ));
    }
    if evidence.operation_id != item.operation_id {
        return Err(PublishFailure::Unavailable(
            "github publication evidence operation_id does not match stable operation".to_string(),
        ));
    }
    if evidence.payload_hash != payload_hash {
        return Err(PublishFailure::Unavailable(
            "github publication evidence payload_hash does not match frozen payload".to_string(),
        ));
    }
    if evidence.status != "confirmed" {
        return Err(PublishFailure::Uncertain(
            "github publication requires confirmed external evidence".to_string(),
        ));
    }

    let issue_number = evidence
        .artifact_id
        .filter(|number| *number > 0)
        .ok_or_else(|| {
            PublishFailure::Unavailable(
                "confirmed github publication evidence requires a positive artifact_id".to_string(),
            )
        })?;
    let artifact_id = github_artifact_id(configured_repository, issue_number);
    let artifact_path = github_artifact_url(configured_repository, issue_number);
    if evidence.artifact_url.as_deref() != Some(artifact_path.as_str()) {
        return Err(PublishFailure::Unavailable(
            "github publication evidence artifact_url is not canonical".to_string(),
        ));
    }

    Ok(AdapterResult {
        artifact_id,
        artifact_path,
    })
}

fn fake_publish(
    worktree: &Path,
    item: &PublicationItem,
    _payload_hash: &str,
) -> Result<AdapterResult, PublishFailure> {
    match env::var("DISTILL_FAKE_TRACKER_MODE").as_deref() {
        Ok("outage") => {
            return Err(PublishFailure::Unavailable(
                "configured tracker unavailable".to_string(),
            ))
        }
        Ok("partial-batch") if item.operation_id.ends_with("issue-02") => {
            return Err(PublishFailure::Uncertain(
                "publication needs-reconciliation".to_string(),
            ))
        }
        _ => {}
    }

    let artifact_id = format!("fake-{}", item.operation_id);
    let artifact_path = fake_artifact_rel(&artifact_id);
    let path = worktree.join(&artifact_path);
    if !path.exists() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                PublishFailure::Unavailable(format!("cannot create fake tracker dir: {err}"))
            })?;
        }
        fs::write(&path, item.body.as_bytes()).map_err(|err| {
            PublishFailure::Unavailable(format!("cannot write fake tracker artifact: {err}"))
        })?;
        storage::atomic_write(
            &worktree
                .join(".fake-tracker/hashes")
                .join(format!("{artifact_id}.sha256")),
            sha256_hex(item.body.as_bytes()).as_bytes(),
            false,
        )
        .map_err(PublishFailure::Unavailable)?;
        append_fake_log(worktree, item)?;
    }

    if env::var("DISTILL_FAKE_TRACKER_MODE").as_deref() == Ok("timeout-before-response") {
        return Err(PublishFailure::Uncertain(
            "publication needs-reconciliation".to_string(),
        ));
    }

    Ok(AdapterResult {
        artifact_id,
        artifact_path,
    })
}

fn append_fake_log(worktree: &Path, item: &PublicationItem) -> Result<(), PublishFailure> {
    let path = worktree.join(".fake-tracker/create-log.jsonl");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| PublishFailure::Unavailable(format!("cannot create fake log: {err}")))?;
    }
    let mut content = fs::read_to_string(&path).unwrap_or_default();
    content.push_str(
        &json!({
            "operation_id": item.operation_id,
            "kind": item.kind,
        })
        .to_string(),
    );
    content.push('\n');
    fs::write(path, content)
        .map_err(|err| PublishFailure::Unavailable(format!("cannot append fake log: {err}")))
}

fn find_fake_artifact_by_operation(
    worktree: &Path,
    operation_id: &str,
) -> Result<Option<AdapterResult>, String> {
    let artifact_id = format!("fake-{operation_id}");
    let artifact_path = fake_artifact_rel(&artifact_id);
    if worktree.join(&artifact_path).is_file() {
        Ok(Some(AdapterResult {
            artifact_id,
            artifact_path,
        }))
    } else {
        Ok(None)
    }
}

fn verify_artifact(worktree: &Path, artifact_path: &str, payload_hash: &str) -> Result<(), String> {
    let content = fs::read(worktree.join(artifact_path))
        .map_err(|err| format!("cannot verify tracker artifact: {err}"))?;
    let content_hash = sha256_hex(&content);
    if content_hash != payload_hash {
        return Err("tracker drift detected against frozen payload".to_string());
    }
    let expected = worktree
        .join(".fake-tracker/hashes")
        .join(format!("{}.sha256", artifact_id_from_path(artifact_path)));
    if expected.is_file() {
        let original_payload_hash = fs::read_to_string(expected)
            .map_err(|err| format!("cannot read tracker hash: {err}"))?;
        if original_payload_hash != content_hash {
            return Err("tracker drift detected against frozen payload".to_string());
        }
    }
    Ok(())
}

fn verify_confirmed_artifact(
    worktree: &Path,
    tracker: &TrackerKind,
    artifact_id: &str,
    artifact_path: &str,
    payload_hash: &str,
) -> Result<(), String> {
    match tracker {
        TrackerKind::GitHub { repository } => {
            let prefix = format!("github:{repository}#");
            let issue_number = artifact_id
                .strip_prefix(&prefix)
                .and_then(|number| number.parse::<u64>().ok())
                .filter(|number| *number > 0)
                .ok_or("confirmed github publication record has invalid artifact_id")?;
            if artifact_id != github_artifact_id(repository, issue_number)
                || artifact_path != github_artifact_url(repository, issue_number)
            {
                return Err(
                    "confirmed github publication record has non-canonical artifact identity"
                        .to_string(),
                );
            }
            Ok(())
        }
        TrackerKind::LocalMarkdown | TrackerKind::Fake => {
            verify_artifact(worktree, artifact_path, payload_hash)
        }
    }
}

fn local_projection_file(
    worktree: &Path,
    tracker: &TrackerKind,
    item: &PublicationItem,
) -> Vec<PlannedFile> {
    if *tracker == TrackerKind::LocalMarkdown {
        vec![PlannedFile {
            path: worktree.join(&item.fallback_path),
            bytes: item.body.as_bytes().to_vec(),
            counts_against_quota: false,
        }]
    } else {
        Vec::new()
    }
}

fn publication_state(item: &PublishedItem) -> Value {
    json!({
        "operation_id": item.operation_id,
        "artifact_id": item.artifact_id,
        "path": item.artifact_path,
        "payload_path": item.payload_path,
        "payload_hash": item.payload_hash,
        "dependency_artifact_ids": item.dependency_artifact_ids,
        "tracker": item.tracker,
        "status": if item.blocked.is_some() { "needs-reconciliation" } else { "confirmed" },
        "title": item.title,
    })
}

fn existing_or_pending_issue_state(
    run_id: &str,
    revision: u64,
    index: usize,
    issue: &IssuePayload,
) -> Value {
    json!({
        "operation_id": format!("{run_id}-r{revision}-issue-{:02}", index + 1),
        "title": issue.title,
        "status": "pending",
    })
}

fn dependency_ids(
    artifact_ids: &[Option<String>],
    dependency_indices: &[usize],
) -> Result<Vec<String>, String> {
    let mut dependencies = Vec::new();
    for index in dependency_indices {
        let id = artifact_ids
            .get(*index)
            .and_then(|id| id.as_ref())
            .ok_or("issue dependency has not been published yet")?;
        dependencies.push(id.clone());
    }
    Ok(dependencies)
}

fn read_record(worktree: &Path, run_id: &str, operation_id: &str) -> Result<Option<Value>, String> {
    let path = record_path(worktree, run_id, operation_id);
    if !path.exists() {
        return Ok(None);
    }
    serde_json::from_str(
        &fs::read_to_string(path)
            .map_err(|err| format!("cannot read publication record: {err}"))?,
    )
    .map(Some)
    .map_err(|err| format!("cannot parse publication record: {err}"))
}

fn write_record(
    worktree: &Path,
    run_id: &str,
    operation_id: &str,
    record: &Value,
) -> Result<(), String> {
    storage::atomic_write_json(&record_path(worktree, run_id, operation_id), record, false)
}

fn record_path(worktree: &Path, run_id: &str, operation_id: &str) -> PathBuf {
    worktree
        .join(".distill/runs")
        .join(run_id)
        .join("publication/records")
        .join(format!("{operation_id}.json"))
}

fn tracker_kind(worktree: &Path) -> Result<TrackerKind, String> {
    let issue_tracker = worktree.join("docs/agents/issue-tracker.md");
    let content = fs::read_to_string(&issue_tracker)
        .map_err(|_| "docs/agents/issue-tracker.md is required".to_string())?;
    if content.contains("Issue tracker: GitHub") {
        let repository = configured_github_repository(&content)?;
        return Ok(TrackerKind::GitHub { repository });
    }
    if content.contains("Fake Adapter") {
        return Ok(TrackerKind::Fake);
    }
    if content.contains("Local Markdown") || content.contains(".scratch") {
        return Ok(TrackerKind::LocalMarkdown);
    }
    Err("configured issue tracker is unsupported".to_string())
}

fn configured_github_repository(content: &str) -> Result<String, String> {
    let marker = "GitHub issues on `";
    let tail = content
        .split_once(marker)
        .map(|(_, tail)| tail)
        .ok_or("github tracker config must declare `owner/repository`")?;
    let repository = tail
        .split_once('`')
        .map(|(repository, _)| repository)
        .ok_or("github tracker config must declare `owner/repository`")?;
    let mut parts = repository.split('/');
    let owner = parts.next().unwrap_or_default();
    let name = parts.next().unwrap_or_default();
    if parts.next().is_some()
        || !valid_github_component(owner)
        || !valid_github_component(name)
        || owner == "."
        || owner == ".."
        || name == "."
        || name == ".."
    {
        return Err("github tracker repository must be a valid owner/repository".to_string());
    }
    Ok(repository.to_string())
}

fn valid_github_component(component: &str) -> bool {
    !component.is_empty()
        && component.len() <= 100
        && component
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn validate_record_tracker(record: &Value, tracker: &TrackerKind) -> Result<(), String> {
    if let Some(recorded) = record["tracker"].as_str() {
        if recorded != tracker_name(tracker) {
            return Err("publication record tracker does not match configured tracker".to_string());
        }
    } else if matches!(tracker, TrackerKind::GitHub { .. }) {
        return Err("github publication record is missing tracker identity".to_string());
    }
    Ok(())
}

fn tracker_name(tracker: &TrackerKind) -> &'static str {
    match tracker {
        TrackerKind::LocalMarkdown => "local-markdown",
        TrackerKind::Fake => "fake",
        TrackerKind::GitHub { .. } => "github",
    }
}

fn github_artifact_id(repository: &str, issue_number: u64) -> String {
    format!("github:{repository}#{issue_number}")
}

fn github_artifact_url(repository: &str, issue_number: u64) -> String {
    format!("https://github.com/{repository}/issues/{issue_number}")
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn fake_artifact_rel(artifact_id: &str) -> String {
    format!(".fake-tracker/artifacts/{artifact_id}.md")
}

fn artifact_id_from_path(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(path)
        .to_string()
}

fn strings_from_array(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(ToOwned::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn operation_revision(operation_id: &str) -> u64 {
    operation_id
        .split("-r")
        .nth(1)
        .and_then(|tail| tail.split('-').next())
        .and_then(|revision| revision.parse().ok())
        .unwrap_or(0)
}
