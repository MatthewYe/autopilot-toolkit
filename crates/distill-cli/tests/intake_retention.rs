use serde_json::{json, Value};
use std::fs;
use std::io::Read;
#[cfg(unix)]
use std::os::unix::fs::symlink;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
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

fn assert_success(output: Output) -> Value {
    assert!(
        output.status.success(),
        "distill failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("stdout should be JSON")
}

fn assert_failure(output: Output, expected: &str) {
    assert!(
        !output.status.success(),
        "distill unexpectedly succeeded\nstdout:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(expected),
        "stderr should contain {expected:?}, got:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
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

fn intake_fixture() -> Value {
    json!({
        "schema_version": 1,
        "sources": [
            {
                "id": "text-1",
                "kind": "text",
                "provenance": {"label": "direct user text"},
                "normalized_text": "Build dashboard from text.",
                "raw_bytes_base64": "QnVpbGQgZGFzaGJvYXJkIGZyb20gdGV4dC4=",
                "hashes": {
                    "normalized_sha256": "87bc642123eea4fbad1debc0511f285e6fe1d01942e8140938eed825a38531f7",
                    "raw_sha256": "87bc642123eea4fbad1debc0511f285e6fe1d01942e8140938eed825a38531f7"
                },
                "extraction": {"tool": "codex-runtime", "truncated": false}
            },
            {
                "id": "file-1",
                "kind": "uploaded_file",
                "provenance": {"filename": "spec.txt", "mime_type": "text/plain"},
                "normalized_text": "Uploaded spec bytes",
                "raw_bytes_base64": "VXBsb2FkZWQgc3BlYyBieXRlcw==",
                "hashes": {
                    "normalized_sha256": "0127d036a03e70985eabfe7a71249deada8048becfc8849225f0c6ae6ea23d64",
                    "raw_sha256": "0127d036a03e70985eabfe7a71249deada8048becfc8849225f0c6ae6ea23d64"
                },
                "extraction": {"tool": "upload-reader", "truncated": false, "bytes_read": 19}
            },
            {
                "id": "link-1",
                "kind": "link",
                "provenance": {"url": "https://example.invalid/spec"},
                "normalized_text": "Fetched link body",
                "raw_bytes_base64": "RmV0Y2hlZCBsaW5rIGJvZHk=",
                "hashes": {
                    "normalized_sha256": "560080cc2f14b8d86853036a77e7cded29034bbda5d0fa95e3aaaf80871cd9d1",
                    "raw_sha256": "560080cc2f14b8d86853036a77e7cded29034bbda5d0fa95e3aaaf80871cd9d1"
                },
                "extraction": {"tool": "browser-fetch", "truncated": false}
            },
            {
                "id": "message-1",
                "kind": "selected_prior_user_message",
                "provenance": {"message_id": "user-msg-7", "role": "user"},
                "normalized_text": "Earlier user requirement",
                "raw_bytes_base64": "RWFybGllciB1c2VyIHJlcXVpcmVtZW50",
                "hashes": {
                    "normalized_sha256": "eeb1666c72a774e7cc5f5d3c5e5b88c75b1781d8c6ae0b3d4a94058e6f246c4d",
                    "raw_sha256": "eeb1666c72a774e7cc5f5d3c5e5b88c75b1781d8c6ae0b3d4a94058e6f246c4d"
                },
                "extraction": {"tool": "conversation-selector", "truncated": false}
            }
        ],
        "conversation_context": "assistant and tool output that must not be captured"
    })
}

fn tiny_intake_fixture() -> Value {
    json!({
        "schema_version": 1,
        "sources": [{
            "id": "tiny",
            "kind": "text",
            "provenance": {"label": "tiny"},
            "normalized_text": "tiny quota body",
            "raw_bytes_base64": "dGlueSBxdW90YSBib2R5",
            "hashes": {
                "normalized_sha256": "4c311c2d80b8c8bb2347b476eaef3a6bf69db43d3b217becdb46e80886dcc6b3",
                "raw_sha256": "4c311c2d80b8c8bb2347b476eaef3a6bf69db43d3b217becdb46e80886dcc6b3"
            },
            "extraction": {"tool": "codex-runtime", "truncated": false}
        }]
    })
}

#[cfg(unix)]
fn start_tiny_run(worktree: &Path, session_id: &str) -> String {
    let start = assert_success(distill(&[
        "start",
        "--json",
        "--runtime",
        "codex",
        "--session-id",
        session_id,
        "--worktree",
        worktree.to_str().unwrap(),
        "--intake-json",
        &tiny_intake_fixture().to_string(),
    ]));
    start["run_id"].as_str().unwrap().to_string()
}

#[cfg(unix)]
fn swap_distill_to_symlink(worktree: &Path, outside_target: &Path) {
    fs::rename(worktree.join(".distill"), outside_target).unwrap();
    symlink(outside_target, worktree.join(".distill")).unwrap();
}

#[test]
fn snapshots_only_explicit_runtime_submitted_sources() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path();
    write_local_tracker_config(worktree);
    let intake = intake_fixture().to_string();

    let start = assert_success(distill(&[
        "start",
        "--json",
        "--runtime",
        "codex",
        "--session-id",
        "thread-intake",
        "--worktree",
        worktree.to_str().unwrap(),
        "--intake-json",
        &intake,
    ]));

    let run_id = start["run_id"].as_str().unwrap();
    assert_eq!(start["stage"], "clarification");
    assert_eq!(start["intake"]["source_count"], 4);
    assert_eq!(
        start["storage"]["limits"]["per_source_bytes"],
        50 * 1024 * 1024
    );
    assert_eq!(start["storage"]["limits"]["run_bytes"], 256 * 1024 * 1024);
    assert_eq!(
        start["storage"]["limits"]["project_bytes"],
        2_u64 * 1024 * 1024 * 1024
    );
    assert_eq!(start["storage"]["limits"]["event_bytes"], 64 * 1024);
    assert_eq!(
        start["storage"]["limits"]["run_event_log_bytes"],
        32 * 1024 * 1024
    );
    assert!(fs::read_to_string(worktree.join(".gitignore"))
        .unwrap()
        .lines()
        .any(|line| line == "/.distill/"));

    let state = state_for(worktree, run_id);
    let snapshot = &state["requirement_snapshot"];
    assert_eq!(snapshot["sources"].as_array().unwrap().len(), 4);
    assert_eq!(snapshot["sources"][0]["kind"], "text");
    assert_eq!(snapshot["sources"][1]["kind"], "uploaded_file");
    assert_eq!(snapshot["sources"][2]["kind"], "link");
    assert_eq!(
        snapshot["sources"][3]["kind"],
        "selected_prior_user_message"
    );
    assert_eq!(
        snapshot["sources"][2]["provenance"]["url"],
        "https://example.invalid/spec"
    );
    assert_eq!(snapshot["sources"][3]["provenance"]["role"], "user");
    assert!(snapshot.to_string().contains("browser-fetch"));
    assert!(!snapshot.to_string().contains("assistant and tool output"));

    let raw_rel = snapshot["sources"][1]["raw_artifact_path"]
        .as_str()
        .unwrap();
    let raw_path = worktree.join(raw_rel);
    assert!(raw_path.is_file(), "raw file artifact should be retained");
    assert_eq!(fs::read(&raw_path).unwrap()[..2], [0x1f, 0x8b]);
}

#[test]
fn rejects_inaccessible_truncated_or_hash_mismatched_sources_before_writing() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path();
    write_local_tracker_config(worktree);

    let mut missing_raw = intake_fixture();
    missing_raw["sources"][1]
        .as_object_mut()
        .unwrap()
        .remove("raw_bytes_base64");
    assert_failure(
        distill(&[
            "start",
            "--json",
            "--runtime",
            "codex",
            "--session-id",
            "thread-missing",
            "--worktree",
            worktree.to_str().unwrap(),
            "--intake-json",
            &missing_raw.to_string(),
        ]),
        "raw_bytes_base64 is required",
    );
    assert!(!worktree.join(".distill").exists());

    let mut truncated = intake_fixture();
    truncated["sources"][0]["extraction"]["truncated"] = json!(true);
    assert_failure(
        distill(&[
            "start",
            "--json",
            "--runtime",
            "codex",
            "--session-id",
            "thread-truncated",
            "--worktree",
            worktree.to_str().unwrap(),
            "--intake-json",
            &truncated.to_string(),
        ]),
        "truncated sources are rejected",
    );

    let mut bad_hash = intake_fixture();
    bad_hash["sources"][0]["hashes"]["raw_sha256"] = json!("0000");
    assert_failure(
        distill(&[
            "start",
            "--json",
            "--runtime",
            "codex",
            "--session-id",
            "thread-badhash",
            "--worktree",
            worktree.to_str().unwrap(),
            "--intake-json",
            &bad_hash.to_string(),
        ]),
        "raw_sha256 does not match",
    );
}

#[test]
fn gitignore_and_distill_paths_fail_closed_on_unsafe_filesystem_state() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path().join("repo");
    fs::create_dir(&worktree).unwrap();
    write_local_tracker_config(&worktree);
    fs::write(worktree.join(".gitignore"), ".scratch/\n").unwrap();
    fs::create_dir(worktree.join(".distill")).unwrap();
    assert_failure(
        distill(&[
            "start",
            "--json",
            "--runtime",
            "codex",
            "--session-id",
            "thread-existing",
            "--worktree",
            worktree.to_str().unwrap(),
            "--intake-json",
            &intake_fixture().to_string(),
        ]),
        ".distill already exists before ignore is effective",
    );

    #[cfg(unix)]
    {
        let symlink_root = tmp.path().join("symlink-repo");
        fs::create_dir(&symlink_root).unwrap();
        write_local_tracker_config(&symlink_root);
        let outside = tmp.path().join("outside-ignore");
        fs::write(&outside, "").unwrap();
        symlink(&outside, symlink_root.join(".gitignore")).unwrap();
        assert_failure(
            distill(&[
                "start",
                "--json",
                "--runtime",
                "codex",
                "--session-id",
                "thread-symlink",
                "--worktree",
                symlink_root.to_str().unwrap(),
                "--intake-json",
                &intake_fixture().to_string(),
            ]),
            ".gitignore must not be a symlink",
        );
        assert!(!symlink_root.join(".distill").exists());

        let readonly_root = tmp.path().join("readonly-repo");
        fs::create_dir(&readonly_root).unwrap();
        write_local_tracker_config(&readonly_root);
        let original_permissions = fs::metadata(&readonly_root).unwrap().permissions();
        fs::set_permissions(&readonly_root, fs::Permissions::from_mode(0o500)).unwrap();
        assert_failure(
            distill(&[
                "start",
                "--json",
                "--runtime",
                "codex",
                "--session-id",
                "thread-readonly",
                "--worktree",
                readonly_root.to_str().unwrap(),
                "--intake-json",
                &intake_fixture().to_string(),
            ]),
            "cannot establish /.distill/ gitignore",
        );
        fs::set_permissions(&readonly_root, original_permissions).unwrap();
        assert!(!readonly_root.join(".distill").exists());
    }
}

