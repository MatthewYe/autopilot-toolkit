//! The `WORKER_REPORT` envelope — the Worker's structured hand-back, and the
//! only channel through which a Worker's claims enter run state.
//!
//! Wire format: a line reading `WORKER_REPORT:` on its own, followed by one
//! JSON object; a single ```json fence around that object is tolerated so the
//! block survives a chat message unchanged, and prose around the object is
//! ignored. The body is a typed contract —
//! unknown fields, missing fields, and a status that contradicts the evidence
//! it carries are all validation failures, and a validation failure is a
//! recorded dispatch failure, never a silent pass.
//!
//! The documented shape lives in `WORKER_REPORT.md` next to `Cargo.toml`.

use serde::{Deserialize, Serialize};

use crate::util::require_text;

/// The marker line that introduces the envelope. The bare `WORKER_REPORT`
/// spelling is accepted too, so a Worker that drops the colon is not failed
/// for punctuation.
pub(crate) const MARKER: &str = "WORKER_REPORT:";

/// What the Worker claims about the ticket as a whole.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ReportStatus {
    Done,
    Blocked,
}

impl ReportStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Done => "done",
            Self::Blocked => "blocked",
        }
    }
}

/// How one test command it ran ended.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum TestOutcome {
    Pass,
    Fail,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReportCommit {
    pub(crate) sha: String,
    pub(crate) subject: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReportTest {
    pub(crate) command: String,
    pub(crate) outcome: TestOutcome,
    pub(crate) evidence: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReportAcceptance {
    pub(crate) criterion: String,
    pub(crate) evidence: String,
}

/// The validated envelope. Stored verbatim on the dispatch record so the
/// Worker's self-report survives into the escalation report.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkerReport {
    pub(crate) status: ReportStatus,
    pub(crate) branch: String,
    pub(crate) commits: Vec<ReportCommit>,
    pub(crate) tests: Vec<ReportTest>,
    pub(crate) acceptance: Vec<ReportAcceptance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) design_notes: Option<String>,
    #[serde(default)]
    pub(crate) blockers: Vec<String>,
}

impl WorkerReport {
    /// The one-line summary the CLI prints once the envelope validates.
    pub(crate) fn summary(&self) -> serde_json::Value {
        serde_json::json!({
            "status": self.status.as_str(),
            "branch": self.branch,
            "commits": self.commits.len(),
            "tests": self.tests.len(),
            "acceptance": self.acceptance.len(),
            "blockers": self.blockers.len(),
        })
    }
}

/// Parse and validate an envelope out of a Worker's message or file.
pub(crate) fn parse_envelope(text: &str) -> Result<WorkerReport, String> {
    let body = envelope_body(text)?;
    // The first JSON value after the marker is the envelope; a Worker's
    // sign-off after it is not part of the contract and not a failure.
    let mut values = serde_json::Deserializer::from_str(body).into_iter::<WorkerReport>();
    let report = match values.next() {
        Some(Ok(report)) => report,
        Some(Err(err)) => return Err(describe_body_error(body, &err)),
        None => return Err("the envelope body is empty".to_string()),
    };
    validate(&report)?;
    Ok(report)
}

/// Turn a serde failure into an error that names the offending line, so the
/// message is actionable for a Writer/Director that never sees the JSON types.
fn describe_body_error(body: &str, err: &serde_json::Error) -> String {
    let snippet = body
        .lines()
        .nth(err.line().saturating_sub(1))
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .unwrap_or("<end of body>");
    format!("the envelope body is not a valid WORKER_REPORT object: {err} (at {snippet:?})")
}

/// Everything after the marker line, with an optional single code fence
/// removed.
fn envelope_body(text: &str) -> Result<&str, String> {
    let mut offset = 0;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed == MARKER || trimmed == MARKER.trim_end_matches(':') {
            let rest = &text[offset + line.len()..];
            let body = strip_fence(rest.trim())?;
            if body.is_empty() {
                return Err(format!("the `{MARKER}` line carries no envelope body"));
            }
            return Ok(body);
        }
        offset += line.len() + 1;
    }
    Err(format!("no `{MARKER}` line found"))
}

fn strip_fence(body: &str) -> Result<&str, String> {
    let Some(rest) = body.strip_prefix("```") else {
        return Ok(body);
    };
    let (_, after_info) = rest
        .split_once('\n')
        .ok_or_else(|| "the envelope fence is never closed".to_string())?;
    let inner = after_info
        .trim_end()
        .strip_suffix("```")
        .ok_or_else(|| "the envelope fence is never closed".to_string())?;
    Ok(inner.trim())
}

