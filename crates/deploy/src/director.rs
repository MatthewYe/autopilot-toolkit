//! Director CLI artifact build support.
//!
//! The `director-cli` counterpart of the Distill build: same targets, same
//! staging layout, one shared implementation in [`crate::artifacts`].

use std::collections::BTreeMap;
use std::path::Path;

/// Build Director CLI executables for the selected release platforms.
pub fn director_artifacts_command(
    project_root: &Path,
    platform_filter: Option<&str>,
) -> Result<(), anyhow::Error> {
    crate::artifacts::artifacts_command(project_root, director_spec(), platform_filter)
}

/// Stage the Director CLI executables into the tarball staging directory.
pub fn stage_director_executables(
    project_root: &Path,
    autopilot_staging: &Path,
) -> Result<BTreeMap<String, String>, anyhow::Error> {
    crate::artifacts::stage_executables(project_root, director_spec(), autopilot_staging)
}

fn director_spec() -> &'static crate::artifacts::CliSpec {
    crate::artifacts::spec("director").expect("director is a shipped CLI")
}
