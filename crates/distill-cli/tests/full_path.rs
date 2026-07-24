use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn distill_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_distill"))
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crate should live under repo/crates/distill-cli")
        .to_path_buf()
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

fn distill(args: &[&str]) -> Output {
    Command::new(distill_bin())
        .args(args)
        .output()
        .expect("distill should execute")
}

fn assert_success(output: Output) -> Value {
    assert!(
        output.status.success(),
        "distill failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("stdout should be a JSON object")
}

fn state_for(worktree: &Path, run_id: &str) -> Value {
    let state_path = worktree
        .join(".distill/runs")
        .join(run_id)
        .join("state.json");
    serde_json::from_str(&fs::read_to_string(state_path).unwrap()).unwrap()
}

#[test]
fn codex_text_requirement_advances_only_by_authorized_stage_evidence() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path();
    write_local_tracker_config(worktree);

    let start = assert_success(distill(&[
        "start",
        "--json",
        "--runtime",
        "codex",
        "--session-id",
        "codex-thread-abc",
        "--worktree",
        worktree.to_str().unwrap(),
        "--requirement",
        "Build a small audit dashboard for deployment status.",
    ]));

    assert_eq!(start["status"], "active");
    assert_eq!(start["runtime"], "codex");
    assert_eq!(start["session_id"], "codex-thread-abc");
    assert_eq!(start["stage"], "clarification");
    assert_eq!(start["revision"], 1);
    assert_eq!(start["next_action"], "invoke-skill");
    assert_eq!(start["authorized_action"]["skill"], "grill-with-docs");
    assert_eq!(start["implementation_started"], false);

    let run_id = start["run_id"].as_str().unwrap();
    let started_state = state_for(worktree, run_id);
    assert_eq!(started_state["state"], "active");
    assert_eq!(started_state["session_binding"]["runtime"], "codex");
    assert_eq!(
        started_state["session_binding"]["session_id"],
        "codex-thread-abc"
    );
    assert_eq!(started_state["workflow"]["version"], "distill.v1");
    assert_eq!(
        started_state["workflow"]["source"],
        "embedded:distill.v1.json"
    );
    assert_eq!(
        started_state["completion_evidence"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(started_state["completion_evidence"][0]["stage"], "intake");

    let workflow_file: Value = serde_json::from_str(
        &fs::read_to_string(repo_root().join("crates/distill-cli/workflows/distill.v1.json"))
            .expect("workflow definition should exist"),
    )
    .unwrap();
    assert_eq!(started_state["workflow"]["stages"], workflow_file["stages"]);

    let clarification = json!({
        "checkpoint": "clarification-complete",
        "summary": "The deployment dashboard tracks release status by environment.",
        "clarified_requirement": "The deployment dashboard tracks release status by environment.",
        "decisions": [],
        "accepted_assumptions": [],
        "material_unknowns": [],
        "domain_document_artifacts": []
    })
    .to_string();
    let after_clarification = assert_success(distill(&[
        "submit-evidence",
        "--json",
        "--worktree",
        worktree.to_str().unwrap(),
        "--run-id",
        run_id,
        "--session-id",
        "codex-thread-abc",
        "--expected-revision",
        "1",
        "--stage",
        "clarification",
        "--evidence",
        &clarification,
    ]));
    assert_eq!(after_clarification["status"], "active");
    assert_eq!(after_clarification["stage"], "prd");
    assert_eq!(after_clarification["revision"], 2);
    assert_eq!(after_clarification["authorized_action"]["skill"], "to-prd");

    let prd_markdown = "# Audit Dashboard PRD\n\n## Problem Statement\n\nBuild a small audit dashboard for deployment status.\n\n## Testing Decisions\n\nUse CLI-level JSON and filesystem assertions.\n";
    let prd = json!({
        "checkpoint": "testing-seam-confirmed",
        "summary": "PRD captures the caller-visible CLI seam.",
        "prd_markdown": prd_markdown
    })
    .to_string();
    let after_prd = assert_success(distill(&[
        "submit-evidence",
        "--json",
        "--worktree",
        worktree.to_str().unwrap(),
        "--run-id",
        run_id,
        "--session-id",
        "codex-thread-abc",
        "--expected-revision",
        "2",
        "--stage",
        "prd",
        "--evidence",
        &prd,
    ]));
    assert_eq!(after_prd["status"], "active");
    assert_eq!(after_prd["stage"], "issues");
    assert_eq!(after_prd["revision"], 3);
    assert_eq!(after_prd["authorized_action"]["skill"], "to-issues");

    let issues = json!({
        "checkpoint": "slice-breakdown-approved",
        "summary": "One ready vertical slice is sufficient for the fixture.",
        "issues": [{
            "title": "Build audit dashboard slice",
            "body": "Status: ready-for-agent\n\n## Parent\n\n.scratch/distill-tracer/PRD.md\n\n## What to build\n\nCreate the first vertical slice for the audit dashboard.\n\n## Acceptance criteria\n\n- [ ] The dashboard status is externally verifiable end to end.\n\n## Blocked by\n\nNone - can start immediately\n"
        }]
    })
    .to_string();
    let completed = assert_success(distill(&[
        "submit-evidence",
        "--json",
        "--worktree",
        worktree.to_str().unwrap(),
        "--run-id",
        run_id,
        "--session-id",
        "codex-thread-abc",
        "--expected-revision",
        "3",
        "--stage",
        "issues",
        "--evidence",
        &issues,
    ]));

    assert_eq!(completed["status"], "completed");
    assert_eq!(completed["stage"], "completed");
    assert_eq!(completed["revision"], 4);
    assert_eq!(completed["next_action"], "terminal");
    assert_eq!(completed["implementation_started"], false);

    let prd_path = worktree.join(completed["publications"]["prd"]["path"].as_str().unwrap());
    assert!(prd_path.is_file(), "PRD should be published");
    let prd = fs::read_to_string(&prd_path).unwrap();
    assert!(prd.contains("## Problem Statement"));
    assert!(prd.contains("Build a small audit dashboard"));

    let issue_path = worktree.join(
        completed["publications"]["issues"][0]["path"]
            .as_str()
            .unwrap(),
    );
    assert!(issue_path.is_file(), "issue should be published");
    let issue = fs::read_to_string(&issue_path).unwrap();
    assert!(issue.contains("Status: ready-for-agent"));
    assert!(issue.contains("## Acceptance criteria"));
    assert!(!issue.contains("implement orchestrator"));

    let report_json_path = worktree.join(completed["report"]["json_path"].as_str().unwrap());
    let report_md_path = worktree.join(completed["report"]["markdown_path"].as_str().unwrap());
    assert!(
        report_json_path.is_file(),
        "canonical JSON report should exist"
    );
    assert!(report_md_path.is_file(), "Markdown report should exist");
    let report_json: Value =
        serde_json::from_str(&fs::read_to_string(&report_json_path).unwrap()).unwrap();
    assert_eq!(report_json["run_id"], completed["run_id"]);
    assert_eq!(report_json["completion"]["state"], "completed");
    assert!(fs::read_to_string(&report_md_path)
        .unwrap()
        .contains("# Distill Completion Report"));

    let completed_state = state_for(worktree, run_id);
    assert_eq!(completed_state["state"], "completed");
    assert_eq!(
        completed_state["completion_evidence"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    assert_eq!(
        completed_state["completion_evidence"][3]["accepted_user_checkpoint"],
        "slice-breakdown-approved"
    );
}

#[test]
fn rejects_skipped_or_reordered_stage_evidence() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path();
    write_local_tracker_config(worktree);

    let start = assert_success(distill(&[
        "start",
        "--json",
        "--runtime",
        "codex",
        "--session-id",
        "codex-thread-xyz",
        "--worktree",
        worktree.to_str().unwrap(),
        "--requirement",
        "Build a small audit dashboard for deployment status.",
    ]));
    let run_id = start["run_id"].as_str().unwrap();

    let skipped_prd = json!({
        "checkpoint": "testing-seam-confirmed",
        "summary": "Trying to skip clarification.",
        "prd_markdown": "# PRD\n"
    })
    .to_string();
    let output = distill(&[
        "submit-evidence",
        "--json",
        "--worktree",
        worktree.to_str().unwrap(),
        "--run-id",
        run_id,
        "--session-id",
        "codex-thread-xyz",
        "--expected-revision",
        "1",
        "--stage",
        "prd",
        "--evidence",
        &skipped_prd,
    ]);

    assert!(
        !output.status.success(),
        "out-of-order evidence should fail"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("stage prd is not authorized; expected clarification"),
        "stderr should explain the authorization failure: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let unchanged = state_for(worktree, run_id);
    assert_eq!(unchanged["state"], "active");
    assert_eq!(unchanged["current_stage"], "clarification");
    assert_eq!(
        unchanged["completion_evidence"].as_array().unwrap().len(),
        1
    );
}

#[test]
fn codex_skill_runs_to_boundaries_with_unmodified_stage_skills() {
    let skill = fs::read_to_string(repo_root().join("skills/autopilot/distill/codex/SKILL.md"))
        .expect("codex distill skill should exist");

    assert!(skill.contains("~/.agents/skills/.autopilot/bin/distill"));
    assert!(skill.contains("--session-id"));
    assert!(skill.contains("thread"));
    assert!(skill.contains("distill}\" start --json"));
    assert!(skill.contains("submit-evidence"));
    assert!(skill.contains("authorized_action"));
    assert!(skill.contains("next_action"));
    assert!(skill.contains("grill-with-docs"));
    assert!(skill.contains("to-prd"));
    assert!(skill.contains("to-issues"));
}