#[test]
fn quota_exhaustion_and_quota_changes_are_atomic_and_audited() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path();
    write_local_tracker_config(worktree);
    let tiny_quota = assert_success(distill(&[
        "set-project-quota",
        "--json",
        "--worktree",
        worktree.to_str().unwrap(),
        "--bytes",
        "4",
    ]));
    assert_eq!(tiny_quota["storage"]["limits"]["project_bytes"], 4);

    assert_failure(
        distill(&[
            "start",
            "--json",
            "--runtime",
            "codex",
            "--session-id",
            "thread-quota",
            "--worktree",
            worktree.to_str().unwrap(),
            "--intake-json",
            &json!({
                "schema_version": 1,
                "sources": [{
                    "id": "too-large",
                    "kind": "text",
                    "provenance": {"label": "tiny"},
                    "normalized_text": "aaaaa",
                    "raw_bytes_base64": "YWFhYWE=",
                    "hashes": {
                        "normalized_sha256": "ed968e840d10d2d313a870bc131a4e2c311d7ad09bdf32b3418147221f51a6e2",
                        "raw_sha256": "ed968e840d10d2d313a870bc131a4e2c311d7ad09bdf32b3418147221f51a6e2"
                    },
                    "extraction": {"tool": "codex-runtime", "truncated": false}
                }]
            }).to_string(),
        ]),
        "project quota exceeded",
    );
    assert!(!worktree.join(".distill/runs").exists());

    let raised = assert_success(distill(&[
        "set-project-quota",
        "--json",
        "--worktree",
        worktree.to_str().unwrap(),
        "--bytes",
        "16384",
    ]));
    assert_eq!(raised["storage"]["limits"]["project_bytes"], 16384);

    let start = assert_success(distill(&[
        "start",
        "--json",
        "--runtime",
        "codex",
        "--session-id",
        "thread-quota-ok",
        "--worktree",
        worktree.to_str().unwrap(),
        "--intake-json",
        &intake_fixture().to_string(),
    ]));
    assert!(start["storage"]["usage"]["project_bytes"].as_u64().unwrap() > 0);
    assert_failure(
        distill(&[
            "set-project-quota",
            "--json",
            "--worktree",
            worktree.to_str().unwrap(),
            "--bytes",
            "1",
        ]),
        "cannot set project quota below current usage",
    );

    let audit = fs::read_to_string(worktree.join(".distill/quota-events.jsonl")).unwrap();
    assert!(audit.contains("\"new_project_bytes\":4"));
    assert!(audit.contains("\"new_project_bytes\":16384"));
}

