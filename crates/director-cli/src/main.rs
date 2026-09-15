//! `director` — the precompiled offline CLI that owns autopilot-director
//! workflow state.
//!
//! The code owns every state transition; the skill's prose dispatches and
//! adjudicates. This skeleton owns the run-state schema and the `init` /
//! `inspect` seam.

use serde_json::{json, Value};
use std::env;
use std::process;

mod args;
mod state;
mod storage;
mod util;

pub(crate) const CURRENT_SCHEMA_VERSION: u64 = 1;

fn main() {
    if let Err(err) = run() {
        eprintln!("ERROR: {err}");
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("init") => {
            let response = init_run(args::parse_init_args(args.collect())?)?;
            print_json(&response)
        }
        Some("inspect") => {
            let response = inspect_run(args::parse_inspect_args(args.collect())?)?;
            print_json(&response)
        }
        Some("--help" | "-h") | None => {
            println!("{}", args::USAGE);
            Ok(())
        }
        Some(other) => Err(format!("unknown command: {other}\n{}", args::USAGE)),
    }
}

fn print_json(value: &Value) -> Result<(), String> {
    println!(
        "{}",
        serde_json::to_string_pretty(value).map_err(|err| format!("json error: {err}"))?
    );
    Ok(())
}

/// Create the worktree-local run state at revision 0, refusing unless
/// `.director/` is git-ignored (ADR 0025 pattern) and no run already exists.
/// The title-derived `--slug` fixes the spec branch (ADR 0047) from the first
/// revision, so no later transition has to rewrite it.
fn init_run(args: args::InitArgs) -> Result<Value, String> {
    storage::ensure_worktree(&args.worktree)?;
    storage::ensure_director_ignored(&args.worktree)?;
    state::ensure_no_existing_run(&args.worktree)?;

    let run_state = state::RunState::new(
        util::run_id_for_spec(args.spec_issue),
        args.spec_issue,
        util::branch_for_spec(args.spec_issue, &args.slug),
    );
    state::write_state(&args.worktree, &run_state)?;

    Ok(json!({
        "command": "init",
        "run_id": run_state.run_id,
        "spec_issue": run_state.spec_issue,
        "branch": run_state.branch,
        "schema_version": run_state.schema_version,
        "revision": run_state.revision,
        "status": run_state.status.as_str(),
        "state_path": state::state_path(&args.worktree).display().to_string(),
    }))
}

/// Read the run state back through the typed schema gate.
fn inspect_run(args: args::InspectArgs) -> Result<Value, String> {
    storage::ensure_worktree(&args.worktree)?;
    let run_state = state::read_state(&args.worktree)?;
    let tickets = run_state
        .tickets
        .iter()
        .map(|ticket| {
            json!({
                "ticket": ticket.ticket,
                "title": ticket.title,
                "status": ticket.status.as_str(),
                "rounds": ticket.rounds.len(),
                "round_cap": ticket.round_cap,
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "command": "inspect",
        "run_id": run_state.run_id,
        "spec_issue": run_state.spec_issue,
        "branch": run_state.branch,
        "schema_version": run_state.schema_version,
        "revision": run_state.revision,
        "status": run_state.status.as_str(),
        "tickets": tickets,
        "spec_gate": {
            "rounds": run_state.spec_gate.rounds.len(),
            "round_cap": run_state.spec_gate.round_cap,
        },
    }))
}
