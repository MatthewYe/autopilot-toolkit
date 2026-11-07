#!/usr/bin/env rust-script
//! ```cargo
//! [dependencies]
//! serde_json = "1"
//! ```
//!
//! Integration tests for the scripts/upstream-drift.rs CLI contract.
//! Drives the real entry script via std::process::Command against a fixture
//! project root (PROJECT_ROOT) with a committed pin; `--tag` simulates an
//! upstream release without a network round-trip.
//!
//! Run: rust-script --test tests/test_upstream_drift.rs

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn main() {
    println!("Run with: rust-script --test tests/test_upstream_drift.rs");
}

// ── Helpers ─────────────────────────────────────────────────────────────

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new(prefix: &str) -> Self {
        let n = TEMP_COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("{}-{}-{}", prefix, std::process::id(), n));
        fs::create_dir_all(&dir).expect("create temp dir");
        TempDir(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Find the actual project root — the directory containing scripts/upstream-drift.rs.
fn actual_project_root() -> PathBuf {
    let src = Path::new(file!());
    if let (Some(_tests_dir), Some(proj)) = (src.parent(), src.parent().and_then(|p| p.parent())) {
        let candidate = proj.to_path_buf();
        if candidate.join("scripts/upstream-drift.rs").exists() {
            return candidate;
        }
    }
    if let Ok(root) = std::env::var("PROJECT_ROOT") {
        let p = PathBuf::from(&root);
        if p.join("scripts/upstream-drift.rs").exists() {
            return p;
        }
    }
    panic!("Cannot find project root (scripts/upstream-drift.rs not found)");
}

fn run_drift(script: &Path, project: &Path, args: &[&str]) -> (String, String, i32) {
    assert!(
        script.exists(),
        "upstream-drift.rs not found at {:?}",
        script
    );

    let output = Command::new("rust-script")
        // `-f`: the cache ignores path-dependency changes (`crates/*`,
        // upstream rust-script#122).
        .arg("-f")
        .arg(script)
        .args(args)
        .env("PROJECT_ROOT", project)
        .output()
        .expect("failed to run rust-script");

    (
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
        output.status.code().unwrap_or(-1),
    )
}

/// Writes a minimal lock file so `shared::project_root()` treats `project`
/// as the project root (PROJECT_ROOT is honored only for a directory that
/// contains `.skill-lock.json`).
fn write_lock(project: &Path) {
    fs::write(
        project.join(".skill-lock.json"),
        "{\n  \"version\": 4,\n  \"skills\": [],\n  \"dismissed\": {}\n}\n",
    )
    .expect("write lock");
}

/// Writes a valid tag pin (v1.3.2) into `project`.
fn write_pin(project: &Path) {
    write_lock(project);
    let pin = serde_json::json!({
        "schemaVersion": 1,
        "source": "mattpocock/skills",
        "sourceUrl": "https://github.com/mattpocock/skills.git",
        "ref": "v1.3.2",
        "refKind": "tag",
        "resolvedCommit": "d1478fcb140db0b93c0c8df17bcfd311844749d0",
        "version": "1.3.2",
        "syncedAt": "2026-11-07T10:09:13Z",
    });
    fs::write(
        project.join(".upstream-pin.json"),
        serde_json::to_string_pretty(&pin).expect("serialize pin"),
    )
    .expect("write pin");
}

fn json_stdout(stdout: &str) -> serde_json::Value {
    serde_json::from_str(stdout.trim()).unwrap_or_else(|err| {
        panic!("stdout is not valid JSON ({err}): {stdout}");
    })
}

// ── Tests ───────────────────────────────────────────────────────────────

#[test]
fn reports_drift_and_renders_the_tracking_issue() {
    let project = TempDir::new("drift-detect");
    write_pin(project.path());
    let script = actual_project_root().join("scripts/upstream-drift.rs");

    let (stdout, stderr, code) = run_drift(&script, project.path(), &["--tag", "v9.9.9", "--json"]);

    assert_eq!(code, 0, "stderr: {stderr}\nstdout: {stdout}");
    let report = json_stdout(&stdout);
    assert_eq!(report["drifted"], serde_json::Value::Bool(true));
    assert_eq!(report["latest"]["tag"], "v9.9.9");
    assert_eq!(report["pinned"]["ref"], "v1.3.2");
    let title = report["title"].as_str().expect("title");
    assert!(title.starts_with("upstream drift:"), "{title}");
    assert!(title.contains("v9.9.9"), "{title}");
    let body = report["body"].as_str().expect("body");
    assert!(body.contains("v9.9.9"), "{body}");
    assert!(
        body.contains("rust-script scripts/sync-upstream.rs v9.9.9"),
        "{body}"
    );
}

#[test]
fn reports_no_drift_when_the_pin_is_the_newest_release() {
    let project = TempDir::new("drift-current");
    write_pin(project.path());
    let script = actual_project_root().join("scripts/upstream-drift.rs");

    let (stdout, stderr, code) = run_drift(&script, project.path(), &["--tag", "v1.3.2", "--json"]);

    assert_eq!(code, 0, "stderr: {stderr}\nstdout: {stdout}");
    let report = json_stdout(&stdout);
    assert_eq!(report["drifted"], serde_json::Value::Bool(false));
    assert_eq!(report["latest"]["tag"], "v1.3.2");
}

#[test]
fn human_output_reports_no_drift_quietly() {
    let project = TempDir::new("drift-human");
    write_pin(project.path());
    let script = actual_project_root().join("scripts/upstream-drift.rs");

    let (stdout, stderr, code) = run_drift(&script, project.path(), &["--tag", "v1.3.2"]);

    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(stdout.contains("No drift:"), "{stdout}");
    assert!(!stdout.contains("DRIFT:"), "{stdout}");
}

#[test]
fn fails_when_the_pin_is_missing() {
    let project = TempDir::new("drift-no-pin");
    write_lock(project.path());
    let script = actual_project_root().join("scripts/upstream-drift.rs");

    let (_stdout, stderr, code) = run_drift(&script, project.path(), &["--tag", "v9.9.9"]);

    assert_eq!(code, 1, "stderr: {stderr}");
    assert!(stderr.contains(".upstream-pin.json"), "{stderr}");
    assert!(stderr.contains("scripts/sync-upstream.rs"), "{stderr}");
}

#[test]
fn fails_for_a_tag_that_is_not_a_release_version() {
    let project = TempDir::new("drift-bad-tag");
    write_pin(project.path());
    let script = actual_project_root().join("scripts/upstream-drift.rs");

    let (_stdout, stderr, code) = run_drift(&script, project.path(), &["--tag", "nightly"]);

    assert_eq!(code, 1, "stderr: {stderr}");
    assert!(stderr.contains("no upstream release tag"), "{stderr}");
}

#[test]
fn rejects_unknown_arguments() {
    let project = TempDir::new("drift-usage");
    write_pin(project.path());
    let script = actual_project_root().join("scripts/upstream-drift.rs");

    let (_stdout, stderr, code) = run_drift(&script, project.path(), &["--bogus"]);

    assert_eq!(code, 1, "stderr: {stderr}");
    assert!(stderr.contains("Unknown argument"), "{stderr}");
}
