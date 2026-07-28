use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn distill_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_distill"))
}

fn write_fake_tracker_config(root: &Path) {
    fs::create_dir_all(root.join("docs/agents")).unwrap();
    fs::write(
        root.join("docs/agents/issue-tracker.md"),
        "# Issue tracker: Fake Adapter\n\nAdapter state lives in `.fake-tracker/`.\n",
    )
    .unwrap();
    fs::write(
        root.join("docs/agents/triage-labels.md"),
        "| Label | Value |\n| --- | --- |\n| ready-for-agent | ready-for-agent |\n",
    )
    .unwrap();
    fs::write(root.join("docs/agents/domain.md"), "# Domain\n").unwrap();
}

fn write_local_tracker_config(root: &Path) {
    fs::create_dir_all(root.join("docs/agents")).unwrap();
    fs::write(
        root.join("docs/agents/issue-tracker.md"),
        "# Issue tracker: Local Markdown\n\nIssues and PRDs live in `.scratch/`.\n",
    )
    .unwrap();
    fs::write(
        root.join("docs/agents/triage-labels.md"),
        "| Label | Value |\n| --- | --- |\n| ready-for-agent | ready-for-agent |\n",
    )
    .unwrap();
    fs::write(root.join("docs/agents/domain.md"), "# Domain\n").unwrap();
}

fn write_github_tracker_config(root: &Path) {
    fs::create_dir_all(root.join("docs/agents")).unwrap();
    fs::write(
        root.join("docs/agents/issue-tracker.md"),
        "# Issue tracker: GitHub\n\nIssues and PRDs for this repo live as GitHub issues on `matthewye/autopilot-toolkit`.\nNever fall back to `.scratch`.\n",
    )
    .unwrap();
    fs::write(
        root.join("docs/agents/triage-labels.md"),
        "| Label | Value |\n| --- | --- |\n| ready-for-agent | ready-for-agent |\n",
    )
    .unwrap();
    fs::write(root.join("docs/agents/domain.md"), "# Domain\n").unwrap();
}

fn distill(args: &[&str]) -> Output {
    Command::new(distill_bin())
        .args(args)
        .output()
        .expect("distill should execute")
}

fn distill_with_env(args: &[&str], envs: &[(&str, &str)]) -> Output {
    let mut command = Command::new(distill_bin());
    command.args(args);
    for (key, value) in envs {
        command.env(key, value);
    }
    command.output().expect("distill should execute")
}

