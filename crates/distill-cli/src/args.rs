use std::path::PathBuf;

use crate::util::{parse_revision, require_non_empty, require_run_id};

pub(crate) const SUPPORTED_RUNTIMES: [&str; 3] = ["codex", "kimi", "reasonix"];

pub(crate) struct StartArgs {
    pub(crate) runtime: String,
    pub(crate) session_id: String,
    pub(crate) worktree: PathBuf,
    pub(crate) requirement: Option<String>,
    pub(crate) intake_json: Option<String>,
    pub(crate) json: bool,
}

pub(crate) struct SubmitArgs {
    pub(crate) run_id: String,
    pub(crate) session_id: String,
    pub(crate) expected_revision: u64,
    pub(crate) worktree: PathBuf,
    pub(crate) stage: String,
    pub(crate) evidence: String,
    pub(crate) json: bool,
}

pub(crate) struct TakeoverArgs {
    pub(crate) run_id: String,
    pub(crate) from_session: String,
    pub(crate) to_session: String,
    pub(crate) expected_revision: u64,
    pub(crate) worktree: PathBuf,
    pub(crate) reason: String,
    pub(crate) user_authorized: bool,
    pub(crate) json: bool,
}

pub(crate) struct SupersedeArgs {
    pub(crate) run_id: String,
    pub(crate) session_id: String,
    pub(crate) expected_revision: u64,
    pub(crate) worktree: PathBuf,
    pub(crate) reason: String,
    pub(crate) requirement: String,
    pub(crate) json: bool,
}

pub(crate) struct InspectArgs {
    pub(crate) run_id: String,
    pub(crate) session_id: String,
    pub(crate) expected_revision: u64,
    pub(crate) worktree: PathBuf,
    pub(crate) json: bool,
}

pub(crate) struct EventsArgs {
    pub(crate) run_id: String,
    pub(crate) worktree: PathBuf,
    pub(crate) after: u64,
    pub(crate) json: bool,
}

pub(crate) struct RenderReportArgs {
    pub(crate) run_id: String,
    pub(crate) worktree: PathBuf,
    pub(crate) renderer: String,
    pub(crate) json: bool,
}

pub(crate) struct QuotaArgs {
    pub(crate) worktree: PathBuf,
    pub(crate) bytes: u64,
    pub(crate) json: bool,
}

pub(crate) struct PurgeArgs {
    pub(crate) worktree: PathBuf,
    pub(crate) run_id: String,
    pub(crate) session_id: String,
    pub(crate) expected_revision: u64,
    pub(crate) user_authorized: bool,
    pub(crate) json: bool,
}

pub(crate) struct AbortArgs {
    pub(crate) worktree: PathBuf,
    pub(crate) run_id: String,
    pub(crate) session_id: String,
    pub(crate) expected_revision: u64,
    pub(crate) reason: String,
    pub(crate) user_authorized: bool,
    pub(crate) json: bool,
}

pub(crate) fn require_supported_runtime(runtime: Option<String>) -> Result<String, String> {
    let runtime = runtime.ok_or("--runtime is required")?;
    if SUPPORTED_RUNTIMES.contains(&runtime.as_str()) {
        Ok(runtime)
    } else {
        Err(format!(
            "--runtime must be one of: {}",
            SUPPORTED_RUNTIMES.join(", ")
        ))
    }
}

pub(crate) fn parse_start_args(args: Vec<String>) -> Result<StartArgs, String> {
    let mut runtime = None;
    let mut session_id = None;
    let mut worktree = None;
    let mut requirement = None;
    let mut intake_json = None;
    let mut json = false;

    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--json" => json = true,
            "--runtime" => runtime = iter.next(),
            "--session-id" => session_id = iter.next(),
            "--worktree" => worktree = iter.next().map(PathBuf::from),
            "--requirement" => requirement = iter.next(),
            "--intake-json" => intake_json = iter.next(),
            other => return Err(format!("unknown start argument: {other}")),
        }
    }

    let runtime = require_supported_runtime(runtime)?;
    let session_id = require_non_empty(session_id, "--session-id")?;
    if requirement.is_none() && intake_json.is_none() {
        return Err("--requirement or --intake-json is required".to_string());
    }
    if requirement
        .as_ref()
        .is_some_and(|requirement| requirement.trim().is_empty())
    {
        return Err("--requirement must not be empty".to_string());
    }
    if intake_json
        .as_ref()
        .is_some_and(|intake_json| intake_json.trim().is_empty())
    {
        return Err("--intake-json must not be empty".to_string());
    }

    Ok(StartArgs {
        runtime,
        session_id,
        worktree: worktree.ok_or("--worktree is required")?,
        requirement,
        intake_json,
        json,
    })
}

