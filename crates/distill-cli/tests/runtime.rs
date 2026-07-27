use serde_json::Value;
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

fn assert_success(output: Output) -> Value {
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

#[test]
fn start_accepts_only_supported_runtime_values() {
    for runtime in ["codex", "kimi", "reasonix"] {
        let tmp = tempfile::tempdir().unwrap();
        let worktree = tmp.path();
        write_local_tracker_config(worktree);

        let started = assert_success(distill(&[
            "start",
            "--json",
            "--runtime",
            runtime,
            "--session-id",
            &format!("{runtime}-session"),
            "--worktree",
            worktree.to_str().unwrap(),
            "--requirement",
            "Build a small audit dashboard for deployment status.",
        ]));

        assert_eq!(started["runtime"], runtime);
        assert_eq!(started["session_binding"]["runtime"], runtime);
        assert_eq!(started["session_id"], format!("{runtime}-session"));
        assert_eq!(started["stage"], "clarification");
        assert_eq!(started["next_action"], "invoke-skill");
    }

    let tmp = tempfile::tempdir().unwrap();
    write_local_tracker_config(tmp.path());
    assert_error_contains(
        distill(&[
            "start",
            "--json",
            "--runtime",
            "banana",
            "--session-id",
            "banana-session",
            "--worktree",
            tmp.path().to_str().unwrap(),
            "--requirement",
            "Build a small audit dashboard for deployment status.",
        ]),
        "--runtime must be one of: codex, kimi, reasonix",
    );
}