fn json_success(output: Output) -> Value {
    assert!(
        output.status.success(),
        "distill failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("stdout should be JSON")
}

fn assert_error_contains(output: Output, expected: &str) {
    assert!(!output.status.success(), "command should fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(expected),
        "stderr should contain {expected:?}, got {stderr:?}"
    );
}

fn start(worktree: &Path, session: &str) -> Value {
    json_success(distill(&[
        "start",
        "--json",
        "--runtime",
        "codex",
        "--session-id",
        session,
        "--worktree",
        worktree.to_str().unwrap(),
        "--requirement",
        "Build a small audit dashboard for deployment status.",
    ]))
}

fn submit_evidence(
    worktree: &Path,
    run_id: &str,
    session: &str,
    revision: u64,
    stage: &str,
    evidence: &Value,
) -> Output {
    let evidence = evidence.to_string();
    distill(&[
        "submit-evidence",
        "--json",
        "--worktree",
        worktree.to_str().unwrap(),
        "--run-id",
        run_id,
        "--session-id",
        session,
        "--expected-revision",
        &revision.to_string(),
        "--stage",
        stage,
        "--evidence",
        &evidence,
    ])
}

fn submit_evidence_with_env(
    worktree: &Path,
    run_id: &str,
    session: &str,
    revision: u64,
    stage: &str,
    evidence: &Value,
    envs: &[(&str, &str)],
) -> Output {
    let evidence = evidence.to_string();
    distill_with_env(
        &[
            "submit-evidence",
            "--json",
            "--worktree",
            worktree.to_str().unwrap(),
            "--run-id",
            run_id,
            "--session-id",
            session,
            "--expected-revision",
            &revision.to_string(),
            "--stage",
            stage,
            "--evidence",
            &evidence,
        ],
        envs,
    )
}

fn state_for(worktree: &Path, run_id: &str) -> Value {
    serde_json::from_str(
        &fs::read_to_string(
            worktree
                .join(".distill/runs")
                .join(run_id)
                .join("state.json"),
        )
        .unwrap(),
    )
    .unwrap()
}

fn record_for(worktree: &Path, run_id: &str, operation_id: &str) -> Value {
    serde_json::from_str(
        &fs::read_to_string(
            worktree
                .join(".distill/runs")
                .join(run_id)
                .join("publication/records")
                .join(format!("{operation_id}.json")),
        )
        .unwrap(),
    )
    .unwrap()
}

fn prd_evidence() -> Value {
    json!({
        "checkpoint": "testing-seam-confirmed",
        "summary": "PRD captures the CLI seam.",
        "prd_markdown": "# PRD\n\n## Problem Statement\n\nBuild a fake-tracker safe dashboard.\n"
    })
}

fn github_prd_evidence(operation_id: &str, issue_number: u64, status: &str) -> Value {
    let mut evidence = prd_evidence();
    let payload = evidence["prd_markdown"].as_str().unwrap();
    let payload_hash = format!("{:x}", Sha256::digest(payload.as_bytes()));
    evidence["external_publication"] = json!({
        "tracker": "github",
        "repository": "matthewye/autopilot-toolkit",
        "operation_id": operation_id,
        "payload_hash": payload_hash,
        "status": status,
        "artifact_id": issue_number,
        "artifact_url": format!(
            "https://github.com/matthewye/autopilot-toolkit/issues/{issue_number}"
        ),
    });
    evidence
}

fn issue_evidence_for(parent: &str) -> Value {
    let api_body = format!(
        "---\nkey: 01-build-api-slice\ntitle: Build API slice\ntype: issue\nstatus: ready-for-agent\nparent: {parent}\n---\n\n## What to build\n\nBuild the API slice.\n\n## Acceptance Criteria\n\n- [ ] API works.\n\n## Blocked by\n\n- None — can start immediately.\n\n## Comments\n"
    );
    let ui_body = format!(
        "---\nkey: 02-build-ui-slice\ntitle: Build UI slice\ntype: issue\nstatus: ready-for-agent\nparent: {parent}\n---\n\n## What to build\n\nBuild the UI slice.\n\n## Acceptance Criteria\n\n- [ ] UI works.\n\n## Blocked by\n\n- Build API slice\n\n## Comments\n"
    );
    json!({
        "checkpoint": "slice-breakdown-approved",
        "summary": "Two slices exercise dependencies.",
        "issues": [
            {
                "title": "Build API slice",
                "body": api_body,
                "depends_on": []
            },
            {
                "title": "Build UI slice",
                "body": ui_body,
                "depends_on": [0]
            }
        ]
    })
}

fn issue_evidence() -> Value {
    issue_evidence_for(".scratch/distill-tracer/PRD.md")
}

fn github_issue_evidence(run_id: &str, revision: u64) -> Value {
    let mut evidence = issue_evidence();
    for (index, issue) in evidence["issues"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .enumerate()
    {
        let operation_id = format!("{run_id}-r{revision}-issue-{:02}", index + 1);
        let payload_hash = format!(
            "{:x}",
            Sha256::digest(issue["body"].as_str().unwrap().as_bytes())
        );
        let issue_number = 60 + index as u64;
        issue["external_publication"] = json!({
            "tracker": "github",
            "repository": "matthewye/autopilot-toolkit",
            "operation_id": operation_id,
            "payload_hash": payload_hash,
            "status": "confirmed",
            "artifact_id": issue_number,
            "artifact_url": format!(
                "https://github.com/matthewye/autopilot-toolkit/issues/{issue_number}"
            ),
        });
    }
    evidence
}

fn reach_prd_stage(worktree: &Path, session: &str) -> (String, u64) {
    let started = start(worktree, session);
    let run_id = started["run_id"].as_str().unwrap().to_string();
    let clarification = json!({
        "checkpoint": "clarification-complete",
        "summary": "Deployment status by environment is the core workflow.",
        "clarified_requirement": "Deployment status by environment is the core workflow.",
        "decisions": [],
        "accepted_assumptions": [],
        "material_unknowns": [],
        "domain_document_artifacts": []
    });
    let after_clarification = json_success(submit_evidence(
        worktree,
        &run_id,
        session,
        1,
        "clarification",
        &clarification,
    ));
    (run_id, after_clarification["revision"].as_u64().unwrap())
}

fn fake_create_log_count(worktree: &Path) -> usize {
    let path = worktree.join(".fake-tracker/create-log.jsonl");
    fs::read_to_string(path).unwrap_or_default().lines().count()
}

fn stage_state(state: &Value, stage_id: &str) -> String {
    state["stages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|stage| stage["id"] == stage_id)
        .unwrap()["state"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn fake_adapter_success_freezes_payloads_and_records_revision() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path();
    write_fake_tracker_config(worktree);
    let (run_id, prd_revision) = reach_prd_stage(worktree, "session-publication-success");

    let after_prd = json_success(submit_evidence(
        worktree,
        &run_id,
        "session-publication-success",
        prd_revision,
        "prd",
        &prd_evidence(),
    ));
    assert_eq!(after_prd["stage"], "issues");
    assert_eq!(
        after_prd["publications"]["prd"]["operation_id"],
        format!("{run_id}-r2-prd")
    );

    let completed = json_success(submit_evidence(
        worktree,
        &run_id,
        "session-publication-success",
        after_prd["revision"].as_u64().unwrap(),
        "issues",
        &issue_evidence(),
    ));
    assert_eq!(completed["status"], "completed");
    assert_eq!(fake_create_log_count(worktree), 3);

    let prd_record = record_for(worktree, &run_id, &format!("{run_id}-r2-prd"));
    assert_eq!(prd_record["revision"], 2);
    assert_eq!(prd_record["status"], "confirmed");
    assert_eq!(
        prd_record["payload_hash"],
        "b1c6fffc30ee784f4b2b57122e0efb70696e0ad38ab567c0b924f322e9b097c8"
    );
    assert_eq!(
        fs::read_to_string(worktree.join(prd_record["payload_path"].as_str().unwrap())).unwrap(),
        prd_evidence()["prd_markdown"].as_str().unwrap()
    );

    let first_issue = record_for(worktree, &run_id, &format!("{run_id}-r3-issue-01"));
    let second_issue = record_for(worktree, &run_id, &format!("{run_id}-r3-issue-02"));
    assert_eq!(
        second_issue["dependency_artifact_ids"][0],
        first_issue["artifact_id"]
    );
}

#[test]
fn issue_evidence_requires_explicit_dependency_edges() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path();
    write_local_tracker_config(worktree);
    let (run_id, prd_revision) = reach_prd_stage(worktree, "session-explicit-dependencies");
    let after_prd = json_success(submit_evidence(
        worktree,
        &run_id,
        "session-explicit-dependencies",
        prd_revision,
        "prd",
        &prd_evidence(),
    ));
    let issues = json!({
        "checkpoint": "slice-breakdown-approved",
        "summary": "Dependency edges must never be inferred from prose.",
        "issues": [{
            "title": "Build API slice",
            "body": "Status: ready-for-agent\n\n## Blocked by\n\nNone\n"
        }]
    });

    assert_error_contains(
        submit_evidence(
            worktree,
            &run_id,
            "session-explicit-dependencies",
            after_prd["revision"].as_u64().unwrap(),
            "issues",
            &issues,
        ),
        "missing field `depends_on`",
    );
}

#[test]
fn local_markdown_rejects_noncanonical_ticket_contract() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path();
    write_local_tracker_config(worktree);
    let (run_id, prd_revision) = reach_prd_stage(worktree, "session-canonical-ticket");
    let after_prd = json_success(submit_evidence(
        worktree,
        &run_id,
        "session-canonical-ticket",
        prd_revision,
        "prd",
        &prd_evidence(),
    ));
    let issues = json!({
        "checkpoint": "slice-breakdown-approved",
        "summary": "Legacy status-only Markdown is not a runnable ticket.",
        "issues": [{
            "title": "Build API slice",
            "body": "---\nStatus: ready-for-agent\n---\n\n## What to build\n\nBuild the API.\n\n## Acceptance criteria\n\n- [ ] API works.\n\n## Blocked by\n\n- None\n",
            "depends_on": []
        }]
    });

    assert_error_contains(
        submit_evidence(
            worktree,
            &run_id,
            "session-canonical-ticket",
            after_prd["revision"].as_u64().unwrap(),
            "issues",
            &issues,
        ),
        "canonical local ticket",
    );
}

#[test]
fn local_markdown_rejects_preexisting_prd_from_another_run_without_overwrite() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path();
    write_local_tracker_config(worktree);
    let tracker_path = worktree.join(".scratch/distill-tracer/PRD.md");
    fs::create_dir_all(tracker_path.parent().unwrap()).unwrap();
    let existing = "existing feature PRD\n";
    fs::write(&tracker_path, existing).unwrap();
    let (run_id, prd_revision) = reach_prd_stage(worktree, "session-local-path-collision");

    assert_error_contains(
        submit_evidence(
            worktree,
            &run_id,
            "session-local-path-collision",
            prd_revision,
            "prd",
            &prd_evidence(),
        ),
        "local tracker path already exists with different content",
    );
    assert_eq!(fs::read_to_string(&tracker_path).unwrap(), existing);
}

#[test]
fn local_markdown_publishes_prd_and_issues_under_the_supplied_feature_slug() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path();
    write_local_tracker_config(worktree);
    let (run_id, prd_revision) = reach_prd_stage(worktree, "session-local-feature-slug");
    let mut prd = prd_evidence();
    prd["feature_slug"] = json!("search-performance");

    let after_prd = json_success(submit_evidence(
        worktree,
        &run_id,
        "session-local-feature-slug",
        prd_revision,
        "prd",
        &prd,
    ));
    assert_eq!(
        after_prd["publications"]["prd"]["path"],
        ".scratch/search-performance/PRD.md"
    );
    assert_eq!(
        fs::read_to_string(worktree.join(".scratch/search-performance/PRD.md")).unwrap(),
        prd["prd_markdown"].as_str().unwrap()
    );

    let completed = json_success(submit_evidence(
        worktree,
        &run_id,
        "session-local-feature-slug",
        after_prd["revision"].as_u64().unwrap(),
        "issues",
        &issue_evidence_for(".scratch/search-performance/PRD.md"),
    ));
    assert_eq!(completed["status"], "completed");
    assert_eq!(
        completed["publications"]["issues"][0]["path"],
        ".scratch/search-performance/issues/01-build-api-slice.md"
    );
    assert_eq!(
        completed["publications"]["issues"][1]["path"],
        ".scratch/search-performance/issues/02-build-ui-slice.md"
    );
}

