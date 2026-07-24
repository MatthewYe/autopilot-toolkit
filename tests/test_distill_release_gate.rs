#!/usr/bin/env rust-script
//! ```cargo
//! [dependencies]
//! serde_json = "1"
//! sha2 = "0.10"
//! ```
//!
//! Release-gate evidence checks for Distill issue #64.
//! Run with: rust-script --test tests/test_distill_release_gate.rs

use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("Run with: rust-script --test tests/test_distill_release_gate.rs");
}

fn project_root() -> PathBuf {
    let src = Path::new(file!());
    if let Some(root) = src.parent().and_then(|p| p.parent()) {
        if root
            .join("docs/reports/64-distill-release-gate-matrix.json")
            .exists()
        {
            return root.to_path_buf();
        }
    }
    if let Ok(root) = std::env::var("PROJECT_ROOT") {
        let root = PathBuf::from(root);
        if root
            .join("docs/reports/64-distill-release-gate-matrix.json")
            .exists()
        {
            return root;
        }
    }
    panic!("cannot find project root");
}

fn matrix() -> Value {
    let bytes = fs::read(project_root().join("docs/reports/64-distill-release-gate-matrix.json"))
        .expect("release gate matrix should be readable");
    serde_json::from_slice(&bytes).expect("release gate matrix should be valid JSON")
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).expect("JSON evidence should be readable"))
        .expect("evidence should be valid JSON")
}

fn sha256(path: &Path) -> String {
    format!("{:x}", Sha256::digest(fs::read(path).unwrap()))
}

fn runtime_row<'a>(matrix: &'a Value, runtime: &str) -> &'a Value {
    matrix["runtime_rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["runtime"] == runtime)
        .unwrap_or_else(|| panic!("missing runtime row for {runtime}"))
}

fn assert_lifecycle_checks(lifecycle: &Value, runtime: &str) {
    assert_eq!(
        lifecycle["uploaded_file_intake"]["status"], "passed",
        "{runtime} uploaded file intake should pass"
    );
    assert_eq!(
        lifecycle["waiting_boundary"]["status"], "passed",
        "{runtime} waiting boundary should pass"
    );
    assert_eq!(
        lifecycle["one_unfinished_run"]["status"], "passed",
        "{runtime} one-unfinished-run check should pass"
    );
    assert_eq!(
        lifecycle["one_unfinished_run"]["same_run_id_from_two_starts"], true,
        "{runtime} repeated native start should resume the same run"
    );
    assert_eq!(
        lifecycle["cross_session_rejection"]["status"], "passed",
        "{runtime} cross-session rejection should pass"
    );
    assert!(
        lifecycle["cross_session_rejection"]["error"]
            .as_str()
            .unwrap_or_default()
            .contains("session id does not match active run binding"),
        "{runtime} cross-session rejection should preserve runner error"
    );
    assert_eq!(
        lifecycle["explicit_takeover"]["status"], "passed",
        "{runtime} explicit takeover should pass"
    );
    assert_eq!(
        lifecycle["explicit_takeover"]["user_authorized"], true,
        "{runtime} takeover must be explicitly user-authorized"
    );
    assert_eq!(
        lifecycle["recoverable_context_drift"]["status"], "passed",
        "{runtime} drift recovery should pass"
    );
    assert_eq!(
        lifecycle["recoverable_context_drift"]["reasoned_immaterial_acknowledgment_accepted"], true,
        "{runtime} drift recovery should require reasoned immaterial acknowledgment"
    );
    assert_eq!(
        lifecycle["publication_reconciliation"]["status"], "passed",
        "{runtime} publication reconciliation should pass"
    );
    assert_eq!(
        lifecycle["publication_reconciliation"]["first_status"], "needs-reconciliation",
        "{runtime} first publication attempt should yield reconciliation"
    );
    assert_eq!(
        lifecycle["publication_reconciliation"]["create_count"], 1,
        "{runtime} reconciliation must not duplicate publication"
    );
    assert_eq!(
        lifecycle["terminal_release"]["status"], "passed",
        "{runtime} terminal release should pass"
    );
    assert_eq!(
        lifecycle["terminal_release"]["session_released"], true,
        "{runtime} terminal release should release the session"
    );
    assert_eq!(
        lifecycle["terminal_release"]["implementation_started"], false,
        "{runtime} Distill must not start implementation"
    );
}

