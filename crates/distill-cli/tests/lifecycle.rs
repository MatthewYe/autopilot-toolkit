use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn distill_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_distill"))
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

fn submit(worktree: &Path, run_id: &str, session: &str, revision: u64, stage: &str) -> Output {
    let evidence = match stage {
        "clarification" => json!({
            "checkpoint": "clarification-complete",
            "summary": "Deployment status by environment is the core workflow.",
            "clarified_requirement": "Deployment status by environment is the core workflow.",
            "decisions": [],
            "accepted_assumptions": [],
            "material_unknowns": [],
            "domain_document_artifacts": []
        }),
        "prd" => json!({
            "checkpoint": "testing-seam-confirmed",
            "summary": "PRD keeps the CLI seam.",
            "prd_markdown": "# PRD\n\n## Problem Statement\n\nBuild a small audit dashboard.\n"
        }),
        "issues" => json!({
            "checkpoint": "slice-breakdown-approved",
            "summary": "One slice is ready.",
            "issues": [{
                "title": "Build audit dashboard",
                "body": "---\nkey: 01-build-audit-dashboard\ntitle: Build audit dashboard\ntype: issue\nstatus: ready-for-agent\nparent: .scratch/distill-tracer/PRD.md\n---\n\n## What to build\n\nBuild the audit dashboard.\n\n## Acceptance Criteria\n\n- [ ] The dashboard is visible.\n\n## Blocked by\n\n- None — can start immediately.\n\n## Comments\n",
                "depends_on": []
            }]
        }),
        other => panic!("unknown stage fixture: {other}"),
    }
    .to_string();
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

fn init_git_repo(worktree: &Path) {
    Command::new("git")
        .args(["init"])
        .current_dir(worktree)
        .output()
        .unwrap();
    Command::new("git")
        .args(["add", "."])
        .current_dir(worktree)
        .output()
        .unwrap();
    Command::new("git")
        .args([
            "-c",
            "user.email=test@example.com",
            "-c",
            "user.name=Test",
            "commit",
            "-m",
            "baseline",
        ])
        .current_dir(worktree)
        .output()
        .unwrap();
}

#[test]
fn revisions_locks_and_session_bindings_guard_mutations() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path();
    write_local_tracker_config(worktree);

    let first = start(worktree, "session-a");
    let run_a = first["run_id"].as_str().unwrap();
    assert_eq!(first["revision"], 1);

    let resumed = start(worktree, "session-a");
    assert_eq!(resumed["run_id"], run_a, "same session resumes active run");

    let independent = start(worktree, "session-b");
    assert_ne!(
        independent["run_id"], run_a,
        "different sessions may own independent unfinished runs"
    );

    assert_error_contains(
        submit(worktree, run_a, "session-b", 1, "clarification"),
        "session id does not match active run binding",
    );
    assert_error_contains(
        submit(worktree, run_a, "session-a", 0, "clarification"),
        "expected revision 0 is stale; current revision is 1",
    );

    let lock_path = worktree
        .join(".distill/runs")
        .join(run_a)
        .join("state.lock");
    fs::write(&lock_path, "{\"owner\":\"other-writer\"}").unwrap();
    assert_error_contains(
        submit(worktree, run_a, "session-a", 1, "clarification"),
        "run is locked by another writer",
    );
    fs::remove_file(lock_path).unwrap();

    let advanced = json_success(submit(worktree, run_a, "session-a", 1, "clarification"));
    assert_eq!(advanced["revision"], 2);
    assert_eq!(advanced["stage"], "prd");
    assert_error_contains(
        submit(worktree, run_a, "session-a", 2, "clarification"),
        "stage clarification is already completed",
    );
}

