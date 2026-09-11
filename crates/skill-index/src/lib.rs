//! Skill discovery, classification, and manifest generation.
//!
//! Pure data — discovers skills from the source tree, classifies them
//! (agnostic vs. coupled with runtime variants), and generates the
//! manifest.json used by the tarball install pipeline.
//!
//! Public API:
//! - `discover_skills(project_root)` → `Result<Vec<ExpectedSetEntry>>`
//! - `classify_skill(skill_dir)` → `(SkillType, Vec<String>, bool)`
//! - `generate_manifest(skills, version)` → `Manifest`

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

// ── Public types ────────────────────────────────────────────────────────────

/// Whether a skill is runtime-agnostic or coupled (has per-runtime variants).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillType {
    /// Single SKILL.md, works on any runtime.
    Agnostic,
    /// Has per-runtime variant subdirectories (reasonix/, codex/, kimi/).
    Coupled,
}

/// Whether an Expected-set entry's source directory resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolutionStatus {
    /// The source directory exists.
    Resolved,
    /// The failed-entry state (CONTEXT.md): the provenance points at a
    /// directory that does not exist. The entry is still returned, with its
    /// reason.
    Missing { reason: String },
}

/// What a Skill file is: a skill body or a custom-agent definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillFileKind {
    /// A `SKILL.md` — the root fallback or a runtime variant's skill body.
    Skill,
    /// An `agent.toml` custom-agent definition shipped instead of a
    /// `SKILL.md` (see ADR 0036 / ADR 0045).
    AgentDefinition,
}

/// One Skill file an Expected-set entry owns (CONTEXT.md).
///
/// A variant that ships an `agent.toml` instead of a `SKILL.md` appears as a
/// resolved [`SkillFileKind::AgentDefinition`]; a variant directory carrying
/// neither is a missing `Skill` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillFile {
    /// The runtime variant directory this file lives under; `None` for the
    /// root fallback.
    pub variant: Option<String>,
    /// Absolute path of the file this entry owns (the expected path, even for
    /// a missing file).
    pub path: PathBuf,
    pub kind: SkillFileKind,
    /// Whether the file exists on disk.
    pub resolution: ResolutionStatus,
}

/// One Expected-set entry: a skill the toolkit owns.
#[derive(Debug, Clone)]
pub struct ExpectedSetEntry {
    pub name: String,
    /// "autopilot", "upstream", or "vendor".
    pub source: String,
    pub skill_type: SkillType,
    /// Variant directory names (e.g. ["codex", "kimi", "reasonix"]).
    /// Empty for agnostic skills.
    pub variants: Vec<String>,
    /// Whether a codex/agent.toml file exists (only meaningful for coupled skills).
    pub codex_agent: bool,
    /// The skill's source directory, joined to the project root. For a failed
    /// entry this is the expected location that was not found.
    pub source_dir: PathBuf,
    /// Whether `source_dir` exists. Retained for consumers that apply
    /// directory-level strictness; the skill-file list is authoritative for
    /// "which files does this entry own".
    pub resolution: ResolutionStatus,
    /// The Skill files this entry owns, in deterministic order: the root
    /// fallback first, then each runtime variant in runtime order.
    pub skill_files: Vec<SkillFile>,
}

impl ExpectedSetEntry {
    /// Whether any Skill file this entry owns is missing.
    pub fn is_failed(&self) -> bool {
        self.skill_files
            .iter()
            .any(|file| matches!(file.resolution, ResolutionStatus::Missing { .. }))
    }

    /// The Skill file for one variant; `None` addresses the root fallback.
    pub fn skill_file(&self, variant: Option<&str>) -> Option<&SkillFile> {
        self.skill_files
            .iter()
            .find(|file| file.variant.as_deref() == variant)
    }
}