#[test]
fn event_log_limits_allow_oversized_stage_evidence_via_artifact_reference() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path();
    write_local_tracker_config(worktree);
    let start = assert_success(distill(&[
        "start",
        "--json",
        "--runtime",
        "codex",
        "--session-id",
        "thread-event",
        "--worktree",
        worktree.to_str().unwrap(),
        "--intake-json",
        &intake_fixture().to_string(),
    ]));
    let run_id = start["run_id"].as_str().unwrap();
    let oversized = json!({
        "checkpoint": "clarification-complete",
        "summary": "x".repeat(64 * 1024),
        "clarified_requirement": "Large clarification evidence.",
        "decisions": [],
        "accepted_assumptions": [],
        "material_unknowns": [],
        "domain_document_artifacts": []
    })
    .to_string();

    let advanced = assert_success(distill(&[
        "submit-evidence",
        "--json",
        "--worktree",
        worktree.to_str().unwrap(),
        "--run-id",
        run_id,
        "--session-id",
        "thread-event",
        "--expected-revision",
        "1",
        "--stage",
        "clarification",
        "--evidence",
        &oversized,
    ]));
    assert_eq!(advanced["stage"], "prd");
    let events = fs::read_to_string(
        worktree
            .join(".distill/runs")
            .join(run_id)
            .join("events.jsonl"),
    )
    .unwrap();
    for line in events.lines() {
        assert!(line.len() < 64 * 1024);
    }
    let stage_event: Value = events
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .find(|event: &Value| {
            event["type"] == "stage-completed" && event["payload"]["stage"] == "clarification"
        })
        .unwrap();
    let artifact_path = stage_event["payload"]["evidence"]["artifact_path"]
        .as_str()
        .unwrap();
    assert!(worktree.join(artifact_path).is_file());
}

