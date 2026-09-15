//! Integration tests drive the compiled `director` binary through the process
//! seam: a temp-dir worktree, asserted exit codes, structured stdout, and the
//! resulting state files.

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn director_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_director"))
}

fn director(args: &[&str]) -> Output {
    Command::new(director_bin())
        .args(args)
        .output()
        .expect("director should execute")
}

const SLUG: &str = "autopilot-director";

fn init(worktree: &Path, spec_issue: u64, slug: &str) -> Output {
    director(&[
        "init",
        "--worktree",
        worktree.to_str().unwrap(),
        "--spec-issue",
        &spec_issue.to_string(),
        "--slug",
        slug,
    ])
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

fn assert_error_contains(output: Output, expected: &str) {
    assert!(!output.status.success(), "command should fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(expected),
        "stderr should contain {expected:?}, got {stderr:?}"
    );
}

fn state_for(worktree: &Path) -> Value {
    serde_json::from_str(
        &fs::read_to_string(worktree.join(".director/state.json"))
            .expect("state.json should exist"),
    )
    .expect("state.json should be JSON")
}

fn git(worktree: &Path, args: &[&str]) -> Output {
    Command::new("git")
        .args(args)
        .current_dir(worktree)
        .output()
        .expect("git should execute")
}

fn init_git_repo(worktree: &Path) {
    git(worktree, &["init"]);
    git(
        worktree,
        &[
            "-c",
            "user.email=test@example.com",
            "-c",
            "user.name=Test",
            "commit",
            "--allow-empty",
            "-m",
            "init",
        ],
    );
}

#[test]
fn init_writes_the_initial_state_at_revision_zero() {
    let dir = tempfile::tempdir().unwrap();
    init_git_repo(dir.path());

    let response = json_success(init(dir.path(), 128, SLUG));

    assert_eq!(response["command"], "init");
    assert_eq!(response["run_id"], "spec-128");
    assert_eq!(response["spec_issue"], 128);
    assert_eq!(response["schema_version"], 1);
    assert_eq!(response["revision"], 0);
    assert_eq!(response["status"], "init");

    let state = state_for(dir.path());
    assert_eq!(state["schema_version"], 1);
    assert_eq!(state["run_id"], "spec-128");
    assert_eq!(state["spec_issue"], 128);
    assert_eq!(
        state["branch"], "codex/spec-128-autopilot-director",
        "the recorded branch is ADR 0047-shaped from revision 0"
    );
    assert_eq!(state["status"], "init");
    assert_eq!(state["revision"], 0);
    assert_eq!(state["tickets"].as_array().unwrap().len(), 0);
    assert_eq!(state["spec_gate"]["round_cap"], 3);
    assert_eq!(state["spec_gate"]["rounds"].as_array().unwrap().len(), 0);
}

#[test]
fn init_refuses_a_malformed_slug_without_writing_state() {
    let dir = tempfile::tempdir().unwrap();
    init_git_repo(dir.path());

    assert_error_contains(init(dir.path(), 128, "Autopilot Director"), "--slug");
    assert!(
        !dir.path().join(".director/state.json").exists(),
        "refused init must not write state"
    );
}

#[test]
fn init_refuses_when_the_state_directory_is_not_git_ignored() {
    let dir = tempfile::tempdir().unwrap();
    init_git_repo(dir.path());
    // Run state already exists and is still trackable: fail closed before
    // touching it.
    fs::create_dir(dir.path().join(".director")).unwrap();

    assert_error_contains(init(dir.path(), 128, SLUG), "before ignore is effective");
    assert!(
        !dir.path().join(".director/state.json").exists(),
        "refused init must not write state"
    );
}

#[test]
fn init_establishes_the_ignore_rule_and_leaves_state_untracked() {
    let dir = tempfile::tempdir().unwrap();
    init_git_repo(dir.path());

    json_success(init(dir.path(), 128, SLUG));

    let gitignore = fs::read_to_string(dir.path().join(".gitignore")).unwrap();
    assert_eq!(gitignore, "/.director/\n");
    let status = git(dir.path(), &["status", "--porcelain"]);
    let status = String::from_utf8_lossy(&status.stdout);
    assert!(
        status.contains(".gitignore"),
        ".gitignore change must be visible to the Director: {status:?}"
    );
    assert!(
        !status.contains(".director"),
        "run state must never be trackable: {status:?}"
    );
    assert!(
        git(dir.path(), &["check-ignore", ".director/state.json"])
            .status
            .success(),
        "state must be git-ignored"
    );
}

#[test]
fn init_is_idempotent_but_never_overwrites_an_existing_run() {
    let dir = tempfile::tempdir().unwrap();
    init_git_repo(dir.path());
    json_success(init(dir.path(), 128, SLUG));
    let before = state_for(dir.path());

    assert_error_contains(init(dir.path(), 128, SLUG), "already exists");
    assert_eq!(
        state_for(dir.path()),
        before,
        "refused init must not mutate"
    );

    let gitignore = fs::read_to_string(dir.path().join(".gitignore")).unwrap();
    assert_eq!(
        gitignore, "/.director/\n",
        "the ignore rule is established exactly once"
    );
}

#[test]
fn init_requires_an_existing_worktree() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("nope");
    assert_error_contains(init(&missing, 128, SLUG), "worktree does not exist");
}

#[test]
fn init_requires_a_spec_issue() {
    let dir = tempfile::tempdir().unwrap();
    let output = director(&["init", "--worktree", dir.path().to_str().unwrap()]);
    assert_error_contains(output, "--spec-issue");
}

#[test]
fn inspect_reads_the_state_back_through_the_schema_gate() {
    let dir = tempfile::tempdir().unwrap();
    init_git_repo(dir.path());
    json_success(init(dir.path(), 128, SLUG));

    let response = json_success(director(&[
        "inspect",
        "--worktree",
        dir.path().to_str().unwrap(),
    ]));

    assert_eq!(response["command"], "inspect");
    assert_eq!(response["run_id"], "spec-128");
    assert_eq!(response["revision"], 0);
    assert_eq!(response["status"], "init");
    assert_eq!(response["tickets"].as_array().unwrap().len(), 0);
}

#[test]
fn inspect_refuses_state_that_does_not_match_the_schema() {
    let dir = tempfile::tempdir().unwrap();
    init_git_repo(dir.path());
    json_success(init(dir.path(), 128, SLUG));
    let path = dir.path().join(".director/state.json");
    let mut state = state_for(dir.path());
    state["invented_field"] = serde_json::json!(true);
    fs::write(&path, serde_json::to_string_pretty(&state).unwrap()).unwrap();

    let output = director(&["inspect", "--worktree", dir.path().to_str().unwrap()]);
    assert_error_contains(output, "does not match the run state schema");
}