pub(crate) fn parse_submit_args(args: Vec<String>) -> Result<SubmitArgs, String> {
    let mut run_id = None;
    let mut session_id = None;
    let mut expected_revision = None;
    let mut worktree = None;
    let mut stage = None;
    let mut evidence = None;
    let mut json = false;

    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--json" => json = true,
            "--run-id" => run_id = iter.next(),
            "--session-id" => session_id = iter.next(),
            "--expected-revision" => expected_revision = iter.next(),
            "--worktree" => worktree = iter.next().map(PathBuf::from),
            "--stage" => stage = iter.next(),
            "--evidence" => evidence = iter.next(),
            other => return Err(format!("unknown submit-evidence argument: {other}")),
        }
    }

    Ok(SubmitArgs {
        run_id: require_run_id(run_id, "--run-id")?,
        session_id: require_non_empty(session_id, "--session-id")?,
        expected_revision: parse_revision(expected_revision, "--expected-revision")?,
        worktree: worktree.ok_or("--worktree is required")?,
        stage: require_non_empty(stage, "--stage")?,
        evidence: require_non_empty(evidence, "--evidence")?,
        json,
    })
}

pub(crate) fn parse_quota_args(args: Vec<String>) -> Result<QuotaArgs, String> {
    let mut worktree = None;
    let mut bytes = None;
    let mut json = false;
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--json" => json = true,
            "--worktree" => worktree = iter.next().map(PathBuf::from),
            "--bytes" => bytes = iter.next(),
            other => return Err(format!("unknown set-project-quota argument: {other}")),
        }
    }
    Ok(QuotaArgs {
        worktree: worktree.ok_or("--worktree is required")?,
        bytes: parse_revision(bytes, "--bytes")?,
        json,
    })
}

pub(crate) fn parse_purge_args(args: Vec<String>) -> Result<PurgeArgs, String> {
    let mut worktree = None;
    let mut run_id = None;
    let mut session_id = None;
    let mut expected_revision = None;
    let mut user_authorized = false;
    let mut json = false;
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--json" => json = true,
            "--worktree" => worktree = iter.next().map(PathBuf::from),
            "--run-id" => run_id = iter.next(),
            "--session-id" => session_id = iter.next(),
            "--expected-revision" => expected_revision = iter.next(),
            "--user-authorized" => user_authorized = true,
            other => return Err(format!("unknown purge argument: {other}")),
        }
    }
    Ok(PurgeArgs {
        worktree: worktree.ok_or("--worktree is required")?,
        run_id: require_run_id(run_id, "--run-id")?,
        session_id: require_non_empty(session_id, "--session-id")?,
        expected_revision: parse_revision(expected_revision, "--expected-revision")?,
        user_authorized,
        json,
    })
}

pub(crate) fn parse_abort_args(args: Vec<String>) -> Result<AbortArgs, String> {
    let mut worktree = None;
    let mut run_id = None;
    let mut session_id = None;
    let mut expected_revision = None;
    let mut reason = None;
    let mut user_authorized = false;
    let mut json = false;
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--json" => json = true,
            "--worktree" => worktree = iter.next().map(PathBuf::from),
            "--run-id" => run_id = iter.next(),
            "--session-id" => session_id = iter.next(),
            "--expected-revision" => expected_revision = iter.next(),
            "--reason" => reason = iter.next(),
            "--user-authorized" => user_authorized = true,
            other => return Err(format!("unknown abort argument: {other}")),
        }
    }
    Ok(AbortArgs {
        worktree: worktree.ok_or("--worktree is required")?,
        run_id: require_run_id(run_id, "--run-id")?,
        session_id: require_non_empty(session_id, "--session-id")?,
        expected_revision: parse_revision(expected_revision, "--expected-revision")?,
        reason: require_non_empty(reason, "--reason")?,
        user_authorized,
        json,
    })
}

