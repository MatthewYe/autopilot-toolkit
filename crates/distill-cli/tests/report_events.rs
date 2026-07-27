use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn distill_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_distill"))
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

fn write_local_tracker_config(root: &Path) {
    fs::create_dir_all(root.join("docs/agents")).unwrap();
    fs::write(
        root.join("docs/agents/issue-tracker.md"),
        "# Issue tracker: Local Markdown\n\nIssues and PRDs live in `.scratch/`.\n",
    )
    .unwrap();
    fs::write(root.join("docs/agents/triage-labels.md"), "# Labels\n").unwrap();
    fs::write(root.join("docs/agents/domain.md"), "# Domain\n").unwrap();
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
        "Build a release readiness dashboard.",
    ]))
}

fn submit(
    worktree: &Path,
    run_id: &str,
    session: &str,
    revision: u64,
    stage: &str,
    evidence: Value,
) -> Value {
    let evidence = evidence.to_string();
    json_success(distill(&[
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
    ]))
}

fn complete_run(worktree: &Path, session: &str) -> (String, Value) {
    let started = start(worktree, session);
    let run_id = started["run_id"].as_str().unwrap().to_string();
    let after_clarification = submit(
        worktree,
        &run_id,
        session,
        1,
        "clarification",
        json!({
            "checkpoint": "clarification-complete",
            "summary": "The dashboard tracks release readiness across environments.",
            "clarified_requirement": "The dashboard tracks release readiness across environments.",
            "decisions": [],
            "accepted_assumptions": [],
            "material_unknowns": [],
            "domain_document_artifacts": []
        }),
    );
    let after_prd = submit(
        worktree,
        &run_id,
        session,
        after_clarification["revision"].as_u64().unwrap(),
        "prd",
        json!({
            "checkpoint": "testing-seam-confirmed",
            "summary": "The PRD fixes the CLI seam.",
            "prd_markdown": "# PRD\n\n## Problem Statement\n\nBuild release readiness reporting.\n"
        }),
    );
    let completed = submit(
        worktree,
        &run_id,
        session,
        after_prd["revision"].as_u64().unwrap(),
        "issues",
        json!({
            "checkpoint": "slice-breakdown-approved",
            "summary": "One independently implementable slice is ready.",
            "issues": [{
                "title": "Build readiness summary",
                "body": "Status: ready-for-agent\n\n## Acceptance criteria\n\n- [ ] Summary is queryable.\n"
            }]
        }),
    );
    (run_id, completed)
}

#[test]
fn completed_run_exposes_versioned_state_events_and_canonical_report() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path();
    write_local_tracker_config(worktree);
    let (run_id, completed) = complete_run(worktree, "session-report");
    assert_eq!(completed["status"], "completed");

    let polled = json_success(distill(&[
        "inspect",
        "--json",
        "--worktree",
        worktree.to_str().unwrap(),
        "--run-id",
        &run_id,
        "--session-id",
        "session-report",
        "--expected-revision",
        completed["revision"].as_u64().unwrap().to_string().as_str(),
    ]));
    assert_eq!(polled["schema_version"], 1);
    assert_eq!(polled["status"], "completed");
    assert_eq!(polled["session_binding"]["released"], true);

    let events = json_success(distill(&[
        "events",
        "--json",
        "--worktree",
        worktree.to_str().unwrap(),
        "--run-id",
        &run_id,
        "--after",
        "0",
    ]));
    assert_eq!(events["schema_version"], 1);
    let events = events["events"].as_array().unwrap();
    let types = events
        .iter()
        .map(|event| event["type"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(types.contains(&"session-bound"));
    assert!(types.contains(&"intake-completed"));
    assert!(types.contains(&"stage-completed"));
    assert!(types.contains(&"publication-recorded"));
    assert!(types.contains(&"terminal-completed"));
    for (index, event) in events.iter().enumerate() {
        assert_eq!(event["event_version"], 1);
        assert_eq!(event["sequence"], (index as u64) + 1);
    }

    let report_path = worktree.join(polled["report"]["json_path"].as_str().unwrap());
    let report: Value = serde_json::from_str(&fs::read_to_string(report_path).unwrap()).unwrap();
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["report_version"], "distill.report.v1");
    assert_eq!(report["status"], "completed");
    assert_eq!(report["run_id"], run_id);
    assert!(report["sources"].as_array().unwrap().len() >= 1);
    assert!(report["requirement"]["text"]
        .as_str()
        .unwrap()
        .contains("release readiness"));
    assert!(report["decisions"].is_array());
    assert!(report["assumptions"].is_array());
    assert!(report["domain_changes"].is_array());
    assert!(report["publications"]["issues"].as_array().unwrap().len() == 1);
    assert_eq!(report["final_revision"], completed["revision"]);
    assert_eq!(report["versions"]["state_schema"], 1);
    assert_eq!(report["session"]["released"], true);
    assert!(report["storage"]["limits"].is_object());

    let markdown_path = worktree.join(polled["report"]["markdown_path"].as_str().unwrap());
    let markdown = fs::read_to_string(markdown_path).unwrap();
    assert!(markdown.contains("Distill Completion Report"));
}

