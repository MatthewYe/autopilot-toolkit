//! Process-seam tests for the Worker dispatch ledger and the `WORKER_REPORT`
//! envelope: the compiled `director` binary drives a temp-dir worktree, and
//! every assertion reads exit codes, structured stdout, or the state file.

use serde_json::Value;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn director_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_director"))
}

/// Run `director <args...> --worktree <dir>`.
fn run(dir: &Path, args: &[&str]) -> Output {
    let borrowed = with_worktree(dir, args);
    Command::new(director_bin())
        .args(&borrowed)
        .output()
        .expect("director should execute")
}

/// Run `director <args...> --worktree <dir>` with the envelope on stdin.
fn run_with_stdin(dir: &Path, args: &[&str], stdin: &str) -> Output {
    let borrowed = with_worktree(dir, args);
    let mut child = Command::new(director_bin())
        .args(&borrowed)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("director should spawn");
    child
        .stdin
        .as_mut()
        .expect("stdin is piped")
        .write_all(stdin.as_bytes())
        .expect("writing the envelope should succeed");
    child.wait_with_output().expect("director should finish")
}

fn with_worktree(dir: &Path, args: &[&str]) -> Vec<String> {
    let mut full: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    full.push("--worktree".to_string());
    full.push(dir.to_str().unwrap().to_string());
    full
}

