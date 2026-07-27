//! Shared infrastructure for the autopilot-toolkit workspace.
//!
//! Provides:
//! - `project_root()` — unified project root derivation
//! - `SkillLock` / `LockedSkill` — strong types for `.skill-lock.json`
//! - `load_skill_lock()` — single parse entrypoint for `.skill-lock.json`

use serde::Deserialize;
use std::path::PathBuf;

// ── .skill-lock.json types ─────────────────────────────────────────────────

/// A single locked skill entry from `.skill-lock.json`.
///
/// Only the fields needed by consumers are extracted; the full JSON blob
/// contains additional metadata (`source`, `sourceUrl`, `pluginName`,
/// `installedAt`, `updatedAt`) that is deliberately omitted.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct LockedSkill {
    /// The skill name (key in the `skills` JSON object).
    /// Populated by the custom deserializer — not present in the JSON value.
    #[serde(default)]
    pub name: String,
    /// The source type for this skill (e.g. `"github"`, `"local"`).
    /// Deserialized from the per-skill `sourceType` JSON field.
    #[serde(rename = "sourceType", default)]
    pub source_type: String,
    /// Relative path to the skill's SKILL.md within the upstream repo
    /// (e.g. `skills/engineering/tdd/SKILL.md`).
    #[serde(rename = "skillPath")]
    pub skill_path: String,
    /// Git tree hash of the skill's directory at lock time.
    #[serde(rename = "skillFolderHash")]
    pub skill_folder_hash: String,
}

/// Top-level structure of `.skill-lock.json`.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct SkillLock {
    /// Lockfile format version (currently `4`).
    pub version: u32,
    /// The source type shared by all locked skills (e.g. `"github"`).
    /// Deserialized from the top-level `source_type` JSON field when present;
    /// defaults to `"github"`.
    #[serde(default = "default_source_type")]
    pub source_type: String,
    /// All locked skills, flattened from the `skills` JSON object.
    #[serde(rename = "skills", deserialize_with = "deserialize_skills_map")]
    pub skills: Vec<LockedSkill>,
}

fn default_source_type() -> String {
    "github".to_string()
}

/// Custom deserializer: converts the `skills` JSON object `{ "name": { ... } }`
/// into `Vec<LockedSkill>`, injecting the key as each entry's `name` field.
fn deserialize_skills_map<'de, D>(deserializer: D) -> Result<Vec<LockedSkill>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let map: std::collections::BTreeMap<String, LockedSkill> =
        std::collections::BTreeMap::deserialize(deserializer)?;
    Ok(map
        .into_iter()
        .map(|(name, mut skill)| {
            skill.name = name;
            skill
        })
        .collect())
}

// ── .skill-lock.json loader ────────────────────────────────────────────────

/// Read and parse `.skill-lock.json` from the project root.
///
/// Returns an error if the file is missing, unreadable, or contains invalid
/// JSON / unexpected structure.
pub fn load_skill_lock() -> Result<SkillLock, String> {
    let root = project_root();
    load_skill_lock_at(&root)
}

/// Read and parse `.skill-lock.json` from a specific directory.
///
/// Prefer `load_skill_lock()` in production code; this variant is useful
/// for tests that operate on synthetic project roots.
pub fn load_skill_lock_at(root: &std::path::Path) -> Result<SkillLock, String> {
    let lock_path = root.join(".skill-lock.json");
    let content =
        std::fs::read_to_string(&lock_path).map_err(|e| format!("cannot read {:?}: {}", lock_path, e))?;
    let lock: SkillLock =
        serde_json::from_str(&content).map_err(|e| format!("invalid .skill-lock.json: {}", e))?;
    Ok(lock)
}

// ── project root derivation ────────────────────────────────────────────────