#[test]
fn local_markdown_rejects_unsafe_feature_slug_before_writing() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path();
    write_local_tracker_config(worktree);
    let (run_id, prd_revision) = reach_prd_stage(worktree, "session-local-unsafe-slug");
    let mut prd = prd_evidence();
    prd["feature_slug"] = json!("../existing-feature");

    assert_error_contains(
        submit_evidence(
            worktree,
            &run_id,
            "session-local-unsafe-slug",
            prd_revision,
            "prd",
            &prd,
        ),
        "feature_slug must be 1-80 lowercase ASCII letters, digits, or hyphens",
    );
    assert!(!worktree.join(".scratch/existing-feature/PRD.md").exists());
    assert!(!worktree.join(".scratch/distill-tracer/PRD.md").exists());
}

#[test]
fn local_markdown_resume_reports_human_drift_without_overwrite() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path();
    write_local_tracker_config(worktree);
    let (run_id, prd_revision) = reach_prd_stage(worktree, "session-local-drift");

    assert_error_contains(
        submit_evidence_with_env(
            worktree,
            &run_id,
            "session-local-drift",
            prd_revision,
            "prd",
            &prd_evidence(),
            &[("DISTILL_FAIL_WRITE_STATE_FOR_RUN", &run_id)],
        ),
        "injected state write failure",
    );
    let tracker_path = worktree.join(".scratch/distill-tracer/PRD.md");
    assert!(tracker_path.is_file());
    fs::write(&tracker_path, "human edited local tracker content\n").unwrap();

    assert_error_contains(
        submit_evidence(
            worktree,
            &run_id,
            "session-local-drift",
            prd_revision,
            "prd",
            &prd_evidence(),
        ),
        "tracker drift detected",
    );
    assert_eq!(
        fs::read_to_string(&tracker_path).unwrap(),
        "human edited local tracker content\n"
    );
}