/// Field-by-field validation. Serde covers presence and type; this covers
/// emptiness and the contradictions between the declared status and the
/// evidence that status claims.
fn validate(report: &WorkerReport) -> Result<(), String> {
    require_text(&report.branch, "branch")?;
    for (index, commit) in report.commits.iter().enumerate() {
        require_text(&commit.sha, &format!("commits[{index}].sha"))?;
        require_text(&commit.subject, &format!("commits[{index}].subject"))?;
    }
    for (index, test) in report.tests.iter().enumerate() {
        require_text(&test.command, &format!("tests[{index}].command"))?;
        require_text(&test.evidence, &format!("tests[{index}].evidence"))?;
    }
    for (index, acceptance) in report.acceptance.iter().enumerate() {
        require_text(
            &acceptance.criterion,
            &format!("acceptance[{index}].criterion"),
        )?;
        require_text(
            &acceptance.evidence,
            &format!("acceptance[{index}].evidence"),
        )?;
    }
    for (index, blocker) in report.blockers.iter().enumerate() {
        require_text(blocker, &format!("blockers[{index}]"))?;
    }
    if let Some(notes) = &report.design_notes {
        require_text(notes, "design_notes")?;
    }

    match report.status {
        ReportStatus::Done => {
            if report.commits.is_empty() {
                return Err("a `done` report must list at least one commit".to_string());
            }
            if report.tests.is_empty() {
                return Err(
                    "a `done` report must carry at least one test evidence entry".to_string(),
                );
            }
            if report.acceptance.is_empty() {
                return Err(
                    "a `done` report must carry a per-acceptance-criterion self-check".to_string(),
                );
            }
            if !report.blockers.is_empty() {
                return Err("a `done` report must not carry blockers".to_string());
            }
            if let Some(index) = report
                .tests
                .iter()
                .position(|test| test.outcome == TestOutcome::Fail)
            {
                return Err(format!(
                    "tests[{index}].outcome is `fail` while the report claims `done`"
                ));
            }
        }
        ReportStatus::Blocked => {
            if report.blockers.is_empty() {
                return Err("a `blocked` report must name at least one blocker".to_string());
            }
        }
    }
    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"WORKER_REPORT:
{
  "status": "done",
  "branch": "codex/132-worker-report",
  "commits": [{ "sha": "abc1234", "subject": "feat(director-cli): envelope" }],
  "tests": [
    { "command": "cargo test -p director-cli", "outcome": "pass", "evidence": "62 passed" }
  ],
  "acceptance": [
    { "criterion": "envelope validated", "evidence": "tests/dispatch.rs::malformed_report" }
  ],
  "design_notes": "typed envelope",
  "blockers": []
}
"#;

    fn envelope_json() -> serde_json::Value {
        serde_json::from_str(GOOD.split_once('\n').unwrap().1).unwrap()
    }

    fn parse_mutated(mutate: impl FnOnce(&mut serde_json::Value)) -> Result<WorkerReport, String> {
        let mut value = envelope_json();
        mutate(&mut value);
        parse_envelope(&format!(
            "{MARKER}\n{}",
            serde_json::to_string(&value).unwrap()
        ))
    }

    #[test]
    fn a_complete_envelope_parses() {
        let report = parse_envelope(GOOD).unwrap();
        assert_eq!(report.status, ReportStatus::Done);
        assert_eq!(report.commits.len(), 1);
        assert_eq!(report.tests[0].outcome, TestOutcome::Pass);
        assert!(report.blockers.is_empty());
        assert_eq!(report.summary()["commits"], 1);
    }

    #[test]
    fn prose_around_the_envelope_is_ignored() {
        let text = format!("Here is what I did.\n\n{GOOD}\nThanks!");
        let report = parse_envelope(&text).unwrap();
        assert_eq!(report.status, ReportStatus::Done);
    }

    #[test]
    fn a_fenced_envelope_parses() {
        let text = format!("{MARKER}\n```json\n{}\n```\n", envelope_json());
        let report = parse_envelope(&text).unwrap();
        assert_eq!(report.branch, "codex/132-worker-report");
    }

    #[test]
    fn the_bare_marker_spelling_is_accepted() {
        let text = GOOD.replacen(MARKER, "WORKER_REPORT", 1);
        assert!(parse_envelope(&text).is_ok());
    }

    #[test]
    fn a_missing_marker_is_refused() {
        let error = parse_envelope("{\"status\": \"done\"}").unwrap_err();
        assert!(error.contains("no `WORKER_REPORT:` line"), "got: {error}");
    }

    #[test]
    fn an_unclosed_fence_is_refused() {
        let error = parse_envelope(&format!("{MARKER}\n```json\n{{}}")).unwrap_err();
        assert!(error.contains("never closed"), "got: {error}");
    }

    #[test]
    fn an_empty_body_is_refused() {
        let error = parse_envelope(MARKER).unwrap_err();
        assert!(error.contains("no envelope body"), "got: {error}");
    }

    #[test]
    fn a_missing_field_is_refused_by_name() {
        let error = parse_mutated(|value| {
            value.as_object_mut().unwrap().remove("tests");
        })
        .unwrap_err();
        assert!(error.contains("tests"), "got: {error}");
    }

    #[test]
    fn a_wrongly_typed_field_is_refused() {
        let error = parse_mutated(|value| {
            value["commits"] = serde_json::json!("abc1234 feat: thing");
        })
        .unwrap_err();
        assert!(error.contains("commits"), "got: {error}");
    }

    #[test]
    fn an_unknown_field_is_refused() {
        let error = parse_mutated(|value| {
            value["confidence"] = serde_json::json!("high");
        })
        .unwrap_err();
        assert!(error.contains("confidence"), "got: {error}");
    }

    #[test]
    fn an_unknown_status_is_refused() {
        let error = parse_mutated(|value| {
            value["status"] = serde_json::json!("probably");
        })
        .unwrap_err();
        assert!(error.contains("status"), "got: {error}");
    }

    #[test]
    fn empty_text_fields_are_refused_by_path() {
        let error = parse_mutated(|value| {
            value["commits"][0]["sha"] = serde_json::json!("   ");
        })
        .unwrap_err();
        assert!(error.contains("commits[0].sha"), "got: {error}");

        let error = parse_mutated(|value| {
            value["tests"][0]["evidence"] = serde_json::json!("");
        })
        .unwrap_err();
        assert!(error.contains("tests[0].evidence"), "got: {error}");
    }

    #[test]
    fn a_done_report_without_evidence_is_refused() {
        let error = parse_mutated(|value| {
            value["commits"] = serde_json::json!([]);
        })
        .unwrap_err();
        assert!(error.contains("at least one commit"), "got: {error}");

        let error = parse_mutated(|value| {
            value["tests"] = serde_json::json!([]);
        })
        .unwrap_err();
        assert!(error.contains("test evidence"), "got: {error}");

        let error = parse_mutated(|value| {
            value["acceptance"] = serde_json::json!([]);
        })
        .unwrap_err();
        assert!(error.contains("self-check"), "got: {error}");
    }

    #[test]
    fn a_done_report_contradicting_itself_is_refused() {
        let error = parse_mutated(|value| {
            value["tests"][0]["outcome"] = serde_json::json!("fail");
        })
        .unwrap_err();
        assert!(error.contains("claims `done`"), "got: {error}");

        let error = parse_mutated(|value| {
            value["blockers"] = serde_json::json!(["needs a human"]);
        })
        .unwrap_err();
        assert!(error.contains("must not carry blockers"), "got: {error}");
    }

    #[test]
    fn a_blocked_report_must_name_a_blocker() {
        let error = parse_mutated(|value| {
            value["status"] = serde_json::json!("blocked");
        })
        .unwrap_err();
        assert!(error.contains("at least one blocker"), "got: {error}");

        let report = parse_mutated(|value| {
            value["status"] = serde_json::json!("blocked");
            value["commits"] = serde_json::json!([]);
            value["tests"] = serde_json::json!([]);
            value["acceptance"] = serde_json::json!([]);
            value["blockers"] = serde_json::json!(["waiting on the upstream fix"]);
        })
        .unwrap();
        assert_eq!(report.status, ReportStatus::Blocked);
    }

    #[test]
    fn empty_design_notes_are_refused() {
        let error = parse_mutated(|value| {
            value["design_notes"] = serde_json::json!("  ");
        })
        .unwrap_err();
        assert!(error.contains("design_notes"), "got: {error}");
    }

    #[test]
    fn a_missing_design_notes_field_is_fine() {
        let report = parse_mutated(|value| {
            value.as_object_mut().unwrap().remove("design_notes");
        })
        .unwrap();
        assert_eq!(report.design_notes, None);
    }
}