#[test]
fn start_uses_project_lock_and_rejects_duplicate_session_bindings() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path();
    write_local_tracker_config(worktree);

    let first = start(worktree, "session-start-lock");
    let run_id = first["run_id"].as_str().unwrap();
    let duplicate_dir = worktree.join(".distill/runs/manual-duplicate");
    fs::create_dir_all(&duplicate_dir).unwrap();
    let mut duplicate = state_for(worktree, run_id);
    duplicate["run_id"] = json!("manual-duplicate");
    fs::write(
        duplicate_dir.join("state.json"),
        serde_json::to_string_pretty(&duplicate).unwrap(),
    )
    .unwrap();

    assert_error_contains(
        distill(&[
            "start",
            "--json",
            "--runtime",
            "codex",
            "--session-id",
            "session-start-lock",
            "--worktree",
            worktree.to_str().unwrap(),
            "--requirement",
            "Attempt duplicate resume.",
        ]),
        "multiple unfinished runs are bound to session session-start-lock",
    );

    fs::remove_dir_all(duplicate_dir).unwrap();
    let start_lock = worktree.join(".distill/start.lock");
    fs::write(&start_lock, "{\"owner\":\"other-start\"}").unwrap();
    assert_error_contains(
        distill(&[
            "start",
            "--json",
            "--runtime",
            "codex",
            "--session-id",
            "session-start-lock-2",
            "--worktree",
            worktree.to_str().unwrap(),
            "--requirement",
            "Attempt concurrent start.",
        ]),
        "project start is locked by another writer",
    );

    fs::write(&start_lock, "{\"owner\":\"lost-start\",\"stale\":true}").unwrap();
    let recovered = start(worktree, "session-start-lock-2");
    assert_eq!(recovered["session_id"], "session-start-lock-2");
}

#[test]
fn lost_process_locks_takeover_and_supersession_are_audited() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path();
    write_local_tracker_config(worktree);

    let first = start(worktree, "session-a");
    let run_id = first["run_id"].as_str().unwrap();

    let stale_lock = worktree
        .join(".distill/runs")
        .join(run_id)
        .join("state.lock");
    fs::write(&stale_lock, "{\"owner\":\"lost-process\",\"stale\":true}").unwrap();
    let recovered = json_success(submit(worktree, run_id, "session-a", 1, "clarification"));
    assert_eq!(
        recovered["revision"], 2,
        "stale lost-process lock is recovered"
    );

    let takeover = json_success(distill(&[
        "takeover",
        "--json",
        "--worktree",
        worktree.to_str().unwrap(),
        "--run-id",
        run_id,
        "--from-session",
        "session-a",
        "--to-session",
        "session-c",
        "--expected-revision",
        "2",
        "--reason",
        "Original Codex thread was interrupted.",
        "--user-authorized",
    ]));
    assert_eq!(takeover["session_id"], "session-c");
    assert_eq!(takeover["revision"], 3);

    assert_error_contains(
        submit(worktree, run_id, "session-a", 3, "prd"),
        "session id does not match active run binding",
    );

    let after_prd = json_success(submit(worktree, run_id, "session-c", 3, "prd"));
    assert_eq!(after_prd["revision"], 4);

    let superseded = json_success(distill(&[
        "supersede",
        "--json",
        "--worktree",
        worktree.to_str().unwrap(),
        "--run-id",
        run_id,
        "--session-id",
        "session-c",
        "--expected-revision",
        "4",
        "--reason",
        "New compliance requirement changes the accepted PRD.",
        "--requirement",
        "Build an audit dashboard with compliance controls.",
    ]));
    assert_eq!(superseded["status"], "superseded");
    let successor_id = superseded["successor_run_id"].as_str().unwrap();
    assert_ne!(successor_id, run_id);

    let old_state = state_for(worktree, run_id);
    assert_eq!(old_state["state"], "superseded");
    assert_eq!(old_state["superseded_by"], successor_id);
    assert_eq!(old_state["handoffs"].as_array().unwrap().len(), 1);

    let successor = state_for(worktree, successor_id);
    assert_eq!(successor["state"], "active");
    assert_eq!(successor["predecessor_run_id"], run_id);
    assert_eq!(successor["session_binding"]["session_id"], "session-c");
}

