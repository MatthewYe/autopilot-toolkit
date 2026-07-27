#!/usr/bin/env rust-script
//! ```cargo
//! [dependencies]
//! serde_json = "1"
//! ```
//!
//! Kimi-specific static verification for the autopilot-distill skill variant.
//! Run with: rust-script --test tests/test_kimi_distill.rs

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    println!("Run with: rust-script --test tests/test_kimi_distill.rs");
}

fn project_root() -> PathBuf {
    let src = Path::new(file!());
    if let Some(root) = src.parent().and_then(|p| p.parent()) {
        if root
            .join("skills/autopilot/autopilot-distill/kimi/SKILL.md")
            .exists()
        {
            return root.to_path_buf();
        }
    }
    if let Ok(root) = std::env::var("PROJECT_ROOT") {
        let root = PathBuf::from(root);
        if root
            .join("skills/autopilot/autopilot-distill/kimi/SKILL.md")
            .exists()
        {
            return root;
        }
    }
    panic!("cannot find project root");
}

fn kimi_skill() -> String {
    fs::read_to_string(project_root().join("skills/autopilot/autopilot-distill/kimi/SKILL.md"))
        .expect("Kimi autopilot-distill skill should be readable")
}

fn smoke_report() -> String {
    fs::read_to_string(project_root().join("docs/reports/62-kimi-distill-smoke.md"))
        .expect("Kimi smoke report should be readable")
}

fn smoke_evidence() -> Value {
    let bytes = fs::read(project_root().join("docs/reports/62-kimi-distill-smoke-evidence.json"))
        .expect("Kimi smoke evidence JSON should be readable");
    serde_json::from_slice(&bytes).expect("Kimi smoke evidence should be valid JSON")
}

fn assert_hex_hash(value: &Value, len: usize) {
    let hash = value.as_str().expect("hash field should be a string");
    assert_eq!(hash.len(), len);
    assert!(hash.chars().all(|ch| ch.is_ascii_hexdigit()));
}

#[test]
fn kimi_variant_defines_runtime_native_identity_and_fails_closed() {
    let skill = kimi_skill();
    assert!(skill.contains("~/.kimi-code/session_index.jsonl"));
    assert!(skill.contains("sessionDir"));
    assert!(skill.contains("current Kimi session"));
    assert!(skill.contains("fail closed"));
    assert!(skill.contains("ambiguous"));
    assert!(skill.contains("Do not guess"));
    assert!(skill.contains("recency"));
    assert!(skill.contains("Do not use a user-supplied"));
    assert!(skill.contains("--runtime kimi"));
    assert!(skill.contains("--session-id \"<kimi-session-id>\""));
}

#[test]
fn kimi_variant_preserves_shared_runner_and_stage_contract() {
    let skill = kimi_skill();
    for command in [
        "start --json --runtime kimi",
        "submit-evidence --json",
        "takeover --json",
        "--expected-revision",
    ] {
        assert!(
            skill.contains(command),
            "Kimi skill should document runner command fragment: {command}"
        );
    }
    for checkpoint in [
        "clarification-complete",
        "testing-seam-confirmed",
        "slice-breakdown-approved",
    ] {
        assert!(
            skill.contains(checkpoint),
            "Kimi skill should preserve checkpoint {checkpoint}"
        );
    }
    for skill_name in ["grill-with-docs", "to-spec", "to-tickets"] {
        assert!(
            skill.contains(skill_name),
            "Kimi skill should invoke authorized executor {skill_name}"
        );
    }
    assert!(skill.contains("run_id"));
    assert!(skill.contains("stage"));
    assert!(skill.contains("revision"));
    assert!(skill.contains("next_action"));
    assert!(skill.contains("authorized_action"));
    assert!(skill.contains("needs-reconciliation"));
}

