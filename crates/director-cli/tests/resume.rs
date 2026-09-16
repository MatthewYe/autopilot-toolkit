//! Process-seam tests for resume: worktree drift blocks a resumed session, and
//! the explicit `--accept-drift` acknowledgement re-baselines it. The last test
//! drives a schema-1 fixture through the CLI, which is also the migration
//! chain's public proof.

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn director_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_director"))
}

fn run(dir: &Path, args: &[&str]) -> Output {
    let mut full: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    full.push("--worktree".to_string());
    full.push(dir.to_str().unwrap().to_string());
    let borrowed: Vec<&str> = full.iter().map(String::as_str).collect();
    Command::new(director_bin())
        .args(&borrowed)
        .output()
        .expect("director should execute")
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

fn state_for(worktree: &Path) -> Value {
    serde_json::from_str(
        &fs::read_to_string(worktree.join(".director/state.json"))
            .expect("state.json should exist"),
    )
    .expect("state.json should be JSON")
}

fn state_bytes(worktree: &Path) -> Vec<u8> {
    fs::read(worktree.join(".director/state.json")).expect("state.json should exist")
}

fn git(worktree: &Path, args: &[&str]) -> Output {
    let output = Command::new("git")
        .args(args)
        .current_dir(worktree)
        .output()
        .expect("git should execute");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn init_git_repo(worktree: &Path) {
    git(worktree, &["init"]);
    commit(worktree, "init");
}

fn commit(worktree: &Path, message: &str) {
    git(worktree, &["add", "-A"]);
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
            message,
        ],
    );
}

fn setup() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    init_git_repo(dir.path());
    json_success(run(
        dir.path(),
        &[
            "init",
            "--spec-issue",
            "128",
            "--slug",
            "autopilot-director",
        ],
    ));
    dir
}

#[test]
fn resume_succeeds_without_moving_the_run() {
    let dir = setup();
    let before_bytes = state_bytes(dir.path());
    let before = state_for(dir.path());

    let response = json_success(run(dir.path(), &["resume"]));
    assert_eq!(response["command"], "resume");
    assert_eq!(response["revision"], before["revision"]);
    assert_eq!(response["drift"].as_array().unwrap().len(), 0);
    assert_eq!(response["rebaselined"], false);
    assert_eq!(
        state_bytes(dir.path()),
        before_bytes,
        "a clean resume must leave the state file byte-identical"
    );
}

#[test]
fn resume_blocks_on_worktree_drift_and_names_both_sides() {
    let dir = setup();
    let recorded_head = state_for(dir.path())["worktree"]["head"]
        .as_str()
        .unwrap()
        .to_string();
    let before = state_bytes(dir.path());

    // Somebody else commits in the worktree while the session is away.
    fs::write(dir.path().join("drift.txt"), "unexpected\n").unwrap();
    commit(dir.path(), "drift");
    let live_head = String::from_utf8(git(dir.path(), &["rev-parse", "HEAD"]).stdout)
        .unwrap()
        .trim()
        .to_string();

    let stderr = assert_error_contains(run(dir.path(), &["resume"]), "worktree drift detected");
    assert!(
        stderr.contains(&recorded_head),
        "the error names the recorded HEAD: {stderr}"
    );
    assert!(
        stderr.contains(&live_head),
        "the error names the live HEAD: {stderr}"
    );
    assert!(
        stderr.contains("--accept-drift"),
        "the error names the sanctioned remedy: {stderr}"
    );
    assert_eq!(
        state_bytes(dir.path()),
        before,
        "a blocked resume must not touch state"
    );
}

#[test]
fn accept_drift_rebaselines_the_fingerprint_and_bumps_the_revision() {
    let dir = setup();
    let revision_before = state_for(dir.path())["revision"].clone();
    fs::write(dir.path().join("drift.txt"), "acknowledged\n").unwrap();
    commit(dir.path(), "drift");

    let response = json_success(run(dir.path(), &["resume", "--accept-drift"]));
    assert_eq!(response["rebaselined"], true);
    assert!(
        !response["drift"].as_array().unwrap().is_empty(),
        "the acknowledgement records what it accepted"
    );
    assert_ne!(response["revision"], revision_before);

    // Re-baselined: the next resume is clean.
    let again = json_success(run(dir.path(), &["resume"]));
    assert_eq!(again["rebaselined"], false);
    assert_eq!(again["drift"].as_array().unwrap().len(), 0);
}

#[test]
fn a_branch_switch_is_drift_too() {
    let dir = setup();
    git(dir.path(), &["checkout", "-b", "somewhere-else"]);
    assert_error_contains(run(dir.path(), &["resume"]), "branch:");
}

#[test]
fn a_schema_one_state_migrates_through_resume() {
    let dir = tempfile::tempdir().unwrap();
    init_git_repo(dir.path());
    fs::create_dir_all(dir.path().join(".director")).unwrap();
    fs::write(
        dir.path().join(".director/state.json"),
        include_str!("fixtures/state-v1.json"),
    )
    .unwrap();

    // A pre-fingerprint state has no baseline to compare: it blocks rather
    // than silently trusting an unknown worktree.
    assert_error_contains(run(dir.path(), &["resume"]), "no worktree fingerprint");

    let response = json_success(run(dir.path(), &["resume", "--accept-drift"]));
    assert_eq!(response["rebaselined"], true);

    let state = state_for(dir.path());
    assert_eq!(
        state["schema_version"], 2,
        "the write lands on the current schema"
    );
    assert!(state["worktree"]["head"].is_string());
    // The ledger survived the migration intact.
    assert_eq!(state["tickets"].as_array().unwrap().len(), 2);
    assert_eq!(
        state["tickets"][0]["rounds"][0]["findings"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(state["tickets"][0]["status"], "done");
}