#[test]
fn large_evidence_is_stored_as_artifact_and_events_stay_below_budget() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path();
    write_local_tracker_config(worktree);
    let started = start(worktree, "session-large-evidence");
    let run_id = started["run_id"].as_str().unwrap().to_string();
    let after_clarification = submit(
        worktree,
        &run_id,
        "session-large-evidence",
        1,
        "clarification",
        json!({
            "checkpoint": "clarification-complete",
            "summary": "Large PRD evidence should not be embedded in events.",
            "clarified_requirement": "Large PRD evidence should not be embedded in events.",
            "decisions": [],
            "accepted_assumptions": [],
            "material_unknowns": [],
            "domain_document_artifacts": []
        }),
    );
    let large_markdown = format!("# PRD\n\n{}", "Long evidence body.\n".repeat(5_000));
    let after_prd = submit(
        worktree,
        &run_id,
        "session-large-evidence",
        after_clarification["revision"].as_u64().unwrap(),
        "prd",
        json!({
            "checkpoint": "testing-seam-confirmed",
            "summary": "Large PRD captured.",
            "prd_markdown": large_markdown
        }),
    );
    assert_eq!(after_prd["stage"], "issues");

    let event_log = fs::read_to_string(
        worktree
            .join(".distill/runs")
            .join(&run_id)
            .join("events.jsonl"),
    )
    .unwrap();
    for line in event_log.lines() {
        assert!(line.len() < 64 * 1024, "event line exceeded event budget");
    }
    let prd_event: Value = event_log
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .find(|event| event["type"] == "stage-completed" && event["payload"]["stage"] == "prd")
        .expect("prd stage event should exist");
    let artifact_path = prd_event["payload"]["evidence"]["artifact_path"]
        .as_str()
        .unwrap();
    assert_eq!(
        prd_event["payload"]["evidence"]["sha256"]
            .as_str()
            .unwrap()
            .len(),
        64
    );
    assert!(worktree.join(artifact_path).is_file());
    assert!(fs::read_to_string(worktree.join(artifact_path))
        .unwrap()
        .contains("Long evidence body."));
}