#[test]
fn purge_removes_replayable_content_but_keeps_tombstone_and_publication_refs() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path();
    write_local_tracker_config(worktree);
    let start = assert_success(distill(&[
        "start",
        "--json",
        "--runtime",
        "codex",
        "--session-id",
        "thread-purge",
        "--worktree",
        worktree.to_str().unwrap(),
        "--intake-json",
        &intake_fixture().to_string(),
    ]));
    let run_id = start["run_id"].as_str().unwrap();
    let raw_rel = state_for(worktree, run_id)["requirement_snapshot"]["sources"][0]
        ["raw_artifact_path"]
        .as_str()
        .unwrap()
        .to_string();

    let clarification = json!({
        "checkpoint": "clarification-complete",
        "summary": "Clarified.",
        "clarified_requirement": "Clarified intake fixture.",
        "decisions": [],
        "accepted_assumptions": [],
        "material_unknowns": [],
        "domain_document_artifacts": []
    })
    .to_string();
    assert_success(distill(&[
        "submit-evidence",
        "--json",
        "--worktree",
        worktree.to_str().unwrap(),
        "--run-id",
        run_id,
        "--session-id",
        "thread-purge",
        "--expected-revision",
        "1",
        "--stage",
        "clarification",
        "--evidence",
        &clarification,
    ]));
    let prd = json!({
        "checkpoint": "testing-seam-confirmed",
        "summary": "PRD.",
        "prd_markdown": "# PRD\n"
    })
    .to_string();
    assert_success(distill(&[
        "submit-evidence",
        "--json",
        "--worktree",
        worktree.to_str().unwrap(),
        "--run-id",
        run_id,
        "--session-id",
        "thread-purge",
        "--expected-revision",
        "2",
        "--stage",
        "prd",
        "--evidence",
        &prd,
    ]));
    let issues = json!({
        "checkpoint": "slice-breakdown-approved",
        "summary": "Issue.",
        "issues": [{
            "title": "First slice",
            "body": "---\nkey: 01-first-slice\ntitle: First slice\ntype: issue\nstatus: ready-for-agent\nparent: .scratch/distill-tracer/PRD.md\n---\n\n## What to build\n\nBuild the first slice.\n\n## Acceptance Criteria\n\n- [ ] The slice works.\n\n## Blocked by\n\n- None — can start immediately.\n\n## Comments\n",
            "depends_on": []
        }]
    })
    .to_string();
    assert_success(distill(&[
        "submit-evidence",
        "--json",
        "--worktree",
        worktree.to_str().unwrap(),
        "--run-id",
        run_id,
        "--session-id",
        "thread-purge",
        "--expected-revision",
        "3",
        "--stage",
        "issues",
        "--evidence",
        &issues,
    ]));

    assert!(worktree.join(&raw_rel).exists());
    let purged = assert_success(distill(&[
        "purge",
        "--json",
        "--worktree",
        worktree.to_str().unwrap(),
        "--run-id",
        run_id,
        "--session-id",
        "thread-purge",
        "--expected-revision",
        "4",
        "--user-authorized",
    ]));
    assert_eq!(purged["status"], "purged");
    assert!(!worktree.join(&raw_rel).exists());
    assert!(!worktree
        .join(".distill/runs")
        .join(run_id)
        .join("snapshots")
        .exists());

    let tombstone_path = worktree
        .join(".distill/runs")
        .join(run_id)
        .join("tombstone.json");
    let tombstone: Value =
        serde_json::from_str(&fs::read_to_string(tombstone_path).unwrap()).unwrap();
    assert_eq!(tombstone["run_id"], run_id);
    assert_eq!(tombstone["state"], "purged");
    assert_eq!(tombstone["source_hashes"].as_array().unwrap().len(), 4);
    assert_eq!(
        tombstone["publications"]["prd"]["path"],
        ".scratch/distill-tracer/PRD.md"
    );
    assert_eq!(state_for(worktree, run_id)["state"], "purged");
}