#[test]
fn supersession_does_not_mark_old_run_when_successor_write_fails() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path();
    write_local_tracker_config(worktree);

    let first = start(worktree, "session-atomic");
    let run_id = first["run_id"].as_str().unwrap();
    let after_clarification = json_success(submit(
        worktree,
        run_id,
        "session-atomic",
        1,
        "clarification",
    ));
    assert_eq!(after_clarification["revision"], 2);

    let blocked_successor = worktree
        .join(".distill/runs")
        .join("run-session-atomic-12345");
    fs::write(&blocked_successor, "not a directory").unwrap();
    assert_error_contains(
        distill_with_env(
            &[
                "supersede",
                "--json",
                "--worktree",
                worktree.to_str().unwrap(),
                "--run-id",
                run_id,
                "--session-id",
                "session-atomic",
                "--expected-revision",
                "2",
                "--reason",
                "New information makes clarification obsolete.",
                "--requirement",
                "Build a changed audit dashboard.",
            ],
            &[("DISTILL_FIXED_TIMESTAMP_MILLIS", "12345")],
        ),
        "cannot create successor run",
    );

    let old_state = state_for(worktree, run_id);
    assert_eq!(old_state["state"], "active");
    assert!(old_state["superseded_by"].is_null());
}

#[test]
fn supersession_rolls_back_successor_when_predecessor_write_fails() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path();
    write_local_tracker_config(worktree);

    let first = start(worktree, "session-predecessor-fail");
    let run_id = first["run_id"].as_str().unwrap();
    let after_clarification = json_success(submit(
        worktree,
        run_id,
        "session-predecessor-fail",
        1,
        "clarification",
    ));
    assert_eq!(after_clarification["revision"], 2);

    assert_error_contains(
        distill_with_env(
            &[
                "supersede",
                "--json",
                "--worktree",
                worktree.to_str().unwrap(),
                "--run-id",
                run_id,
                "--session-id",
                "session-predecessor-fail",
                "--expected-revision",
                "2",
                "--reason",
                "New information makes clarification obsolete.",
                "--requirement",
                "Build a changed audit dashboard.",
            ],
            &[
                ("DISTILL_FIXED_TIMESTAMP_MILLIS", "54321"),
                ("DISTILL_FAIL_WRITE_STATE_FOR_RUN", run_id),
            ],
        ),
        "cannot supersede predecessor run",
    );

    let old_state = state_for(worktree, run_id);
    assert_eq!(old_state["state"], "active");
    assert!(old_state["superseded_by"].is_null());

    let successor_path = worktree
        .join(".distill/runs")
        .join("run-session-predecessor-fail-54321")
        .join("state.json");
    if successor_path.exists() {
        let successor: Value =
            serde_json::from_str(&fs::read_to_string(successor_path).unwrap()).unwrap();
        assert_ne!(successor["state"], "active");
    }
}

#[test]
fn supersession_restores_predecessor_when_successor_activation_fails() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path();
    write_local_tracker_config(worktree);

    let first = start(worktree, "session-activation-fail");
    let run_id = first["run_id"].as_str().unwrap();
    let after_clarification = json_success(submit(
        worktree,
        run_id,
        "session-activation-fail",
        1,
        "clarification",
    ));
    assert_eq!(after_clarification["revision"], 2);

    assert_error_contains(
        distill_with_env(
            &[
                "supersede",
                "--json",
                "--worktree",
                worktree.to_str().unwrap(),
                "--run-id",
                run_id,
                "--session-id",
                "session-activation-fail",
                "--expected-revision",
                "2",
                "--reason",
                "New information makes clarification obsolete.",
                "--requirement",
                "Build a changed audit dashboard.",
            ],
            &[
                ("DISTILL_FIXED_TIMESTAMP_MILLIS", "67890"),
                (
                    "DISTILL_FAIL_WRITE_STATE_FOR_RUN",
                    "run-session-activation-fail-67890",
                ),
            ],
        ),
        "cannot activate successor run",
    );

    let old_state = state_for(worktree, run_id);
    assert_eq!(old_state["state"], "active");
    assert!(old_state["superseded_by"].is_null());

    let successor_path = worktree
        .join(".distill/runs")
        .join("run-session-activation-fail-67890")
        .join("state.json");
    if successor_path.exists() {
        let successor: Value =
            serde_json::from_str(&fs::read_to_string(successor_path).unwrap()).unwrap();
        assert_ne!(successor["state"], "active");
    }
}