fn assert_kimi_lifecycle_checks(evidence: &Value) {
    let scenarios = &evidence["scenarios"];
    assert_eq!(evidence["runtime"], "kimi");
    assert_eq!(evidence["native_session"]["match_count"], 1);
    assert_eq!(evidence["native_session"]["user_supplied"], false);
    assert_eq!(evidence["native_session"]["env_var_used"], false);
    assert_eq!(evidence["runner"]["status"], "completed");
    assert_eq!(evidence["runner"]["session_released"], true);
    assert_eq!(evidence["runner"]["implementation_started"], false);

    assert_eq!(scenarios["uploaded_file_intake"]["result"], "pass");
    assert_eq!(scenarios["invoke_skill_waiting_boundary"]["result"], "pass");
    assert!(scenarios["invoke_skill_waiting_boundary"]["detail"]
        .as_str()
        .unwrap_or_default()
        .contains("next_action=invoke-skill"));
    assert_eq!(scenarios["same_session_resume"]["result"], "pass");
    assert!(scenarios["same_session_resume"]["detail"]
        .as_str()
        .unwrap_or_default()
        .contains("identical unfinished run"));
    assert_eq!(
        scenarios["one_unfinished_run_enforcement"]["result"],
        "pass"
    );
    assert!(scenarios["one_unfinished_run_enforcement"]["detail"]
        .as_str()
        .unwrap_or_default()
        .contains("no second unfinished run"));
    assert_eq!(scenarios["cross_session_rejection"]["result"], "pass");
    assert!(scenarios["cross_session_rejection"]["submit_error"]
        .as_str()
        .unwrap_or_default()
        .contains("session id does not match active run binding"));
    assert_eq!(
        scenarios["explicit_user_authorized_takeover"]["result"],
        "pass"
    );
    assert_eq!(
        scenarios["explicit_user_authorized_takeover"]["user_authorized"],
        true
    );
    assert!(
        scenarios["explicit_user_authorized_takeover"]["unauthorized_error"]
            .as_str()
            .unwrap_or_default()
            .contains("takeover requires --user-authorized")
    );
    assert_eq!(
        scenarios["context_drift_block_and_acknowledge"]["result"],
        "pass"
    );
    assert!(
        scenarios["context_drift_block_and_acknowledge"]["blocked_error"]
            .as_str()
            .unwrap_or_default()
            .contains("context drift detected")
    );
    assert_eq!(
        scenarios["context_drift_block_and_acknowledge"]
            ["reasoned_immaterial_acknowledgment_accepted"],
        true
    );
    assert_eq!(
        scenarios["fake_adapter_timeout_before_response_reconciliation"]["result"],
        "pass"
    );
    assert_eq!(
        scenarios["fake_adapter_timeout_before_response_reconciliation"]["first_status"],
        "needs-reconciliation"
    );
    assert_eq!(
        scenarios["fake_adapter_timeout_before_response_reconciliation"]["create_count"],
        1
    );
    assert_eq!(
        scenarios["fake_adapter_timeout_before_response_reconciliation"]["duplicate_create"],
        false
    );
    assert_eq!(scenarios["terminal_session_release"]["result"], "pass");
    assert!(scenarios["terminal_session_release"]["runs"]
        .as_object()
        .unwrap()
        .values()
        .all(|run| run["released"] == true));
    assert_eq!(scenarios["no_product_implementation"]["result"], "pass");
}

