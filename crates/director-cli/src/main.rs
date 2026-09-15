//! `director` — the precompiled offline CLI that owns autopilot-director
//! workflow state.
//!
//! The code owns every state transition; the skill's prose dispatches and
//! adjudicates. Each command prints one JSON object on stdout, writes errors
//! to stderr, and exits non-zero on any refusal.
//!
//! Storage: `.director/state.json` inside the worktree is the single
//! authoritative record of one Spec run. There is deliberately no separate
//! event log — the revision-stamped state plus the per-round finding ledger
//! and the dispatch ledger is the audit record.

use std::env;
use std::io::Read;
use std::path::Path;
use std::process;

use serde_json::{json, Value};

mod args;
mod gate;
mod report;
mod state;
mod storage;
mod transition;
mod util;

pub(crate) const CURRENT_SCHEMA_VERSION: u64 = 1;

fn main() {
    if let Err(err) = run() {
        eprintln!("ERROR: {err}");
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut argv = env::args().skip(1);
    let command = argv.next();
    let rest: Vec<String> = argv.collect();
    match command.as_deref() {
        Some("init") => print_json(&init_run(rest)?),
        Some("inspect") => print_json(&inspect_run(rest)?),
        Some("ticket") => ticket_command(&rest),
        Some("run") => run_command(&rest),
        Some("round") => round_command(&rest),
        Some("finding") => finding_command(&rest),
        Some("dispatch") => dispatch_command(&rest),
        Some("report") => report_command(&rest),
        Some("gate") => gate_command(rest),
        Some("--help" | "-h") | None => {
            println!("{}", args::USAGE);
            Ok(())
        }
        Some(other) => Err(format!("unknown command: {other}\n{}", args::USAGE)),
    }
}

/// Split `<verb> <subcommand> --flags...` — every command with subcommands
/// starts with its bare subcommand word.
fn split_subcommand(rest: &[String]) -> Result<(&str, Vec<String>), String> {
    match rest.split_first() {
        Some((head, tail)) if !head.starts_with("--") => Ok((head.as_str(), tail.to_vec())),
        _ => Err(format!("expected a subcommand\n{}", args::USAGE)),
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
fn init_run(raw: Vec<String>) -> Result<Value, String> {
    let mut flags = args::Flags::parse(raw)?;
    let worktree = args::worktree(&mut flags)?;
    let spec_issue = args::parse_u64(&flags.required("--spec-issue")?, "--spec-issue")?;
    let slug = flags.required("--slug")?;
    flags.reject_unknown()?;
    util::validate_slug(&slug)?;

    storage::ensure_worktree(&worktree)?;
    storage::ensure_director_ignored(&worktree)?;
    state::ensure_no_existing_run(&worktree)?;

    let run_state = state::RunState::new(
        util::run_id_for_spec(spec_issue),
        spec_issue,
        util::branch_for_spec(spec_issue, &slug),
    );
    state::write_state(&worktree, &run_state)?;

    Ok(json!({
        "command": "init",
        "run_id": run_state.run_id,
        "spec_issue": run_state.spec_issue,
        "branch": run_state.branch,
        "schema_version": run_state.schema_version,
        "revision": run_state.revision,
        "status": run_state.status.as_str(),
        "state_path": state::state_path(&worktree).display().to_string(),
    }))
}

/// Read the run state back through the typed schema gate.
fn inspect_run(raw: Vec<String>) -> Result<Value, String> {
    let mut flags = args::Flags::parse(raw)?;
    let worktree = args::worktree(&mut flags)?;
    flags.reject_unknown()?;

    let run_state = load(&worktree)?;
    let tickets = run_state
        .tickets
        .iter()
        .map(|ticket| {
            json!({
                "ticket": ticket.ticket,
                "title": ticket.title,
                "status": ticket.status.as_str(),
                "blocked_by": ticket.blocked_by,
                "rounds": ticket.rounds.len(),
                "round_cap": ticket.round_cap,
                "open_round": ticket.open_round().map(|round| round.round),
                "gate": gate::ticket_verdict(ticket).to_json(),
                "dispatches": ticket
                    .dispatches
                    .iter()
                    .map(|record| json!({
                        "worker": record.worker,
                        "attempt": record.attempt,
                        "status": record.status.as_str(),
                        "reason": record.reason,
                        "report": record.report.as_ref().map(|report| report.summary()),
                    }))
                    .collect::<Vec<_>>(),
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
        "spec_gate": gate::spec_verdict(&run_state).to_json(),
    }))
}

fn ticket_command(rest: &[String]) -> Result<(), String> {
    let (subcommand, tail) = split_subcommand(rest)?;
    match subcommand {
        "add" => {
            let mut flags = args::Flags::parse(tail)?;
            let worktree = args::worktree(&mut flags)?;
            let ticket = args::parse_u64(&flags.required("--ticket")?, "--ticket")?;
            let title = flags.required("--title")?;
            let blocked_by = flags
                .repeated("--blocked-by")?
                .iter()
                .map(|raw| args::parse_u64(raw, "--blocked-by"))
                .collect::<Result<Vec<u64>, String>>()?;
            flags.reject_unknown()?;

            let mut run_state = load(&worktree)?;
            let response =
                transition::add_ticket(&worktree, &mut run_state, ticket, &title, &blocked_by)?;
            print_json(&response)
        }
        "transition" => {
            let mut flags = args::Flags::parse(tail)?;
            let worktree = args::worktree(&mut flags)?;
            let ticket = args::parse_u64(&flags.required("--ticket")?, "--ticket")?;
            let to = state::TicketStatus::parse(&flags.required("--to")?)?;
            flags.reject_unknown()?;

            let mut run_state = load(&worktree)?;
            let response = transition::transition_ticket(&worktree, &mut run_state, ticket, to)?;
            print_json(&response)
        }
        other => Err(format!(
            "unknown ticket subcommand: {other}\n{}",
            args::USAGE
        )),
    }
}

fn run_command(rest: &[String]) -> Result<(), String> {
    let (subcommand, tail) = split_subcommand(rest)?;
    match subcommand {
        "transition" => {
            let mut flags = args::Flags::parse(tail)?;
            let worktree = args::worktree(&mut flags)?;
            let to = state::RunStatus::parse(&flags.required("--to")?)?;
            flags.reject_unknown()?;

            let mut run_state = load(&worktree)?;
            let response = transition::transition_run(&worktree, &mut run_state, to)?;
            print_json(&response)
        }
        other => Err(format!("unknown run subcommand: {other}\n{}", args::USAGE)),
    }
}

fn round_command(rest: &[String]) -> Result<(), String> {
    let (subcommand, tail) = split_subcommand(rest)?;
    match subcommand {
        "open" => {
            let mut flags = args::Flags::parse(tail)?;
            let worktree = args::worktree(&mut flags)?;
            let layer = layer(&mut flags)?;
            flags.reject_unknown()?;

            let mut run_state = load(&worktree)?;
            let response = transition::open_round(&worktree, &mut run_state, layer)?;
            print_json(&response)
        }
        "close" => {
            let mut flags = args::Flags::parse(tail)?;
            let worktree = args::worktree(&mut flags)?;
            let layer = layer(&mut flags)?;
            let round = args::parse_u64(&flags.required("--round")?, "--round")?;
            flags.reject_unknown()?;

            let mut run_state = load(&worktree)?;
            let response = transition::close_round(&worktree, &mut run_state, layer, round)?;
            print_json(&response)
        }
        other => Err(format!(
            "unknown round subcommand: {other}\n{}",
            args::USAGE
        )),
    }
}

fn finding_command(rest: &[String]) -> Result<(), String> {
    let (subcommand, tail) = split_subcommand(rest)?;
    match subcommand {
        "record" => {
            let mut flags = args::Flags::parse(tail)?;
            let worktree = args::worktree(&mut flags)?;
            let layer = layer(&mut flags)?;
            let round = args::parse_u64(&flags.required("--round")?, "--round")?;
            let axis = state::ReviewAxis::parse(&flags.required("--axis")?)?;
            let id = flags.required("--id")?;
            let hash = flags.required("--hash")?;
            let summary = flags.required("--summary")?;
            let disposition = disposition(&mut flags)?;
            flags.reject_unknown()?;

            let mut run_state = load(&worktree)?;
            let response = transition::record_finding(
                &worktree,
                &mut run_state,
                layer,
                round,
                transition::NewFinding {
                    id,
                    axis,
                    hash,
                    summary,
                    disposition,
                },
            )?;
            print_json(&response)
        }
        "dispose" => {
            let mut flags = args::Flags::parse(tail)?;
            let worktree = args::worktree(&mut flags)?;
            let layer = layer(&mut flags)?;
            let round = args::parse_u64(&flags.required("--round")?, "--round")?;
            let id = flags.required("--id")?;
            let disposition = disposition(&mut flags)?
                .ok_or_else(|| "--fixed or --rejected is required".to_string())?;
            flags.reject_unknown()?;

            let mut run_state = load(&worktree)?;
            let response = transition::dispose_finding(
                &worktree,
                &mut run_state,
                layer,
                round,
                &id,
                disposition,
            )?;
            print_json(&response)
        }
        other => Err(format!(
            "unknown finding subcommand: {other}\n{}",
            args::USAGE
        )),
    }
}

fn dispatch_command(rest: &[String]) -> Result<(), String> {
    let (subcommand, tail) = split_subcommand(rest)?;
    match subcommand {
        "begin" => {
            let mut flags = args::Flags::parse(tail)?;
            let worktree = args::worktree(&mut flags)?;
            let ticket = args::parse_u64(&flags.required("--ticket")?, "--ticket")?;
            let worker = flags.required("--worker")?;
            flags.reject_unknown()?;

            let mut run_state = load(&worktree)?;
            let response = transition::begin_dispatch(&worktree, &mut run_state, ticket, &worker)?;
            print_json(&response)
        }
        "finish" => {
            let mut flags = args::Flags::parse(tail)?;
            let worktree = args::worktree(&mut flags)?;
            let ticket = args::parse_u64(&flags.required("--ticket")?, "--ticket")?;
            let outcome = flags.required("--outcome")?;
            let reason = flags.optional("--reason")?;
            flags.reject_unknown()?;

            let mut run_state = load(&worktree)?;
            let response = match outcome.as_str() {
                "ok" => {
                    if reason.is_some() {
                        return Err("--reason only applies to `--outcome failed`".to_string());
                    }
                    transition::finish_dispatch_ok(&worktree, &mut run_state, ticket)?
                }
                "failed" => {
                    let reason =
                        reason.ok_or_else(|| "--outcome failed requires --reason".to_string())?;
                    transition::finish_dispatch_failed(&worktree, &mut run_state, ticket, &reason)?
                }
                other => {
                    return Err(format!(
                        "unknown --outcome {other:?}; expected `ok` or `failed`"
                    ));
                }
            };
            print_json(&response)
        }
        other => Err(format!(
            "unknown dispatch subcommand: {other}\n{}",
            args::USAGE
        )),
    }
}

/// Validate the Worker's `WORKER_REPORT` envelope and attach it to the open
/// dispatch. A malformed envelope is recorded as a dispatch failure, so this
/// command's error path is itself a state transition.
fn report_command(rest: &[String]) -> Result<(), String> {
    let (subcommand, tail) = split_subcommand(rest)?;
    match subcommand {
        "validate" => {
            let mut flags = args::Flags::parse(tail)?;
            let worktree = args::worktree(&mut flags)?;
            let ticket = args::parse_u64(&flags.required("--ticket")?, "--ticket")?;
            let file = flags.optional("--file")?;
            flags.reject_unknown()?;

            let raw = match file {
                Some(path) => std::fs::read_to_string(&path)
                    .map_err(|err| format!("cannot read report {path:?}: {err}"))?,
                None => read_stdin()?,
            };
            let mut run_state = load(&worktree)?;
            let response =
                transition::record_worker_report(&worktree, &mut run_state, ticket, &raw)?;
            print_json(&response)
        }
        other => Err(format!(
            "unknown report subcommand: {other}\n{}",
            args::USAGE
        )),
    }
}

fn read_stdin() -> Result<String, String> {
    let mut buffer = String::new();
    std::io::stdin()
        .read_to_string(&mut buffer)
        .map_err(|err| format!("cannot read the WORKER_REPORT from stdin: {err}"))?;
    Ok(buffer)
}

fn gate_command(raw: Vec<String>) -> Result<(), String> {
    let mut flags = args::Flags::parse(raw)?;
    let worktree = args::worktree(&mut flags)?;
    let ticket = flags
        .optional("--ticket")?
        .map(|raw| args::parse_u64(&raw, "--ticket"))
        .transpose()?;
    flags.reject_unknown()?;

    let run_state = load(&worktree)?;
    let verdict = match ticket {
        Some(ticket) => {
            let ticket_state = run_state
                .ticket(ticket)
                .ok_or_else(|| format!("ticket #{ticket} is not registered"))?;
            gate::ticket_verdict(ticket_state)
        }
        None => gate::spec_verdict(&run_state),
    };
    print_json(&json!({
        "command": "gate",
        "verdict": verdict.to_json(),
        "revision": run_state.revision,
    }))?;
    if verdict.zero {
        Ok(())
    } else {
        Err(format!(
            "the {} gate is not at zero: {} undispositioned finding(s) across {} round(s)",
            verdict.layer.as_str(),
            verdict.undispositioned,
            verdict.rounds_used
        ))
    }
}

/// Exactly one of `--ticket <n>` / `--spec` selects the gate layer.
fn layer(flags: &mut args::Flags) -> Result<gate::GateLayer, String> {
    let spec = flags.boolean("--spec")?;
    let ticket = flags
        .optional("--ticket")?
        .map(|raw| args::parse_u64(&raw, "--ticket"))
        .transpose()?;
    match (spec, ticket) {
        (true, None) => Ok(gate::GateLayer::Spec),
        (false, Some(ticket)) => Ok(gate::GateLayer::Ticket(ticket)),
        (true, Some(_)) => Err("--spec and --ticket are mutually exclusive".to_string()),
        (false, None) => Err("one of --ticket or --spec is required".to_string()),
    }
}

/// The Director's disposition for a finding: fixed, or rejected with a
/// written reason. Silence is not a disposition.
fn disposition(flags: &mut args::Flags) -> Result<Option<state::FindingDisposition>, String> {
    let fixed = flags.optional("--fixed")?;
    let rejected = flags.optional("--rejected")?;
    match (fixed, rejected) {
        (Some(commit), None) => Ok(Some(state::FindingDisposition::Fixed {
            commit: Some(commit),
        })),
        (None, Some(reason)) => {
            if reason.trim().is_empty() {
                return Err(
                    "--rejected needs a written reason; an empty reason does not count".to_string(),
                );
            }
            Ok(Some(state::FindingDisposition::Rejected { reason }))
        }
        (Some(_), Some(_)) => Err("--fixed and --rejected are mutually exclusive".to_string()),
        (None, None) => Ok(None),
    }
}

fn load(worktree: &Path) -> Result<state::RunState, String> {
    storage::ensure_worktree(worktree)?;
    state::read_state(worktree)
}
