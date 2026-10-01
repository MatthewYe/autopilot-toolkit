use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn project(root: &Path) {
    fs::create_dir_all(root.join("docs/agents")).unwrap();
    for (name, content) in [
        (
            "issue-tracker",
            "# Issue tracker: Local Markdown\n\nIssues live in `.scratch/`.\n",
        ),
        ("triage-labels", "# Labels\n"),
        ("domain", "# Domain\n"),
    ] {
        fs::write(root.join(format!("docs/agents/{name}.md")), content).unwrap();
    }
}

fn cli(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_distill"))
        .args(args)
        .args(["--json", "--worktree", root.to_str().unwrap()])
        .output()
        .unwrap()
}

fn success(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn failure(output: Output, expected: &str) {
    assert!(!output.status.success(), "command unexpectedly succeeded");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(expected),
        "expected {expected:?}, got {stderr}"
    );
}

fn start(root: &Path) -> Value {
    success(cli(
        root,
        &[
            "start",
            "--runtime",
            "codex",
            "--session-id",
            "glossary-test",
            "--requirement",
            "Build a local audit dashboard.",
        ],
    ))
}

fn state(root: &Path, run: &str) -> Value {
    serde_json::from_slice(&fs::read(root.join(format!(".distill/runs/{run}/state.json"))).unwrap())
        .unwrap()
}

fn evidence(root: &Path, names: &[&str]) -> Value {
    let artifacts: Vec<Value> = names
        .iter()
        .map(|name| {
            let bytes = fs::read(root.join(name)).unwrap();
            json!({"path": name, "sha256": format!("{:x}", Sha256::digest(bytes))})
        })
        .collect();
    json!({
        "checkpoint": "clarification-complete",
        "summary": "Use a local CLI seam.",
        "clarified_requirement": "Build a local audit dashboard.",
        "decisions": [], "accepted_assumptions": [], "material_unknowns": [],
        "domain_document_artifacts": artifacts
    })
}

fn submit(root: &Path, run: &str, evidence: &Value) -> Output {
    cli(
        root,
        &[
            "submit-evidence",
            "--run-id",
            run,
            "--session-id",
            "glossary-test",
            "--expected-revision",
            "1",
            "--stage",
            "clarification",
            "--evidence",
            &evidence.to_string(),
        ],
    )
}

#[test]
fn captures_and_resumes_new_legacy_both_and_absent_glossaries() {
    for names in [
        vec![],
        vec!["CONTEXT.md"],
        vec!["GLOSSARY.md"],
        vec!["CONTEXT.md", "GLOSSARY.md"],
    ] {
        let tmp = tempfile::tempdir().unwrap();
        project(tmp.path());
        for name in &names {
            fs::write(tmp.path().join(name), format!("# {name}\n")).unwrap();
        }
        let started = start(tmp.path());
        let run = started["run_id"].as_str().unwrap();
        let before = state(tmp.path(), run);
        let documents = before["context_baseline"]["domain_documents"]
            .as_array()
            .unwrap();
        let roots: Vec<&str> = documents
            .iter()
            .filter_map(|doc| doc["path"].as_str())
            .filter(|path| !path.contains('/'))
            .collect();
        assert_eq!(roots, names, "every present glossary must be captured");
        assert_eq!(start(tmp.path())["run_id"], run);
        assert_eq!(
            state(tmp.path(), run),
            before,
            "resumption must preserve the existing baseline"
        );
        assert_eq!(
            success(submit(tmp.path(), run, &evidence(tmp.path(), &[])))["stage"],
            "prd"
        );
    }
}

#[test]
fn accepts_owned_edits_to_both_glossary_names() {
    for names in [
        vec!["CONTEXT.md"],
        vec!["GLOSSARY.md"],
        vec!["CONTEXT.md", "GLOSSARY.md"],
    ] {
        let tmp = tempfile::tempdir().unwrap();
        project(tmp.path());
        for name in &names {
            fs::write(tmp.path().join(name), "# Before\n").unwrap();
        }
        let started = start(tmp.path());
        let run = started["run_id"].as_str().unwrap();
        for name in &names {
            fs::write(tmp.path().join(name), "# After\n").unwrap();
        }
        let submitted = evidence(tmp.path(), &names);
        assert_eq!(success(submit(tmp.path(), run, &submitted))["stage"], "prd");
        assert_eq!(
            state(tmp.path(), run)["clarification"]["domain_document_artifacts"],
            submitted["domain_document_artifacts"]
        );
    }
}