pub(crate) fn parse_takeover_args(args: Vec<String>) -> Result<TakeoverArgs, String> {
    let mut run_id = None;
    let mut from_session = None;
    let mut to_session = None;
    let mut expected_revision = None;
    let mut worktree = None;
    let mut reason = None;
    let mut user_authorized = false;
    let mut json = false;

    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--json" => json = true,
            "--run-id" => run_id = iter.next(),
            "--from-session" => from_session = iter.next(),
            "--to-session" => to_session = iter.next(),
            "--expected-revision" => expected_revision = iter.next(),
            "--worktree" => worktree = iter.next().map(PathBuf::from),
            "--reason" => reason = iter.next(),
            "--user-authorized" => user_authorized = true,
            other => return Err(format!("unknown takeover argument: {other}")),
        }
    }

    Ok(TakeoverArgs {
        run_id: require_run_id(run_id, "--run-id")?,
        from_session: require_non_empty(from_session, "--from-session")?,
        to_session: require_non_empty(to_session, "--to-session")?,
        expected_revision: parse_revision(expected_revision, "--expected-revision")?,
        worktree: worktree.ok_or("--worktree is required")?,
        reason: require_non_empty(reason, "--reason")?,
        user_authorized,
        json,
    })
}

pub(crate) fn parse_supersede_args(args: Vec<String>) -> Result<SupersedeArgs, String> {
    let mut run_id = None;
    let mut session_id = None;
    let mut expected_revision = None;
    let mut worktree = None;
    let mut reason = None;
    let mut requirement = None;
    let mut json = false;

    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--json" => json = true,
            "--run-id" => run_id = iter.next(),
            "--session-id" => session_id = iter.next(),
            "--expected-revision" => expected_revision = iter.next(),
            "--worktree" => worktree = iter.next().map(PathBuf::from),
            "--reason" => reason = iter.next(),
            "--requirement" => requirement = iter.next(),
            other => return Err(format!("unknown supersede argument: {other}")),
        }
    }

    Ok(SupersedeArgs {
        run_id: require_run_id(run_id, "--run-id")?,
        session_id: require_non_empty(session_id, "--session-id")?,
        expected_revision: parse_revision(expected_revision, "--expected-revision")?,
        worktree: worktree.ok_or("--worktree is required")?,
        reason: require_non_empty(reason, "--reason")?,
        requirement: require_non_empty(requirement, "--requirement")?,
        json,
    })
}

pub(crate) fn parse_inspect_args(args: Vec<String>) -> Result<InspectArgs, String> {
    let mut run_id = None;
    let mut session_id = None;
    let mut expected_revision = None;
    let mut worktree = None;
    let mut json = false;

    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--json" => json = true,
            "--run-id" => run_id = iter.next(),
            "--session-id" => session_id = iter.next(),
            "--expected-revision" => expected_revision = iter.next(),
            "--worktree" => worktree = iter.next().map(PathBuf::from),
            other => return Err(format!("unknown inspect argument: {other}")),
        }
    }

    Ok(InspectArgs {
        run_id: require_run_id(run_id, "--run-id")?,
        session_id: require_non_empty(session_id, "--session-id")?,
        expected_revision: parse_revision(expected_revision, "--expected-revision")?,
        worktree: worktree.ok_or("--worktree is required")?,
        json,
    })
}

pub(crate) fn parse_events_args(args: Vec<String>) -> Result<EventsArgs, String> {
    let mut run_id = None;
    let mut worktree = None;
    let mut after = None;
    let mut json = false;

    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--json" => json = true,
            "--run-id" => run_id = iter.next(),
            "--worktree" => worktree = iter.next().map(PathBuf::from),
            "--after" => after = iter.next(),
            other => return Err(format!("unknown events argument: {other}")),
        }
    }

    Ok(EventsArgs {
        run_id: require_run_id(run_id, "--run-id")?,
        worktree: worktree.ok_or("--worktree is required")?,
        after: parse_revision(Some(after.unwrap_or_else(|| "0".to_string())), "--after")?,
        json,
    })
}