#[test]
fn matrix_maps_every_issue_64_acceptance_criterion() {
    let matrix = matrix();
    assert_eq!(matrix["issue"], 64);
    assert_eq!(matrix["schema_version"], 1);
    let criteria = matrix["acceptance_criteria"]
        .as_array()
        .expect("acceptance_criteria should be an array");
    assert_eq!(criteria.len(), 8);
    for index in 1..=8 {
        let id = format!("AC{index}");
        assert!(
            criteria.iter().any(|criterion| criterion["id"] == id),
            "missing criterion {id}"
        );
    }
}

#[test]
fn matrix_declares_release_complete_only_with_no_missing_rows() {
    let matrix = matrix();
    if matrix["status"] == "done" {
        assert!(matrix["blockers"].as_array().unwrap().is_empty());
        assert!(matrix["acceptance_criteria"]
            .as_array()
            .unwrap()
            .iter()
            .all(|criterion| criterion["status"] == "verified"
                && criterion["missing_checks"].as_array().unwrap().is_empty()));
    } else {
        assert_eq!(matrix["status"], "blocked");
        assert!(!matrix["blockers"].as_array().unwrap().is_empty());
        assert!(matrix["acceptance_criteria"]
            .as_array()
            .unwrap()
            .iter()
            .any(|criterion| criterion["status"] == "blocked"
                && !criterion["missing_checks"].as_array().unwrap().is_empty()));
    }
    assert!(matrix["package_rows"]
        .as_array()
        .unwrap()
        .iter()
        .all(|row| row["artifact_status"] == "verified"
            && row["install_selection_status"] == "verified"));
    assert!(matrix["runtime_rows"]
        .as_array()
        .unwrap()
        .iter()
        .all(
            |row| row["native_identity"] == "verified" && row["live_smoke"]["status"] == "verified"
        ));
}

#[test]
fn matrix_references_existing_runtime_and_package_evidence() {
    let matrix = matrix();
    let evidence = matrix["evidence"]
        .as_object()
        .expect("evidence should be an object");
    for key in [
        "issue_62_kimi",
        "issue_63_reasonix",
        "issue_64_codex",
        "issue_64_reasonix_lifecycle",
        "issue_64_kimi_lifecycle",
        "package",
    ] {
        let path = evidence[key]["path"]
            .as_str()
            .unwrap_or_else(|| panic!("evidence {key} should have a path"));
        assert!(
            project_root().join(path).is_file(),
            "evidence path should exist: {path}"
        );
    }
    assert_eq!(evidence["issue_62_kimi"]["runtime"], "kimi");
    assert_eq!(evidence["issue_63_reasonix"]["runtime"], "reasonix");
    assert_eq!(evidence["issue_64_codex"]["runtime"], "codex");
    assert_eq!(evidence["issue_64_kimi_lifecycle"]["runtime"], "kimi");
}

#[test]
fn package_evidence_matches_native_artifacts_and_release_tarball() {
    let root = project_root();
    let evidence = read_json(&root.join("docs/reports/64-distill-package-evidence.json"));
    let archive = root.join(evidence["release_candidate"]["path"].as_str().unwrap());
    assert_eq!(sha256(&archive), evidence["release_candidate"]["sha256"]);

    for artifact in evidence["artifacts"].as_array().unwrap() {
        let platform = artifact["platform"].as_str().unwrap();
        let path = root.join("dist/distill").join(platform).join("distill");
        let bytes = fs::read(&path).unwrap();
        assert_eq!(sha256(&path), artifact["sha256"]);
        match platform {
            "darwin-arm64" | "darwin-x64" => {
                assert_eq!(&bytes[..4], &[0xcf, 0xfa, 0xed, 0xfe])
            }
            "linux-arm64" | "linux-x64" => assert_eq!(&bytes[..4], b"\x7fELF"),
            _ => panic!("unsupported package row {platform}"),
        }
        assert!(!bytes.starts_with(b"#!"), "{platform} must not be a script");
    }

    let listing = Command::new("tar")
        .args(["-tzf", archive.to_str().unwrap()])
        .output()
        .expect("tar should inspect release candidate");
    assert!(listing.status.success());
    let listing = String::from_utf8(listing.stdout).unwrap();
    for platform in ["darwin-arm64", "darwin-x64", "linux-arm64", "linux-x64"] {
        assert!(listing.contains(&format!(
            "./.autopilot/bin/distill-artifacts/{platform}/distill"
        )));
    }
}