#[test]
fn detects_modification_and_deletion_of_either_glossary_without_mutating_state() {
    for name in ["CONTEXT.md", "GLOSSARY.md"] {
        for delete in [false, true] {
            let tmp = tempfile::tempdir().unwrap();
            project(tmp.path());
            for file in ["CONTEXT.md", "GLOSSARY.md"] {
                fs::write(tmp.path().join(file), "# Before\n").unwrap();
            }
            let started = start(tmp.path());
            let run = started["run_id"].as_str().unwrap();
            let before = state(tmp.path(), run);
            if delete {
                fs::remove_file(tmp.path().join(name)).unwrap();
            } else {
                fs::write(tmp.path().join(name), "# After\n").unwrap();
            }
            let output = submit(tmp.path(), run, &evidence(tmp.path(), &[]));
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            failure(output, "context drift detected");
            assert!(
                stderr.contains("domain_documents") && stderr.contains(name),
                "{stderr}"
            );
            assert_eq!(state(tmp.path(), run), before);
        }
    }
}

#[test]
fn rejects_invalid_paths_and_hashes_for_glossary_evidence() {
    for (path, expected) in [
        ("GLOSSARY.md", "hash mismatch"),
        ("CONTEXT.md", "hash mismatch"),
        ("../GLOSSARY.md", "path is not allowed"),
        ("other.md", "path is not allowed"),
    ] {
        let tmp = tempfile::tempdir().unwrap();
        project(tmp.path());
        fs::write(tmp.path().join("GLOSSARY.md"), "# Glossary\n").unwrap();
        fs::write(tmp.path().join("CONTEXT.md"), "# Legacy\n").unwrap();
        let started = start(tmp.path());
        let mut submitted = evidence(tmp.path(), &[]);
        submitted["domain_document_artifacts"] = json!([{"path": path, "sha256": "wrong"}]);
        failure(
            submit(tmp.path(), started["run_id"].as_str().unwrap(), &submitted),
            expected,
        );
    }
}

#[test]
fn abort_records_new_and_legacy_glossary_artifacts() {
    let tmp = tempfile::tempdir().unwrap();
    project(tmp.path());
    let started = start(tmp.path());
    let run = started["run_id"].as_str().unwrap();
    for name in ["CONTEXT.md", "GLOSSARY.md"] {
        fs::write(tmp.path().join(name), "# New term\n").unwrap();
    }
    success(cli(
        tmp.path(),
        &[
            "abort",
            "--run-id",
            run,
            "--session-id",
            "glossary-test",
            "--expected-revision",
            "1",
            "--reason",
            "Fixture is complete.",
            "--user-authorized",
        ],
    ));
    assert_eq!(
        state(tmp.path(), run)["abort"]["domain_document_artifacts"],
        evidence(tmp.path(), &["CONTEXT.md", "GLOSSARY.md"])["domain_document_artifacts"]
    );
}

#[cfg(unix)]
#[test]
fn rejects_glossary_symlinks_during_capture_evidence_and_abort() {
    for name in ["CONTEXT.md", "GLOSSARY.md"] {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("project");
        project(&root);
        let outside = tmp.path().join("outside.md");
        fs::write(&outside, "# Outside\n").unwrap();
        std::os::unix::fs::symlink(&outside, root.join(name)).unwrap();
        failure(
            cli(
                &root,
                &[
                    "start",
                    "--runtime",
                    "codex",
                    "--session-id",
                    "glossary-test",
                    "--requirement",
                    "Build a dashboard.",
                ],
            ),
            "symlink",
        );
        fs::remove_file(root.join(name)).unwrap();
        let started = start(&root);
        let run = started["run_id"].as_str().unwrap();
        std::os::unix::fs::symlink(&outside, root.join(name)).unwrap();
        failure(submit(&root, run, &evidence(&root, &[name])), "symlink");
        failure(
            cli(
                &root,
                &[
                    "abort",
                    "--run-id",
                    run,
                    "--session-id",
                    "glossary-test",
                    "--expected-revision",
                    "1",
                    "--reason",
                    "Stop fixture.",
                    "--user-authorized",
                ],
            ),
            "symlink",
        );
    }
}