#[test]
fn local_markdown_rejects_issue_without_agent_ready_triage_before_writing() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path();
    write_local_tracker_config(worktree);
    let (run_id, prd_revision) = reach_prd_stage(worktree, "session-local-triage");
    let after_prd = json_success(submit_evidence(
        worktree,
        &run_id,
        "session-local-triage",
        prd_revision,
        "prd",
        &prd_evidence(),
    ));
    let issues_revision = after_prd["revision"].as_u64().unwrap();
    let evidence = json!({
        "checkpoint": "slice-breakdown-approved",
        "summary": "One local issue.",
        "issues": [{
            "title": "Missing triage",
            "body": "---\nkey: 01-missing-triage\ntitle: Missing triage\ntype: issue\nparent: .scratch/distill-tracer/PRD.md\n---\n\n## What to build\n\nA local issue without status metadata.\n\n## Acceptance Criteria\n\n- [ ] The issue is ready.\n\n## Blocked by\n\n- None — can start immediately.\n\n## Comments\n",
            "depends_on": []
        }]
    });

    assert_error_contains(
        submit_evidence(
            worktree,
            &run_id,
            "session-local-triage",
            issues_revision,
            "issues",
            &evidence,
        ),
        "frontmatter status: ready-for-agent",
    );
    assert!(!worktree.join(".scratch/distill-tracer/issues").exists());
}