fn json_success(output: Output) -> Value {
    assert!(
        output.status.success(),
        "director failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("stdout should be JSON")
}

fn assert_error_contains(output: Output, expected: &str) -> String {
    assert!(!output.status.success(), "command should fail");
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(
        stderr.contains(expected),
        "stderr should contain {expected:?}, got {stderr:?}"
    );
    stderr
}

fn state_for(dir: &Path) -> Value {
    serde_json::from_str(
        &fs::read_to_string(dir.join(".director/state.json")).expect("state.json should exist"),
    )
    .expect("state.json should be JSON")
}

fn ticket_state(dir: &Path, ticket: u64) -> Value {
    state_for(dir)["tickets"]
        .as_array()
        .expect("tickets should be an array")
        .iter()
        .find(|entry| entry["ticket"] == ticket)
        .cloned()
        .expect("ticket should be registered")
}

/// A worktree with one ticket sitting in `implementing`, ready to dispatch.
fn ready_ticket(dir: &Path, ticket: u64) {
    let number = ticket.to_string();
    json_success(run(
        dir,
        &[
            "init",
            "--spec-issue",
            "128",
            "--slug",
            "autopilot-director",
        ],
    ));
    json_success(run(
        dir,
        &[
            "ticket",
            "add",
            "--ticket",
            &number,
            "--title",
            "director-cli WORKER_REPORT envelope",
        ],
    ));
    json_success(run(
        dir,
        &[
            "ticket",
            "transition",
            "--ticket",
            &number,
            "--to",
            "implementing",
        ],
    ));
}

const GOOD_ENVELOPE: &str = r#"WORKER_REPORT:
{
  "status": "done",
  "branch": "codex/132-worker-report",
  "commits": [{ "sha": "abc1234", "subject": "feat(director-cli): envelope" }],
  "tests": [
    { "command": "cargo test -p director-cli", "outcome": "pass", "evidence": "71 passed" }
  ],
  "acceptance": [
    { "criterion": "envelope is validated", "evidence": "tests/dispatch.rs" }
  ],
  "blockers": []
}"#;

const BLOCKED_ENVELOPE: &str = r#"WORKER_REPORT:
{
  "status": "blocked",
  "branch": "codex/132-worker-report",
  "commits": [],
  "tests": [],
  "acceptance": [],
  "blockers": ["the seam I need is not on the branch"]
}"#;

#[test]
fn a_validated_report_is_attached_and_visible_in_inspect() {
    let dir = tempfile::tempdir().unwrap();
    ready_ticket(dir.path(), 132);

    let begun = json_success(run(
        dir.path(),
        &[
            "dispatch", "begin", "--ticket", "132", "--worker", "worker-a",
        ],
    ));
    assert_eq!(begun["attempt"], 1);
    assert_eq!(begun["retry"], false);

    let validated = json_success(run_with_stdin(
        dir.path(),
        &["report", "validate", "--ticket", "132"],
        GOOD_ENVELOPE,
    ));
    assert_eq!(validated["report"]["status"], "done");
    assert_eq!(validated["report"]["commits"], 1);
    assert_eq!(validated["report"]["tests"], 1);

    let finished = json_success(run(
        dir.path(),
        &["dispatch", "finish", "--ticket", "132", "--outcome", "ok"],
    ));
    assert_eq!(finished["outcome"], "ok");

    let inspected = json_success(run(dir.path(), &["inspect"]));
    let dispatches = inspected["tickets"][0]["dispatches"]
        .as_array()
        .expect("inspect should list dispatches");
    assert_eq!(dispatches.len(), 1);
    assert_eq!(dispatches[0]["worker"], "worker-a");
    assert_eq!(dispatches[0]["status"], "ok");
    assert_eq!(dispatches[0]["report"]["status"], "done");
    assert_eq!(dispatches[0]["report"]["commits"], 1);
}

#[test]
fn a_report_read_from_a_file_validates() {
    let dir = tempfile::tempdir().unwrap();
    ready_ticket(dir.path(), 132);
    let path = dir.path().join("report.txt");
    fs::write(&path, GOOD_ENVELOPE).unwrap();

    json_success(run(
        dir.path(),
        &[
            "dispatch", "begin", "--ticket", "132", "--worker", "worker-a",
        ],
    ));
    let validated = json_success(run(
        dir.path(),
        &[
            "report",
            "validate",
            "--ticket",
            "132",
            "--file",
            path.to_str().unwrap(),
        ],
    ));
    assert_eq!(validated["report"]["acceptance"], 1);
}

#[test]
fn a_malformed_report_is_a_recorded_dispatch_failure() {
    let dir = tempfile::tempdir().unwrap();
    ready_ticket(dir.path(), 132);
    json_success(run(
        dir.path(),
        &[
            "dispatch", "begin", "--ticket", "132", "--worker", "worker-a",
        ],
    ));

    assert_error_contains(
        run_with_stdin(
            dir.path(),
            &["report", "validate", "--ticket", "132"],
            "I think I am done, no envelope here.",
        ),
        "no `WORKER_REPORT:` line",
    );

    let ticket = ticket_state(dir.path(), 132);
    assert_eq!(ticket["dispatches"][0]["status"], "failed");
    assert!(
        ticket["dispatches"][0]["reason"]
            .as_str()
            .unwrap()
            .contains("malformed WORKER_REPORT"),
        "the recorded reason should name the malformed envelope: {ticket}"
    );
    assert_eq!(
        ticket["status"], "implementing",
        "the first failure leaves the ticket open for its sanctioned retry"
    );
}

#[test]
fn an_envelope_that_contradicts_itself_is_a_recorded_failure() {
    let dir = tempfile::tempdir().unwrap();
    ready_ticket(dir.path(), 132);
    json_success(run(
        dir.path(),
        &[
            "dispatch", "begin", "--ticket", "132", "--worker", "worker-a",
        ],
    ));

    let contradictory = GOOD_ENVELOPE.replace("\"status\": \"done\"", "\"status\": \"blocked\"");
    assert_error_contains(
        run_with_stdin(
            dir.path(),
            &["report", "validate", "--ticket", "132"],
            &contradictory,
        ),
        "at least one blocker",
    );
    assert_eq!(
        ticket_state(dir.path(), 132)["dispatches"][0]["status"],
        "failed"
    );
}

#[test]
fn one_same_worker_retry_then_escalation() {
    let dir = tempfile::tempdir().unwrap();
    ready_ticket(dir.path(), 132);

    json_success(run(
        dir.path(),
        &[
            "dispatch", "begin", "--ticket", "132", "--worker", "worker-a",
        ],
    ));
    assert_error_contains(
        run_with_stdin(
            dir.path(),
            &["report", "validate", "--ticket", "132"],
            "nothing to see here",
        ),
        "one same-Worker retry remains",
    );

    let retry = json_success(run(
        dir.path(),
        &[
            "dispatch", "begin", "--ticket", "132", "--worker", "worker-a",
        ],
    ));
    assert_eq!(retry["attempt"], 2);
    assert_eq!(retry["retry"], true);

    let stderr = assert_error_contains(
        run_with_stdin(
            dir.path(),
            &["report", "validate", "--ticket", "132"],
            "still not an envelope",
        ),
        "escalated",
    );
    assert!(
        stderr.contains("retry budget"),
        "the escalation should name the budget: {stderr}"
    );

    let ticket = ticket_state(dir.path(), 132);
    assert_eq!(ticket["status"], "escalated");
    assert_eq!(ticket["dispatches"].as_array().unwrap().len(), 2);
}

#[test]
fn the_retry_must_reuse_the_worker_that_failed() {
    let dir = tempfile::tempdir().unwrap();
    ready_ticket(dir.path(), 132);
    json_success(run(
        dir.path(),
        &[
            "dispatch", "begin", "--ticket", "132", "--worker", "worker-a",
        ],
    ));
    assert_error_contains(
        run_with_stdin(
            dir.path(),
            &["report", "validate", "--ticket", "132"],
            "not an envelope",
        ),
        "retry",
    );

    assert_error_contains(
        run(
            dir.path(),
            &[
                "dispatch", "begin", "--ticket", "132", "--worker", "worker-b",
            ],
        ),
        "same Worker that failed",
    );
    // The refusal left the ledger alone.
    assert_eq!(
        ticket_state(dir.path(), 132)["dispatches"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn the_budget_refuses_an_attempt_after_escalation() {
    let dir = tempfile::tempdir().unwrap();
    ready_ticket(dir.path(), 132);
    for _ in 0..2 {
        json_success(run(
            dir.path(),
            &[
                "dispatch", "begin", "--ticket", "132", "--worker", "worker-a",
            ],
        ));
        let _ = run_with_stdin(
            dir.path(),
            &["report", "validate", "--ticket", "132"],
            "not an envelope",
        );
    }

    assert_error_contains(
        run(
            dir.path(),
            &[
                "dispatch", "begin", "--ticket", "132", "--worker", "worker-a",
            ],
        ),
        "escalated",
    );
    assert_eq!(ticket_state(dir.path(), 132)["status"], "escalated");
}

#[test]
fn a_failed_finish_records_its_reason_and_keeps_one_retry() {
    let dir = tempfile::tempdir().unwrap();
    ready_ticket(dir.path(), 132);
    json_success(run(
        dir.path(),
        &[
            "dispatch", "begin", "--ticket", "132", "--worker", "worker-a",
        ],
    ));

    let failed = json_success(run(
        dir.path(),
        &[
            "dispatch",
            "finish",
            "--ticket",
            "132",
            "--outcome",
            "failed",
            "--reason",
            "cargo cannot run offline",
        ],
    ));
    assert_eq!(failed["outcome"], "failed");
    assert_eq!(failed["retry_remaining"], true);
    assert_eq!(
        ticket_state(dir.path(), 132)["dispatches"][0]["reason"],
        "cargo cannot run offline"
    );

    json_success(run(
        dir.path(),
        &[
            "dispatch", "begin", "--ticket", "132", "--worker", "worker-a",
        ],
    ));
    assert_error_contains(
        run(
            dir.path(),
            &[
                "dispatch",
                "finish",
                "--ticket",
                "132",
                "--outcome",
                "failed",
                "--reason",
                "still failing",
            ],
        ),
        "escalated",
    );
    assert_eq!(ticket_state(dir.path(), 132)["status"], "escalated");
}

#[test]
fn a_failed_finish_needs_a_reason() {
    let dir = tempfile::tempdir().unwrap();
    ready_ticket(dir.path(), 132);
    json_success(run(
        dir.path(),
        &[
            "dispatch", "begin", "--ticket", "132", "--worker", "worker-a",
        ],
    ));

    assert_error_contains(
        run(
            dir.path(),
            &[
                "dispatch",
                "finish",
                "--ticket",
                "132",
                "--outcome",
                "failed",
            ],
        ),
        "requires --reason",
    );
    assert_error_contains(
        run(
            dir.path(),
            &[
                "dispatch",
                "finish",
                "--ticket",
                "132",
                "--outcome",
                "failed",
                "--reason",
                "   ",
            ],
        ),
        "--reason must not be empty",
    );
    assert_eq!(
        ticket_state(dir.path(), 132)["dispatches"][0]["status"],
        "started"
    );
}

#[test]
fn finishing_ok_needs_a_validated_report() {
    let dir = tempfile::tempdir().unwrap();
    ready_ticket(dir.path(), 132);
    json_success(run(
        dir.path(),
        &[
            "dispatch", "begin", "--ticket", "132", "--worker", "worker-a",
        ],
    ));

    assert_error_contains(
        run(
            dir.path(),
            &["dispatch", "finish", "--ticket", "132", "--outcome", "ok"],
        ),
        "no validated WORKER_REPORT",
    );
    assert_eq!(
        ticket_state(dir.path(), 132)["dispatches"][0]["status"],
        "started"
    );
}

#[test]
fn a_blocked_report_is_finished_as_a_failure() {
    let dir = tempfile::tempdir().unwrap();
    ready_ticket(dir.path(), 132);
    json_success(run(
        dir.path(),
        &[
            "dispatch", "begin", "--ticket", "132", "--worker", "worker-a",
        ],
    ));
    json_success(run_with_stdin(
        dir.path(),
        &["report", "validate", "--ticket", "132"],
        BLOCKED_ENVELOPE,
    ));

    assert_error_contains(
        run(
            dir.path(),
            &["dispatch", "finish", "--ticket", "132", "--outcome", "ok"],
        ),
        "reports `blocked`",
    );

    let failed = json_success(run(
        dir.path(),
        &[
            "dispatch",
            "finish",
            "--ticket",
            "132",
            "--outcome",
            "failed",
            "--reason",
            "the seam is missing",
        ],
    ));
    assert_eq!(failed["outcome"], "failed");
    let ticket = ticket_state(dir.path(), 132);
    assert_eq!(ticket["dispatches"][0]["status"], "failed");
    assert_eq!(ticket["dispatches"][0]["report"]["status"], "blocked");
}

#[test]
fn a_report_without_an_open_dispatch_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    ready_ticket(dir.path(), 132);

    assert_error_contains(
        run_with_stdin(
            dir.path(),
            &["report", "validate", "--ticket", "132"],
            GOOD_ENVELOPE,
        ),
        "no open dispatch",
    );
    assert!(ticket_state(dir.path(), 132)["dispatches"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn a_dispatch_only_opens_while_the_ticket_is_implementing() {
    let dir = tempfile::tempdir().unwrap();
    ready_ticket(dir.path(), 132);
    json_success(run(
        dir.path(),
        &["ticket", "transition", "--ticket", "132", "--to", "gating"],
    ));

    assert_error_contains(
        run(
            dir.path(),
            &[
                "dispatch", "begin", "--ticket", "132", "--worker", "worker-a",
            ],
        ),
        "dispatches begin in `implementing`",
    );
}

#[test]
fn resuming_an_escalated_ticket_opens_a_fresh_budget_window() {
    let dir = tempfile::tempdir().unwrap();
    ready_ticket(dir.path(), 132);
    for _ in 0..2 {
        json_success(run(
            dir.path(),
            &[
                "dispatch", "begin", "--ticket", "132", "--worker", "worker-a",
            ],
        ));
        let _ = run_with_stdin(
            dir.path(),
            &["report", "validate", "--ticket", "132"],
            "not an envelope",
        );
    }

    json_success(run(
        dir.path(),
        &[
            "ticket",
            "transition",
            "--ticket",
            "132",
            "--to",
            "implementing",
        ],
    ));
    let retried = json_success(run(
        dir.path(),
        &[
            "dispatch", "begin", "--ticket", "132", "--worker", "worker-c",
        ],
    ));
    assert_eq!(retried["attempt"], 3);
    assert_eq!(retried["retry"], false);
}

// ── finding record hardening (ticket #132 acceptance criteria) ──

/// A ticket with one open review round, ready to record findings.
fn reviewing_ticket(dir: &Path, ticket: u64) {
    ready_ticket(dir, ticket);
    let number = ticket.to_string();
    json_success(run(
        dir,
        &[
            "ticket",
            "transition",
            "--ticket",
            &number,
            "--to",
            "gating",
        ],
    ));
    json_success(run(dir, &["round", "open", "--ticket", &number]));
}

#[test]
fn a_finding_needs_a_hash_that_identifies_it() {
    let dir = tempfile::tempdir().unwrap();
    reviewing_ticket(dir.path(), 132);

    assert_error_contains(
        run(
            dir.path(),
            &[
                "finding",
                "record",
                "--ticket",
                "132",
                "--round",
                "1",
                "--axis",
                "spec",
                "--id",
                "f1",
                "--hash",
                "   ",
                "--summary",
                "an unhashed finding",
            ],
        ),
        "finding hash must not be empty",
    );
    assert_error_contains(
        run(
            dir.path(),
            &[
                "finding",
                "record",
                "--ticket",
                "132",
                "--round",
                "1",
                "--axis",
                "spec",
                "--id",
                " ",
                "--hash",
                "hash-f1",
                "--summary",
                "an unnamed finding",
            ],
        ),
        "finding id must not be empty",
    );
    assert!(ticket_state(dir.path(), 132)["rounds"][0]["findings"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn a_rejection_needs_a_written_reason() {
    let dir = tempfile::tempdir().unwrap();
    reviewing_ticket(dir.path(), 132);
    json_success(run(
        dir.path(),
        &[
            "finding",
            "record",
            "--ticket",
            "132",
            "--round",
            "1",
            "--axis",
            "standards",
            "--id",
            "f1",
            "--hash",
            "hash-f1",
            "--summary",
            "a real finding",
        ],
    ));

    assert_error_contains(
        run(
            dir.path(),
            &[
                "finding",
                "dispose",
                "--ticket",
                "132",
                "--round",
                "1",
                "--id",
                "f1",
                "--rejected",
                "   ",
            ],
        ),
        "written reason",
    );
    assert!(ticket_state(dir.path(), 132)["rounds"][0]["findings"][0]
        .get("disposition")
        .is_none());
}
