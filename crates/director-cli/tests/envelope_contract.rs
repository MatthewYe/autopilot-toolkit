//! Envelope contract: the byte-exact JSON every command prints on stdout.
//!
//! Ticket #19 split the state machine into pure record methods plus one commit
//! seam, and collapsed the envelope construction into a single builder. None of
//! that was allowed to move a single byte of the CLI's output contract, so the
//! commands whose envelopes an existing test already asserts field-by-field get
//! their full bytes pinned here too, and so do the ones that had no field-level
//! assertion at all.
//!
//! The goldens under `tests/fixtures/envelopes/` were captured from the
//! pre-split binary with `scripts/capture-envelope-goldens.sh`, which is also the
//! only sanctioned way to reproduce them. Every byte counts: the trailing
//! newline, the two-space indent, and the key order — serde_json's default
//! `BTreeMap` maps sort their keys, so the envelopes read alphabetically.
//!
//! The commands are replayed in the capture's exact order because `revision` is
//! part of the envelope and is a pure function of that order. Reordering the
//! steps, or dropping one that has no golden, is itself a contract change.

use serde_json::Value;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn director_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_director"))
}

/// Run `director <args...> --worktree <dir>`, asserting it succeeded.
fn run(worktree: &Path, args: &[&str]) -> Output {
    let output = run_unchecked(worktree, args);
    assert!(
        output.status.success(),
        "director {args:?} should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

/// Run `director <args...> --worktree <dir>` without asserting the exit status.
fn run_unchecked(worktree: &Path, args: &[&str]) -> Output {
    director_with(worktree, args, None)
}

fn director_with(worktree: &Path, args: &[&str], stdin: Option<&str>) -> Output {
    let mut full: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    full.push("--worktree".to_string());
    full.push(worktree.to_str().unwrap().to_string());
    let mut command = Command::new(director_bin());
    command.args(&full);
    match stdin {
        None => command.output().expect("director should execute"),
        Some(input) => {
            let mut child = command
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("director should spawn");
            child
                .stdin
                .as_mut()
                .expect("stdin is piped")
                .write_all(input.as_bytes())
                .expect("writing the envelope should succeed");
            child.wait_with_output().expect("director should finish")
        }
    }
}

/// Hold one command's stdout to its golden byte-for-byte.
///
/// A golden is the *complete* stdout of one invocation — `director` prints
/// exactly one JSON object per command and nothing else — so the comparison
/// needs no parsing and cannot be relaxed by a field-level assertion elsewhere.
#[track_caller]
fn assert_envelope(output: Output, golden: &str) {
    assert!(
        output.status.success(),
        "{golden} should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let expected = include_bytes_golden(golden);
    assert!(
        output.stdout == expected,
        "{golden} envelope drifted\n--- golden ---\n{}\n--- live ---\n{}",
        String::from_utf8_lossy(expected),
        String::from_utf8_lossy(&output.stdout)
    );
}

/// Look a golden up by name so a failure message can name the fixture file.
fn include_bytes_golden(name: &str) -> &'static [u8] {
    match name {
        "dispatch-begin" => include_bytes!("fixtures/envelopes/dispatch-begin.stdout"),
        "dispatch-begin-2" => include_bytes!("fixtures/envelopes/dispatch-begin-2.stdout"),
        "dispatch-finish-failed" => {
            include_bytes!("fixtures/envelopes/dispatch-finish-failed.stdout")
        }
        "dispatch-finish-ok" => include_bytes!("fixtures/envelopes/dispatch-finish-ok.stdout"),
        "finding-dispose" => include_bytes!("fixtures/envelopes/finding-dispose.stdout"),
        "finding-record" => include_bytes!("fixtures/envelopes/finding-record.stdout"),
        "gate-ticket-zero" => include_bytes!("fixtures/envelopes/gate-ticket-zero.stdout"),
        "report-validate" => include_bytes!("fixtures/envelopes/report-validate.stdout"),
        "round-close-201" => include_bytes!("fixtures/envelopes/round-close-201.stdout"),
        "round-close-ticket" => include_bytes!("fixtures/envelopes/round-close-ticket.stdout"),
        "round-open-201" => include_bytes!("fixtures/envelopes/round-open-201.stdout"),
        "round-open-spec" => include_bytes!("fixtures/envelopes/round-open-spec.stdout"),
        "round-open-ticket" => include_bytes!("fixtures/envelopes/round-open-ticket.stdout"),
        "run-spec-gating" => include_bytes!("fixtures/envelopes/run-spec-gating.stdout"),
        "run-transition" => include_bytes!("fixtures/envelopes/run-transition.stdout"),
        "ticket-201-done" => include_bytes!("fixtures/envelopes/ticket-201-done.stdout"),
        "ticket-201-gating" => include_bytes!("fixtures/envelopes/ticket-201-gating.stdout"),
        "ticket-201-implementing" => {
            include_bytes!("fixtures/envelopes/ticket-201-implementing.stdout")
        }
        "ticket-add" => include_bytes!("fixtures/envelopes/ticket-add.stdout"),
        "ticket-add-201" => include_bytes!("fixtures/envelopes/ticket-add-201.stdout"),
        "ticket-done" => include_bytes!("fixtures/envelopes/ticket-done.stdout"),
        "ticket-gating" => include_bytes!("fixtures/envelopes/ticket-gating.stdout"),
        "ticket-implementing" => include_bytes!("fixtures/envelopes/ticket-implementing.stdout"),
        other => panic!("no golden named {other}"),
    }
}

fn state_for(worktree: &Path) -> Value {
    serde_json::from_str(
        &fs::read_to_string(worktree.join(".director/state.json"))
            .expect("state.json should exist"),
    )
    .expect("state.json should be JSON")
}

fn git(worktree: &Path, args: &[&str]) {
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
}

fn setup() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-q", "-b", "main"]);
    git(
        dir.path(),
        &[
            "-c",
            "user.email=test@example.com",
            "-c",
            "user.name=Test",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "init",
        ],
    );
    dir
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

/// The capture sequence, replayed once: every envelope that an existing test
/// asserted only field-by-field — or not at all — is compared byte-for-byte at
/// the revision the golden recorded.
#[test]
fn the_captured_envelope_sequence_is_byte_identical() {
    let dir = setup();
    let worktree = dir.path();
    let envelope_path = worktree.join("report.txt");
    fs::write(&envelope_path, GOOD_ENVELOPE).unwrap();

    // `init`'s envelope embeds the temp path and the captured HEAD, so it has no
    // golden — but revision 0 is itself a contract: `init` never goes through the
    // commit seam and never bumps.
    let init = run(
        worktree,
        &[
            "init",
            "--spec-issue",
            "128",
            "--slug",
            "autopilot-director",
        ],
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&init.stdout).unwrap()["revision"],
        0
    );

    assert_envelope(
        run(
            worktree,
            &["ticket", "add", "--ticket", "200", "--title", "demo ticket"],
        ),
        "ticket-add",
    );
    assert_envelope(
        run(worktree, &["run", "transition", "--to", "running"]),
        "run-transition",
    );
    assert_envelope(
        run(
            worktree,
            &[
                "ticket",
                "transition",
                "--ticket",
                "200",
                "--to",
                "implementing",
            ],
        ),
        "ticket-implementing",
    );

    // dispatch layer
    assert_envelope(
        run(
            worktree,
            &[
                "dispatch", "begin", "--ticket", "200", "--worker", "worker-a",
            ],
        ),
        "dispatch-begin",
    );
    assert_envelope(
        run(
            worktree,
            &[
                "report",
                "validate",
                "--ticket",
                "200",
                "--file",
                envelope_path.to_str().unwrap(),
            ],
        ),
        "report-validate",
    );
    assert_envelope(
        run(
            worktree,
            &["dispatch", "finish", "--ticket", "200", "--outcome", "ok"],
        ),
        "dispatch-finish-ok",
    );
    assert_envelope(
        run(
            worktree,
            &[
                "dispatch", "begin", "--ticket", "200", "--worker", "worker-a",
            ],
        ),
        "dispatch-begin-2",
    );
    assert_envelope(
        run(
            worktree,
            &[
                "dispatch",
                "finish",
                "--ticket",
                "200",
                "--outcome",
                "failed",
                "--reason",
                "cargo cannot run offline",
            ],
        ),
        "dispatch-finish-failed",
    );

    // ticket layer
    assert_envelope(
        run(
            worktree,
            &["ticket", "transition", "--ticket", "200", "--to", "gating"],
        ),
        "ticket-gating",
    );
    assert_envelope(
        run(worktree, &["round", "open", "--ticket", "200"]),
        "round-open-ticket",
    );
    assert_envelope(
        run(
            worktree,
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
                "f1",
                "--hash",
                "hash-f1",
                "--summary",
                "duplicated load path",
            ],
        ),
        "finding-record",
    );
    assert_envelope(
        run(
            worktree,
            &[
                "finding", "dispose", "--ticket", "200", "--round", "1", "--id", "f1", "--fixed",
                "abc1234",
            ],
        ),
        "finding-dispose",
    );
    assert_envelope(
        run(
            worktree,
            &["round", "close", "--ticket", "200", "--round", "1"],
        ),
        "round-close-ticket",
    );
    assert_envelope(
        run(worktree, &["gate", "--ticket", "200"]),
        "gate-ticket-zero",
    );
    assert_envelope(
        run(
            worktree,
            &["ticket", "transition", "--ticket", "200", "--to", "done"],
        ),
        "ticket-done",
    );

    // second ticket, so the run can reach `spec-gating`
    assert_envelope(
        run(
            worktree,
            &[
                "ticket",
                "add",
                "--ticket",
                "201",
                "--title",
                "second ticket",
            ],
        ),
        "ticket-add-201",
    );
    assert_envelope(
        run(
            worktree,
            &[
                "ticket",
                "transition",
                "--ticket",
                "201",
                "--to",
                "implementing",
            ],
        ),
        "ticket-201-implementing",
    );
    assert_envelope(
        run(
            worktree,
            &["ticket", "transition", "--ticket", "201", "--to", "gating"],
        ),
        "ticket-201-gating",
    );
    assert_envelope(
        run(worktree, &["round", "open", "--ticket", "201"]),
        "round-open-201",
    );
    assert_envelope(
        run(
            worktree,
            &["round", "close", "--ticket", "201", "--round", "1"],
        ),
        "round-close-201",
    );
    assert_envelope(
        run(
            worktree,
            &["ticket", "transition", "--ticket", "201", "--to", "done"],
        ),
        "ticket-201-done",
    );
    assert_envelope(
        run(worktree, &["run", "transition", "--to", "spec-gating"]),
        "run-spec-gating",
    );

    // The spec-layer envelope keeps its explicit `"ticket": null`.
    assert_envelope(
        run(worktree, &["round", "open", "--spec"]),
        "round-open-spec",
    );
}