#[test]
fn timeout_before_response_enters_reconciliation_and_retry_does_not_duplicate() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path();
    write_fake_tracker_config(worktree);
    let (run_id, prd_revision) = reach_prd_stage(worktree, "session-timeout");

    let reconciliation = json_success(submit_evidence_with_env(
        worktree,
        &run_id,
        "session-timeout",
        prd_revision,
        "prd",
        &prd_evidence(),
        &[("DISTILL_FAKE_TRACKER_MODE", "timeout-before-response")],
    ));
    assert_eq!(reconciliation["stage"], "prd");
    assert_eq!(fake_create_log_count(worktree), 1);
    assert_eq!(
        stage_state(&state_for(worktree, &run_id), "prd"),
        "needs-reconciliation"
    );

    let resumed = json_success(submit_evidence(
        worktree,
        &run_id,
        "session-timeout",
        reconciliation["revision"].as_u64().unwrap(),
        "prd",
        &prd_evidence(),
    ));
    assert_eq!(resumed["stage"], "issues");
    assert_eq!(
        fake_create_log_count(worktree),
        1,
        "resume should verify the external artifact instead of republishing"
    );
}

#[test]
fn response_before_local_state_write_recovers_without_republishing() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path();
    write_fake_tracker_config(worktree);
    let (run_id, prd_revision) = reach_prd_stage(worktree, "session-local-write");

    assert_error_contains(
        submit_evidence_with_env(
            worktree,
            &run_id,
            "session-local-write",
            prd_revision,
            "prd",
            &prd_evidence(),
            &[("DISTILL_FAIL_WRITE_STATE_FOR_RUN", &run_id)],
        ),
        "injected state write failure",
    );
    assert_eq!(fake_create_log_count(worktree), 1);
    assert_eq!(state_for(worktree, &run_id)["current_stage"], "prd");

    let resumed = json_success(submit_evidence(
        worktree,
        &run_id,
        "session-local-write",
        prd_revision,
        "prd",
        &prd_evidence(),
    ));
    assert_eq!(resumed["stage"], "issues");
    assert_eq!(fake_create_log_count(worktree), 1);
}

