//! Skill discovery, classification, and manifest generation.
//!
//! Pure data — discovers skills from the source tree, classifies them
//! (agnostic vs. coupled with runtime variants), and generates the
//! manifest.json used by the tarball install pipeline.
//!
//! Public API:
//! - `discover_skills(project_root)` → `Result<Vec<DiscoveredSkill>>`
//! - `discover_vendor_skill_dirs(project_root)` → lock-driven vendor directories
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

/// A skill discovered from the source tree.
#[derive(Debug, Clone)]
pub struct DiscoveredSkill {
    pub name: String,
    /// "autopilot", "upstream", or "vendor".
    pub source: String,
    pub skill_type: SkillType,
    /// Variant directory names (e.g. ["codex", "kimi", "reasonix"]).
    /// Empty for agnostic skills.
    pub variants: Vec<String>,
    /// Whether a codex/agent.toml file exists (only meaningful for coupled skills).
    pub codex_agent: bool,
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
const RUNTIME_VARIANTS: &[&str] = &["codex", "kimi", "reasonix"];

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

// ── discover_vendor_skill_dirs ──────────────────────────────────────────────

/// Resolve vendor skill directories from `.vendor-lock.json`.
///
/// The vendor lock is the source of truth for which vendor skills belong to
/// the toolkit: a directory without a lock entry is an orphan and is ignored.
/// A missing vendor lock yields an empty list (no vendor skills installed).
pub fn discover_vendor_skill_dirs(
    project_root: &Path,
) -> Result<Vec<(String, PathBuf)>, anyhow::Error> {
    if !project_root.join(shared::VENDOR_LOCK_FILE).is_file() {
        return Ok(Vec::new());
    }

    let lock = shared::load_vendor_lock_at(project_root).map_err(|e| anyhow::anyhow!(e))?;
    let mut entries: Vec<(String, PathBuf)> = Vec::new();
    for skill in &lock.skills {
        let dir = project_root.join(skill.vendor_dir());
        if dir.is_dir() {
            entries.push((skill.name.clone(), dir));
        }
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(entries)
}

// ── discover_skills ─────────────────────────────────────────────────────────

/// Discover all skills from the source tree.
///
/// Scans `skills/autopilot/` for autopilot (custom) skills, reads
/// `.vendor-lock.json` for third-party vendored skills, and reads
/// `.skill-lock.json` for upstream (vendored) skills.
pub fn discover_skills(project_root: &Path) -> Result<Vec<DiscoveredSkill>, anyhow::Error> {
    let mut skills: Vec<DiscoveredSkill> = Vec::new();

    // ── Autopilot skills ──
    let autopilot_dir = project_root.join("skills").join("autopilot");
    if autopilot_dir.is_dir() {
        for entry in std::fs::read_dir(&autopilot_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            let src_dir = entry.path();

            let (skill_type, variants, codex_agent) = classify_skill(&src_dir);
            skills.push(DiscoveredSkill {
                name,
                source: "autopilot".to_string(),
                skill_type,
                variants,
                codex_agent,
            });
        }
    }

    // ── Vendor skills (lock-driven: provenance is mandatory) ──
    for (name, src_dir) in discover_vendor_skill_dirs(project_root)? {
        let (skill_type, variants, codex_agent) = classify_skill(&src_dir);
        skills.push(DiscoveredSkill {
            name,
            source: "vendor".to_string(),
            skill_type,
            variants,
            codex_agent,
        });
    }

    // ── Upstream skills (from .skill-lock.json) ──
    // Delegate parsing to the shared crate for a single source of truth.
    match shared::load_skill_lock() {
        Ok(lock) => {
            for skill in &lock.skills {
                // Check that the source directory exists before adding to the index
                let src_parent = Path::new(&skill.skill_path)
                    .parent()
                    .unwrap_or(Path::new(""));
                let src_dir = project_root
                    .join("skills")
                    .join("upstream")
                    .join(src_parent);
                if !src_dir.is_dir() {
                    continue;
                }
                skills.push(DiscoveredSkill {
                    name: skill.name.clone(),
                    source: "upstream".to_string(),
                    skill_type: SkillType::Agnostic,
                    variants: vec![],
                    codex_agent: false,
                });
            }
        }
        Err(e) => {
            // Treat a missing lock file as non-fatal (upstream skills simply omitted).
            // Parse / IO errors are surfaced.
            let lock_path = project_root.join(".skill-lock.json");
            if !lock_path.is_file() {
                // lock file absent — no upstream skills to discover
            } else {
                return Err(anyhow::anyhow!("failed to parse .skill-lock.json: {e}"));
            }
        }
    }

    // Sort by name for deterministic output
    skills.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(skills)
}

// ── generate_manifest ───────────────────────────────────────────────────────

/// Generate a manifest.json document from discovered skills.
pub fn generate_manifest(skills: &[DiscoveredSkill], version: &str) -> Manifest {
    let mut map = BTreeMap::new();
    for skill in skills {
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

    #[test]
    fn discover_vendor_skill_dirs_is_lock_driven() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        std::fs::create_dir_all(root.join("skills/vendor/show-me")).unwrap();
        std::fs::write(
            root.join("skills/vendor/show-me/SKILL.md"),
            "---\nname: show-me\ndescription: test\n---\n",
        )
        .unwrap();

        // Orphan directory without a lock entry — must be ignored.
        std::fs::create_dir_all(root.join("skills/vendor/orphan")).unwrap();
        std::fs::write(
            root.join("skills/vendor/orphan/SKILL.md"),
            "---\nname: orphan\ndescription: test\n---\n",
        )
        .unwrap();

        std::fs::write(
            root.join(".vendor-lock.json"),
            r#"{
                "version": 1,
                "skills": {
                    "show-me": {
                        "skillPath": "plugins/show-me/skills/show-me/SKILL.md",
                        "skillFolderHash": "abc123",
                        "vendorPath": "skills/vendor/show-me"
                    }
                }
            }"#,
        )
        .unwrap();

        let entries = discover_vendor_skill_dirs(root).unwrap();
        assert_eq!(entries.len(), 1, "only locked vendor skills are discovered");
        assert_eq!(entries[0].0, "show-me");
        assert!(entries[0].1.ends_with("skills/vendor/show-me"));
    }

    #[test]
    fn discover_vendor_skill_dirs_missing_lock_is_empty() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("skills/vendor/show-me")).unwrap();

