//! Upstream drift check — compares the committed pin with the newest
//! release tag of the upstream repository.
//!
//! This is the testable core behind `scripts/upstream-drift.rs` and the
//! scheduled `.github/workflows/upstream-drift.yml`. It never mutates the
//! project: it reads `.upstream-pin.json`, asks the upstream repository for
//! its tags (`git ls-remote --tags --refs`), and describes the gap. Opening,
//! updating, or closing the tracking issue is the workflow's job; syncing is
//! always a human-reviewed step.
//!
//! The comparison is version-based: an upstream tag counts as *drift* only
//! when its `vMAJOR.MINOR.PATCH` release version is newer than the version
//! recorded in the pin. Tags that are not plain release versions (for
//! example the legacy `mattpocock-skills@1.2.3` tags or pre-release tags)
//! are ignored, as is a pin that is newer than any upstream release.

use std::fs;
use std::path::Path;
use std::process::Command;

use shared::{UpstreamPin, UPSTREAM_PIN_FILE};

/// Prefix of the single tracking issue the scheduled check maintains.
///
/// `scripts/upstream-drift.rs` embeds it in the issue title and the workflow
/// filters open issues by it, so changing it is a workflow change too.
pub const DRIFT_ISSUE_TITLE_PREFIX: &str = "upstream drift:";

// ── Release versions ───────────────────────────────────────────────────────

/// A parsed upstream release tag (`vMAJOR.MINOR.PATCH`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ReleaseVersion {
    /// Major component.
    pub major: u64,
    /// Minor component.
    pub minor: u64,
    /// Patch component.
    pub patch: u64,
}

impl ReleaseVersion {
    /// Parses `vMAJOR.MINOR.PATCH` (tags) or `MAJOR.MINOR.PATCH` (the
    /// upstream `package.json` version recorded in the pin). Returns `None`
    /// for anything else, including pre-release suffixes and the legacy
    /// scoped tags.
    pub fn parse(tag: &str) -> Option<Self> {
        let rest = tag.strip_prefix('v').unwrap_or(tag);
        let mut parts = rest.split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next()?.parse().ok()?;
        let patch = parts.next()?.parse().ok()?;
        if parts.next().is_some() {
            return None;
        }
        Some(Self {
            major,
            minor,
            patch,
        })
    }