#[test]
fn partial_issue_batch_resumes_missing_slices_with_real_dependency_ids() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path();
    write_fake_tracker_config(worktree);
    let (run_id, prd_revision) = reach_prd_stage(worktree, "session-partial");
    let after_prd = json_success(submit_evidence(
        worktree,
        &run_id,
        "session-partial",
        prd_revision,
        "prd",
        &prd_evidence(),
    ));

    let reconciliation = json_success(submit_evidence_with_env(
        worktree,
        &run_id,
        "session-partial",
        after_prd["revision"].as_u64().unwrap(),
        "issues",
        &issue_evidence(),
        &[("DISTILL_FAKE_TRACKER_MODE", "partial-batch")],
    ));
    assert_eq!(reconciliation["stage"], "issues");
    assert_eq!(fake_create_log_count(worktree), 2);

    let completed = json_success(submit_evidence(
        worktree,
        &run_id,
        "session-partial",
        reconciliation["revision"].as_u64().unwrap(),
        "issues",
        &issue_evidence(),
    ));
    assert_eq!(completed["status"], "completed");
    assert_eq!(fake_create_log_count(worktree), 3);

    let first_issue = record_for(worktree, &run_id, &format!("{run_id}-r3-issue-01"));
    let second_issue = record_for(worktree, &run_id, &format!("{run_id}-r3-issue-02"));
    assert_eq!(
        second_issue["dependency_artifact_ids"][0],
        first_issue["artifact_id"]
    );
}

#[test]
fn tracker_drift_and_outage_are_reported_without_local_fallback_or_overwrite() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path();
    write_fake_tracker_config(worktree);
    let (run_id, prd_revision) = reach_prd_stage(worktree, "session-drift-outage");

    assert_error_contains(
        submit_evidence_with_env(
            worktree,
            &run_id,
            "session-drift-outage",
            prd_revision,
            "prd",
            &prd_evidence(),
            &[("DISTILL_FAKE_TRACKER_MODE", "outage")],
        ),
        "configured tracker unavailable",
    );
    assert!(!worktree.join(".scratch/distill-tracer/PRD.md").exists());

    assert_error_contains(
        submit_evidence_with_env(
            worktree,
            &run_id,
            "session-drift-outage",
            prd_revision,
            "prd",
            &prd_evidence(),
            &[("DISTILL_FAIL_WRITE_STATE_FOR_RUN", &run_id)],
        ),
        "injected state write failure",
    );
    let record = record_for(worktree, &run_id, &format!("{run_id}-r2-prd"));
    let artifact_path = worktree.join(record["artifact_path"].as_str().unwrap());
    fs::write(&artifact_path, "human edited tracker content\n").unwrap();

    assert_error_contains(
        submit_evidence(
            worktree,
            &run_id,
            "session-drift-outage",
            prd_revision,
            "prd",
            &prd_evidence(),
        ),
        "tracker drift detected",
    );
    assert_eq!(
        fs::read_to_string(&artifact_path).unwrap(),
        "human edited tracker content\n"
    );
}

#[test]
fn missing_tracker_config_prevents_start() {
    let tmp = tempfile::tempdir().unwrap();
    assert_error_contains(
        distill(&[
            "start",
            "--json",
            "--runtime",
            "codex",
            "--session-id",
            "session-no-config",
            "--worktree",
            tmp.path().to_str().unwrap(),
            "--requirement",
            "Build a dashboard.",
        ]),
        "docs/agents/issue-tracker.md is required",
    );
}

#[test]
fn github_tracker_accepts_only_confirmed_executor_receipt_without_local_fallback() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path();
    write_github_tracker_config(worktree);
    let (run_id, prd_revision) = reach_prd_stage(worktree, "session-github-confirmed");
    let operation_id = format!("{run_id}-r{prd_revision}-prd");

    let after_prd = json_success(submit_evidence(
        worktree,
        &run_id,
        "session-github-confirmed",
        prd_revision,
        "prd",
        &github_prd_evidence(&operation_id, 55, "confirmed"),
    ));

    assert_eq!(after_prd["stage"], "issues");
    assert_eq!(
        after_prd["publications"]["prd"]["artifact_id"],
        "github:matthewye/autopilot-toolkit#55"
    );
    assert_eq!(
        after_prd["publications"]["prd"]["path"],
        "https://github.com/matthewye/autopilot-toolkit/issues/55"
    );
    assert!(!worktree.join(".scratch/distill-tracer/PRD.md").exists());
    assert!(!worktree.join(".fake-tracker").exists());
}

