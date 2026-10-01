//! The thin command layer: load → call one record method → commit.
//!
//! Every state-machine mutation lives in [`crate::records`] as a pure method on
//! the record it belongs to, and returns an [`Outcome`]. This module owns the
//! only persistence seam ([`commit`]) and the dispatch that turns an outcome
//! into either a printed envelope or a refusal — including the four
//! `RefusedWithMutation` paths, where the incident must reach disk *before* the
//! error surfaces.
//!
//! It stays this thin on purpose: `write(worktree, state)` is reached from
//! exactly one function (`commit`), so the question "did this command persist
//! anything?" has one answer to read.

use std::path::Path;

use serde_json::Value;

use crate::gate::GateLayer;
pub(crate) use crate::records::NewFinding;
use crate::state::{FindingDisposition, RunState, RunStatus, TicketStatus};

use super::Outcome;

/// What a command did, once its one mutation had its chance to land.
pub(crate) enum Response {
    /// Print this envelope and exit zero.
    Envelope(Value),
    /// Print this refusal on stderr and exit non-zero.
    Refused(String),
}

/// Commit the outcome of one mutation.
///
/// The record methods bump the revision themselves, so the bump *is* the
/// signal: a write happens exactly when the revision moved. That makes
/// [`Outcome::Refused`] free, keeps a clean `resume` byte-identical (its record
/// method never bumps), and forces the four [`Outcome::RefusedWithMutation`]
/// paths to land on disk before their refusal surfaces.
fn commit(
    worktree: &Path,
    state: &mut RunState,
    entry_revision: u64,
    outcome: Outcome,
) -> Response {
    if !changed(state, entry_revision) {
        return match outcome {
            Outcome::Applied(envelope) => Response::Envelope(envelope),
            Outcome::Refused(refusal) | Outcome::RefusedWithMutation(refusal) => {
                Response::Refused(refusal)
            }
        };
    }
    match outcome {
        Outcome::Applied(envelope) => match write(worktree, state) {
            Ok(()) => Response::Envelope(restamp(envelope, state.revision)),
            Err(err) => Response::Refused(err),
        },
        Outcome::Refused(refusal) => {
            // Only a bumped revision reaches this arm, so a plain `Refused`
            // here would be a mutation the seam silently drops. A transition
            // that refuses must not bump; one that mutates and then refuses
            // must say so, as `RefusedWithMutation`.
            debug_assert!(
                !changed(state, entry_revision),
                "a refused transition must not bump the revision"
            );
            Response::Refused(refusal)
        }
        Outcome::RefusedWithMutation(refusal) => match write(worktree, state) {
            Ok(()) => Response::Refused(refusal),
            Err(err) => Response::Refused(format!("{refusal}\n{err}")),
        },
    }
}

/// Persist the state the record method already mutated. The envelope's revision
/// is re-stamped from `state.revision` afterwards, so a record method never has
/// to know whether — or when — it gets persisted.
fn write(worktree: &Path, state: &mut RunState) -> Result<(), String> {
    crate::state::write_state(worktree, state)
}

/// Did the record method this response came from advance the run?
fn changed(state: &RunState, entry_revision: u64) -> bool {
    state.revision != entry_revision
}

/// Re-stamp the envelope with the revision the write just persisted, keeping
/// the key order a `serde_json::Map` implies.
fn restamp(mut envelope: Value, revision: u64) -> Value {
    if let Value::Object(object) = &mut envelope {
        object.insert("revision".to_string(), Value::from(revision));
    }
    envelope
}

// ── record commands ──

pub(crate) fn add_ticket(
    worktree: &Path,
    state: &mut RunState,
    ticket: u64,
    title: &str,
    blocked_by: &[u64],
) -> Result<Value, String> {
    let entry_revision = state.revision;
    let outcome = state.record_add_ticket(ticket, title, blocked_by)?;
    unpack(commit(worktree, state, entry_revision, outcome))
}

pub(crate) fn transition_run(
    worktree: &Path,
    state: &mut RunState,
    to: RunStatus,
) -> Result<Value, String> {
    let entry_revision = state.revision;
    let outcome = state.record_transition_run(to)?;
    unpack(commit(worktree, state, entry_revision, outcome))
}

pub(crate) fn transition_ticket(
    worktree: &Path,
    state: &mut RunState,
    ticket: u64,
    to: TicketStatus,
) -> Result<Value, String> {
    let entry_revision = state.revision;
    let outcome = state.record_transition_ticket(ticket, to)?;
    unpack(commit(worktree, state, entry_revision, outcome))
}