#[test]
fn baselines_drift_corrupt_state_and_migrations_fail_or_advance_safely() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path();
    write_local_tracker_config(worktree);
    init_git_repo(worktree);
    fs::write(
        worktree.join("docs/agents/domain.md"),
        "# Domain\n\nchanged before start\n",
    )
    .unwrap();

    let first = start(worktree, "session-drift");
    let run_id = first["run_id"].as_str().unwrap();
    let state = state_for(worktree, run_id);
    assert_eq!(state["schema_version"], 1);
    assert_eq!(state["context_baseline"]["dirty"], true);
    assert!(state["context_baseline"]["domain_documents"][0]["path"]
        .as_str()
        .unwrap()
        .ends_with("docs/agents/domain.md"));

    fs::write(
        worktree.join("docs/agents/domain.md"),
        "# Domain\n\nchanged after start\n",
    )
    .unwrap();
    assert_error_contains(
        submit(worktree, run_id, "session-drift", 1, "clarification"),
        "context drift detected",
    );

    let ack = json!({
        "checkpoint": "clarification-complete",
        "summary": "The domain edit is spelling-only for this run.",
        "clarified_requirement": "Build a small audit dashboard for deployment status.",
        "decisions": [],
        "accepted_assumptions": [],
        "material_unknowns": [],
        "domain_document_artifacts": [],
        "drift_acknowledgment": {
            "material": false,
            "reason": "The changed domain prose does not alter the dashboard requirement."
        }
    })
    .to_string();
    let acknowledged = json_success(distill(&[
        "submit-evidence",
        "--json",
        "--worktree",
        worktree.to_str().unwrap(),
        "--run-id",
        run_id,
        "--session-id",
        "session-drift",
        "--expected-revision",
        "1",
        "--stage",
        "clarification",
        "--evidence",
        &ack,
    ]));
    assert_eq!(acknowledged["revision"], 2);
    let acknowledged_state = state_for(worktree, run_id);
    assert_eq!(
        acknowledged_state["drift_acknowledgments"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    let corrupt_dir = worktree.join(".distill/runs/corrupt");
    fs::create_dir_all(&corrupt_dir).unwrap();
    fs::write(corrupt_dir.join("state.json"), "{not json").unwrap();
    assert_error_contains(
        distill(&[
            "inspect",
            "--json",
            "--worktree",
            worktree.to_str().unwrap(),
            "--run-id",
            "corrupt",
            "--session-id",
            "session-drift",
            "--expected-revision",
            "1",
        ]),
        "cannot parse state",
    );

    let legacy_dir = worktree.join(".distill/runs/legacy");
    fs::create_dir_all(&legacy_dir).unwrap();
    fs::write(
        legacy_dir.join("state.json"),
        json!({
            "schema_version": 0,
            "run_id": "legacy",
            "state": "active",
            "revision": 1,
            "current_stage": "clarification",
            "session_binding": {"runtime": "codex", "session_id": "legacy-session"},
            "workflow": first["workflow"],
            "requirement": {"source": "explicit-text", "text": "legacy requirement"},
            "stages": state["stages"],
            "completion_evidence": state["completion_evidence"],
            "publications": {"prd": Value::Null, "issues": []},
            "report": Value::Null,
            "implementation_started": false
        })
        .to_string(),
    )
    .unwrap();
    let migrated = json_success(distill(&[
        "inspect",
        "--json",
        "--worktree",
        worktree.to_str().unwrap(),
        "--run-id",
        "legacy",
        "--session-id",
        "legacy-session",
        "--expected-revision",
        "1",
    ]));
    assert_eq!(migrated["schema_version"], 1);
    assert!(legacy_dir.join("state.schema-0.backup.json").is_file());
    assert_eq!(migrated["migration_events"].as_array().unwrap().len(), 1);

    let newer_dir = worktree.join(".distill/runs/newer");
    fs::create_dir_all(&newer_dir).unwrap();
    fs::write(
        newer_dir.join("state.json"),
        json!({
            "schema_version": 999,
            "run_id": "newer",
            "state": "active",
            "revision": 1,
            "session_binding": {"runtime": "codex", "session_id": "newer-session"}
        })
        .to_string(),
    )
    .unwrap();
    assert_error_contains(
        distill(&[
            "inspect",
            "--json",
            "--worktree",
            worktree.to_str().unwrap(),
            "--run-id",
            "newer",
            "--session-id",
            "newer-session",
            "--expected-revision",
            "1",
        ]),
        "state schema 999 is newer than this CLI supports",
    );
}

#[test]
fn worktree_head_branch_and_status_drift_are_gated_like_domain_drift() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path();
    write_local_tracker_config(worktree);
    fs::write(worktree.join("README.md"), "baseline\n").unwrap();
    init_git_repo(worktree);

    let first = start(worktree, "session-worktree-drift");
    let run_id = first["run_id"].as_str().unwrap();
    fs::write(worktree.join("README.md"), "changed worktree status\n").unwrap();
    assert_error_contains(
        submit(
            worktree,
            run_id,
            "session-worktree-drift",
            1,
            "clarification",
        ),
        "context drift detected",
    );

    let ack = json!({
        "checkpoint": "clarification-complete",
        "summary": "The README edit is unrelated to the requirement.",
        "clarified_requirement": "Build a small audit dashboard for deployment status.",
        "decisions": [],
        "accepted_assumptions": [],
        "material_unknowns": [],
        "domain_document_artifacts": [],
        "drift_acknowledgment": {
            "material": false,
            "reason": "README status drift does not alter the dashboard requirement."
        }
    })
    .to_string();
    let acknowledged = json_success(distill(&[
        "submit-evidence",
        "--json",
        "--worktree",
        worktree.to_str().unwrap(),
        "--run-id",
        run_id,
        "--session-id",
        "session-worktree-drift",
        "--expected-revision",
        "1",
        "--stage",
        "clarification",
        "--evidence",
        &ack,
    ]));
    assert_eq!(acknowledged["revision"], 2);
    let drift_records = state_for(worktree, run_id)["drift_acknowledgments"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(drift_records.len(), 1);
    assert!(drift_records[0]["detected"]
        .to_string()
        .contains("git_status"));
}

#[test]
fn inspect_migration_requires_expected_revision() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path();
    write_local_tracker_config(worktree);
    let first = start(worktree, "session-inspect");
    let state = state_for(worktree, first["run_id"].as_str().unwrap());

    let legacy_dir = worktree.join(".distill/runs/inspect-legacy");
    fs::create_dir_all(&legacy_dir).unwrap();
    fs::write(
        legacy_dir.join("state.json"),
        json!({
            "schema_version": 0,
            "run_id": "inspect-legacy",
            "state": "active",
            "revision": 1,
            "current_stage": "clarification",
            "session_binding": {"runtime": "codex", "session_id": "inspect-session"},
            "workflow": first["workflow"],
            "requirement": {"source": "explicit-text", "text": "legacy requirement"},
            "stages": state["stages"],
            "completion_evidence": state["completion_evidence"],
            "publications": {"prd": Value::Null, "issues": []},
            "report": Value::Null,
            "implementation_started": false
        })
        .to_string(),
    )
    .unwrap();

    assert_error_contains(
        distill(&[
            "inspect",
            "--json",
            "--worktree",
            worktree.to_str().unwrap(),
            "--run-id",
            "inspect-legacy",
            "--session-id",
            "inspect-session",
        ]),
        "--expected-revision is required",
    );

    let migrated = json_success(distill(&[
        "inspect",
        "--json",
        "--worktree",
        worktree.to_str().unwrap(),
        "--run-id",
        "inspect-legacy",
        "--session-id",
        "inspect-session",
        "--expected-revision",
        "1",
    ]));
    assert_eq!(migrated["schema_version"], 1);
}