#[test]
fn native_runtime_evidence_is_completed_released_and_fail_closed() {
    let root = project_root();
    let matrix = matrix();
    for key in ["issue_62_kimi", "issue_64_codex"] {
        let evidence_path = matrix["evidence"][key]["path"].as_str().unwrap();
        let evidence = read_json(&root.join(evidence_path));
        assert_eq!(evidence["native_session"]["match_count"], 1);
        let final_report = if evidence["runner"]["final_report"].is_object() {
            &evidence["runner"]["final_report"]
        } else {
            &evidence["runner"]
        };
        assert_eq!(final_report["status"], "completed");
        assert_eq!(final_report["session_released"], true);
    }
    let codex = read_json(&root.join("docs/reports/64-codex-distill-smoke-evidence.json"));
    assert_eq!(codex["native_session"]["user_supplied"], false);
    assert_eq!(codex["runner"]["implementation_started"], false);
}

#[test]
fn native_lifecycle_gate_directly_verifies_each_runtime_or_fails_closed() {
    let root = project_root();
    let matrix = matrix();

    let codex = read_json(&root.join("docs/reports/64-codex-distill-smoke-evidence.json"));
    assert_eq!(codex["native_session"]["match_count"], 1);
    assert_eq!(codex["native_session"]["user_supplied"], false);
    assert_eq!(codex["native_lifecycle_run"]["uploaded_file_intake"], true);
    assert_eq!(
        codex["native_lifecycle_run"]["waiting_boundary"]["next_action"],
        "invoke-skill"
    );
    assert_eq!(
        codex["native_lifecycle_run"]["same_session_resume"]["same_run_id"],
        true
    );
    assert!(codex["native_lifecycle_run"]["cross_session_rejection"]
        .as_str()
        .unwrap()
        .contains("session id does not match active run binding"));
    assert_eq!(
        codex["native_lifecycle_run"]["takeover"]["user_authorized"],
        true
    );
    assert_eq!(
        codex["native_lifecycle_run"]["recoverable_block"]
            ["recovered_with_reasoned_immaterial_acknowledgment"],
        true
    );
    assert_eq!(
        codex["native_lifecycle_run"]["publication_reconciliation"]
            ["create_count_before_and_after_reconciliation"],
        1
    );
    assert_eq!(
        codex["native_lifecycle_run"]["terminal"]["session_released"],
        true
    );
    assert_eq!(
        codex["native_lifecycle_run"]["terminal"]["implementation_started"],
        false
    );

    let reasonix = read_json(&root.join("docs/reports/64-reasonix-lifecycle-evidence.json"));
    assert_eq!(reasonix["runtime"], "reasonix");
    assert_eq!(reasonix["native_session"]["match_count"], 1);
    assert_lifecycle_checks(&reasonix["lifecycle"], "reasonix");

    let kimi_lifecycle = root.join("docs/reports/64-kimi-lifecycle-evidence.json");
    if kimi_lifecycle.exists() {
        let kimi = read_json(&kimi_lifecycle);
        assert_kimi_lifecycle_checks(&kimi);
        assert_eq!(
            runtime_row(&matrix, "kimi")["lifecycle_evidence"],
            kimi_lifecycle
                .strip_prefix(&root)
                .unwrap()
                .to_str()
                .unwrap()
        );
    } else {
        assert_eq!(matrix["status"], "blocked");
        assert_eq!(runtime_row(&matrix, "kimi")["lifecycle"], "missing");
        assert!(matrix["blockers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|blocker| blocker["id"] == "kimi-lifecycle-evidence"));
    }
}