#[test]
fn retained_source_bytes_are_losslessly_compressed() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path();
    write_local_tracker_config(worktree);
    let start = assert_success(distill(&[
        "start",
        "--json",
        "--runtime",
        "codex",
        "--session-id",
        "thread-compression",
        "--worktree",
        worktree.to_str().unwrap(),
        "--intake-json",
        &intake_fixture().to_string(),
    ]));
    let run_id = start["run_id"].as_str().unwrap();
    let state = state_for(worktree, run_id);
    let raw_rel = state["requirement_snapshot"]["sources"][1]["raw_artifact_path"]
        .as_str()
        .unwrap();
    let bytes = fs::read(worktree.join(raw_rel)).unwrap();
    let mut decoder = flate2::read::GzDecoder::new(&bytes[..]);
    let mut decoded = String::new();
    decoder.read_to_string(&mut decoded).unwrap();
    assert_eq!(decoded, "Uploaded spec bytes");
}

#[test]
fn caller_supplied_run_ids_cannot_escape_distill_runs() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path();
    write_local_tracker_config(worktree);
    fs::write(worktree.join(".gitignore"), "/.distill/\n").unwrap();
    let outside = worktree.join(".distill/outside");
    fs::create_dir_all(outside.join("artifacts")).unwrap();
    fs::write(outside.join("artifacts/secret.txt"), "must remain").unwrap();
    fs::write(
        outside.join("state.json"),
        json!({
            "schema_version": 1,
            "run_id": "../outside",
            "state": "active",
            "revision": 1,
            "current_stage": "clarification",
            "session_binding": {"runtime": "codex", "session_id": "thread-attack"},
            "workflow": {"version": "distill.v1", "source": "test", "stages": []},
            "requirement": {"source": "explicit-text", "text": "attack"},
            "requirement_snapshot": {
                "sources": [{
                    "id": "attack",
                    "kind": "text",
                    "hashes": {"normalized_sha256": "h", "raw_sha256": "h"},
                    "raw_artifact_path": ".distill/outside/artifacts/secret.txt"
                }]
            },
            "stages": [],
            "completion_evidence": [],
            "publications": {"prd": null, "issues": []},
            "report": null,
            "context_baseline": {"domain_documents": []},
            "drift_acknowledgments": [],
            "handoffs": [],
            "migration_events": [],
            "implementation_started": false
        })
        .to_string(),
    )
    .unwrap();

    assert_failure(
        distill(&[
            "purge",
            "--json",
            "--worktree",
            worktree.to_str().unwrap(),
            "--run-id",
            "../outside",
            "--session-id",
            "thread-attack",
            "--expected-revision",
            "1",
            "--user-authorized",
        ]),
        "run id is unsafe",
    );
    assert_eq!(
        fs::read_to_string(outside.join("artifacts/secret.txt")).unwrap(),
        "must remain"
    );
}