pub(crate) fn open_round(
    worktree: &Path,
    state: &mut RunState,
    layer: GateLayer,
) -> Result<Value, String> {
    let entry_revision = state.revision;
    let outcome = state.record_open_round(layer)?;
    unpack(commit(worktree, state, entry_revision, outcome))
}

pub(crate) fn close_round(
    worktree: &Path,
    state: &mut RunState,
    layer: GateLayer,
    number: u64,
) -> Result<Value, String> {
    let entry_revision = state.revision;
    let outcome = state.record_close_round(layer, number)?;
    unpack(commit(worktree, state, entry_revision, outcome))
}

pub(crate) fn record_finding(
    worktree: &Path,
    state: &mut RunState,
    layer: GateLayer,
    number: u64,
    finding: NewFinding,
) -> Result<Value, String> {
    let entry_revision = state.revision;
    let outcome = state.record_finding(layer, number, finding)?;
    unpack(commit(worktree, state, entry_revision, outcome))
}

pub(crate) fn dispose_finding(
    worktree: &Path,
    state: &mut RunState,
    layer: GateLayer,
    number: u64,
    id: &str,
    disposition: FindingDisposition,
) -> Result<Value, String> {
    let entry_revision = state.revision;
    let outcome = state.record_dispose_finding(layer, number, id, disposition)?;
    unpack(commit(worktree, state, entry_revision, outcome))
}

pub(crate) fn begin_dispatch(
    worktree: &Path,
    state: &mut RunState,
    ticket: u64,
    worker: &str,
) -> Result<Value, String> {
    let entry_revision = state.revision;
    let outcome = state.record_begin_dispatch(ticket, worker)?;
    unpack(commit(worktree, state, entry_revision, outcome))
}

pub(crate) fn record_worker_report(
    worktree: &Path,
    state: &mut RunState,
    ticket: u64,
    raw: &str,
) -> Result<Value, String> {
    let entry_revision = state.revision;
    let outcome = state.record_worker_report(ticket, raw)?;
    unpack(commit(worktree, state, entry_revision, outcome))
}

pub(crate) fn finish_dispatch_ok(
    worktree: &Path,
    state: &mut RunState,
    ticket: u64,
) -> Result<Value, String> {
    let entry_revision = state.revision;
    let outcome = state.record_finish_dispatch_ok(ticket)?;
    unpack(commit(worktree, state, entry_revision, outcome))
}

pub(crate) fn finish_dispatch_failed(
    worktree: &Path,
    state: &mut RunState,
    ticket: u64,
    reason: &str,
) -> Result<Value, String> {
    let entry_revision = state.revision;
    let outcome = state.record_finish_dispatch_failed(ticket, reason)?;
    unpack(commit(worktree, state, entry_revision, outcome))
}

/// Revalidate the recorded worktree fingerprint before a resumed session trusts
/// the run state. The only read-shaped command here: a clean resume returns
/// `Applied` without a revision bump, so [`commit`] writes nothing.
pub(crate) fn resume_run(
    worktree: &Path,
    state: &mut RunState,
    accept_drift: bool,
) -> Result<Value, String> {
    let entry_revision = state.revision;
    let live = crate::storage::capture_fingerprint(worktree)?;
    let outcome = state.record_resume(worktree, live, accept_drift)?;
    unpack(commit(worktree, state, entry_revision, outcome))
}

