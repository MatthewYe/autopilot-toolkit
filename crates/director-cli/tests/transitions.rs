//! Process-seam tests for the transition engine: the compiled `director`
//! binary drives a temp-dir worktree, and every assertion reads exit codes,
//! structured stdout, or the resulting state file.

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

/// Run `director <args...> --worktree <dir>`.
fn run(dir: &Path, args: &[&str]) -> Output {
    let mut full: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    full.push("--worktree".to_string());
    full.push(dir.to_str().unwrap().to_string());
    let borrowed: Vec<&str> = full.iter().map(String::as_str).collect();
    director(&borrowed)
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

/// A fresh worktree with a run initialized at revision 0.
fn setup() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    init_git_repo(dir.path());
    json_success(director(&[
        "init",
        "--worktree",
        dir.path().to_str().unwrap(),
        "--spec-issue",
        "128",
        "--slug",
        "autopilot-director",
    ]));
    dir
}

fn add_ticket(dir: &Path, ticket: u64) {
    json_success(run(
        dir,
        &[
            "ticket",
            "add",
            "--ticket",
            &ticket.to_string(),
            "--title",
            "demo ticket",
        ],
    ));
}

/// Drive an already-registered ticket through its whole cycle to `done` with
/// a single resolved review round.
fn drive_ticket_to_done(dir: &Path, ticket: u64) {
    let ticket_arg = ticket.to_string();
    json_success(run(
        dir,
        &[
            "ticket",
            "transition",
            "--ticket",
            &ticket_arg,
            "--to",
            "implementing",
        ],
    ));
    json_success(run(
        dir,
        &[
            "ticket",
            "transition",
            "--ticket",
            &ticket_arg,
            "--to",
            "gating",
        ],
    ));
    json_success(run(dir, &["round", "open", "--ticket", &ticket_arg]));
    json_success(run(
        dir,
        &[
            "finding",
            "record",
            "--ticket",
            &ticket_arg,
            "--round",
            "1",
            "--axis",
            "standards",
            "--id",
            "f1",
            "--hash",
            "hash-f1",
            "--summary",
            "duplicated load path",
        ],
    ));
    json_success(run(
        dir,
        &[
            "finding",
            "dispose",
            "--ticket",
            &ticket_arg,
            "--round",
            "1",
            "--id",
            "f1",
            "--fixed",
            "abc1234",
        ],
    ));
    json_success(run(
        dir,
        &["round", "close", "--ticket", &ticket_arg, "--round", "1"],
    ));
    json_success(run(
        dir,
        &[
            "ticket",
            "transition",
            "--ticket",
            &ticket_arg,
            "--to",
            "done",
        ],
    ));
}

#[test]
fn run_transitions_follow_the_chain_and_bump_the_revision() {
    let dir = setup();

    let response = json_success(run(dir.path(), &["run", "transition", "--to", "running"]));
    assert_eq!(response["from"], "init");
    assert_eq!(response["to"], "running");
    assert_eq!(response["revision"], 1);

    // `running → done` skips `spec-gating` and `pr-open`: refused, untouched.
    let before = state_bytes(dir.path());
    let stderr = assert_error_contains(
        run(dir.path(), &["run", "transition", "--to", "done"]),
        "illegal transition",
    );
    assert!(
        stderr.contains("spec-gating"),
        "legal targets listed: {stderr}"
    );
    assert_eq!(state_bytes(dir.path()), before);

    json_success(run(
        dir.path(),
        &["ticket", "add", "--ticket", "200", "--title", "one"],
    ));
    assert_error_contains(
        run(dir.path(), &["run", "transition", "--to", "spec-gating"]),
        "tickets not done",
    );
}

#[test]
fn a_ticket_cycles_to_done_only_at_absolute_zero() {
    let dir = setup();
    add_ticket(dir.path(), 200);
    drive_ticket_to_done(dir.path(), 200);

    let state = state_for(dir.path());
    assert_eq!(state["tickets"][0]["status"], "done");
    assert_eq!(state["tickets"][0]["rounds"].as_array().unwrap().len(), 1);

    let verdict = json_success(run(dir.path(), &["gate", "--ticket", "200"]));
    assert_eq!(verdict["verdict"]["zero"], true);
    assert_eq!(verdict["verdict"]["findings_total"], 1);
    assert_eq!(verdict["verdict"]["undispositioned"], 0);
}

