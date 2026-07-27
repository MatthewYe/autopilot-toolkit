use serde_json::{json, Map, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const RUNTIMES: [&str; 3] = ["codex", "reasonix", "kimi"];

fn distill_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_distill"))
}

fn distill(args: &[&str], envs: &[(&str, &str)]) -> Output {
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

fn write_fake_tracker_config(worktree: &Path) {
    fs::create_dir_all(worktree.join("docs/agents")).unwrap();
    fs::write(
        worktree.join("docs/agents/issue-tracker.md"),
        "# Issue tracker: Fake Adapter\n\nAdapter state lives in `.fake-tracker/`.\n",
    )
    .unwrap();
    fs::write(
        worktree.join("docs/agents/triage-labels.md"),
        "| Label | Value |\n| --- | --- |\n| ready-for-agent | ready-for-agent |\n",
    )
    .unwrap();
    fs::write(worktree.join("docs/agents/domain.md"), "# Domain\n").unwrap();
}

fn intake_fixture(runtime: &str) -> Value {
    json!({
        "schema_version": 1,
        "sources": [{
            "id": "release-file",
            "kind": "uploaded_file",
            "provenance": {
                "filename": "release-requirement.txt",
                "mime_type": "text/plain",
                "runtime": runtime,
            },
            "normalized_text": "Release matrix file intake.",
            "raw_bytes_base64": "UmVsZWFzZSBtYXRyaXggZmlsZSBpbnRha2Uu",
            "hashes": {
                "normalized_sha256": "7f1a9facb9527ccd5e11aa4cd608db3217af407310bb107a7d1099282dd82d18",
                "raw_sha256": "7f1a9facb9527ccd5e11aa4cd608db3217af407310bb107a7d1099282dd82d18",
            },
            "extraction": {
                "tool": format!("{runtime}-file-reader"),
                "truncated": false,
            },
        }],
    })
}

fn start(worktree: &Path, runtime: &str, session: &str) -> Output {
    let intake = intake_fixture(runtime).to_string();
    distill(
        &[
            "start",
            "--json",
            "--runtime",
            runtime,
            "--session-id",
            session,
            "--worktree",
            worktree.to_str().unwrap(),
            "--intake-json",
            &intake,
        ],
        &[],
    )
}

fn submit(
    worktree: &Path,
    run_id: &str,
    session: &str,
    revision: u64,
    stage: &str,
    evidence: &Value,
    envs: &[(&str, &str)],
) -> Output {
    let revision = revision.to_string();
    let evidence = evidence.to_string();
    distill(
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
            &revision,
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

fn fake_create_count(worktree: &Path) -> usize {
    fs::read_to_string(worktree.join(".fake-tracker/create-log.jsonl"))
        .unwrap_or_default()
        .lines()
        .count()
}

fn object_shape(value: &Value) -> Value {
    match value {
        Value::Null => json!("null"),
        Value::Bool(_) => json!("bool"),
        Value::Number(_) => json!("number"),
        Value::String(_) => json!("string"),
        Value::Array(values) => Value::Array(values.iter().map(object_shape).collect()),
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(key, value)| (key.clone(), object_shape(value)))
                .collect::<Map<_, _>>(),
        ),
    }
}

struct RuntimeArtifacts {
    state: Value,
    events: Value,
    evidence: Vec<Value>,
    publications: Value,
    report: Value,
}

fn exercise_runtime(runtime: &str) -> RuntimeArtifacts {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path();
    write_fake_tracker_config(worktree);
    let original_session = format!("{runtime}-release-session");
    let takeover_session = format!("{runtime}-takeover-session");
    let foreign_session = format!("{runtime}-foreign-session");

    let started = json_success(start(worktree, runtime, &original_session));
    let run_id = started["run_id"].as_str().unwrap().to_string();
    assert_eq!(started["runtime"], runtime);
    assert_eq!(started["next_action"], "invoke-skill");
    assert_eq!(started["authorized_action"]["stage"], "clarification");
    assert_eq!(started["implementation_started"], false);
    assert_eq!(
        state_for(worktree, &run_id)["requirement_snapshot"]["sources"][0]["kind"],
        "uploaded_file"
    );

    let resumed = json_success(start(worktree, runtime, &original_session));
    assert_eq!(resumed["run_id"], run_id, "same session must resume");
    assert_eq!(resumed["revision"], 1);

    let clarification = json!({
        "checkpoint": "clarification-complete",
        "summary": "The release matrix uses one bounded fixture.",
        "clarified_requirement": "The release matrix uses one bounded fixture.",
        "decisions": [],
        "accepted_assumptions": [],
        "material_unknowns": [],
        "domain_document_artifacts": []
    });
    assert_error_contains(
        submit(
            worktree,
            &run_id,
            &foreign_session,
            1,
            "clarification",
            &clarification,
            &[],
        ),
        "session id does not match active run binding",
    );

    let duplicate_dir = worktree.join(".distill/runs/duplicate-unfinished");
    fs::create_dir_all(&duplicate_dir).unwrap();
    let mut duplicate = state_for(worktree, &run_id);
    duplicate["run_id"] = json!("duplicate-unfinished");
    fs::write(
        duplicate_dir.join("state.json"),
        serde_json::to_vec_pretty(&duplicate).unwrap(),
    )
    .unwrap();
    assert_error_contains(
        start(worktree, runtime, &original_session),
        &format!("multiple unfinished runs are bound to session {original_session}"),
    );
    fs::remove_dir_all(duplicate_dir).unwrap();

    let takeover = json_success(distill(
        &[
            "takeover",
            "--json",
            "--worktree",
            worktree.to_str().unwrap(),
            "--run-id",
            &run_id,
            "--from-session",
            &original_session,
            "--to-session",
            &takeover_session,
            "--expected-revision",
            "1",
            "--reason",
            "Release-matrix fixture explicitly transfers the interrupted session.",
            "--user-authorized",
        ],
        &[],
    ));
    assert_eq!(takeover["session_id"], takeover_session);
    assert_eq!(takeover["revision"], 2);

    fs::write(
        worktree.join("docs/agents/domain.md"),
        "# Domain\n\nNon-material release-matrix note.\n",
    )
    .unwrap();
    assert_error_contains(
        submit(
            worktree,
            &run_id,
            &takeover_session,
            2,
            "clarification",
            &clarification,
            &[],
        ),
        "context drift detected",
    );
    let clarification_with_recovery = json!({
        "checkpoint": "clarification-complete",
        "summary": "The release matrix uses one bounded fixture.",
        "clarified_requirement": "The release matrix uses one bounded fixture.",
        "decisions": [],
        "accepted_assumptions": [],
        "material_unknowns": [],
        "domain_document_artifacts": [],
        "drift_acknowledgment": {
            "material": false,
            "reason": "The added domain note does not change the fixture requirement."
        }
    });
    let after_clarification = json_success(submit(
        worktree,
        &run_id,
        &takeover_session,
        2,
        "clarification",
        &clarification_with_recovery,
        &[],
    ));
    assert_eq!(after_clarification["stage"], "prd");
    fs::write(worktree.join("docs/agents/domain.md"), "# Domain\n").unwrap();

    let prd = json!({
        "checkpoint": "testing-seam-confirmed",
        "summary": "The PRD preserves the shared CLI seam.",
        "prd_markdown": "# Release Matrix PRD\n\n## Problem Statement\n\nExercise the shared Distill runner.\n"
    });
    let reconciliation = json_success(submit(
        worktree,
        &run_id,
        &takeover_session,
        after_clarification["revision"].as_u64().unwrap(),
        "prd",
        &prd,
        &[("DISTILL_FAKE_TRACKER_MODE", "timeout-before-response")],
    ));
    assert_eq!(reconciliation["stage"], "prd");
    assert_eq!(
        reconciliation["publication_blocked"],
        "publication needs-reconciliation"
    );
    assert_eq!(fake_create_count(worktree), 1);

    let after_prd = json_success(submit(
        worktree,
        &run_id,
        &takeover_session,
        reconciliation["revision"].as_u64().unwrap(),
        "prd",
        &prd,
        &[],
    ));
    assert_eq!(after_prd["stage"], "issues");
    assert_eq!(
        fake_create_count(worktree),
        1,
        "reconciliation must verify rather than duplicate publication"
    );

    let issues = json!({
        "checkpoint": "slice-breakdown-approved",
        "summary": "One implementation slice is ready for a later agent.",
        "issues": [{
            "title": "Implement the release fixture",
            "body": "Status: ready-for-agent\n\n## Acceptance criteria\n\n- [ ] The fixture passes.\n"
        }]
    });
    let completed = json_success(submit(
        worktree,
        &run_id,
        &takeover_session,
        after_prd["revision"].as_u64().unwrap(),
        "issues",
        &issues,
        &[],
    ));
    assert_eq!(completed["status"], "completed");
    assert_eq!(completed["next_action"], "terminal");
    assert_eq!(completed["implementation_started"], false);
    assert_eq!(completed["session_binding"]["released"], true);

    let state = state_for(worktree, &run_id);
    let events = json_success(distill(
        &[
            "events",
            "--json",
            "--worktree",
            worktree.to_str().unwrap(),
            "--run-id",
            &run_id,
            "--after",
            "0",
        ],
        &[],
    ));
    let evidence_dir = worktree
        .join(".distill/runs")
        .join(&run_id)
        .join("artifacts/evidence");
    let mut evidence_paths = fs::read_dir(evidence_dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    evidence_paths.sort();
    let evidence = evidence_paths
        .iter()
        .map(|path| serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap())
        .collect();
    let report: Value = serde_json::from_str(
        &fs::read_to_string(worktree.join(completed["report"]["json_path"].as_str().unwrap()))
            .unwrap(),
    )
    .unwrap();

    RuntimeArtifacts {
        publications: state["publications"].clone(),
        state,
        events,
        evidence,
        report,
    }
}

#[test]
fn shared_binary_release_contract_is_schema_equivalent_for_every_runtime() {
    let artifacts = RUNTIMES
        .iter()
        .map(|runtime| (*runtime, exercise_runtime(runtime)))
        .collect::<Vec<_>>();
    let baseline = &artifacts[0].1;

    for (runtime, candidate) in artifacts.iter().skip(1) {
        assert_eq!(
            object_shape(&candidate.state),
            object_shape(&baseline.state),
            "{runtime} state schema differs from codex"
        );
        assert_eq!(
            object_shape(&candidate.events),
            object_shape(&baseline.events),
            "{runtime} event schema differs from codex"
        );
        assert_eq!(
            object_shape(&Value::Array(candidate.evidence.clone())),
            object_shape(&Value::Array(baseline.evidence.clone())),
            "{runtime} evidence schema differs from codex"
        );
        assert_eq!(
            object_shape(&candidate.publications),
            object_shape(&baseline.publications),
            "{runtime} publication schema differs from codex"
        );
        assert_eq!(
            object_shape(&candidate.report),
            object_shape(&baseline.report),
            "{runtime} report schema differs from codex"
        );
    }
}