#[cfg(unix)]
#[test]
fn submit_evidence_fails_closed_after_distill_symlink_swap() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path().join("repo");
    fs::create_dir(&worktree).unwrap();
    write_local_tracker_config(&worktree);
    let run_id = start_tiny_run(&worktree, "thread-submit-symlink");
    swap_distill_to_symlink(&worktree, &tmp.path().join("outside-submit-distill"));
    let evidence = json!({
        "checkpoint": "clarification-complete",
        "summary": "Clarified.",
        "clarified_requirement": "Clarified intake fixture.",
        "decisions": [],
        "accepted_assumptions": [],
        "material_unknowns": [],
        "domain_document_artifacts": []
    })
    .to_string();

    assert_failure(
        distill(&[
            "submit-evidence",
            "--json",
            "--worktree",
            worktree.to_str().unwrap(),
            "--run-id",
            &run_id,
            "--session-id",
            "thread-submit-symlink",
            "--expected-revision",
            "1",
            "--stage",
            "clarification",
            "--evidence",
            &evidence,
        ]),
        ".distill must not be a symlink",
    );
}

#[cfg(unix)]
#[test]
fn takeover_fails_closed_after_distill_symlink_swap() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path().join("repo");
    fs::create_dir(&worktree).unwrap();
    write_local_tracker_config(&worktree);
    let run_id = start_tiny_run(&worktree, "thread-takeover-symlink");
    swap_distill_to_symlink(&worktree, &tmp.path().join("outside-takeover-distill"));

    assert_failure(
        distill(&[
            "takeover",
            "--json",
            "--worktree",
            worktree.to_str().unwrap(),
            "--run-id",
            &run_id,
            "--from-session",
            "thread-takeover-symlink",
            "--to-session",
            "thread-takeover-next",
            "--expected-revision",
            "1",
            "--reason",
            "explicit user handoff",
            "--user-authorized",
        ]),
        ".distill must not be a symlink",
    );
}

#[cfg(unix)]
#[test]
fn supersede_fails_closed_after_distill_symlink_swap() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path().join("repo");
    fs::create_dir(&worktree).unwrap();
    write_local_tracker_config(&worktree);
    let run_id = start_tiny_run(&worktree, "thread-supersede-symlink");
    swap_distill_to_symlink(&worktree, &tmp.path().join("outside-supersede-distill"));

    assert_failure(
        distill(&[
            "supersede",
            "--json",
            "--worktree",
            worktree.to_str().unwrap(),
            "--run-id",
            &run_id,
            "--session-id",
            "thread-supersede-symlink",
            "--expected-revision",
            "1",
            "--reason",
            "new material requirement",
            "--requirement",
            "Build a revised tiny thing.",
        ]),
        ".distill must not be a symlink",
    );
}