    /// Renders the version back as the canonical `vMAJOR.MINOR.PATCH` tag.
    pub fn as_tag(&self) -> String {
        format!("v{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl std::fmt::Display for ReleaseVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

// ── Drift report ───────────────────────────────────────────────────────────

/// An upstream release tag that participates in the comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpstreamRelease {
    /// The tag as it appears upstream, e.g. `v1.3.2`.
    pub tag: String,
    /// Its parsed release version.
    pub version: ReleaseVersion,
}

/// The outcome of comparing the committed pin with the newest upstream
/// release tag.
#[derive(Debug)]
pub struct DriftCheck {
    /// The committed pin this check ran against.
    pub pinned: UpstreamPin,
    /// The pin's parsed release version.
    pub pinned_version: ReleaseVersion,
    /// The newest upstream release tag.
    pub latest: UpstreamRelease,
    /// True when `latest.version > pinned_version`.
    pub drifted: bool,
}

/// Extracts tag names from `git ls-remote --tags --refs` output lines of the
/// form `<sha>\trefs/tags/<tag>`.
pub fn parse_remote_tags(output: &str) -> Vec<String> {
    output
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .filter_map(|(_, reference)| reference.strip_prefix("refs/tags/"))
        .map(str::to_string)
        .collect()
}

/// Picks the newest `vMAJOR.MINOR.PATCH` tag, ignoring legacy and
/// pre-release tags that do not parse as a release version.
pub fn latest_release_tag(tags: &[String]) -> Option<UpstreamRelease> {
    tags.iter()
        .filter_map(|tag| {
            let version = ReleaseVersion::parse(tag)?;
            Some(UpstreamRelease {
                tag: tag.clone(),
                version,
            })
        })
        .max_by_key(|release| release.version)
}

/// Asks `repo_url` for its release tags via `git ls-remote --tags --refs`.
pub fn fetch_release_tags(repo_url: &str) -> Result<Vec<String>, String> {
    let output = Command::new("git")
        .args(["ls-remote", "--tags", "--refs", repo_url])
        .output()
        .map_err(|err| format!("cannot run git ls-remote: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "git ls-remote --tags {} failed: {}",
            repo_url,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(parse_remote_tags(&String::from_utf8_lossy(&output.stdout)))
}

/// Loads the committed `.upstream-pin.json` from `project_root`.
pub fn load_pin(project_root: &Path) -> Result<UpstreamPin, String> {
    let path = project_root.join(UPSTREAM_PIN_FILE);
    let bytes = fs::read(&path).map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            format!(
                "no {} at {} (run scripts/sync-upstream.rs first)",
                UPSTREAM_PIN_FILE,
                path.display()
            )
        } else {
            format!("cannot read {}: {err}", path.display())
        }
    })?;
    UpstreamPin::parse(&bytes)
}

/// Compares `pin` with the newest release tag in `tags`.
pub fn evaluate(pin: UpstreamPin, tags: &[String]) -> Result<DriftCheck, String> {
    let pinned_version = ReleaseVersion::parse(&pin.version).ok_or_else(|| {
        format!(
            "pin version '{}' is not vMAJOR.MINOR.PATCH; cannot compare it with upstream release tags",
            pin.version
        )
    })?;
    let latest = latest_release_tag(tags).ok_or_else(|| {
        format!(
            "no upstream release tag (vMAJOR.MINOR.PATCH) found among {} tag(s)",
            tags.len()
        )
    })?;
    let drifted = latest.version > pinned_version;
    Ok(DriftCheck {
        pinned: pin,
        pinned_version,
        latest,
        drifted,
    })
}

// ── Issue rendering ────────────────────────────────────────────────────────

/// Title of the tracking issue for `check`. It always starts with
/// [`DRIFT_ISSUE_TITLE_PREFIX`] so the workflow can find it again.
pub fn issue_title(check: &DriftCheck) -> String {
    format!(
        "{} {} released, pinned {}",
        DRIFT_ISSUE_TITLE_PREFIX, check.latest.tag, check.pinned.version
    )
}

/// Body of the tracking issue for `check`, updated in place on every run
/// while the drift persists.
pub fn issue_body(check: &DriftCheck) -> String {
    let pin = &check.pinned;
    let mut body = String::new();
    body.push_str("The vendored upstream snapshot is behind the newest release of `");
    body.push_str(&pin.source);
    body.push_str("`.\n\n");
    body.push_str("| | Pinned | Upstream |\n");
    body.push_str("| --- | --- | --- |\n");
    body.push_str(&format!(
        "| ref | `{}` | `{}` |\n",
        pin.ref_name, check.latest.tag
    ));
    body.push_str(&format!(
        "| version | `{}` | `{}` |\n",
        pin.version, check.latest.version
    ));
    body.push_str(&format!("| commit | `{}` | — |\n", pin.resolved_commit));
    body.push_str(&format!("| synced | {} | — |\n", pin.synced_at));
    body.push_str("\n### To resolve\n\n");
    body.push_str(&format!(
        "1. Review the upstream diff between `{}` and the `{}` tag.\n",
        pin.resolved_commit, check.latest.tag
    ));
    body.push_str(&format!(
        "2. On a clean worktree, run `rust-script scripts/sync-upstream.rs {}`.\n",
        check.latest.tag
    ));
    body.push_str(
        "3. If the sync's validation gate reports changed skill counts, update `AGENTS.md`, \
         `README.md`, and `GLOSSARY.md` to match and run the sync again.\n",
    );
    body.push_str(
        "4. Open a PR with the refreshed `skills/upstream/`, `.skill-lock.json`, \
         `.upstream-pin.json`, and the updated documentation counts.\n\n",
    );
    body.push_str(
        "The sync never pushes or merges by itself — this issue only says that it is due.\n\n",
    );
    body.push_str(
        "_Maintained by `.github/workflows/upstream-drift.yml`: while the drift persists this \
         issue is updated in place; once the pin catches up it is closed automatically. \
         Please do not open duplicates._\n",
    );
    body
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use shared::{UpstreamPin, UpstreamRefKind, UPSTREAM_PIN_SCHEMA_VERSION};
    use std::path::PathBuf;

    fn pin(version: &str) -> UpstreamPin {
        UpstreamPin {
            schema_version: UPSTREAM_PIN_SCHEMA_VERSION,
            source: "mattpocock/skills".to_string(),
            source_url: "https://github.com/mattpocock/skills.git".to_string(),
            ref_name: format!("v{version}"),
            ref_kind: UpstreamRefKind::Tag,
            resolved_commit: "d1478fcb140db0b93c0c8df17bcfd311844749d0".to_string(),
            version: version.to_string(),
            synced_at: "2026-11-07T10:09:13Z".to_string(),
            reason: None,
        }
    }

    fn tags(list: &[&str]) -> Vec<String> {
        list.iter().map(|tag| tag.to_string()).collect()
    }

    #[test]
    fn parses_release_versions_and_rejects_others() {
        assert_eq!(
            ReleaseVersion::parse("v1.3.2"),
            Some(ReleaseVersion {
                major: 1,
                minor: 3,
                patch: 2
            })
        );
        assert_eq!(ReleaseVersion::parse("v1.3.2").unwrap().as_tag(), "v1.3.2");
        assert_eq!(
            ReleaseVersion::parse("1.3.2"),
            ReleaseVersion::parse("v1.3.2")
        );
        for tag in [
            "v1.3",
            "v1.3.2.4",
            "v1.3.2-beta",
            "mattpocock-skills@1.2.3",
            "v",
            "not-a-version",
        ] {
            assert_eq!(ReleaseVersion::parse(tag), None, "tag {tag}");
        }
        assert!(
            ReleaseVersion::parse("v1.10.0").unwrap() > ReleaseVersion::parse("v1.9.9").unwrap()
        );
        assert!(
            ReleaseVersion::parse("v2.0.0").unwrap() > ReleaseVersion::parse("v1.99.99").unwrap()
        );
    }

    #[test]
    fn parses_remote_tag_lines() {
        let output = "\
d1478fcb140db0b93c0c8df17bcfd311844749d0\trefs/tags/v1.3.2
0123456789abcdef0123456789abcdef01234567\trefs/tags/v1.3.1
aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\trefs/tags/mattpocock-skills@1.2.3
";
        assert_eq!(
            parse_remote_tags(output),
            tags(&["v1.3.2", "v1.3.1", "mattpocock-skills@1.2.3"])
        );
    }

    #[test]
    fn picks_the_newest_release_ignoring_legacy_tags() {
        let latest = latest_release_tag(&tags(&[
            "v1.2.10",
            "v1.3.2",
            "v1.3.1",
            "mattpocock-skills@9.9.9",
            "v1.4.0-beta.1",
        ]))
        .expect("a release tag");
        assert_eq!(latest.tag, "v1.3.2");
        assert_eq!(
            latest.version,
            ReleaseVersion {
                major: 1,
                minor: 3,
                patch: 2
            }
        );
    }

    #[test]
    fn evaluates_drift_when_upstream_is_newer() {
        let check = evaluate(pin("1.3.2"), &tags(&["v1.3.1", "v1.3.2", "v1.3.3"])).unwrap();
        assert!(check.drifted);
        assert_eq!(check.latest.tag, "v1.3.3");
        assert_eq!(check.pinned_version.to_string(), "1.3.2");
    }

    #[test]
    fn reports_no_drift_when_pin_is_current_or_ahead() {
        let current = evaluate(pin("1.3.2"), &tags(&["v1.3.1", "v1.3.2"])).unwrap();
        assert!(!current.drifted);
        let ahead = evaluate(pin("1.4.0"), &tags(&["v1.3.2"])).unwrap();
        assert!(!ahead.drifted);
    }

    #[test]
    fn fails_when_the_pin_version_is_not_a_release_version() {
        let err = evaluate(pin("main"), &tags(&["v1.3.2"])).unwrap_err();
        assert!(err.contains("not vMAJOR.MINOR.PATCH"), "{err}");
    }

    #[test]
    fn fails_when_no_release_tag_exists() {
        let err = evaluate(pin("1.3.2"), &tags(&["mattpocock-skills@1.2.3"])).unwrap_err();
        assert!(err.contains("no upstream release tag"), "{err}");
    }

    #[test]
    fn issue_title_and_body_describe_the_gap() {
        let check = evaluate(pin("1.3.2"), &tags(&["v1.3.3"])).unwrap();
        let title = issue_title(&check);
        assert!(title.starts_with(DRIFT_ISSUE_TITLE_PREFIX), "{title}");
        assert!(title.contains("v1.3.3"), "{title}");
        assert!(title.contains("1.3.2"), "{title}");
        let body = issue_body(&check);
        assert!(body.contains("v1.3.3"), "{body}");
        assert!(
            body.contains("d1478fcb140db0b93c0c8df17bcfd311844749d0"),
            "{body}"
        );
        assert!(
            body.contains("rust-script scripts/sync-upstream.rs v1.3.3"),
            "{body}"
        );
    }

    #[test]
    fn title_is_stable_for_the_same_latest_tag() {
        let first = evaluate(pin("1.3.2"), &tags(&["v1.3.3"])).unwrap();
        let second = evaluate(pin("1.3.2"), &tags(&["v1.3.2", "v1.3.3"])).unwrap();
        assert_eq!(issue_title(&first), issue_title(&second));
    }

    #[test]
    fn loads_pin_and_reports_a_missing_one() {
        let dir = tempfile::tempdir().unwrap();
        let err = load_pin(dir.path()).unwrap_err();
        assert!(err.contains(UPSTREAM_PIN_FILE), "{err}");
        assert!(err.contains("scripts/sync-upstream.rs"), "{err}");

        std::fs::write(dir.path().join(UPSTREAM_PIN_FILE), pin("1.3.2").to_bytes()).unwrap();
        let loaded = load_pin(dir.path()).unwrap();
        assert_eq!(loaded.version, "1.3.2");
    }

    #[test]
    fn fetches_release_tags_from_a_local_repository() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("upstream");
        std::fs::create_dir_all(&repo).unwrap();
        let git = |args: &[&str]| {
            let output = Command::new("git")
                .args(args)
                .current_dir(&repo)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "git {:?} failed: {}",
                args,
                String::from_utf8_lossy(&output.stderr)
            );
            output
        };
        git(&["init", "--quiet"]);
        std::fs::write(repo.join("README.md"), "upstream\n").unwrap();
        git(&[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "add",
            ".",
        ]);
        git(&[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "--quiet",
            "-m",
            "fixture",
        ]);
        git(&["tag", "v1.2.0"]);
        git(&["tag", "v1.3.2"]);

        let repo_url: PathBuf = repo.clone();
        let found = fetch_release_tags(repo_url.to_str().unwrap()).unwrap();
        assert_eq!(found, tags(&["v1.2.0", "v1.3.2"]));
        assert_eq!(latest_release_tag(&found).unwrap().tag, "v1.3.2");
    }
}
