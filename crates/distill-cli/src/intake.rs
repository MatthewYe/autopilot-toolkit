use crate::storage::{self, StorageLimits};
use base64::Engine;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::PathBuf;

#[derive(Deserialize)]
pub struct IntakeBundle {
    schema_version: u64,
    sources: Vec<RuntimeSource>,
}

#[derive(Deserialize)]
struct RuntimeSource {
    id: String,
    kind: SourceKind,
    provenance: Value,
    normalized_text: String,
    raw_bytes_base64: Option<String>,
    hashes: SourceHashes,
    extraction: Value,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum SourceKind {
    Text,
    UploadedFile,
    Link,
    SelectedPriorUserMessage,
}

impl SourceKind {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::UploadedFile => "uploaded_file",
            Self::Link => "link",
            Self::SelectedPriorUserMessage => "selected_prior_user_message",
        }
    }
}

#[derive(Deserialize)]
struct SourceHashes {
    normalized_sha256: String,
    raw_sha256: String,
}

pub struct IntakeSnapshot {
    pub manifest: Value,
    pub total_raw_bytes: u64,
    pub source_count: usize,
    pub files: Vec<storage::RunFile>,
}

pub fn prepare_intake(
    run_id: &str,
    bundle_json: &str,
    limits: &StorageLimits,
) -> Result<IntakeSnapshot, String> {
    storage::validate_run_id(run_id)?;
    let bundle: IntakeBundle = serde_json::from_str(bundle_json)
        .map_err(|err| format!("--intake-json must be valid runtime intake JSON: {err}"))?;
    if bundle.schema_version != 1 {
        return Err("intake schema_version must be 1".to_string());
    }
    if bundle.sources.is_empty() {
        return Err("intake requires at least one explicit source".to_string());
    }

    let mut prepared = Vec::new();
    let mut source_ids = HashSet::new();
    let mut artifact_paths = HashSet::new();
    let mut total_raw_bytes = 0_u64;
    for source in bundle.sources {
        validate_source_identity(&source)?;
        if !source_ids.insert(source.id.clone()) {
            return Err(format!("duplicate source id: {}", source.id));
        }
        let truncated = source
            .extraction
            .get("truncated")
            .and_then(Value::as_bool)
            .ok_or("extraction.truncated is required")?;
        if truncated {
            return Err("truncated sources are rejected".to_string());
        }
        let raw_bytes_base64 = source.raw_bytes_base64.as_deref().unwrap_or_default();
        let raw = base64::engine::general_purpose::STANDARD
            .decode(raw_bytes_base64)
            .map_err(|err| format!("raw_bytes_base64 is invalid for {}: {err}", source.id))?;
        if raw.len() as u64 > limits.per_source_bytes {
            return Err(format!(
                "source {} exceeds per-source quota: {} > {}",
                source.id,
                raw.len(),
                limits.per_source_bytes
            ));
        }
        let normalized_sha = sha256_hex(source.normalized_text.as_bytes());
        if normalized_sha != source.hashes.normalized_sha256 {
            return Err(format!(
                "normalized_sha256 does not match for {}",
                source.id
            ));
        }
        let raw_sha = sha256_hex(&raw);
        if raw_sha != source.hashes.raw_sha256 {
            return Err(format!("raw_sha256 does not match for {}", source.id));
        }
        total_raw_bytes = total_raw_bytes.saturating_add(raw.len() as u64);
        let artifact_path = format!(
            "artifacts/sources/{}-{}.raw.gz",
            safe_id(&source.id)?,
            source.hashes.raw_sha256
        );
        if !artifact_paths.insert(artifact_path.clone()) {
            return Err(format!("duplicate source artifact path: {artifact_path}"));
        }
        prepared.push((source, raw, artifact_path));
    }

    let mut manifest_sources = Vec::new();
    let mut files = Vec::new();
    for (source, raw, artifact_path) in prepared {
        let compressed = storage::gzip_bytes(&raw)?;
        let raw_rel = format!(".distill/runs/{run_id}/{artifact_path}");
        files.push(storage::RunFile {
            relative_path: PathBuf::from(&artifact_path),
            bytes: compressed,
        });
        manifest_sources.push(json!({
            "id": source.id,
            "kind": source.kind.as_str(),
            "provenance": source.provenance,
            "normalized_text": source.normalized_text,
            "hashes": {
                "normalized_sha256": source.hashes.normalized_sha256,
                "raw_sha256": source.hashes.raw_sha256,
            },
            "extraction": source.extraction,
            "raw_artifact_path": raw_rel,
            "retention": {
                "raw_bytes_retained": true,
                "compression": "gzip",
                "lossless": true,
            }
        }));
    }

    let manifest = json!({
        "schema_version": 1,
        "sources": manifest_sources,
        "retention": {
            "raw_bytes_retained": true,
            "purgeable": true,
        },
        "storage": {
            "total_raw_bytes": total_raw_bytes,
        }
    });
    let manifest_bytes =
        serde_json::to_vec_pretty(&manifest).map_err(|err| format!("json error: {err}"))?;
    files.push(storage::RunFile {
        relative_path: PathBuf::from("snapshots/requirements.json"),
        bytes: manifest_bytes,
    });
    Ok(IntakeSnapshot {
        source_count: manifest["sources"].as_array().map_or(0, Vec::len),
        manifest,
        total_raw_bytes,
        files,
    })
}

pub fn bundle_from_text(requirement: &str) -> String {
    let hash = sha256_hex(requirement.as_bytes());
    json!({
        "schema_version": 1,
        "sources": [{
            "id": "text-1",
            "kind": "text",
            "provenance": {"label": "direct user text"},
            "normalized_text": requirement,
            "raw_bytes_base64": base64::engine::general_purpose::STANDARD.encode(requirement.as_bytes()),
            "hashes": {
                "normalized_sha256": hash,
                "raw_sha256": hash,
            },
            "extraction": {
                "tool": "distill-cli-text-argument",
                "truncated": false,
            }
        }]
    })
    .to_string()
}

pub fn source_hashes(snapshot: &Value) -> Vec<Value> {
    snapshot["sources"]
        .as_array()
        .map(|sources| {
            sources
                .iter()
                .map(|source| {
                    json!({
                        "id": source["id"],
                        "kind": source["kind"],
                        "normalized_sha256": source["hashes"]["normalized_sha256"],
                        "raw_sha256": source["hashes"]["raw_sha256"],
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn validate_source_identity(source: &RuntimeSource) -> Result<(), String> {
    safe_id(&source.id)?;
    if matches!(source.kind, SourceKind::SelectedPriorUserMessage)
        && source.provenance.get("role").and_then(Value::as_str) != Some("user")
    {
        return Err("selected prior messages must have provenance.role=user".to_string());
    }
    if source
        .raw_bytes_base64
        .as_deref()
        .unwrap_or_default()
        .is_empty()
    {
        return Err("raw_bytes_base64 is required".to_string());
    }
    if source.normalized_text.is_empty() {
        return Err("normalized_text is required".to_string());
    }
    Ok(())
}

fn safe_id(id: &str) -> Result<String, String> {
    if id.is_empty()
        || id.len() > 80
        || !id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        return Err(format!("source id is unsafe: {id}"));
    }
    Ok(id.to_string())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}