        let entries = discover_vendor_skill_dirs(tmp.path()).unwrap();
        assert!(
            entries.is_empty(),
            "a missing vendor lock must yield no vendor skills"
        );
    }

    #[test]
    fn generate_manifest_includes_all_skills() {
        let skills = vec![
            DiscoveredSkill {
                name: "toolkit-setup".into(),
                source: "autopilot".into(),
                skill_type: SkillType::Agnostic,
                variants: vec![],
                codex_agent: false,
            },
            DiscoveredSkill {
                name: "autopilot-orchestrator".into(),
                source: "autopilot".into(),
                skill_type: SkillType::Coupled,
                variants: vec!["codex".into(), "kimi".into(), "reasonix".into()],
                codex_agent: false,
            },
            DiscoveredSkill {
                name: "autopilot-implementer".into(),
                source: "autopilot".into(),
                skill_type: SkillType::Coupled,
                variants: vec!["kimi".into(), "reasonix".into()],
                codex_agent: true,
            },
            DiscoveredSkill {
                name: "tdd".into(),
                source: "upstream".into(),
                skill_type: SkillType::Agnostic,
                variants: vec![],
                codex_agent: false,
            },
            DiscoveredSkill {
                name: "show-me".into(),
                source: "vendor".into(),
                skill_type: SkillType::Agnostic,
                variants: vec![],
                codex_agent: false,
            },
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