pub(crate) fn parse_render_report_args(args: Vec<String>) -> Result<RenderReportArgs, String> {
    let mut run_id = None;
    let mut worktree = None;
    let mut renderer = None;
    let mut json = false;

    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--json" => json = true,
            "--run-id" => run_id = iter.next(),
            "--worktree" => worktree = iter.next().map(PathBuf::from),
            "--renderer" => renderer = iter.next(),
            other => return Err(format!("unknown render-report argument: {other}")),
        }
    }

    Ok(RenderReportArgs {
        run_id: require_run_id(run_id, "--run-id")?,
        worktree: worktree.ok_or("--worktree is required")?,
        renderer: require_non_empty(renderer, "--renderer")?,
        json,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- parse_start_args ---

    #[test]
    fn test_parse_start_args_normal() {
        let args = vec![
            "--runtime", "codex",
            "--session-id", "sess1",
            "--worktree", "/tmp/ws",
            "--requirement", "do stuff",
        ].into_iter().map(|s| s.to_string()).collect();
        let result = parse_start_args(args).unwrap();
        assert_eq!(result.runtime, "codex");
        assert_eq!(result.session_id, "sess1");
        assert_eq!(result.worktree, PathBuf::from("/tmp/ws"));
        assert_eq!(result.requirement.as_deref(), Some("do stuff"));
        assert!(!result.json);
    }

    #[test]
    fn test_parse_start_args_with_json_flag() {
        let args = vec![
            "--json",
            "--runtime", "kimi",
            "--session-id", "s1",
            "--worktree", "/tmp/ws",
            "--requirement", "req",
        ].into_iter().map(|s| s.to_string()).collect();
        let result = parse_start_args(args).unwrap();
        assert!(result.json);
        assert_eq!(result.runtime, "kimi");
    }

    #[test]
    fn test_parse_start_args_with_intake_json() {
        let args = vec![
            "--runtime", "reasonix",
            "--session-id", "s1",
            "--worktree", "/tmp/ws",
            "--intake-json", "{\"key\":\"val\"}",
        ].into_iter().map(|s| s.to_string()).collect();
        let result = parse_start_args(args).unwrap();
        assert_eq!(result.intake_json.as_deref(), Some("{\"key\":\"val\"}"));
        assert!(result.requirement.is_none());
    }

    #[test]
    fn test_parse_start_args_missing_runtime() {
        let args = vec![
            "--session-id", "s1",
            "--worktree", "/tmp/ws",
            "--requirement", "req",
        ].into_iter().map(|s| s.to_string()).collect();
        let result = parse_start_args(args);
        assert!(result.is_err());
        assert!(result.err().unwrap().contains("runtime"));
    }

    #[test]
    fn test_parse_start_args_unsupported_runtime() {
        let args = vec![
            "--runtime", "unknown",
            "--session-id", "s1",
            "--worktree", "/tmp/ws",
            "--requirement", "req",
        ].into_iter().map(|s| s.to_string()).collect();
        let result = parse_start_args(args);
        assert!(result.is_err());
        assert!(result.err().unwrap().contains("runtime must be one of"));
    }

    #[test]
    fn test_parse_start_args_missing_session_id() {
        let args = vec![
            "--runtime", "codex",
            "--worktree", "/tmp/ws",
            "--requirement", "req",
        ].into_iter().map(|s| s.to_string()).collect();
        let result = parse_start_args(args);
        assert!(result.is_err());
        assert!(result.err().unwrap().contains("session-id"));
    }

    #[test]
    fn test_parse_start_args_missing_worktree() {
        let args = vec![
            "--runtime", "codex",
            "--session-id", "s1",
            "--requirement", "req",
        ].into_iter().map(|s| s.to_string()).collect();
        let result = parse_start_args(args);
        assert!(result.is_err());
        assert!(result.err().unwrap().contains("worktree"));
    }

    #[test]
    fn test_parse_start_args_missing_requirement_and_intake() {
        let args = vec![
            "--runtime", "codex",
            "--session-id", "s1",
            "--worktree", "/tmp/ws",
        ].into_iter().map(|s| s.to_string()).collect();
        let result = parse_start_args(args);
        assert!(result.is_err());
        assert!(result.err().unwrap().contains("requirement or --intake-json"));
    }

    #[test]
    fn test_parse_start_args_empty_requirement() {
        let args = vec![
            "--runtime", "codex",
            "--session-id", "s1",
            "--worktree", "/tmp/ws",
            "--requirement", "",
        ].into_iter().map(|s| s.to_string()).collect();
        let result = parse_start_args(args);
        assert!(result.is_err());
        assert!(result.err().unwrap().contains("requirement must not be empty"));
    }

    #[test]
    fn test_parse_start_args_empty_intake_json() {
        let args = vec![
            "--runtime", "codex",
            "--session-id", "s1",
            "--worktree", "/tmp/ws",
            "--intake-json", "",
        ].into_iter().map(|s| s.to_string()).collect();
        let result = parse_start_args(args);
        assert!(result.is_err());
        assert!(result.err().unwrap().contains("intake-json must not be empty"));
    }

    #[test]
    fn test_parse_start_args_unknown_arg() {
        let args = vec![
            "--runtime", "codex",
            "--session-id", "s1",
            "--worktree", "/tmp/ws",
            "--requirement", "req",
            "--unknown-flag",
        ].into_iter().map(|s| s.to_string()).collect();
        let result = parse_start_args(args);
        assert!(result.is_err());
        assert!(result.err().unwrap().contains("unknown start argument"));
    }

    #[test]
    fn test_parse_start_args_whitespace_requirement() {
        let args = vec![
            "--runtime", "codex",
            "--session-id", "s1",
            "--worktree", "/tmp/ws",
            "--requirement", "   ",
        ].into_iter().map(|s| s.to_string()).collect();
        let result = parse_start_args(args);
        assert!(result.is_err());
        assert!(result.err().unwrap().contains("requirement must not be empty"));
    }

    // --- parse_submit_args ---

    #[test]
    fn test_parse_submit_args_normal() {
        let args = vec![
            "--run-id", "run-abc-123",
            "--session-id", "sess1",
            "--expected-revision", "5",
            "--worktree", "/tmp/ws",
            "--stage", "clarification",
            "--evidence", "done",
        ].into_iter().map(|s| s.to_string()).collect();
        let result = parse_submit_args(args).unwrap();
        assert_eq!(result.run_id, "run-abc-123");
        assert_eq!(result.session_id, "sess1");
        assert_eq!(result.expected_revision, 5);
        assert_eq!(result.worktree, PathBuf::from("/tmp/ws"));
        assert_eq!(result.stage, "clarification");
        assert_eq!(result.evidence, "done");
        assert!(!result.json);
    }

    #[test]
    fn test_parse_submit_args_with_json() {
        let args = vec![
            "--json",
            "--run-id", "run-x",
            "--session-id", "s1",
            "--expected-revision", "1",
            "--worktree", "/tmp/ws",
            "--stage", "intake",
            "--evidence", "ev",
        ].into_iter().map(|s| s.to_string()).collect();
        let result = parse_submit_args(args).unwrap();
        assert!(result.json);
    }

    #[test]
    fn test_parse_submit_args_missing_run_id() {
        let args = vec![
            "--session-id", "s1",
            "--expected-revision", "1",
            "--worktree", "/tmp/ws",
            "--stage", "intake",
            "--evidence", "ev",
        ].into_iter().map(|s| s.to_string()).collect();
        let result = parse_submit_args(args);
        assert!(result.is_err());
        assert!(result.err().unwrap().contains("run-id"));
    }

    #[test]
    fn test_parse_submit_args_invalid_run_id() {
        let args = vec![
            "--run-id", "run with spaces",
            "--session-id", "s1",
            "--expected-revision", "1",
            "--worktree", "/tmp/ws",
            "--stage", "intake",
            "--evidence", "ev",
        ].into_iter().map(|s| s.to_string()).collect();
        let result = parse_submit_args(args);
        assert!(result.is_err());
        assert!(result.err().unwrap().contains("unsafe"));
    }

    #[test]
    fn test_parse_submit_args_missing_session_id() {
        let args = vec![
            "--run-id", "run-x",
            "--expected-revision", "1",
            "--worktree", "/tmp/ws",
            "--stage", "intake",
            "--evidence", "ev",
        ].into_iter().map(|s| s.to_string()).collect();
        let result = parse_submit_args(args);
        assert!(result.is_err());
        assert!(result.err().unwrap().contains("session-id"));
    }

    #[test]
    fn test_parse_submit_args_missing_expected_revision() {
        let args = vec![
            "--run-id", "run-x",
            "--session-id", "s1",
            "--worktree", "/tmp/ws",
            "--stage", "intake",
            "--evidence", "ev",
        ].into_iter().map(|s| s.to_string()).collect();
        let result = parse_submit_args(args);
        assert!(result.is_err());
        assert!(result.err().unwrap().contains("expected-revision"));
    }

    #[test]
    fn test_parse_submit_args_invalid_revision() {
        let args = vec![
            "--run-id", "run-x",
            "--session-id", "s1",
            "--expected-revision", "abc",
            "--worktree", "/tmp/ws",
            "--stage", "intake",
            "--evidence", "ev",
        ].into_iter().map(|s| s.to_string()).collect();
        let result = parse_submit_args(args);
        assert!(result.is_err());
        assert!(result.err().unwrap().contains("must be an integer"));
    }

    #[test]
    fn test_parse_submit_args_missing_worktree() {
        let args = vec![
            "--run-id", "run-x",
            "--session-id", "s1",
            "--expected-revision", "1",
            "--stage", "intake",
            "--evidence", "ev",
        ].into_iter().map(|s| s.to_string()).collect();
        let result = parse_submit_args(args);
        assert!(result.is_err());
        assert!(result.err().unwrap().contains("worktree"));
    }

    #[test]
    fn test_parse_submit_args_missing_stage() {
        let args = vec![
            "--run-id", "run-x",
            "--session-id", "s1",
            "--expected-revision", "1",
            "--worktree", "/tmp/ws",
            "--evidence", "ev",
        ].into_iter().map(|s| s.to_string()).collect();
        let result = parse_submit_args(args);
        assert!(result.is_err());
        assert!(result.err().unwrap().contains("stage"));
    }

    #[test]
    fn test_parse_submit_args_missing_evidence() {
        let args = vec![
            "--run-id", "run-x",
            "--session-id", "s1",
            "--expected-revision", "1",
            "--worktree", "/tmp/ws",
            "--stage", "intake",
        ].into_iter().map(|s| s.to_string()).collect();
        let result = parse_submit_args(args);
        assert!(result.is_err());
        assert!(result.err().unwrap().contains("evidence"));
    }

    #[test]
    fn test_parse_submit_args_unknown_arg() {
        let args = vec![
            "--run-id", "run-x",
            "--session-id", "s1",
            "--expected-revision", "1",
            "--worktree", "/tmp/ws",
            "--stage", "intake",
            "--evidence", "ev",
            "--unknown",
        ].into_iter().map(|s| s.to_string()).collect();
        let result = parse_submit_args(args);
        assert!(result.is_err());
        assert!(result.err().unwrap().contains("unknown submit-evidence argument"));
    }

    // --- require_supported_runtime ---

    #[test]
    fn test_require_supported_runtime_codex() {
        assert_eq!(
            require_supported_runtime(Some("codex".to_string())).unwrap(),
            "codex"
        );
    }

    #[test]
    fn test_require_supported_runtime_kimi() {
        assert_eq!(
            require_supported_runtime(Some("kimi".to_string())).unwrap(),
            "kimi"
        );
    }

    #[test]
    fn test_require_supported_runtime_reasonix() {
        assert_eq!(
            require_supported_runtime(Some("reasonix".to_string())).unwrap(),
            "reasonix"
        );
    }

    #[test]
    fn test_require_supported_runtime_none() {
        let result = require_supported_runtime(None);
        assert!(result.is_err());
        assert!(result.err().unwrap().contains("runtime is required"));
    }

    #[test]
    fn test_require_supported_runtime_invalid() {
        let result = require_supported_runtime(Some("gpt".to_string()));
        assert!(result.is_err());
        assert!(result.err().unwrap().contains("runtime must be one of"));
    }
}