/// Turn a committed response into the `Result` the CLI prints: an envelope is a
/// success, a refusal is an error.
fn unpack(response: Response) -> Result<Value, String> {
    match response {
        Response::Envelope(envelope) => Ok(envelope),
        Response::Refused(refusal) => Err(refusal),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_git_worktree(worktree: &Path) {
        for args in [
            vec!["init", "-q", "-b", "main"],
            vec![
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
        ] {
            let output = std::process::Command::new("git")
                .args(&args)
                .current_dir(worktree)
                .output()
                .expect("git should run");
            assert!(
                output.status.success(),
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }

    fn fresh() -> RunState {
        RunState::new(
            "spec-128".to_string(),
            128,
            "codex/spec-128-autopilot-director".to_string(),
        )
    }

    #[test]
    fn the_commit_seam_persists_a_mutated_outcome_once() {
        let dir = tempfile::tempdir().unwrap();
        init_git_worktree(dir.path());
        let mut state = fresh();

        let entry_revision = state.revision;
        let outcome = state
            .record_add_ticket(200, "one", &[])
            .expect("a legal add has no hard error");
        assert!(changed(&state, entry_revision), "the record method bumps");
        let response = commit(dir.path(), &mut state, entry_revision, outcome);

        assert!(matches!(response, Response::Envelope(_)));
        assert_eq!(state.revision, entry_revision + 1);
        let reloaded = crate::state::read_state(dir.path()).unwrap();
        assert_eq!(reloaded.revision, state.revision);
        assert_eq!(reloaded.tickets.len(), 1);
    }

    #[test]
    fn a_pure_refusal_never_touches_disk() {
        let dir = tempfile::tempdir().unwrap();
        init_git_worktree(dir.path());
        let mut state = fresh();
        state
            .record_add_ticket(200, "one", &[])
            .expect("a legal add has no hard error");

        let entry_revision = state.revision;
        // Ticket #200 is `pending`: opening a review round is refused.
        let outcome = state
            .record_open_round(GateLayer::Ticket(200))
            .expect("a refusal is an outcome, not a hard error");
        assert!(matches!(outcome, Outcome::Refused(_)));

        let response = commit(dir.path(), &mut state, entry_revision, outcome);
        assert!(matches!(response, Response::Refused(_)));
        assert_eq!(state.revision, entry_revision);
        assert!(
            !crate::state::state_path(dir.path()).exists(),
            "a pure refusal must not create a state file"
        );
    }

    #[test]
    fn a_refusal_with_mutation_persists_before_it_refuses() {
        let dir = tempfile::tempdir().unwrap();
        init_git_worktree(dir.path());
        let mut state = fresh();
        state
            .record_add_ticket(200, "one", &[])
            .expect("a legal add has no hard error");
        // Drive the ticket to a gating status with the round cap already spent.
        let ticket_state = state.ticket_mut(200).unwrap();
        ticket_state.status = TicketStatus::Gating;
        ticket_state.round_cap = 0;

        // Setup ends here: everything the commit seam can see is captured now.
        let entry_revision = state.revision;
        let outcome = state
            .record_open_round(GateLayer::Ticket(200))
            .expect("a refusal is an outcome, not a hard error");
        assert!(
            matches!(outcome, Outcome::RefusedWithMutation(_)),
            "an exhausted cap escalates and records an incident"
        );
        assert_eq!(state.ticket(200).unwrap().status, TicketStatus::Escalated);

        assert!(changed(&state, entry_revision), "the record method bumps");
        let response = commit(dir.path(), &mut state, entry_revision, outcome);
        assert!(matches!(response, Response::Refused(_)));
        assert_eq!(state.revision, entry_revision + 1);
        let reloaded = crate::state::read_state(dir.path()).unwrap();
        assert_eq!(
            reloaded.revision,
            entry_revision + 1,
            "the incident must be on disk before the refusal surfaces"
        );
        assert_eq!(
            reloaded.ticket(200).unwrap().status,
            TicketStatus::Escalated,
            "and the escalation is what is on disk"
        );
    }

    #[test]
    fn a_clean_resume_reports_a_verdict_without_writing() {
        let dir = tempfile::tempdir().unwrap();
        init_git_worktree(dir.path());

        // Seed a written state through the commit seam itself, so the fixture is
        // what a real run leaves behind.
        let mut state = fresh();
        state.worktree = Some(crate::storage::capture_fingerprint(dir.path()).unwrap());
        let entry_revision = state.revision;
        let outcome = state
            .record_transition_run(RunStatus::Running)
            .expect("a legal run transition has no hard error");
        let seeded = commit(dir.path(), &mut state, entry_revision, outcome);
        assert!(matches!(seeded, Response::Envelope(_)));
        let before = std::fs::read(crate::state::state_path(dir.path())).unwrap();
        assert!(
            before.windows(6).any(|window| window == b"\"head\""),
            "the fixture must carry a recorded fingerprint"
        );

        let entry_revision = state.revision;
        let live = crate::storage::capture_fingerprint(dir.path()).unwrap();
        let outcome = state
            .record_resume(dir.path(), live, false)
            .expect("a clean resume is not a hard error");
        assert!(matches!(outcome, Outcome::Applied(_)));
        assert!(
            !changed(&state, entry_revision),
            "a clean resume does not bump"
        );

        let response = commit(dir.path(), &mut state, entry_revision, outcome);
        assert!(matches!(response, Response::Envelope(_)));
        assert_eq!(state.revision, entry_revision);
        assert_eq!(
            std::fs::read(crate::state::state_path(dir.path())).unwrap(),
            before,
            "a clean resume must leave the state file byte-identical"
        );
    }
}
