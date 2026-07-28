use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::symlink;
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

fn write_project(root: &Path) {
    fs::create_dir_all(root.join("docs/agents")).unwrap();
    fs::create_dir_all(root.join("docs/adr")).unwrap();
    fs::write(
        root.join("docs/agents/issue-tracker.md"),
        "# Issue tracker: Local Markdown\n\nIssues and PRDs live in `.scratch/`.\n",
    )
    .unwrap();
    fs::write(root.join("docs/agents/triage-labels.md"), "# Labels\n").unwrap();
    fs::write(root.join("docs/agents/domain.md"), "# Domain\n").unwrap();
}

fn init_git(root: &Path) {
    json_success_command(
        Command::new("git")
            .args(["init"])
            .current_dir(root)
            .output()
            .unwrap(),
    );
    json_success_command(
        Command::new("git")
            .args(["add", "."])
            .current_dir(root)
            .output()
            .unwrap(),
    );
    json_success_command(
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
            .current_dir(root)
            .output()
            .unwrap(),
    );
}

fn json_success_command(output: Output) {
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
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
        "Build a release-readiness dashboard.",
    ]))
}

fn submit(
    worktree: &Path,
    run_id: &str,
    session: &str,
    revision: u64,
    stage: &str,
    evidence: &Value,
) -> Output {
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
        &evidence.to_string(),
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

fn sha256(content: &[u8]) -> String {
    Sha256::digest(content)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn clarification_evidence(worktree: &Path) -> Value {
    let domain_path = worktree.join("docs/agents/domain.md");
    let domain_bytes = fs::read(domain_path).unwrap();
    json!({
        "checkpoint": "clarification-complete",
        "summary": "Clarification fixed the delivery and testing seams.",
        "clarified_requirement": "Build a release-readiness dashboard with a JSON CLI seam.",
        "decisions": ["Expose release readiness through the existing CLI."],
        "accepted_assumptions": ["The local markdown tracker is authoritative for this fixture."],
        "material_unknowns": [{
            "description": "Which runtime adapter drives the smoke test?",
            "material": true,
            "resolved": true,
            "resolution": "Codex drives this fixture."
        }],
        "domain_document_artifacts": [{
            "path": "docs/agents/domain.md",
            "sha256": sha256(&domain_bytes)
        }]
    })
}

#[test]
fn start_requires_all_safe_project_configuration_before_creating_a_run() {
    for missing in [
        "docs/agents/issue-tracker.md",
        "docs/agents/triage-labels.md",
        "docs/agents/domain.md",
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let worktree = tmp.path();
        write_project(worktree);
        fs::remove_file(worktree.join(missing)).unwrap();
        assert_error_contains(
            distill(&[
                "start",
                "--json",
                "--runtime",
                "codex",
                "--session-id",
                "session-missing-config",
                "--worktree",
                worktree.to_str().unwrap(),
                "--requirement",
                "Build a release-readiness dashboard.",
            ]),
            missing,
        );
        assert!(
            !worktree.join(".distill/runs").exists(),
            "missing {missing} must fail before run creation"
        );
    }

    #[cfg(unix)]
    {
        let tmp = tempfile::tempdir().unwrap();
        let worktree = tmp.path();
        write_project(worktree);
        let outside = tmp.path().join("outside-domain.md");
        fs::write(&outside, "# Outside domain\n").unwrap();
        fs::remove_file(worktree.join("docs/agents/domain.md")).unwrap();
        symlink(&outside, worktree.join("docs/agents/domain.md")).unwrap();
        assert_error_contains(
            distill(&[
                "start",
                "--json",
                "--runtime",
                "codex",
                "--session-id",
                "session-unsafe-config",
                "--worktree",
                worktree.to_str().unwrap(),
                "--requirement",
                "Build a release-readiness dashboard.",
            ]),
            "project configuration must not be a symlink",
        );
        assert!(!worktree.join(".distill/runs").exists());
    }
}

#[test]
fn clarification_rejects_unresolved_material_unknowns_and_persists_report_fields() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path();
    write_project(worktree);
    let started = start(worktree, "session-clarification-contract");
    let run_id = started["run_id"].as_str().unwrap();

    assert_error_contains(
        submit(
            worktree,
            run_id,
            "session-clarification-contract",
            1,
            "clarification",
            &json!({
                "checkpoint": "clarification-complete",
                "summary": "Under-specified completion."
            }),
        ),
        "evidence.clarified_requirement is required",
    );

    let mut unresolved = clarification_evidence(worktree);
    unresolved["material_unknowns"][0]["resolved"] = json!(false);
    unresolved["material_unknowns"][0]["resolution"] = json!(Value::Null);
    assert_error_contains(
        submit(
            worktree,
            run_id,
            "session-clarification-contract",
            1,
            "clarification",
            &unresolved,
        ),
        "unresolved material unknown",
    );
    assert_eq!(state_for(worktree, run_id)["revision"], 1);

    let clarified = json_success(submit(
        worktree,
        run_id,
        "session-clarification-contract",
        1,
        "clarification",
        &clarification_evidence(worktree),
    ));
    assert_eq!(clarified["stage"], "prd");
    let state = state_for(worktree, run_id);
    assert_eq!(
        state["clarification"]["clarified_requirement"],
        "Build a release-readiness dashboard with a JSON CLI seam."
    );
    assert_eq!(
        state["clarification"]["decisions"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        state["clarification"]["accepted_assumptions"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        state["clarification"]["domain_document_artifacts"][0]["path"],
        "docs/agents/domain.md"
    );

    let after_prd = json_success(submit(
        worktree,
        run_id,
        "session-clarification-contract",
        2,
        "prd",
        &json!({
            "checkpoint": "testing-seam-confirmed",
            "summary": "PRD fixes the CLI seam.",
            "prd_markdown": "# PRD\n\nBuild the dashboard.\n"
        }),
    ));
    let completed = json_success(submit(
        worktree,
        run_id,
        "session-clarification-contract",
        after_prd["revision"].as_u64().unwrap(),
        "issues",
        &json!({
            "checkpoint": "slice-breakdown-approved",
            "summary": "One slice is ready.",
            "issues": [{
                "title": "Build dashboard",
                "body": "---\nkey: 01-build-dashboard\ntitle: Build dashboard\ntype: issue\nstatus: ready-for-agent\nparent: .scratch/distill-tracer/PRD.md\n---\n\n## What to build\n\nBuild the dashboard.\n\n## Acceptance Criteria\n\n- [ ] The dashboard works.\n\n## Blocked by\n\n- None — can start immediately.\n\n## Comments\n",
                "depends_on": []
            }]
        }),
    ));
    let report: Value = serde_json::from_str(
        &fs::read_to_string(worktree.join(completed["report"]["json_path"].as_str().unwrap()))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        report["requirement"]["text"],
        "Build a release-readiness dashboard with a JSON CLI seam."
    );
    assert_eq!(report["decisions"], state["clarification"]["decisions"]);
    assert_eq!(
        report["assumptions"],
        state["clarification"]["accepted_assumptions"]
    );
    assert_eq!(
        report["material_unknowns"],
        state["clarification"]["material_unknowns"]
    );
    assert_eq!(
        report["domain_changes"],
        state["clarification"]["domain_document_artifacts"]
    );
}

#[test]
fn clarification_accepts_owned_domain_edits_and_rejects_symlink_artifacts() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path();
    write_project(worktree);
    init_git(worktree);
    let started = start(worktree, "session-domain-edit");
    let run_id = started["run_id"].as_str().unwrap();
    fs::write(
        worktree.join("docs/agents/domain.md"),
        "# Domain\n\nRelease readiness is a deployability signal.\n",
    )
    .unwrap();
    let completed = json_success(submit(
        worktree,
        run_id,
        "session-domain-edit",
        1,
        "clarification",
        &clarification_evidence(worktree),
    ));
    assert_eq!(completed["stage"], "prd");

    let second = start(worktree, "session-domain-symlink");
    let second_id = second["run_id"].as_str().unwrap();
    let outside = tmp.path().join("outside.md");
    fs::write(&outside, "# Outside\n").unwrap();
    let linked = worktree.join("docs/adr/9999-linked.md");
    #[cfg(unix)]
    symlink(&outside, &linked).unwrap();
    let mut evidence = clarification_evidence(worktree);
    evidence["domain_document_artifacts"] = json!([{
        "path": "docs/adr/9999-linked.md",
        "sha256": sha256(&fs::read(&outside).unwrap())
    }]);
    assert_error_contains(
        submit(
            worktree,
            second_id,
            "session-domain-symlink",
            1,
            "clarification",
            &evidence,
        ),
        "must not be a symlink",
    );
}

#[test]
fn waiting_and_blocked_boundaries_are_durable_and_resumable() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path();
    write_project(worktree);
    let started = start(worktree, "session-boundaries");
    let run_id = started["run_id"].as_str().unwrap();

    let waiting = json_success(submit(
        worktree,
        run_id,
        "session-boundaries",
        1,
        "clarification",
        &json!({
            "checkpoint": "clarification-complete",
            "status": "waiting",
            "reason": "A user decision is required.",
            "required_next_action": "Ask whether archived releases are included."
        }),
    ));
    assert_eq!(waiting["status"], "active");
    assert_eq!(waiting["stage_state"], "waiting");
    assert_eq!(waiting["next_action"], "wait");
    assert!(waiting["authorized_action"].is_null());
    assert_eq!(
        waiting["required_next_action"],
        "Ask whether archived releases are included."
    );
    assert_eq!(state_for(worktree, run_id)["stages"][1]["state"], "waiting");

    let blocked = json_success(submit(
        worktree,
        run_id,
        "session-boundaries",
        2,
        "clarification",
        &json!({
            "checkpoint": "clarification-complete",
            "status": "blocked",
            "reason": "The tracker configuration must be repaired.",
            "required_next_action": "Repair tracker configuration, then resubmit."
        }),
    ));
    assert_eq!(blocked["status"], "blocked");
    assert_eq!(blocked["stage_state"], "blocked");
    assert_eq!(blocked["next_action"], "unblock");
    assert!(blocked["authorized_action"].is_null());

    let taken_over = json_success(distill(&[
        "takeover",
        "--json",
        "--worktree",
        worktree.to_str().unwrap(),
        "--run-id",
        run_id,
        "--from-session",
        "session-boundaries",
        "--to-session",
        "session-boundaries-recovery",
        "--expected-revision",
        "3",
        "--reason",
        "The user moved blocked recovery to a replacement session.",
        "--user-authorized",
    ]));
    assert_eq!(taken_over["status"], "blocked");
    assert_eq!(taken_over["revision"], 4);

    let resumed = json_success(submit(
        worktree,
        run_id,
        "session-boundaries-recovery",
        4,
        "clarification",
        &clarification_evidence(worktree),
    ));
    assert_eq!(resumed["status"], "active");
    assert_eq!(resumed["stage"], "prd");
    assert_eq!(resumed["stage_state"], "active");
}

#[test]
fn abort_is_user_authorized_terminal_transition_and_releases_session() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path();
    write_project(worktree);
    fs::write(
        worktree.join("docs/adr/0000-preexisting.md"),
        "# Pre-existing decision\n\nUnchanged.\n",
    )
    .unwrap();
    let started = start(worktree, "session-abort");
    let run_id = started["run_id"].as_str().unwrap();
    fs::write(
        worktree.join("docs/adr/0001-run-decision.md"),
        "# Run decision\n\nUse the JSON CLI seam.\n",
    )
    .unwrap();

    assert_error_contains(
        distill(&[
            "abort",
            "--json",
            "--worktree",
            worktree.to_str().unwrap(),
            "--run-id",
            run_id,
            "--session-id",
            "session-abort",
            "--expected-revision",
            "1",
            "--reason",
            "The user no longer wants this requirement distilled.",
        ]),
        "abort requires --user-authorized",
    );

    let aborted = json_success(distill(&[
        "abort",
        "--json",
        "--worktree",
        worktree.to_str().unwrap(),
        "--run-id",
        run_id,
        "--session-id",
        "session-abort",
        "--expected-revision",
        "1",
        "--reason",
        "The user no longer wants this requirement distilled.",
        "--user-authorized",
    ]));
    assert_eq!(aborted["status"], "aborted");
    assert_eq!(aborted["stage"], "aborted");
    assert_eq!(aborted["revision"], 2);
    assert_eq!(aborted["session_binding"]["released"], true);
    assert_eq!(aborted["next_action"], "terminal");
    let state = state_for(worktree, run_id);
    assert_eq!(
        state["abort"]["reason"],
        "The user no longer wants this requirement distilled."
    );
    assert_eq!(
        state["abort"]["domain_document_artifacts"][0]["path"],
        "docs/adr/0001-run-decision.md"
    );
    assert_eq!(
        state["abort"]["domain_document_artifacts"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    let replacement = start(worktree, "session-abort");
    assert_ne!(replacement["run_id"], run_id);
}

#[test]
fn abort_fails_closed_on_domain_symlinks_created_after_start() {
    #[cfg(unix)]
    {
        let tmp = tempfile::tempdir().unwrap();
        let worktree = tmp.path();
        write_project(worktree);
        let started = start(worktree, "session-abort-symlink");
        let run_id = started["run_id"].as_str().unwrap();
        let outside = tmp.path().join("outside-domain.md");
        fs::write(&outside, "# Outside domain\n").unwrap();
        fs::remove_file(worktree.join("docs/agents/domain.md")).unwrap();
        symlink(&outside, worktree.join("docs/agents/domain.md")).unwrap();

        assert_error_contains(
            distill(&[
                "abort",
                "--json",
                "--worktree",
                worktree.to_str().unwrap(),
                "--run-id",
                run_id,
                "--session-id",
                "session-abort-symlink",
                "--expected-revision",
                "1",
                "--reason",
                "User requested abort.",
                "--user-authorized",
            ]),
            "domain document artifact must not be a symlink",
        );
        let state = state_for(worktree, run_id);
        assert_eq!(state["state"], "active");
        assert_eq!(state["revision"], 1);
    }
}

#[test]
fn purge_requires_user_authority_and_is_a_revisioned_audited_transition() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path();
    write_project(worktree);
    let started = start(worktree, "session-purge-contract");
    let run_id = started["run_id"].as_str().unwrap();

    assert_error_contains(
        distill(&[
            "purge",
            "--json",
            "--worktree",
            worktree.to_str().unwrap(),
            "--run-id",
            run_id,
            "--session-id",
            "session-purge-contract",
            "--expected-revision",
            "1",
        ]),
        "purge requires --user-authorized",
    );
    assert_eq!(state_for(worktree, run_id)["revision"], 1);

    let purged = json_success(distill(&[
        "purge",
        "--json",
        "--worktree",
        worktree.to_str().unwrap(),
        "--run-id",
        run_id,
        "--session-id",
        "session-purge-contract",
        "--expected-revision",
        "1",
        "--user-authorized",
    ]));
    assert_eq!(purged["status"], "purged");
    assert_eq!(purged["revision"], 2);
    let state = state_for(worktree, run_id);
    assert_eq!(state["revision"], 2);
    let events = fs::read_to_string(
        worktree
            .join(".distill/runs")
            .join(run_id)
            .join("events.jsonl"),
    )
    .unwrap();
    let event_values = events
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert!(event_values
        .iter()
        .any(|event| event["type"] == "run-purge-authorized"
            && event["revision"] == 2
            && event["payload"]["user_authorized"] == true));
    let purge_event: Value = serde_json::from_str(events.lines().last().unwrap()).unwrap();
    assert_eq!(purge_event["type"], "run-purged");
    assert_eq!(purge_event["revision"], 2);
    assert_eq!(purge_event["payload"]["user_authorized"], true);

    assert_error_contains(
        distill(&[
            "purge",
            "--json",
            "--worktree",
            worktree.to_str().unwrap(),
            "--run-id",
            run_id,
            "--session-id",
            "session-purge-contract",
            "--expected-revision",
            "2",
            "--user-authorized",
        ]),
        "run is already purged",
    );
    assert_eq!(state_for(worktree, run_id)["revision"], 2);
}

#[test]
fn interrupted_purge_is_durably_recoverable_at_the_same_revision() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path();
    write_project(worktree);
    let started = start(worktree, "session-purge-recovery");
    let run_id = started["run_id"].as_str().unwrap();
    let output = Command::new(distill_bin())
        .args([
            "purge",
            "--json",
            "--worktree",
            worktree.to_str().unwrap(),
            "--run-id",
            run_id,
            "--session-id",
            "session-purge-recovery",
            "--expected-revision",
            "1",
            "--user-authorized",
        ])
        .env("DISTILL_FAIL_PURGE_BEFORE_AUTH_EVENT", run_id)
        .output()
        .unwrap();
    assert_error_contains(output, "injected purge interruption");
    let pending = state_for(worktree, run_id);
    assert_eq!(pending["state"], "active");
    assert_eq!(pending["revision"], 2);
    assert_eq!(pending["purge"]["user_authorized"], true);
    assert_eq!(pending["purge"]["cleanup_state"], "pending");
    assert_eq!(pending["purge"]["source_revision"], 1);
    assert_ne!(pending["session_binding"]["released"], true);
    assert_error_contains(
        submit(
            worktree,
            run_id,
            "session-purge-recovery",
            2,
            "clarification",
            &clarification_evidence(worktree),
        ),
        "purge cleanup is pending",
    );

    let recovered = json_success(distill(&[
        "purge",
        "--json",
        "--worktree",
        worktree.to_str().unwrap(),
        "--run-id",
        run_id,
        "--session-id",
        "session-purge-recovery",
        "--expected-revision",
        "2",
        "--user-authorized",
    ]));
    assert_eq!(recovered["status"], "purged");
    assert_eq!(recovered["revision"], 2);
    let events = fs::read_to_string(
        worktree
            .join(".distill/runs")
            .join(run_id)
            .join("events.jsonl"),
    )
    .unwrap();
    let authorized: Value = events
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .find(|event: &Value| event["type"] == "run-purge-authorized")
        .unwrap();
    assert_eq!(authorized["payload"]["expected_revision"], 1);
    assert_eq!(authorized["payload"]["next_revision"], 2);
}
