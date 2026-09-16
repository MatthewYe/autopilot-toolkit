//! Distill CLI artifact build support.
//!
//! The Distill CLI is the first of the shipped CLIs; the machinery lives in
//! [`crate::artifacts`] and is shared with the Director CLI.

use std::collections::BTreeMap;
use std::path::Path;

/// Build Distill CLI executables for the selected release platforms.
///
/// `platform_filter` is an optional comma-separated list of platform names;
/// `None` builds all platforms. An unknown platform name is an error and
/// nothing is built.
pub fn distill_artifacts_command(
    project_root: &Path,
    platform_filter: Option<&str>,
) -> Result<(), anyhow::Error> {
    crate::artifacts::artifacts_command(project_root, distill_spec(), platform_filter)
}

/// Stage distill executables into the autopilot staging directory for tarball inclusion.
/// Returns a map of platform → relative archive path.
pub fn stage_distill_executables(
    project_root: &Path,
    autopilot_staging: &Path,
) -> Result<BTreeMap<String, String>, anyhow::Error> {
    crate::artifacts::stage_executables(project_root, distill_spec(), autopilot_staging)
}

fn distill_spec() -> &'static crate::artifacts::CliSpec {
    crate::artifacts::spec("distill").expect("distill is a shipped CLI")
}