#[test]
fn a_finding_without_a_disposition_keeps_the_gate_open() {
    let dir = setup();
    add_ticket(dir.path(), 200);
    json_success(run(
        dir.path(),
        &[
            "ticket",
            "transition",
            "--ticket",
            "200",
            "--to",
            "implementing",
        ],
    ));
    json_success(run(
        dir.path(),
        &["ticket", "transition", "--ticket", "200", "--to", "gating"],
    ));
    json_success(run(dir.path(), &["round", "open", "--ticket", "200"]));
    json_success(run(
        dir.path(),
        &[
            "finding",
            "record",
            "--ticket",
            "200",
            "--round",
            "1",
            "--axis",
            "spec",
            "--id",
            "f1",
            "--hash",
            "hash-f1",
            "--summary",
            "acceptance criterion unverified",
        ],
    ));

    // Not zero before any round closes, and `done` is refused while open.
    let gate = run(dir.path(), &["gate", "--ticket", "200"]);
    assert!(!gate.status.success(), "an open gate exits non-zero");
    let verdict: Value = serde_json::from_slice(&gate.stdout).unwrap();
    assert_eq!(verdict["verdict"]["zero"], false);
    assert_eq!(verdict["verdict"]["undispositioned"], 1);

    let before = state_bytes(dir.path());
    assert_error_contains(
        run(
            dir.path(),
            &["ticket", "transition", "--ticket", "200", "--to", "done"],
        ),
        "still open",
    );
    assert_eq!(state_bytes(dir.path()), before);

    // With the round closed, the undispositioned finding is what holds the
    // gate open — and the refusal says so.
    json_success(run(
        dir.path(),
        &["round", "close", "--ticket", "200", "--round", "1"],
    ));
    assert_error_contains(
        run(
            dir.path(),
            &["ticket", "transition", "--ticket", "200", "--to", "done"],
        ),
        "not at zero",
    );

    // Rejection needs a written reason; an empty one does not count.
    assert_error_contains(
        run(
            dir.path(),
            &[
                "finding",
                "dispose",
                "--ticket",
                "200",
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

    json_success(run(
        dir.path(),
        &[
            "finding",
            "dispose",
            "--ticket",
            "200",
            "--round",
            "1",
            "--id",
            "f1",
            "--rejected",
            "the criterion is out of scope for this ticket",
        ],
    ));
    json_success(run(
        dir.path(),
        &["ticket", "transition", "--ticket", "200", "--to", "done"],
    ));
}

#[test]
fn the_third_unresolved_round_escalates_the_ticket() {
    let dir = setup();
    add_ticket(dir.path(), 200);
    json_success(run(
        dir.path(),
        &[
            "ticket",
            "transition",
            "--ticket",
            "200",
            "--to",
            "implementing",
        ],
    ));
    json_success(run(
        dir.path(),
        &["ticket", "transition", "--ticket", "200", "--to", "gating"],
    ));

    for round in 1..=3 {
        let round_arg = round.to_string();
        json_success(run(dir.path(), &["round", "open", "--ticket", "200"]));
        json_success(run(
            dir.path(),
            &[
                "finding",
                "record",
                "--ticket",
                "200",
                "--round",
                &round_arg,
                "--axis",
                "standards",
                "--id",
                &format!("f{round}"),
                "--hash",
                &format!("hash-{round}"),
                "--summary",
                "still open",
            ],
        ));
        json_success(run(
            dir.path(),
            &["round", "close", "--ticket", "200", "--round", &round_arg],
        ));
        json_success(run(
            dir.path(),
            &["ticket", "transition", "--ticket", "200", "--to", "fixing"],
        ));
    }

    // Round 4 is impossible: the state machine escalates instead.
    assert_error_contains(
        run(dir.path(), &["round", "open", "--ticket", "200"]),
        "cap exhausted",
    );
    let state = state_for(dir.path());
    assert_eq!(state["tickets"][0]["status"], "escalated");
    assert_eq!(state["tickets"][0]["rounds"].as_array().unwrap().len(), 3);

    // An escalated ticket resumes only on the human's decision edges.
    assert_error_contains(
        run(
            dir.path(),
            &["ticket", "transition", "--ticket", "200", "--to", "pending"],
        ),
        "illegal transition",
    );
    let resume = json_success(run(
        dir.path(),
        &[
            "ticket",
            "transition",
            "--ticket",
            "200",
            "--to",
            "reviewing",
        ],
    ));
    assert_eq!(resume["to"], "reviewing");
    // Granting more rounds buys another cap's worth, and only then.
    let state = state_for(dir.path());
    assert_eq!(state["tickets"][0]["round_cap"], 6);
}

#[test]
fn the_spec_pr_opens_only_after_every_ticket_and_the_spec_gate_are_zero() {
    let dir = setup();
    json_success(run(dir.path(), &["run", "transition", "--to", "running"]));
    add_ticket(dir.path(), 200);
    assert_error_contains(
        run(dir.path(), &["run", "transition", "--to", "spec-gating"]),
        "tickets not done",
    );
    drive_ticket_to_done(dir.path(), 200);
    json_success(run(
        dir.path(),
        &["run", "transition", "--to", "spec-gating"],
    ));

    // The aggregate gate is open before any spec round ran.
    let before = state_bytes(dir.path());
    assert_error_contains(
        run(dir.path(), &["run", "transition", "--to", "pr-open"]),
        "spec gate is not at zero",
    );
    assert_eq!(state_bytes(dir.path()), before);

    json_success(run(dir.path(), &["round", "open", "--spec"]));
    json_success(run(
        dir.path(),
        &[
            "finding",
            "record",
            "--spec",
            "--round",
            "1",
            "--axis",
            "spec",
            "--id",
            "s1",
            "--hash",
            "hash-s1",
            "--summary",
            "spec-level duplication",
        ],
    ));
    json_success(run(
        dir.path(),
        &[
            "finding", "dispose", "--spec", "--round", "1", "--id", "s1", "--fixed", "def5678",
        ],
    ));
    json_success(run(
        dir.path(),
        &["round", "close", "--spec", "--round", "1"],
    ));

    let verdict = json_success(run(dir.path(), &["gate"]));
    assert_eq!(verdict["verdict"]["layer"], "spec");
    assert_eq!(verdict["verdict"]["zero"], true);

    json_success(run(dir.path(), &["run", "transition", "--to", "pr-open"]));
    json_success(run(dir.path(), &["run", "transition", "--to", "done"]));
    let state = state_for(dir.path());
    assert_eq!(state["status"], "done");
}

#[test]
fn an_open_round_never_reads_as_zero() {
    let dir = setup();
    add_ticket(dir.path(), 200);
    json_success(run(
        dir.path(),
        &[
            "ticket",
            "transition",
            "--ticket",
            "200",
            "--to",
            "implementing",
        ],
    ));
    json_success(run(
        dir.path(),
        &["ticket", "transition", "--ticket", "200", "--to", "gating"],
    ));
    json_success(run(dir.path(), &["round", "open", "--ticket", "200"]));

    // An open round has not run yet: an empty one is not evidence of zero.
    let gate = run(dir.path(), &["gate", "--ticket", "200"]);
    assert!(!gate.status.success(), "an open gate exits non-zero");
    let verdict: Value = serde_json::from_slice(&gate.stdout).unwrap();
    assert_eq!(verdict["verdict"]["zero"], false);
    assert_eq!(verdict["verdict"]["open_round"], 1);
    assert_eq!(verdict["verdict"]["undispositioned"], 0);

    // …so the ticket cannot reach `done` with the round still open.
    let before = state_bytes(dir.path());
    assert_error_contains(
        run(
            dir.path(),
            &["ticket", "transition", "--ticket", "200", "--to", "done"],
        ),
        "still open",
    );
    assert_eq!(
        state_bytes(dir.path()),
        before,
        "a refused transition must not touch state"
    );

    // A closed round that found nothing is a review that ran: that is zero.
    json_success(run(
        dir.path(),
        &["round", "close", "--ticket", "200", "--round", "1"],
    ));
    let gate = run(dir.path(), &["gate", "--ticket", "200"]);
    assert!(gate.status.success(), "a closed empty round is zero");
    json_success(run(
        dir.path(),
        &["ticket", "transition", "--ticket", "200", "--to", "done"],
    ));
}

#[test]
fn round_bookkeeping_refuses_impossible_sequences() {
    let dir = setup();
    add_ticket(dir.path(), 200);

    // A round cannot open before the ticket is gating.
    assert_error_contains(
        run(dir.path(), &["round", "open", "--ticket", "200"]),
        "expected `gating` or `fixing`",
    );

    json_success(run(
        dir.path(),
        &[
            "ticket",
            "transition",
            "--ticket",
            "200",
            "--to",
            "implementing",
        ],
    ));
    json_success(run(
        dir.path(),
        &["ticket", "transition", "--ticket", "200", "--to", "gating"],
    ));
    json_success(run(dir.path(), &["round", "open", "--ticket", "200"]));

    // The same round cannot be opened twice while it is open.
    assert_error_contains(
        run(dir.path(), &["round", "open", "--ticket", "200"]),
        "still open",
    );
    // Findings are recorded once.
    let record = |id: &str| {
        run(
            dir.path(),
            &[
                "finding",
                "record",
                "--ticket",
                "200",
                "--round",
                "1",
                "--axis",
                "standards",
                "--id",
                id,
                "--hash",
                "hash",
                "--summary",
                "note",
            ],
        )
    };
    json_success(record("f1"));
    assert_error_contains(record("f1"), "already recorded");

    // An unknown ticket and a missing layer are both refused.
    assert_error_contains(
        run(dir.path(), &["gate", "--ticket", "999"]),
        "not registered",
    );
    assert_error_contains(
        run(dir.path(), &["round", "open"]),
        "one of --ticket or --spec",
    );
    assert_error_contains(
        run(dir.path(), &["round", "open", "--ticket", "200", "--spec"]),
        "mutually exclusive",
    );
}