/// Return the workspace / project root directory.
///
/// Derivation strategy (first match wins):
/// 1. `PROJECT_ROOT` env var with a `.skill-lock.json` check.
/// 2. Walk up from `file!()` (compile-time source path of *this* library),
///    looking for `.skill-lock.json`.
/// 3. Walk up from `std::env::current_dir()`, looking for `.skill-lock.json`.
/// 4. Fall back to `std::env::current_dir()`.
pub fn project_root() -> PathBuf {
    // 1. PROJECT_ROOT env var (preferred for CI / explicit override)
    if let Ok(root) = std::env::var("PROJECT_ROOT") {
        let p = PathBuf::from(&root);
        if p.join(".skill-lock.json").exists() {
            return p;
        }
    }

    // 2. Derive from compile-time source path (file!() is crates/shared/src/lib.rs)
    let src = std::path::Path::new(file!());
    // file!() returns a path relative to the workspace root when compiled via
    // cargo.  Walk up until we find .skill-lock.json.
    let mut candidate: Option<PathBuf> = Some(src.to_path_buf());
    while let Some(ref dir) = candidate {
        if dir.join(".skill-lock.json").exists() {
            return dir.clone();
        }
        candidate = dir.parent().map(|p| p.to_path_buf());
    }

    // 3. Walk up from current directory
    if let Ok(cwd) = std::env::current_dir() {
        let mut dir = Some(cwd);
        while let Some(d) = dir {
            if d.join(".skill-lock.json").exists() {
                return d;
            }
            dir = d.parent().map(|p| p.to_path_buf());
        }
    }

    // 4. Fallback
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    // ── LockedSkill / SkillLock deserialization ──────────────────────────

    #[test]
    fn deserialize_single_skill() {
        let json = r#"{
            "version": 4,
            "skills": {
                "tdd": {
                    "source": "mattpocock/skills",
                    "sourceType": "github",
                    "sourceUrl": "https://github.com/mattpocock/skills.git",
                    "skillPath": "skills/engineering/tdd/SKILL.md",
                    "skillFolderHash": "7e6fad2aedf8648f6154af32915eebd12c6d51cc",
                    "pluginName": "mattpocock-skills",
                    "installedAt": "2026-05-13T05:30:53.427Z",
                    "updatedAt": "2026-05-13T05:30:53.427Z"
                }
            },
            "dismissed": {}
        }"#;

        let lock: SkillLock = serde_json::from_str(json).expect("should parse");
        assert_eq!(lock.version, 4);
        assert_eq!(lock.source_type, "github");
        assert_eq!(lock.skills.len(), 1);

        let skill = &lock.skills[0];
        assert_eq!(skill.name, "tdd");
        assert_eq!(skill.source_type, "github");
        assert_eq!(skill.skill_path, "skills/engineering/tdd/SKILL.md");
        assert_eq!(
            skill.skill_folder_hash,
            "7e6fad2aedf8648f6154af32915eebd12c6d51cc"
        );
    }

    #[test]
    fn deserialize_multiple_skills_preserves_names() {
        let json = r#"{
            "version": 4,
            "skills": {
                "tdd": {
                    "skillPath": "skills/engineering/tdd/SKILL.md",
                    "skillFolderHash": "aaa111"
                },
                "triage": {
                    "skillPath": "skills/engineering/triage/SKILL.md",
                    "skillFolderHash": "bbb222"
                }
            },
            "dismissed": {}
        }"#;

        let lock: SkillLock = serde_json::from_str(json).expect("should parse");
        let names: Vec<&str> = lock.skills.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["tdd", "triage"]);
    }

    #[test]
    fn deserialize_empty_skills() {
        let json = r#"{"version": 4, "skills": {}, "dismissed": {}}"#;
        let lock: SkillLock = serde_json::from_str(json).expect("should parse");
        assert_eq!(lock.skills.len(), 0);
        assert_eq!(lock.source_type, "github"); // default
    }

    #[test]
    fn deserialize_missing_skillpath_fails() {
        let json = r#"{
            "version": 4,
            "skills": {
                "bad": {
                    "skillFolderHash": "ccc333"
                }
            },
            "dismissed": {}
        }"#;
        let err = serde_json::from_str::<SkillLock>(json).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("skillPath") || msg.contains("missing field"),
            "expected error about missing skillPath, got: {}",
            msg
        );
    }

    #[test]
    fn deserialize_round_trip_via_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let lock_path = dir.path().join(".skill-lock.json");

        let original = r#"{"version":4,"skills":{"sample":{"skillPath":"p.md","skillFolderHash":"abc123"}},"dismissed":{}}"#;
        std::fs::write(&lock_path, original).expect("write");

        let read_back = std::fs::read_to_string(&lock_path).expect("read");
        let lock: SkillLock = serde_json::from_str(&read_back).expect("parse");

        assert_eq!(lock.version, 4);
        assert_eq!(lock.skills.len(), 1);
        assert_eq!(lock.skills[0].name, "sample");
        assert_eq!(lock.skills[0].skill_path, "p.md");
        assert_eq!(lock.skills[0].skill_folder_hash, "abc123");
    }

    // ── load_skill_lock ──────────────────────────────────────────────────

    #[test]
    fn load_skill_lock_from_temp_project() {
        let dir = tempfile::tempdir().expect("tempdir");
        let lock_json = r#"{
            "version": 4,
            "skills": {
                "test-skill": {
                    "skillPath": "skills/test/SKILL.md",
                    "skillFolderHash": "deadbeef"
                }
            },
            "dismissed": {}
        }"#;
        std::fs::write(dir.path().join(".skill-lock.json"), lock_json).expect("write");

        // Override PROJECT_ROOT so load_skill_lock() finds our temp dir
        std::env::set_var("PROJECT_ROOT", dir.path());
        let result = load_skill_lock();
        std::env::remove_var("PROJECT_ROOT");

        let lock = result.expect("should load");
        assert_eq!(lock.skills.len(), 1);
        assert_eq!(lock.skills[0].name, "test-skill");
    }

    #[test]
    fn load_skill_lock_missing_field_is_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        // Missing skillPath in the skill entry
        std::fs::write(
            dir.path().join(".skill-lock.json"),
            r#"{"version":4,"skills":{"bad":{"skillFolderHash":"fff"}},"dismissed":{}}"#,
        )
        .expect("write");

        std::env::set_var("PROJECT_ROOT", dir.path());
        let result = load_skill_lock();
        std::env::remove_var("PROJECT_ROOT");

        assert!(result.is_err(), "expected error for missing field");
        assert!(
            result.unwrap_err().contains("skillPath"),
            "error should mention skillPath"
        );
    }

    // ── project_root ─────────────────────────────────────────────────────

    #[test]
    fn project_root_finds_real_root() {
        let root = project_root();
        assert!(
            root.join(".skill-lock.json").exists(),
            "project_root() = {:?} should contain .skill-lock.json",
            root
        );
        assert!(
            root.join("Cargo.toml").exists(),
            "project_root() = {:?} should contain Cargo.toml",
            root
        );
    }

    #[test]
    fn project_root_respects_env_override() {
        let dir = tempfile::tempdir().expect("tempdir");
        // Create a fake .skill-lock.json so the env override check passes
        std::fs::write(
            dir.path().join(".skill-lock.json"),
            r#"{"version":1,"skills":{},"dismissed":{}}"#,
        )
        .expect("write");

        std::env::set_var("PROJECT_ROOT", dir.path());
        let root = project_root();
        std::env::remove_var("PROJECT_ROOT");

        // Compare canonical forms (macOS /var vs /private/var)
        assert_eq!(
            root.canonicalize().unwrap(),
            dir.path().canonicalize().unwrap()
        );
    }

    #[test]
    fn project_root_consistency() {
        // Calling twice returns the same result
        let r1 = project_root();
        let r2 = project_root();
        assert_eq!(r1, r2);
    }
}