/// The stdin seam and the `--file` seam produce the same envelope bytes.
#[test]
fn report_validate_from_stdin_matches_the_pinned_envelope() {
    let dir = setup();
    let worktree = dir.path();
    run(
        worktree,
        &[
            "init",
            "--spec-issue",
            "128",
            "--slug",
            "autopilot-director",
        ],
    );
    run(
        worktree,
        &["ticket", "add", "--ticket", "200", "--title", "demo ticket"],
    );
    run(worktree, &["run", "transition", "--to", "running"]);
    run(
        worktree,
        &[
            "ticket",
            "transition",
            "--ticket",
            "200",
            "--to",
            "implementing",
        ],
    );
    run(
        worktree,
        &[
            "dispatch", "begin", "--ticket", "200", "--worker", "worker-a",
        ],
    );

    assert_envelope(
        director_with(
            worktree,
            &["report", "validate", "--ticket", "200"],
            Some(GOOD_ENVELOPE),
        ),
        "report-validate",
    );
}

/// The spec-layer cap path is one of the four refusals that must persist before
/// they exit non-zero, and before ticket #19 it had no test at all.
#[test]
fn the_exhausted_spec_round_cap_escalates_the_run_and_persists() {
    let dir = setup();
    let worktree = dir.path();
    run(
        worktree,
        &[
            "init",
            "--spec-issue",
            "128",
            "--slug",
            "autopilot-director",
        ],
    );
    run(worktree, &["run", "transition", "--to", "running"]);
    run(
        worktree,
        &["ticket", "add", "--ticket", "200", "--title", "demo ticket"],
    );
    run(
        worktree,
        &[
            "ticket",
            "transition",
            "--ticket",
            "200",
            "--to",
            "implementing",
        ],
    );
    run(
        worktree,
        &["ticket", "transition", "--ticket", "200", "--to", "gating"],
    );
    run(worktree, &["round", "open", "--ticket", "200"]);
    run(
        worktree,
        &["round", "close", "--ticket", "200", "--round", "1"],
    );
    run(
        worktree,
        &["ticket", "transition", "--ticket", "200", "--to", "done"],
    );
    run(worktree, &["run", "transition", "--to", "spec-gating"]);

    // Three spec rounds that never reach zero: the cap is spent with no zero, and
    // that is the only state in which a fourth round is impossible.
    let mut state = state_for(worktree);
    state["spec_gate"]["rounds"] = Value::Array(
        [1, 2, 3]
            .iter()
            .map(|round| {
                serde_json::json!({
                    "round": round,
                    "status": "complete",
                    "findings": [{
                        "id": format!("s{round}"),
                        "axis": "spec",
                        "hash": format!("hash-s{round}"),
                        "summary": "still open",
                    }],
                })
            })
            .collect(),
    );
    fs::write(
        worktree.join(".director/state.json"),
        serde_json::to_string_pretty(&state).unwrap(),
    )
    .unwrap();
    let revision_before = state["revision"].as_u64().unwrap();

    let refused = run_unchecked(worktree, &["round", "open", "--spec"]);
    assert!(!refused.status.success(), "a spent cap refuses");
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(
        stderr.contains("spec review round cap exhausted"),
        "the refusal names the exhausted cap: {stderr}"
    );
    assert!(
        refused.stdout.is_empty(),
        "a refusal prints no envelope: {}",
        String::from_utf8_lossy(&refused.stdout)
    );

    // The escalation is already on disk: the refusal is about the *next* command.
    let after = state_for(worktree);
    assert_eq!(after["status"], "escalated");
    assert_eq!(after["spec_gate"]["rounds"].as_array().unwrap().len(), 3);
    assert_eq!(
        after["revision"].as_u64().unwrap(),
        revision_before + 1,
        "the incident bumped the revision"
    );
}
