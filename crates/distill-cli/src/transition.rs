//! The single owner of the transition commit protocol.
//!
//! Every state mutation funnels through `commit`: preflight quota, append
//! events, write planned files, write state — in that order. Consolidating
//! the protocol here eliminates the divergence between the three inline
//! copies that previously lived in `main.rs` (the boundary copy charged
//! out-of-run projection bytes against run/project quota; the others did
//! not).

use std::fs;
use std::path::Path;

use serde_json::Value;

use crate::event;
use crate::state;
use crate::storage::{self, PlannedFile, StorageLimits};

/// Commit a planned transition: preflight quota, append events, write planned
/// files, write state — in that order.
///
/// Only `PlannedWrite::RunArtifact` bytes count against run/project quota;
/// `WorktreeProjection` bytes land outside `.distill` and count against
/// neither budget. The write order (and its partial-failure window) is the
/// pre-existing protocol behavior, preserved verbatim.
pub(crate) fn commit(
    worktree: &Path,
    run_id: &str,
    next_state: &Value,
    event_lines: &[String],
    planned_files: Vec<PlannedFile>,
    limits: &StorageLimits,
) -> Result<(), String> {
    let state_bytes =
        serde_json::to_vec_pretty(next_state).map_err(|err| format!("json error: {err}"))?;
    let current_state_bytes = fs::metadata(state::state_path(worktree, run_id)?)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    let additional_distill_bytes = event::event_log_bytes(event_lines)
        .saturating_add(
            planned_files
                .iter()
                .filter(|file| file.write.counts_against_quota())
                .map(|file| file.bytes.len() as u64)
                .fold(0_u64, u64::saturating_add),
        )
        .saturating_add((state_bytes.len() as u64).saturating_sub(current_state_bytes));
    storage::preflight_additional_run_bytes(worktree, run_id, additional_distill_bytes, limits)?;
    event::append_event_lines(worktree, run_id, event_lines, limits)?;
    for file in planned_files {
        state::write_bytes(&file.path, &file.bytes, file.write.atomic())?;
    }
    state::write_state(worktree, next_state)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::PlannedWrite;
    use serde_json::json;

    fn test_limits() -> StorageLimits {
        StorageLimits {
            run_bytes: 64 * 1024,
            ..StorageLimits::default()
        }
    }

    fn test_event_line(run_id: &str, limits: &StorageLimits) -> String {
        event::run_event(
            run_id,
            1,
            2,
            "stage-waiting",
            json!({"stage": "s1"}),
            limits,
        )
        .expect("event line")
    }

    #[test]
    fn worktree_projection_bytes_do_not_count_against_quota() {
        let temp = tempfile::tempdir().expect("tempdir");
        let worktree = temp.path();
        let limits = test_limits();
        let event_lines = vec![test_event_line("run-1", &limits)];
        // 1 MiB landing outside `.distill` — would blow the 64 KiB run
        // budget if counted, must be excluded from the preflight.
        let planned_files = vec![PlannedFile {
            path: worktree.join("docs/issues/projection.md"),
            bytes: vec![b'x'; 1024 * 1024],
            write: PlannedWrite::WorktreeProjection,
        }];
        let next_state = json!({"run_id": "run-1", "revision": 2});

        commit(
            worktree,
            "run-1",
            &next_state,
            &event_lines,
            planned_files,
            &limits,
        )
        .expect("commit succeeds: projection bytes are out of quota scope");

        assert!(worktree.join("docs/issues/projection.md").exists());
        assert!(worktree.join(".distill/runs/run-1/state.json").exists());
        assert!(worktree.join(".distill/runs/run-1/events.jsonl").exists());
    }

    #[test]
    fn run_artifact_bytes_count_against_quota_and_over_budget_commits_write_nothing() {
        let temp = tempfile::tempdir().expect("tempdir");
        let worktree = temp.path();
        let limits = test_limits();
        let event_lines = vec![test_event_line("run-1", &limits)];
        // 1 MiB landing inside `.distill` — blows the 64 KiB run budget.
        let planned_files = vec![PlannedFile {
            path: worktree.join(".distill/runs/run-1/artifacts/big.bin"),
            bytes: vec![b'x'; 1024 * 1024],
            write: PlannedWrite::RunArtifact,
        }];
        let next_state = json!({"run_id": "run-1", "revision": 2});

        let err = commit(
            worktree,
            "run-1",
            &next_state,
            &event_lines,
            planned_files,
            &limits,
        )
        .expect_err("commit must reject an over-budget run");

        assert!(
            err.contains("run quota exceeded"),
            "unexpected error: {err}"
        );
        assert!(!worktree.join(".distill/runs/run-1/events.jsonl").exists());
        assert!(!worktree
            .join(".distill/runs/run-1/artifacts/big.bin")
            .exists());
        assert!(!worktree.join(".distill/runs/run-1/state.json").exists());
    }

    #[test]
    fn commit_writes_run_artifact_with_exact_bytes() {
        let temp = tempfile::tempdir().expect("tempdir");
        let worktree = temp.path();
        let limits = test_limits();
        let event_lines = vec![test_event_line("run-1", &limits)];
        let planned_files = vec![PlannedFile {
            path: worktree.join(".distill/runs/run-1/artifacts/evidence.json"),
            bytes: br#"{"ok":true}"#.to_vec(),
            write: PlannedWrite::RunArtifact,
        }];
        let next_state = json!({"run_id": "run-1", "revision": 2});

        commit(
            worktree,
            "run-1",
            &next_state,
            &event_lines,
            planned_files,
            &limits,
        )
        .expect("commit succeeds");

        let written = std::fs::read(worktree.join(".distill/runs/run-1/artifacts/evidence.json"))
            .expect("artifact written");
        assert_eq!(written, br#"{"ok":true}"#);
    }

    #[test]
    fn events_are_appended_before_state_is_written() {
        let temp = tempfile::tempdir().expect("tempdir");
        let worktree = temp.path();
        let limits = test_limits();
        let event_lines = vec![test_event_line("run-1", &limits)];
        let next_state = json!({"run_id": "run-1", "revision": 2});

        // Force the state write to fail; the events appended earlier in the
        // protocol must already be on disk (the pre-existing partial-failure
        // window, preserved verbatim).
        std::env::set_var("DISTILL_FAIL_WRITE_STATE_FOR_RUN", "run-1");
        let result = commit(
            worktree,
            "run-1",
            &next_state,
            &event_lines,
            vec![],
            &limits,
        );
        std::env::remove_var("DISTILL_FAIL_WRITE_STATE_FOR_RUN");

        assert!(result.is_err());
        let events = std::fs::read_to_string(worktree.join(".distill/runs/run-1/events.jsonl"))
            .expect("events were appended before the state write failed");
        assert!(events.contains("stage-waiting"));
        assert!(!worktree.join(".distill/runs/run-1/state.json").exists());
    }
}
