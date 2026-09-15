//! Command-line parsing for the `director` binary.

use std::path::PathBuf;

use crate::util;

pub(crate) const USAGE: &str = "\
Usage: director init --worktree <path> --spec-issue <n> --slug <slug>
       director inspect --worktree <path>";

#[derive(Debug)]
pub(crate) struct InitArgs {
    pub(crate) worktree: PathBuf,
    pub(crate) spec_issue: u64,
    pub(crate) slug: String,
}

#[derive(Debug)]
pub(crate) struct InspectArgs {
    pub(crate) worktree: PathBuf,
}

fn parse_flags(args: Vec<String>) -> Result<Vec<(String, Option<String>)>, String> {
    let mut parsed = Vec::new();
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        if !arg.starts_with("--") {
            return Err(format!("unexpected argument: {arg}"));
        }
        let (name, inline) = match arg.split_once('=') {
            Some((name, value)) => (name.to_string(), Some(value.to_string())),
            None => (arg, None),
        };
        let value = match inline {
            Some(value) => value,
            None => iter
                .next()
                .ok_or_else(|| format!("{name} requires a value"))?,
        };
        parsed.push((name, Some(value)));
    }
    Ok(parsed)
}

pub(crate) fn parse_init_args(args: Vec<String>) -> Result<InitArgs, String> {
    let mut worktree = None;
    let mut spec_issue = None;
    let mut slug = None;
    for (name, value) in parse_flags(args)? {
        match name.as_str() {
            "--worktree" => worktree = Some(PathBuf::from(value.expect("value required"))),
            "--spec-issue" => {
                let raw = value.expect("value required");
                spec_issue = Some(raw.parse::<u64>().map_err(|_| {
                    format!("--spec-issue must be a positive integer, got {raw:?}")
                })?);
            }
            "--slug" => slug = Some(value.expect("value required")),
            other => return Err(format!("unknown flag for init: {other}\n{USAGE}")),
        }
    }
    let worktree = worktree.ok_or_else(|| format!("init requires --worktree\n{USAGE}"))?;
    let spec_issue = spec_issue.ok_or_else(|| format!("init requires --spec-issue\n{USAGE}"))?;
    if spec_issue == 0 {
        return Err("--spec-issue must be a positive integer".to_string());
    }
    let slug = slug.ok_or_else(|| format!("init requires --slug\n{USAGE}"))?;
    util::validate_slug(&slug)?;
    Ok(InitArgs {
        worktree,
        spec_issue,
        slug,
    })
}

pub(crate) fn parse_inspect_args(args: Vec<String>) -> Result<InspectArgs, String> {
    let mut worktree = None;
    for (name, value) in parse_flags(args)? {
        match name.as_str() {
            "--worktree" => worktree = Some(PathBuf::from(value.expect("value required"))),
            other => return Err(format!("unknown flag for inspect: {other}\n{USAGE}")),
        }
    }
    let worktree = worktree.ok_or_else(|| format!("inspect requires --worktree\n{USAGE}"))?;
    Ok(InspectArgs { worktree })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn init_accepts_inline_and_separate_values() {
        let init = parse_init_args(args(&[
            "--worktree",
            "/tmp/wt",
            "--spec-issue",
            "128",
            "--slug",
            "autopilot-director",
        ]))
        .unwrap();
        assert_eq!(init.worktree, PathBuf::from("/tmp/wt"));
        assert_eq!(init.spec_issue, 128);
        assert_eq!(init.slug, "autopilot-director");

        let inline = parse_init_args(args(&[
            "--worktree=/tmp/wt",
            "--spec-issue=128",
            "--slug=autopilot-director",
        ]))
        .unwrap();
        assert_eq!(inline.spec_issue, 128);
        assert_eq!(inline.slug, "autopilot-director");
    }

    #[test]
    fn init_requires_every_input() {
        assert!(parse_init_args(args(&[]))
            .unwrap_err()
            .contains("--worktree"));
        assert!(parse_init_args(args(&["--worktree", "/tmp/wt"]))
            .unwrap_err()
            .contains("--spec-issue"));
        assert!(
            parse_init_args(args(&["--worktree", "/tmp/wt", "--spec-issue", "128"]))
                .unwrap_err()
                .contains("--slug")
        );
    }

    #[test]
    fn init_rejects_a_malformed_slug() {
        let error = parse_init_args(args(&[
            "--worktree",
            "/tmp/wt",
            "--spec-issue",
            "128",
            "--slug",
            "Autopilot Director",
        ]))
        .unwrap_err();
        assert!(error.contains("--slug"), "got: {error}");
    }

    #[test]
    fn init_rejects_non_positive_and_unknown_flags() {
        let error = parse_init_args(args(&[
            "--worktree",
            "/tmp/wt",
            "--spec-issue",
            "abc",
            "--slug",
            "autopilot-director",
        ]))
        .unwrap_err();
        assert!(error.contains("positive integer"), "got: {error}");
        let error = parse_init_args(args(&[
            "--worktree",
            "/tmp/wt",
            "--spec-issue",
            "0",
            "--slug",
            "autopilot-director",
        ]))
        .unwrap_err();
        assert!(error.contains("positive integer"), "got: {error}");
        let error = parse_init_args(args(&[
            "--worktree",
            "/tmp/wt",
            "--spec-issue",
            "128",
            "--slug",
            "autopilot-director",
            "--branch",
            "x",
        ]))
        .unwrap_err();
        assert!(error.contains("unknown flag"), "got: {error}");
    }

    #[test]
    fn flag_without_value_is_rejected() {
        let error = parse_init_args(args(&["--worktree"])).unwrap_err();
        assert!(error.contains("requires a value"), "got: {error}");
    }
}