#[cfg(unix)]
#[test]
fn inspect_fails_closed_after_distill_symlink_swap() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path().join("repo");
    fs::create_dir(&worktree).unwrap();
    write_local_tracker_config(&worktree);
    let run_id = start_tiny_run(&worktree, "thread-inspect-symlink");
    swap_distill_to_symlink(&worktree, &tmp.path().join("outside-inspect-distill"));

    assert_failure(
        distill(&[
            "inspect",
            "--json",
            "--worktree",
            worktree.to_str().unwrap(),
            "--run-id",
            &run_id,
            "--session-id",
            "thread-inspect-symlink",
            "--expected-revision",
            "1",
        ]),
        ".distill must not be a symlink",
    );
}

#[test]
fn project_quota_accounts_for_actual_persisted_bytes_before_writing_run() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path();
    write_local_tracker_config(worktree);
    assert_success(distill(&[
        "set-project-quota",
        "--json",
        "--worktree",
        worktree.to_str().unwrap(),
        "--bytes",
        "700",
    ]));

    assert_failure(
        distill(&[
            "start",
            "--json",
            "--runtime",
            "codex",
            "--session-id",
            "thread-actual-quota",
            "--worktree",
            worktree.to_str().unwrap(),
            "--intake-json",
            &tiny_intake_fixture().to_string(),
        ]),
        "project quota exceeded",
    );
    assert!(!worktree.join(".distill/runs").exists());
}

#[test]
fn publication_payload_quota_blocks_without_partial_publication_or_stage_mutation() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path();
    write_local_tracker_config(worktree);
    assert_success(distill(&[
        "set-project-quota",
        "--json",
        "--worktree",
        worktree.to_str().unwrap(),
        "--bytes",
        "20000",
    ]));
    let start = assert_success(distill(&[
        "start",
        "--json",
        "--runtime",
        "codex",
        "--session-id",
        "thread-publication-quota",
        "--worktree",
        worktree.to_str().unwrap(),
        "--intake-json",
        &tiny_intake_fixture().to_string(),
    ]));
    let run_id = start["run_id"].as_str().unwrap();
    let clarification = json!({
        "checkpoint": "clarification-complete",
        "summary": "Clarified.",
        "clarified_requirement": "Clarified intake fixture.",
        "decisions": [],
        "accepted_assumptions": [],
        "material_unknowns": [],
        "domain_document_artifacts": []
    })
    .to_string();
    assert_success(distill(&[
        "submit-evidence",
        "--json",
        "--worktree",
        worktree.to_str().unwrap(),
        "--run-id",
        run_id,
        "--session-id",
        "thread-publication-quota",
        "--expected-revision",
        "1",
        "--stage",
        "clarification",
        "--evidence",
        &clarification,
    ]));
    let before = state_for(worktree, run_id);
    let oversized_prd = json!({
        "checkpoint": "testing-seam-confirmed",
        "summary": "PRD.",
        "prd_markdown": "x".repeat(18000)
    })
    .to_string();

    assert_failure(
        distill(&[
            "submit-evidence",
            "--json",
            "--worktree",
            worktree.to_str().unwrap(),
            "--run-id",
            run_id,
            "--session-id",
            "thread-publication-quota",
            "--expected-revision",
            "2",
            "--stage",
            "prd",
            "--evidence",
            &oversized_prd,
        ]),
        "project quota exceeded",
    );
    assert_eq!(state_for(worktree, run_id), before);
    assert!(!worktree.join(".scratch/distill-tracer/PRD.md").exists());
    assert!(!worktree
        .join(".distill/runs")
        .join(run_id)
        .join("artifacts/prd.md")
        .exists());
}

#[test]
fn intake_snapshot_commit_is_run_atomic_when_sources_collide() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree = tmp.path();
    write_local_tracker_config(worktree);
    let mut duplicate = tiny_intake_fixture();
    let repeated = duplicate["sources"][0].clone();
    duplicate["sources"].as_array_mut().unwrap().push(repeated);

    assert_failure(
        distill(&[
            "start",
            "--json",
            "--runtime",
            "codex",
            "--session-id",
            "thread-duplicate",
            "--worktree",
            worktree.to_str().unwrap(),
            "--intake-json",
            &duplicate.to_string(),
        ]),
        "duplicate source id",
    );
    assert!(!worktree.join(".distill/runs").exists());
}