#[test]
fn github_tracker_missing_receipt_blocks_then_reconciles_with_same_operation() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path();
    write_github_tracker_config(worktree);
    let (run_id, prd_revision) = reach_prd_stage(worktree, "session-github-reconcile");
    let operation_id = format!("{run_id}-r{prd_revision}-prd");

    let blocked = json_success(submit_evidence(
        worktree,
        &run_id,
        "session-github-reconcile",
        prd_revision,
        "prd",
        &prd_evidence(),
    ));
    assert_eq!(blocked["stage"], "prd");
    assert_eq!(
        blocked["publication_blocked"],
        "github publication requires confirmed external evidence"
    );
    assert_eq!(blocked["publications"]["prd"]["operation_id"], operation_id);
    assert_eq!(
        blocked["publications"]["prd"]["status"],
        "needs-reconciliation"
    );
    assert!(!worktree.join(".scratch/distill-tracer/PRD.md").exists());

    let still_blocked = json_success(submit_evidence(
        worktree,
        &run_id,
        "session-github-reconcile",
        blocked["revision"].as_u64().unwrap(),
        "prd",
        &github_prd_evidence(&operation_id, 56, "pending"),
    ));
    assert_eq!(
        still_blocked["publication_blocked"],
        "github publication requires confirmed external evidence"
    );

    let reconciled = json_success(submit_evidence(
        worktree,
        &run_id,
        "session-github-reconcile",
        still_blocked["revision"].as_u64().unwrap(),
        "prd",
        &github_prd_evidence(&operation_id, 56, "confirmed"),
    ));
    assert_eq!(reconciled["stage"], "issues");
    assert_eq!(
        reconciled["publications"]["prd"]["artifact_id"],
        "github:matthewye/autopilot-toolkit#56"
    );
}

#[test]
fn github_tracker_rejects_receipts_not_bound_to_frozen_publication() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path();
    write_github_tracker_config(worktree);
    let (run_id, prd_revision) = reach_prd_stage(worktree, "session-github-forged");
    let operation_id = format!("{run_id}-r{prd_revision}-prd");
    let mut evidence = github_prd_evidence(&operation_id, 57, "confirmed");
    evidence["external_publication"]["repository"] = json!("attacker/other-repo");

    assert_error_contains(
        submit_evidence(
            worktree,
            &run_id,
            "session-github-forged",
            prd_revision,
            "prd",
            &evidence,
        ),
        "github publication evidence repository does not match configured tracker",
    );

    let mut evidence = github_prd_evidence(&operation_id, 57, "confirmed");
    evidence["external_publication"]["payload_hash"] = json!("00");
    assert_error_contains(
        submit_evidence(
            worktree,
            &run_id,
            "session-github-forged",
            prd_revision,
            "prd",
            &evidence,
        ),
        "github publication evidence payload_hash does not match frozen payload",
    );

    let mut evidence = github_prd_evidence(&operation_id, 57, "confirmed");
    evidence["external_publication"]["artifact_url"] =
        json!("https://github.com/matthewye/autopilot-toolkit/issues/999");
    assert_error_contains(
        submit_evidence(
            worktree,
            &run_id,
            "session-github-forged",
            prd_revision,
            "prd",
            &evidence,
        ),
        "github publication evidence artifact_url is not canonical",
    );
    assert!(!worktree.join(".scratch/distill-tracer/PRD.md").exists());
}