#[test]
fn renderer_failure_is_retryable_without_reopening_or_mutating_canonical_report() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path();
    write_local_tracker_config(worktree);
    let started = start(worktree, "session-render-retry");
    let run_id = started["run_id"].as_str().unwrap().to_string();
    let after_clarification = submit(
        worktree,
        &run_id,
        "session-render-retry",
        1,
        "clarification",
        json!({
            "checkpoint": "clarification-complete",
            "summary": "Renderer retry behavior is in scope.",
            "clarified_requirement": "Renderer retry behavior is in scope.",
            "decisions": [],
            "accepted_assumptions": [],
            "material_unknowns": [],
            "domain_document_artifacts": []
        }),
    );
    let after_prd = submit(
        worktree,
        &run_id,
        "session-render-retry",
        after_clarification["revision"].as_u64().unwrap(),
        "prd",
        json!({
            "checkpoint": "testing-seam-confirmed",
            "summary": "PRD captured.",
            "prd_markdown": "# PRD\n\n## Problem Statement\n\nRenderer retry test.\n"
        }),
    );
    let issue_evidence = json!({
        "checkpoint": "slice-breakdown-approved",
        "summary": "Complete with renderer failure injected.",
        "issues": [{
            "title": "Build renderer retry",
            "body": "Status: ready-for-agent\n\n## Acceptance criteria\n\n- [ ] Retry works.\n"
        }]
    })
    .to_string();
    let completed = json_success(distill_with_env(
        &[
            "submit-evidence",
            "--json",
            "--worktree",
            worktree.to_str().unwrap(),
            "--run-id",
            &run_id,
            "--session-id",
            "session-render-retry",
            "--expected-revision",
            after_prd["revision"].as_u64().unwrap().to_string().as_str(),
            "--stage",
            "issues",
            "--evidence",
            &issue_evidence,
        ],
        &[("DISTILL_FAIL_RENDERER_FOR_RUN", &run_id)],
    ));
    assert_eq!(completed["status"], "completed");
    assert_eq!(completed["report"]["renderer"]["status"], "failed");
    assert_eq!(completed["report"]["renderer"]["retryable"], true);

    let report_path = worktree.join(completed["report"]["json_path"].as_str().unwrap());
    let before = fs::read_to_string(&report_path).unwrap();
    let report: Value = serde_json::from_str(&before).unwrap();
    assert_eq!(report["warnings"][0]["type"], "renderer-failed");
    assert!(!worktree
        .join(completed["report"]["markdown_path"].as_str().unwrap())
        .exists());

    let rendered = json_success(distill(&[
        "render-report",
        "--json",
        "--worktree",
        worktree.to_str().unwrap(),
        "--run-id",
        &run_id,
        "--renderer",
        "markdown",
    ]));
    assert_eq!(rendered["status"], "rendered");
    assert_eq!(rendered["canonical_hash"], report["canonical_hash"]);
    assert_eq!(fs::read_to_string(&report_path).unwrap(), before);
    assert!(worktree
        .join(rendered["markdown_path"].as_str().unwrap())
        .is_file());

    let state: Value = serde_json::from_str(
        &fs::read_to_string(
            worktree
                .join(".distill/runs")
                .join(&run_id)
                .join("state.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(state["state"], "completed");
}

#[test]
fn audit_stream_records_takeover_quota_and_migration_transitions() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path();
    write_local_tracker_config(worktree);
    let started = start(worktree, "session-transition-audit");
    let run_id = started["run_id"].as_str().unwrap().to_string();

    let taken = json_success(distill(&[
        "takeover",
        "--json",
        "--worktree",
        worktree.to_str().unwrap(),
        "--run-id",
        &run_id,
        "--from-session",
        "session-transition-audit",
        "--to-session",
        "session-transition-audit-new",
        "--expected-revision",
        "1",
        "--reason",
        "User moved the run to a replacement session.",
        "--user-authorized",
    ]));
    assert_eq!(taken["session_id"], "session-transition-audit-new");

    let events = json_success(distill(&[
        "events",
        "--json",
        "--worktree",
        worktree.to_str().unwrap(),
        "--run-id",
        &run_id,
        "--after",
        "0",
    ]));
    assert!(events["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|event| event["type"] == "session-takeover"));

    json_success(distill(&[
        "set-project-quota",
        "--json",
        "--worktree",
        worktree.to_str().unwrap(),
        "--bytes",
        "268435456",
    ]));
    let quota_event: Value = serde_json::from_str(
        fs::read_to_string(worktree.join(".distill/quota-events.jsonl"))
            .unwrap()
            .lines()
            .last()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(quota_event["schema_version"], 1);
    assert_eq!(quota_event["type"], "project-quota-changed");

    let state_path = worktree
        .join(".distill/runs")
        .join(&run_id)
        .join("state.json");
    let mut state: Value = serde_json::from_str(&fs::read_to_string(&state_path).unwrap()).unwrap();
    state["schema_version"] = json!(0);
    fs::write(&state_path, serde_json::to_string_pretty(&state).unwrap()).unwrap();
    json_success(distill(&[
        "inspect",
        "--json",
        "--worktree",
        worktree.to_str().unwrap(),
        "--run-id",
        &run_id,
        "--session-id",
        "session-transition-audit-new",
        "--expected-revision",
        "2",
    ]));
    let events = json_success(distill(&[
        "events",
        "--json",
        "--worktree",
        worktree.to_str().unwrap(),
        "--run-id",
        &run_id,
        "--after",
        "0",
    ]));
    assert!(events["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|event| event["type"] == "state-migrated"));
}