/// A single skill entry in manifest.json.
#[derive(Debug, serde::Serialize)]
pub struct ManifestSkill {
    #[serde(rename = "type")]
    pub skill_type: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub variants: Vec<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub codex_agent: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// The manifest.json document.
#[derive(Debug, serde::Serialize)]
pub struct Manifest {
    pub version: String,
    pub skills: BTreeMap<String, ManifestSkill>,
}

// ── Runtime variant names ───────────────────────────────────────────────────

/// The known runtime variant directory names.
///
/// Single definition of the runtime variant list; consumers (deploy)
/// reference this const instead of hardcoding their own copies.
pub const RUNTIME_VARIANTS: &[&str] = &["codex", "kimi", "reasonix"];

// ── classify_skill ──────────────────────────────────────────────────────────

/// Classify a skill directory.
///
/// Returns `(SkillType, variants, codex_agent)`:
/// - `SkillType::Coupled` if any known variant subdirectory exists
/// - `SkillType::Agnostic` otherwise
/// - `variants`: sorted list of variant dir names that exist
/// - `codex_agent`: whether `codex/agent.toml` exists
pub fn classify_skill(skill_dir: &Path) -> (SkillType, Vec<String>, bool) {
    let mut variants: Vec<String> = Vec::new();
    for v in RUNTIME_VARIANTS {
        if skill_dir.join(v).is_dir() {
            variants.push(v.to_string());
        }
    }
    variants.sort();
    let codex_agent = skill_dir.join("codex").join("agent.toml").is_file();
    let skill_type = if variants.is_empty() {
        SkillType::Agnostic
    } else {
        SkillType::Coupled
    };
    (skill_type, variants, codex_agent)
}

// ── discover_skills ─────────────────────────────────────────────────────────

/// Discover the Expected set from the source tree.
///
/// Scans `skills/autopilot/` for autopilot (custom) skills, reads
/// `.vendor-lock.json` for third-party vendored skills, and reads
/// `.skill-lock.json` for upstream (vendored) skills.
///
/// Entries are returned in deterministic order — autopilot, then vendor, then
/// upstream, name-sorted within each group. A lock entry whose directory is
/// missing yields a failed entry (still returned, not omitted). A missing
/// lock file means the tree has no locked skills; a malformed lock file is an
/// error.
pub fn discover_skills(project_root: &Path) -> Result<Vec<ExpectedSetEntry>, anyhow::Error> {
    let mut entries: Vec<ExpectedSetEntry> = Vec::new();

    // ── Autopilot skills (directory scan; always resolved) ──
    let autopilot_dir = project_root.join("skills").join("autopilot");
    if autopilot_dir.is_dir() {
        for entry in std::fs::read_dir(&autopilot_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            entries.push(resolved_entry(name, "autopilot", entry.path()));
        }
    }

    // ── Vendor skills (lock-driven: provenance is mandatory) ──
    let vendor_lock_path = project_root.join(shared::VENDOR_LOCK_FILE);
    if vendor_lock_path.is_file() {
        let lock = shared::load_vendor_lock_at(project_root)
            .map_err(|e| anyhow::anyhow!("failed to parse {}: {e}", shared::VENDOR_LOCK_FILE))?;
        for skill in &lock.skills {
            let src_dir = project_root.join(skill.vendor_dir());
            if src_dir.is_dir() {
                entries.push(resolved_entry(skill.name.clone(), "vendor", src_dir));
            } else {
                let reason = format!("vendor skill directory not found: {}", src_dir.display());
                entries.push(failed_entry(skill.name.clone(), "vendor", src_dir, reason));
            }
        }
    }

    // ── Upstream skills (lock-driven) ──
    let upstream_lock_path = project_root.join(shared::SKILL_LOCK_FILE);
    if upstream_lock_path.is_file() {
        let lock = shared::load_skill_lock_at(project_root)
            .map_err(|e| anyhow::anyhow!("failed to parse {}: {e}", shared::SKILL_LOCK_FILE))?;
        for skill in &lock.skills {
            match skill.upstream_dir_rel() {
                Some(rel) => {
                    let src_dir = project_root.join(rel);
                    if src_dir.is_dir() {
                        // Upstream skills are runtime-agnostic by policy.
                        let skill_md = src_dir.join("SKILL.md");
                        entries.push(ExpectedSetEntry {
                            name: skill.name.clone(),
                            source: "upstream".to_string(),
                            skill_type: SkillType::Agnostic,
                            variants: vec![],
                            codex_agent: false,
                            source_dir: src_dir,
                            resolution: ResolutionStatus::Resolved,
                            skill_files: vec![skill_file(
                                None,
                                skill_md.clone(),
                                SkillFileKind::Skill,
                                &skill_md,
                            )],
                        });
                    } else {
                        let reason =
                            format!("upstream skill directory not found: {}", src_dir.display());
                        entries.push(failed_entry(
                            skill.name.clone(),
                            "upstream",
                            src_dir,
                            reason,
                        ));
                    }
                }
                None => {
                    // A malformed skillPath resolves to no directory; the entry
                    // is still returned as a failed entry, anchored at the
                    // upstream source root.
                    let reason = format!(
                        "malformed skillPath {:?}: expected a path ending in /SKILL.md",
                        skill.skill_path
                    );
                    entries.push(failed_entry(
                        skill.name.clone(),
                        "upstream",
                        project_root.join("skills").join("upstream"),
                        reason,
                    ));
                }
            }
        }
    }

    // Deterministic order: autopilot, then vendor, then upstream; name-sorted
    // within each group.
    entries.sort_by(|a, b| {
        source_rank(&a.source)
            .cmp(&source_rank(&b.source))
            .then_with(|| a.name.cmp(&b.name))
    });

    Ok(entries)
}

// ── Entry construction helpers ──────────────────────────────────────────────

/// Rank sources for the deterministic Expected-set order.
fn source_rank(source: &str) -> u8 {
    match source {
        "autopilot" => 0,
        "vendor" => 1,
        "upstream" => 2,
        _ => 3,
    }
}

/// Build a resolved entry by classifying the skill's source directory.
fn resolved_entry(name: String, source: &str, source_dir: PathBuf) -> ExpectedSetEntry {
    let (skill_type, variants, codex_agent) = classify_skill(&source_dir);
    let skill_files = skill_files_for(&source_dir, &variants);
    ExpectedSetEntry {
        name,
        source: source.to_string(),
        skill_type,
        variants,
        codex_agent,
        source_dir,
        resolution: ResolutionStatus::Resolved,
        skill_files,
    }
}

/// Enumerate the Skill files a source directory owns: the root fallback first,
/// then each runtime variant in runtime order (ADR-0045).
///
/// A variant that ships an `agent.toml` instead of a `SKILL.md` is an
/// [`SkillFileKind::AgentDefinition`]; a variant directory carrying neither is
/// a missing `Skill` file.
fn skill_files_for(source_dir: &Path, variants: &[String]) -> Vec<SkillFile> {
    let root = source_dir.join("SKILL.md");
    let mut files = vec![skill_file(None, root.clone(), SkillFileKind::Skill, &root)];
    for variant in variants {
        let variant_dir = source_dir.join(variant);
        let skill = variant_dir.join("SKILL.md");
        if skill.is_file() {
            files.push(skill_file(
                Some(variant.clone()),
                skill.clone(),
                SkillFileKind::Skill,
                &skill,
            ));
            continue;
        }
        let agent = variant_dir.join("agent.toml");
        if agent.is_file() {
            files.push(skill_file(
                Some(variant.clone()),
                agent.clone(),
                SkillFileKind::AgentDefinition,
                &agent,
            ));
        } else {
            files.push(SkillFile {
                variant: Some(variant.clone()),
                path: skill.clone(),
                kind: SkillFileKind::Skill,
                resolution: ResolutionStatus::Missing {
                    reason: format!(
                        "variant '{}' ships neither SKILL.md nor agent.toml: {}",
                        variant,
                        variant_dir.display()
                    ),
                },
            });
        }
    }
    files
}

/// Build one Skill file by checking the file on disk.
fn skill_file(
    variant: Option<String>,
    path: PathBuf,
    kind: SkillFileKind,
    present: &Path,
) -> SkillFile {
    let resolution = if present.is_file() {
        ResolutionStatus::Resolved
    } else {
        ResolutionStatus::Missing {
            reason: format!("skill file not found: {}", present.display()),
        }
    };
    SkillFile {
        variant,
        path,
        kind,
        resolution,
    }
}

/// Build a failed entry for provenance that points at a missing directory.
///
/// A missing directory cannot be classified, so the type metadata defaults to
/// agnostic with no variants.
fn failed_entry(
    name: String,
    source: &str,
    source_dir: PathBuf,
    reason: String,
) -> ExpectedSetEntry {
    let root_file = source_dir.join("SKILL.md");
    ExpectedSetEntry {
        name,
        source: source.to_string(),
        skill_type: SkillType::Agnostic,
        variants: vec![],
        codex_agent: false,
        source_dir,
        resolution: ResolutionStatus::Missing {
            reason: reason.clone(),
        },
        skill_files: vec![SkillFile {
            variant: None,
            path: root_file,
            kind: SkillFileKind::Skill,
            resolution: ResolutionStatus::Missing { reason },
        }],
    }
}

// ── generate_manifest ───────────────────────────────────────────────────────

/// Generate a manifest.json document from discovered skills.
///
/// Failed entries are skipped: they have no shipped skill directory, so they
/// must not invent manifest entries.
pub fn generate_manifest(skills: &[ExpectedSetEntry], version: &str) -> Manifest {
    let mut map = BTreeMap::new();
    for skill in skills {
        if !matches!(skill.resolution, ResolutionStatus::Resolved) {
            continue;
        }
        let skill_type_str = match skill.skill_type {
            SkillType::Agnostic if skill.source == "upstream" => "upstream",
            SkillType::Agnostic if skill.source == "vendor" => "vendor",
            SkillType::Agnostic => "agnostic",
            SkillType::Coupled => "coupled",
        };
        map.insert(
            skill.name.clone(),
            ManifestSkill {
                skill_type: skill_type_str.to_string(),
                variants: skill.variants.clone(),
                codex_agent: skill.codex_agent,
            },
        );
    }
    Manifest {
        version: version.to_string(),
        skills: map,
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_agnostic_skill() {
        // A directory with just SKILL.md (no variant subdirs) is agnostic
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("SKILL.md"), "---\nname: test\n---\n").unwrap();
        let (skill_type, variants, codex_agent) = classify_skill(tmp.path());
        assert_eq!(skill_type, SkillType::Agnostic);
        assert!(variants.is_empty());
        assert!(!codex_agent);
    }

    #[test]
    fn classify_coupled_skill() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("reasonix")).unwrap();
        std::fs::write(
            tmp.path().join("reasonix").join("SKILL.md"),
            "---\nname: test\n---\n",
        )
        .unwrap();
        std::fs::create_dir(tmp.path().join("kimi")).unwrap();
        std::fs::write(
            tmp.path().join("kimi").join("SKILL.md"),
            "---\nname: test\n---\n",
        )
        .unwrap();
        let (skill_type, variants, codex_agent) = classify_skill(tmp.path());
        assert_eq!(skill_type, SkillType::Coupled);
        assert_eq!(variants, vec!["kimi", "reasonix"]);
        assert!(!codex_agent);
    }

    #[test]
    fn classify_coupled_with_codex_agent() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("codex")).unwrap();
        std::fs::write(
            tmp.path().join("codex").join("agent.toml"),
            "[agent]\nname = \"test\"\n",
        )
        .unwrap();
        let (skill_type, variants, codex_agent) = classify_skill(tmp.path());
        assert_eq!(skill_type, SkillType::Coupled);
        assert_eq!(variants, vec!["codex"]);
        assert!(codex_agent);
    }

    #[test]
    fn classify_codex_skill_md_without_agent_toml() {
        // codex/SKILL.md without agent.toml — still coupled but codex_agent = false
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("codex")).unwrap();
        std::fs::write(
            tmp.path().join("codex").join("SKILL.md"),
            "---\nname: test\n---\n",
        )
        .unwrap();
        let (skill_type, variants, codex_agent) = classify_skill(tmp.path());
        assert_eq!(skill_type, SkillType::Coupled);
        assert_eq!(variants, vec!["codex"]);
        assert!(!codex_agent);
    }

    // ── Expected-set enumeration fixtures ───────────────────────────────

    fn write_skill_md(dir: &Path) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(
            dir.join("SKILL.md"),
            "---\nname: test\ndescription: test\n---\n",
        )
        .unwrap();
    }

    fn write_upstream_lock(root: &Path, entries: &[(&str, &str)]) {
        let skills = entries
            .iter()
            .map(|(name, path)| {
                format!(
                    r#""{name}": {{"sourceType": "github", "skillPath": "{path}", "skillFolderHash": "abc123"}}"#
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        std::fs::write(
            root.join(".skill-lock.json"),
            format!(r#"{{"version": 4, "skills": {{{skills}}}, "dismissed": {{}}}}"#),
        )
        .unwrap();
    }

    fn write_vendor_lock(root: &Path, entries: &[(&str, &str)]) {
        let skills = entries
            .iter()
            .map(|(name, vendor_path)| {
                format!(
                    r#""{name}": {{"sourceType": "github", "skillPath": "plugins/{name}/skills/{name}/SKILL.md", "skillFolderHash": "abc123", "vendorPath": "{vendor_path}"}}"#
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        std::fs::write(
            root.join(".vendor-lock.json"),
            format!(r#"{{"version": 1, "skills": {{{skills}}}}}"#),
        )
        .unwrap();
    }

    #[test]
    fn expected_set_entries_carry_resolved_dirs_and_status() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // Autopilot: one agnostic skill, one coupled skill with a codex agent.
        write_skill_md(&root.join("skills/autopilot/alpha"));
        write_skill_md(&root.join("skills/autopilot/beta"));
        write_skill_md(&root.join("skills/autopilot/beta/reasonix"));
        std::fs::create_dir_all(root.join("skills/autopilot/beta/codex")).unwrap();
        std::fs::write(
            root.join("skills/autopilot/beta/codex/agent.toml"),
            "[agent]\nname = \"beta\"\n",
        )
        .unwrap();

        // Vendor + upstream locks.
        write_skill_md(&root.join("skills/vendor/show-me"));
        write_vendor_lock(root, &[("show-me", "skills/vendor/show-me")]);
        write_skill_md(&root.join("skills/upstream/skills/engineering/tdd"));
        write_upstream_lock(root, &[("tdd", "skills/engineering/tdd/SKILL.md")]);

        let entries = discover_skills(root).unwrap();
        assert_eq!(entries.len(), 4);

        let alpha = entries.iter().find(|e| e.name == "alpha").unwrap();
        assert_eq!(alpha.source, "autopilot");
        assert_eq!(alpha.skill_type, SkillType::Agnostic);
        assert!(alpha.variants.is_empty());
        assert!(!alpha.codex_agent);
        assert_eq!(alpha.resolution, ResolutionStatus::Resolved);
        assert_eq!(alpha.source_dir, root.join("skills/autopilot/alpha"));

        let beta = entries.iter().find(|e| e.name == "beta").unwrap();
        assert_eq!(beta.skill_type, SkillType::Coupled);
        assert_eq!(beta.variants, vec!["codex", "reasonix"]);
        assert!(beta.codex_agent);
        assert_eq!(beta.source_dir, root.join("skills/autopilot/beta"));

        let show_me = entries.iter().find(|e| e.name == "show-me").unwrap();
        assert_eq!(show_me.source, "vendor");
        assert_eq!(show_me.skill_type, SkillType::Agnostic);
        assert_eq!(show_me.resolution, ResolutionStatus::Resolved);
        assert_eq!(show_me.source_dir, root.join("skills/vendor/show-me"));

        let tdd = entries.iter().find(|e| e.name == "tdd").unwrap();
        assert_eq!(tdd.source, "upstream");
        assert_eq!(tdd.skill_type, SkillType::Agnostic);
        assert_eq!(tdd.resolution, ResolutionStatus::Resolved);
        assert_eq!(
            tdd.source_dir,
            root.join("skills/upstream/skills/engineering/tdd")
        );
    }

    #[test]
    fn vendor_enumeration_is_lock_driven() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        write_skill_md(&root.join("skills/vendor/show-me"));
        // Orphan directory without a lock entry — must be ignored.
        write_skill_md(&root.join("skills/vendor/orphan"));
        write_vendor_lock(root, &[("show-me", "skills/vendor/show-me")]);

        let entries = discover_skills(root).unwrap();
        assert_eq!(entries.len(), 1, "only locked vendor skills are discovered");
        assert_eq!(entries[0].name, "show-me");
        assert_eq!(entries[0].source, "vendor");
        assert_eq!(entries[0].source_dir, root.join("skills/vendor/show-me"));
    }

    #[test]
    fn missing_vendor_directory_yields_failed_entry() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_vendor_lock(root, &[("ghost", "skills/vendor/ghost")]);

        let entries = discover_skills(root).unwrap();
        let ghost = entries
            .iter()
            .find(|e| e.name == "ghost")
            .expect("a failed entry must still be returned");
        assert_eq!(ghost.source, "vendor");
        assert_eq!(ghost.source_dir, root.join("skills/vendor/ghost"));
        match &ghost.resolution {
            ResolutionStatus::Missing { reason } => assert!(
                reason.contains("ghost"),
                "reason should name the missing location, got: {reason}"
            ),
            ResolutionStatus::Resolved => panic!("expected a failed entry"),
        }
    }

    #[test]
    fn missing_upstream_directory_yields_failed_entry() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_upstream_lock(root, &[("gone", "skills/engineering/gone/SKILL.md")]);

        let entries = discover_skills(root).unwrap();
        let gone = entries
            .iter()
            .find(|e| e.name == "gone")
            .expect("a failed entry must still be returned");
        assert_eq!(gone.source, "upstream");
        assert_eq!(
            gone.source_dir,
            root.join("skills/upstream/skills/engineering/gone")
        );
        match &gone.resolution {
            ResolutionStatus::Missing { reason } => assert!(
                reason.contains("gone"),
                "reason should name the missing location, got: {reason}"
            ),
            ResolutionStatus::Resolved => panic!("expected a failed entry"),
        }
    }

    #[test]
    fn missing_locks_yield_no_locked_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_skill_md(&root.join("skills/autopilot/alpha"));

        let entries = discover_skills(root).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].source, "autopilot");
    }

    #[test]
    fn malformed_upstream_lock_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::write(root.join(".skill-lock.json"), "{ not json").unwrap();

        let err = discover_skills(root).unwrap_err().to_string();
        assert!(err.contains(".skill-lock.json"), "got: {err}");
    }

    #[test]
    fn malformed_vendor_lock_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::write(root.join(".vendor-lock.json"), "{ not json").unwrap();

        let err = discover_skills(root).unwrap_err().to_string();
        assert!(err.contains(".vendor-lock.json"), "got: {err}");
    }

    #[test]
    fn expected_set_order_is_deterministic() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // Created in an order that differs from the required output order.
        write_skill_md(&root.join("skills/autopilot/zeta"));
        write_skill_md(&root.join("skills/autopilot/alpha"));
        write_skill_md(&root.join("skills/vendor/vendor-z"));
        write_skill_md(&root.join("skills/vendor/vendor-a"));
        write_skill_md(&root.join("skills/upstream/skills/upstream-b"));
        write_skill_md(&root.join("skills/upstream/skills/upstream-a"));
        write_vendor_lock(
            root,
            &[
                ("vendor-z", "skills/vendor/vendor-z"),
                ("vendor-a", "skills/vendor/vendor-a"),
            ],
        );
        write_upstream_lock(
            root,
            &[
                ("upstream-b", "skills/upstream-b/SKILL.md"),
                ("upstream-a", "skills/upstream-a/SKILL.md"),
            ],
        );

        let names: Vec<String> = discover_skills(root)
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert_eq!(
            names,
            vec![
                "alpha",
                "zeta",
                "vendor-a",
                "vendor-z",
                "upstream-a",
                "upstream-b"
            ],
            "order must be autopilot, vendor, upstream — name-sorted within each group"
        );
    }

    // ── Skill files (ADR-0045) ──────────────────────────────────────────

    #[test]
    fn entry_skill_files_carry_kind_variant_and_resolution() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // Agnostic autopilot skill: exactly one skill file, the root fallback.
        write_skill_md(&root.join("skills/autopilot/alpha"));

        // Coupled autopilot skill: root + a reasonix skill file + a codex
        // variant that ships an agent definition instead.
        let beta = root.join("skills/autopilot/beta");
        write_skill_md(&beta);
        write_skill_md(&beta.join("reasonix"));
        std::fs::create_dir_all(beta.join("codex")).unwrap();
        std::fs::write(beta.join("codex/agent.toml"), "[agent]\nname = \"beta\"\n").unwrap();

        // Vendor and upstream skills carry their root skill file.
        write_skill_md(&root.join("skills/vendor/show-me"));
        write_vendor_lock(root, &[("show-me", "skills/vendor/show-me")]);
        write_skill_md(&root.join("skills/upstream/skills/engineering/tdd"));
        write_upstream_lock(root, &[("tdd", "skills/engineering/tdd/SKILL.md")]);

        let entries = discover_skills(root).unwrap();

        let alpha = entries.iter().find(|e| e.name == "alpha").unwrap();
        assert_eq!(alpha.skill_files.len(), 1);
        let alpha_root = alpha.skill_file(None).expect("root skill file");
        assert_eq!(alpha_root.kind, SkillFileKind::Skill);
        assert_eq!(alpha_root.resolution, ResolutionStatus::Resolved);
        assert_eq!(
            alpha_root.path,
            root.join("skills/autopilot/alpha/SKILL.md")
        );
        assert!(!alpha.is_failed());

        let beta_entry = entries.iter().find(|e| e.name == "beta").unwrap();
        assert_eq!(
            beta_entry.skill_files.len(),
            3,
            "root + reasonix + codex agent definition: {:?}",
            beta_entry.skill_files
        );
        let codex_file = beta_entry.skill_file(Some("codex")).expect("codex file");
        assert_eq!(codex_file.kind, SkillFileKind::AgentDefinition);
        assert_eq!(
            codex_file.path,
            root.join("skills/autopilot/beta/codex/agent.toml")
        );
        assert_eq!(codex_file.resolution, ResolutionStatus::Resolved);
        let reasonix_file = beta_entry
            .skill_file(Some("reasonix"))
            .expect("reasonix file");
        assert_eq!(reasonix_file.kind, SkillFileKind::Skill);
        assert_eq!(
            reasonix_file.path,
            root.join("skills/autopilot/beta/reasonix/SKILL.md")
        );

        let show_me = entries.iter().find(|e| e.name == "show-me").unwrap();
        assert_eq!(show_me.skill_files.len(), 1);
        assert_eq!(
            show_me.skill_file(None).unwrap().path,
            root.join("skills/vendor/show-me/SKILL.md")
        );

        let tdd = entries.iter().find(|e| e.name == "tdd").unwrap();
        assert_eq!(tdd.skill_files.len(), 1);
        assert_eq!(
            tdd.skill_file(None).unwrap().path,
            root.join("skills/upstream/skills/engineering/tdd/SKILL.md")
        );
    }

    #[test]
    fn skill_files_are_ordered_root_first_then_runtime_order() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let skill = root.join("skills/autopilot/omega");
        write_skill_md(&skill);
        for variant in RUNTIME_VARIANTS {
            write_skill_md(&skill.join(variant));
        }

        let entries = discover_skills(root).unwrap();
        let variants: Vec<Option<&str>> = entries[0]
            .skill_files
            .iter()
            .map(|file| file.variant.as_deref())
            .collect();

        assert_eq!(
            variants,
            vec![None, Some("codex"), Some("kimi"), Some("reasonix")],
            "root fallback first, then variants in runtime order"
        );
    }

    #[test]
    fn missing_source_directory_reports_a_missing_root_skill_file() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_vendor_lock(root, &[("ghost", "skills/vendor/ghost")]);

        let entries = discover_skills(root).unwrap();
        let ghost = entries
            .iter()
            .find(|e| e.name == "ghost")
            .expect("a failed entry must still be returned");

        assert_eq!(ghost.skill_files.len(), 1);
        let root_file = ghost.skill_file(None).expect("root skill file");
        assert_eq!(root_file.kind, SkillFileKind::Skill);
        match &root_file.resolution {
            ResolutionStatus::Missing { reason } => assert!(
                reason.contains("ghost"),
                "reason should name the missing location, got: {reason}"
            ),
            ResolutionStatus::Resolved => panic!("expected a missing root skill file"),
        }
        assert!(ghost.is_failed());
    }

    #[test]
    fn missing_root_skill_md_reports_a_missing_skill_file() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // The directory exists and is coupled, but the root fallback file is gone.
        let broken = root.join("skills/autopilot/broken");
        write_skill_md(&broken.join("reasonix"));
        write_skill_md(&broken.join("codex"));

        let entries = discover_skills(root).unwrap();
        let entry = entries.iter().find(|e| e.name == "broken").unwrap();

        match &entry.skill_file(None).expect("root skill file").resolution {
            ResolutionStatus::Missing { reason } => assert!(
                reason.contains("SKILL.md"),
                "reason should name the missing file, got: {reason}"
            ),
            ResolutionStatus::Resolved => panic!("expected a missing root skill file"),
        }
        // The variant files are still present and resolved.
        assert_eq!(
            entry.skill_file(Some("reasonix")).unwrap().resolution,
            ResolutionStatus::Resolved
        );
        assert!(entry.is_failed());
    }

    #[test]
    fn malformed_skill_path_reports_a_missing_root_skill_file() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_upstream_lock(root, &[("odd", "skills/engineering/odd/README.md")]);

        let entries = discover_skills(root).unwrap();
        let odd = entries.iter().find(|e| e.name == "odd").unwrap();

        assert_eq!(odd.skill_files.len(), 1);
        match &odd.skill_file(None).expect("root skill file").resolution {
            ResolutionStatus::Missing { reason } => assert!(
                reason.contains("README.md"),
                "reason should name the malformed path, got: {reason}"
            ),
            ResolutionStatus::Resolved => panic!("expected a missing root skill file"),
        }
    }

    #[test]
    fn variant_without_skill_file_reports_a_missing_skill_file() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let skill = root.join("skills/autopilot/placeholder");
        write_skill_md(&skill);
        // A variant directory with neither SKILL.md nor agent.toml.
        std::fs::create_dir_all(skill.join("codex")).unwrap();

        let entries = discover_skills(root).unwrap();
        let entry = entries.iter().find(|e| e.name == "placeholder").unwrap();
        let codex_file = entry.skill_file(Some("codex")).expect("codex file");
        assert_eq!(codex_file.kind, SkillFileKind::Skill);
        match &codex_file.resolution {
            ResolutionStatus::Missing { reason } => assert!(
                reason.contains("codex"),
                "reason should name the placeholder directory, got: {reason}"
            ),
            ResolutionStatus::Resolved => panic!("expected a missing skill file"),
        }
        assert!(entry.is_failed());
    }

    fn entry(
        name: &str,
        source: &str,
        skill_type: SkillType,
        variants: &[&str],
        codex_agent: bool,
    ) -> ExpectedSetEntry {
        let source_dir = PathBuf::from(format!("/fixture/{name}"));
        let skill_files: Vec<SkillFile> = std::iter::once(None)
            .chain(variants.iter().map(|variant| Some(variant.to_string())))
            .map(|variant| {
                let dir = match &variant {
                    Some(variant) => source_dir.join(variant),
                    None => source_dir.clone(),
                };
                SkillFile {
                    variant,
                    path: dir.join("SKILL.md"),
                    kind: SkillFileKind::Skill,
                    resolution: ResolutionStatus::Resolved,
                }
            })
            .collect();
        ExpectedSetEntry {
            name: name.to_string(),
            source: source.to_string(),
            skill_type,
            variants: variants.iter().map(|v| v.to_string()).collect(),
            codex_agent,
            source_dir,
            resolution: ResolutionStatus::Resolved,
            skill_files,
        }
    }

    #[test]
    fn generate_manifest_includes_all_skills() {
        let skills = vec![
            entry(
                "toolkit-setup",
                "autopilot",
                SkillType::Agnostic,
                &[],
                false,
            ),
            entry(
                "autopilot-orchestrator",
                "autopilot",
                SkillType::Coupled,
                &["codex", "kimi", "reasonix"],
                false,
            ),
            entry(
                "autopilot-implementer",
                "autopilot",
                SkillType::Coupled,
                &["kimi", "reasonix"],
                true,
            ),
            entry("tdd", "upstream", SkillType::Agnostic, &[], false),
            entry("show-me", "vendor", SkillType::Agnostic, &[], false),
        ];
        let manifest = generate_manifest(&skills, "abc123");
        assert_eq!(manifest.version, "abc123");
        assert_eq!(manifest.skills.len(), 5);

        let toolkit = &manifest.skills["toolkit-setup"];
        assert_eq!(toolkit.skill_type, "agnostic");
        assert!(toolkit.variants.is_empty());

        let tdd = &manifest.skills["tdd"];
        assert_eq!(tdd.skill_type, "upstream");

        let show_me = &manifest.skills["show-me"];
        assert_eq!(show_me.skill_type, "vendor");
        assert!(show_me.variants.is_empty());

        let orch = &manifest.skills["autopilot-orchestrator"];
        assert_eq!(orch.skill_type, "coupled");
        assert_eq!(orch.variants.len(), 3);
        assert!(!orch.codex_agent);

        let impler = &manifest.skills["autopilot-implementer"];
        assert_eq!(impler.skill_type, "coupled");
        assert!(impler.codex_agent);
    }

    #[test]
    fn generate_manifest_skips_failed_entries() {
        let skills = vec![
            entry("kept", "autopilot", SkillType::Agnostic, &[], false),
            ExpectedSetEntry {
                name: "ghost".to_string(),
                source: "vendor".to_string(),
                skill_type: SkillType::Agnostic,
                variants: vec![],
                codex_agent: false,
                source_dir: PathBuf::from("/missing/ghost"),
                resolution: ResolutionStatus::Missing {
                    reason: "directory not found: /missing/ghost".to_string(),
                },
                skill_files: vec![SkillFile {
                    variant: None,
                    path: PathBuf::from("/missing/ghost/SKILL.md"),
                    kind: SkillFileKind::Skill,
                    resolution: ResolutionStatus::Missing {
                        reason: "directory not found: /missing/ghost".to_string(),
                    },
                }],
            },
        ];
        let manifest = generate_manifest(&skills, "v1");
        assert_eq!(manifest.skills.len(), 1);
        assert!(manifest.skills.contains_key("kept"));
        assert!(
            !manifest.skills.contains_key("ghost"),
            "failed entries must not invent manifest entries"
        );
    }

    #[test]
    fn discover_skills_in_this_repo() {
        // Integration-style: discover skills from the actual repo root
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let skills = discover_skills(root).unwrap();
        // We should have at least the 7 autopilot skills + upstream + vendor skills
        assert!(
            skills.len() >= 20,
            "expected >= 20 skills, got {}",
            skills.len()
        );

        // toolkit-setup should be agnostic
        let ts = skills
            .iter()
            .find(|s| s.name == "toolkit-setup")
            .expect("toolkit-setup not found");
        assert_eq!(ts.skill_type, SkillType::Agnostic);

        // show-me should be a vendor skill
        let show_me = skills
            .iter()
            .find(|s| s.name == "show-me")
            .expect("show-me not found");
        assert_eq!(show_me.source, "vendor");
        assert_eq!(show_me.skill_type, SkillType::Agnostic);

        // autopilot-orchestrator should be coupled with reasonix/kimi/codex
        let orch = skills
            .iter()
            .find(|s| s.name == "autopilot-orchestrator")
            .expect("autopilot-orchestrator not found");
        assert_eq!(orch.skill_type, SkillType::Coupled);
        assert!(orch.variants.contains(&"reasonix".to_string()));
        assert!(orch.variants.contains(&"kimi".to_string()));
    }
}
