//! Run identity derivation.

/// Refuse blank text wherever the machine promises real content.
pub(crate) fn require_text(value: &str, field: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("{field} must not be empty"));
    }
    Ok(())
}

/// The deterministic run id for one spec run. A worktree holds at most one
/// Spec run (`.director/state.json`), so the id is derived from the spec issue
/// rather than generated: a resumed session can always recompute it.
pub(crate) fn run_id_for_spec(spec_issue: u64) -> String {
    format!("spec-{spec_issue}")
}

/// The local spec branch a run's Ticket boundary commits land on — ADR 0047's
/// `codex/spec-<N>-<slug>`.
///
/// The branch is written at revision 0, so it is derived here, once, rather
/// than patched in later. The slug comes from the spec issue title, which the
/// offline CLI (ADR 0020) cannot read, so the Director — the only role holding
/// the title — passes it to `init` explicitly.
pub(crate) fn branch_for_spec(spec_issue: u64, slug: &str) -> String {
    format!("codex/spec-{spec_issue}-{slug}")
}

/// Validate a title-derived branch slug: ASCII lowercase letters and digits in
/// single-hyphen-separated segments, e.g. `autopilot-director`.
pub(crate) fn validate_slug(slug: &str) -> Result<(), String> {
    let invalid = || {
        format!(
            "--slug must be lowercase ASCII letters and digits separated by single \
             hyphens (max 64 chars), got {slug:?}"
        )
    };
    if slug.is_empty() || slug.len() > 64 {
        return Err(invalid());
    }
    let well_formed = slug.split('-').all(|segment| {
        !segment.is_empty()
            && segment
                .chars()
                .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit())
    });
    if !well_formed {
        return Err(invalid());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_derived_from_the_spec_issue() {
        assert_eq!(run_id_for_spec(128), "spec-128");
        assert_eq!(
            branch_for_spec(128, "autopilot-director"),
            "codex/spec-128-autopilot-director"
        );
    }

    #[test]
    fn slug_validation_accepts_kebab_case_segments() {
        for valid in [
            "a",
            "spec128",
            "ticket-130",
            "autopilot-director",
            "director-cli-skeleton",
        ] {
            assert!(validate_slug(valid).is_ok(), "{valid} should be accepted");
        }
    }

    #[test]
    fn slug_validation_rejects_everything_else() {
        let overlong = "x".repeat(65);
        let mut invalid = vec![
            "",
            "Autopilot",
            "AUTOPILOT",
            "autopilot_director",
            "autopilot.director",
            " autopilot",
            "autopilot ",
            "-autopilot",
            "autopilot-",
            "autopilot--director",
            "autopilot director",
            "autopilot/director",
            "autopilot-director!",
        ];
        invalid.push(&overlong);
        for slug in invalid {
            let error = validate_slug(slug).expect_err(&format!("{slug:?} should be rejected"));
            assert!(
                error.contains("--slug"),
                "error should name the flag: {error}"
            );
        }
    }
}