#[test]
fn github_reconciliation_does_not_overwrite_the_frozen_payload() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path();
    write_github_tracker_config(worktree);
    let (run_id, prd_revision) = reach_prd_stage(worktree, "session-github-frozen");
    let operation_id = format!("{run_id}-r{prd_revision}-prd");
    let original = prd_evidence();

    let blocked = json_success(submit_evidence(
        worktree,
        &run_id,
        "session-github-frozen",
        prd_revision,
        "prd",
        &original,
    ));
    let record = record_for(worktree, &run_id, &operation_id);
    let payload_path = worktree.join(record["payload_path"].as_str().unwrap());

    let mut changed = github_prd_evidence(&operation_id, 57, "confirmed");
    changed["prd_markdown"] = json!("# PRD\n\nChanged after the operation was frozen.\n");
    changed["external_publication"]["payload_hash"] = json!(format!(
        "{:x}",
        Sha256::digest(changed["prd_markdown"].as_str().unwrap().as_bytes())
    ));
    assert_error_contains(
        submit_evidence(
            worktree,
            &run_id,
            "session-github-frozen",
            blocked["revision"].as_u64().unwrap(),
            "prd",
            &changed,
        ),
        "frozen publication payload changed for stable operation",
    );
    assert_eq!(
        fs::read_to_string(payload_path).unwrap(),
        original["prd_markdown"].as_str().unwrap()
    );
}

#[test]
fn github_confirmed_record_resumes_offline_without_duplicate_or_receipt_replay() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path();
    write_github_tracker_config(worktree);
    let (run_id, prd_revision) = reach_prd_stage(worktree, "session-github-resume");
    let operation_id = format!("{run_id}-r{prd_revision}-prd");

    assert_error_contains(
        submit_evidence_with_env(
            worktree,
            &run_id,
            "session-github-resume",
            prd_revision,
            "prd",
            &github_prd_evidence(&operation_id, 58, "confirmed"),
            &[("DISTILL_FAIL_WRITE_STATE_FOR_RUN", &run_id)],
        ),
        "injected state write failure",
    );
    let record = record_for(worktree, &run_id, &operation_id);
    assert_eq!(record["status"], "confirmed");
    assert_eq!(
        record["artifact_id"],
        "github:matthewye/autopilot-toolkit#58"
    );

    let resumed = json_success(submit_evidence(
        worktree,
        &run_id,
        "session-github-resume",
        prd_revision,
        "prd",
        &prd_evidence(),
    ));
    assert_eq!(resumed["stage"], "issues");
    assert_eq!(
        resumed["publications"]["prd"]["artifact_id"],
        "github:matthewye/autopilot-toolkit#58"
    );
}

#[test]
fn github_issue_receipts_preserve_confirmed_dependency_artifact_ids() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path();
    write_github_tracker_config(worktree);
    let (run_id, prd_revision) = reach_prd_stage(worktree, "session-github-issues");
    let after_prd = json_success(submit_evidence(
        worktree,
        &run_id,
        "session-github-issues",
        prd_revision,
        "prd",
        &github_prd_evidence(&format!("{run_id}-r{prd_revision}-prd"), 59, "confirmed"),
    ));
    let issues_revision = after_prd["revision"].as_u64().unwrap();

    let completed = json_success(submit_evidence(
        worktree,
        &run_id,
        "session-github-issues",
        issues_revision,
        "issues",
        &github_issue_evidence(&run_id, issues_revision),
    ));

    assert_eq!(completed["status"], "completed");
    assert_eq!(
        completed["publications"]["issues"][1]["dependency_artifact_ids"][0],
        "github:matthewye/autopilot-toolkit#60"
    );
    assert!(!worktree.join(".scratch/distill-tracer/issues").exists());
}

#[test]
fn github_rejects_dependency_edges_that_contradict_blocked_by_body() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path();
    write_github_tracker_config(worktree);
    let session = "session-github-dependency-contradiction";
    let (run_id, prd_revision) = reach_prd_stage(worktree, session);
    let after_prd = json_success(submit_evidence(
        worktree,
        &run_id,
        session,
        prd_revision,
        "prd",
        &github_prd_evidence(&format!("{run_id}-r{prd_revision}-prd"), 70, "confirmed"),
    ));
    let issues_revision = after_prd["revision"].as_u64().unwrap();
    let mut evidence = github_issue_evidence(&run_id, issues_revision);
    evidence["issues"][1]["depends_on"] = json!([]);

    assert_error_contains(
        submit_evidence(
            worktree,
            &run_id,
            session,
            issues_revision,
            "issues",
            &evidence,
        ),
        "must declare exactly '- None — can start immediately.'",
    );
}