#[test]
fn kimi_variant_documents_resume_rejection_takeover_and_recovery() {
    let skill = kimi_skill();
    for phrase in [
        "Same-session resume",
        "cross-session rejection",
        "one unfinished run",
        "explicit takeover",
        "blocked",
        "recovery",
        "session release",
    ] {
        assert!(
            skill.contains(phrase),
            "Kimi skill should document lifecycle behavior: {phrase}"
        );
    }
}

#[test]
fn smoke_report_records_runtime_discovery_and_native_smoke() {
    let report = smoke_report();
    assert!(report.contains("Issue #62"));
    assert!(report.contains("~/.kimi-code/bin/kimi --version"));
    assert!(report.contains("0.29.0"));
    assert!(report.contains("command -v kimi"));
    assert!(report.contains("not in PATH"));
    assert!(report.contains("distill start --json --runtime kimi"));
    assert!(report.contains("session_d03aeda7-b1a1-4c89-8cfb-0cc78213531d"));
    assert!(report.contains("run-sessiond03aeda7-b1a1-4c8-1784897249926"));
    assert!(report.contains("status: completed"));
    assert!(report.contains("session release"));
    assert!(report.contains("same-session resume"));
    assert!(report.contains("cross-session rejection"));
    assert!(report.contains("explicit user-authorized takeover"));
    assert!(report.contains("stale-revision recovery"));
    assert!(report.contains("purged"));
}

#[test]
fn smoke_evidence_is_machine_readable_and_uses_native_kimi_session() {
    let evidence = smoke_evidence();
    assert_eq!(evidence["schema_version"], 1);
    assert_eq!(evidence["runtime"]["kind"], "kimi");
    assert_eq!(
        evidence["runtime"]["binary"],
        "/Users/xlchen/.kimi-code/bin/kimi"
    );
    assert_eq!(evidence["runtime"]["version"], "0.29.0");

    let native = &evidence["native_session"];
    let session_id = native["session_id"]
        .as_str()
        .expect("native session id should be recorded");
    assert!(session_id.starts_with("session_"));
    assert!(!session_id.contains("smoke-62"));
    assert!(!native["source"]
        .as_str()
        .unwrap()
        .contains("KIMI_SESSION_ID"));
    assert_eq!(native["match_count"], 1);
    assert_eq!(native["work_dir"], evidence["fixture"]["work_dir"]);
    assert!(native["session_dir"]
        .as_str()
        .expect("session dir should be recorded")
        .contains(session_id));
    assert_hex_hash(&native["session_dir_sha256"], 64);
    assert_hex_hash(&native["state_json_sha256"], 64);

    let final_report = &evidence["runner"]["final_report"];
    assert_eq!(final_report["run_id"], evidence["runner"]["run_id"]);
    assert_eq!(final_report["runtime"], "kimi");
    assert_eq!(final_report["session_id"], session_id);
    assert_eq!(final_report["status"], "completed");
    assert_eq!(final_report["stage"], "completed");
    assert_eq!(final_report["revision"], 4);
    assert_eq!(final_report["session_released"], true);
    assert_eq!(final_report["published_issue_count"], 2);
    assert_hex_hash(&final_report["canonical_hash"], 64);
    assert_hex_hash(&final_report["prd_payload_hash"], 64);
    for issue in final_report["issue_payload_hashes"]
        .as_array()
        .expect("issue payload hashes should be an array")
    {
        assert_hex_hash(issue, 64);
    }

    let lifecycle = &evidence["lifecycle"];
    assert_eq!(lifecycle["same_session_resume"]["same_run_id"], true);
    assert_eq!(
        lifecycle["cross_session_rejection"]["error"],
        "ERROR: session id does not match active run binding"
    );
    assert_eq!(lifecycle["takeover"]["status"], "succeeded");
    assert_eq!(
        lifecycle["stale_revision_recovery"]["stale_error"],
        "ERROR: expected revision 1 is stale; current revision is 2"
    );
    assert_eq!(lifecycle["cleanup"]["status"], "purged");
}
