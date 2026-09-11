//! Shared infrastructure for the autopilot-toolkit workspace.
//!
//! Provides:
//! - `project_root()` — unified project root derivation
//! - `SkillLock` / `LockedSkill` — strong types for `.skill-lock.json` and `.vendor-lock.json`
//! - `load_skill_lock_at()` / `load_vendor_lock_at()` — parse entrypoints for a given project root
//! - `load_skill_lock()` / `load_vendor_lock()` — ambient-root conveniences (test-only in this repo)

use serde::Deserialize;
use std::path::{Path, PathBuf};

// ── Lock file names ────────────────────────────────────────────────────────

/// Upstream skill lock (mattpocock/skills snapshot).
pub const SKILL_LOCK_FILE: &str = ".skill-lock.json";
/// Vendor skill lock (third-party skills under `skills/vendor/`).
pub const VENDOR_LOCK_FILE: &str = ".vendor-lock.json";

// ── .skill-lock.json types ─────────────────────────────────────────────────

/// A single locked skill entry from `.skill-lock.json`.
///
/// Only the fields needed by consumers are extracted; the full JSON blob
/// contains additional metadata (`source`, `sourceUrl`, `pluginName`,
/// `updatedAt`) that is deliberately omitted. `installedAt` is retained
/// for consumers that need to preserve the original install timestamp.
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
    /// Local directory for a vendor skill, relative to the project root
    /// (e.g. `skills/vendor/show-me`). Present only in `.vendor-lock.json`.
    #[serde(rename = "vendorPath", default)]
    pub vendor_path: Option<String>,
    /// Timestamp when the skill was first installed (ISO 8601).
    #[serde(rename = "installedAt", default)]
    pub installed_at: Option<String>,
}

impl LockedSkill {
    /// Local vendor skill directory, relative to the project root.
    ///
    /// Prefers the explicit `vendorPath`; falls back to the conventional
    /// `skills/vendor/<name>` location when the field is absent.
    pub fn vendor_dir(&self) -> String {
        self.vendor_path
            .clone()
            .unwrap_or_else(|| format!("skills/vendor/{}", self.name))
    }

    /// The skill's directory inside its upstream checkout, relative to the
    /// lock file's path base (e.g. `skills/engineering/tdd`).
    ///
    /// `None` when `skill_path` does not end in `/SKILL.md`.
    pub fn skill_dir_rel(&self) -> Option<String> {
        self.skill_path
            .strip_suffix("/SKILL.md")
            .map(|rel| rel.to_string())
    }

    /// Project-root-relative directory of an upstream skill
    /// (e.g. `skills/upstream/skills/engineering/tdd`).
    ///
    /// `None` when `skill_path` does not end in `/SKILL.md`.
    pub fn upstream_dir_rel(&self) -> Option<String> {
        self.skill_dir_rel()
            .map(|rel| format!("skills/upstream/{rel}"))
    }

    /// Project-root-relative path of an upstream skill's SKILL.md file
    /// (e.g. `skills/upstream/skills/engineering/tdd/SKILL.md`).
    pub fn upstream_file_rel(&self) -> String {
        format!("skills/upstream/{}", self.skill_path)
    }

    /// Project-root-relative path of a vendor skill's SKILL.md file
    /// (e.g. `skills/vendor/show-me/SKILL.md`).
    pub fn vendor_file_rel(&self) -> String {
        format!("{}/SKILL.md", self.vendor_dir())
    }
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
/// This is the production entrypoint: consumers pass the project root
/// explicitly. The ambient-root `load_skill_lock()` is a convenience used
/// by tests that rely on `project_root()`.
pub fn load_skill_lock_at(root: &Path) -> Result<SkillLock, String> {
    load_lock_at(root, SKILL_LOCK_FILE)
}

/// Read and parse `.vendor-lock.json` from the project root.
///
/// Returns an error if the file is missing, unreadable, or contains invalid
/// JSON / unexpected structure.
pub fn load_vendor_lock() -> Result<SkillLock, String> {
    let root = project_root();
    load_vendor_lock_at(&root)
}

/// Read and parse `.vendor-lock.json` from a specific directory.
///
/// This is the production entrypoint: consumers pass the project root
/// explicitly. The ambient-root `load_vendor_lock()` is a convenience used
/// by tests that rely on `project_root()`.
pub fn load_vendor_lock_at(root: &Path) -> Result<SkillLock, String> {
    load_lock_at(root, VENDOR_LOCK_FILE)
}

/// Shared lock-file reader used by both the upstream and vendor locks.
fn load_lock_at(root: &Path, file_name: &str) -> Result<SkillLock, String> {
    let lock_path = root.join(file_name);
    let content = std::fs::read_to_string(&lock_path)
        .map_err(|e| format!("cannot read {:?}: {}", lock_path, e))?;
    let lock: SkillLock =
        serde_json::from_str(&content).map_err(|e| format!("invalid {}: {}", file_name, e))?;
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
    fn deserialize_vendor_skill_with_vendor_path() {
        let json = r#"{
            "version": 1,
            "skills": {
                "show-me": {
                    "source": "humanlayer/skills",
                    "sourceType": "github",
                    "skillPath": "plugins/show-me/skills/show-me/SKILL.md",
                    "skillFolderHash": "abc123",
                    "vendorPath": "skills/vendor/show-me"
                }
            }
        }"#;

        let lock: SkillLock = serde_json::from_str(json).expect("should parse");
        assert_eq!(lock.version, 1);
        assert_eq!(lock.skills.len(), 1);
        let skill = &lock.skills[0];
        assert_eq!(skill.name, "show-me");
        assert_eq!(skill.source_type, "github");
        assert_eq!(skill.vendor_path.as_deref(), Some("skills/vendor/show-me"));
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

    // ── lock path mapping ────────────────────────────────────────────────

    fn locked_skill(name: &str, skill_path: &str, vendor_path: Option<&str>) -> LockedSkill {
        LockedSkill {
            name: name.to_string(),
            source_type: "github".to_string(),
            skill_path: skill_path.to_string(),
            skill_folder_hash: "abc123".to_string(),
            vendor_path: vendor_path.map(|p| p.to_string()),
            installed_at: None,
        }
    }

    #[test]
    fn upstream_mapping_resolves_file_and_directory() {
        let skill = locked_skill("tdd", "skills/engineering/tdd/SKILL.md", None);
        assert_eq!(
            skill.skill_dir_rel().as_deref(),
            Some("skills/engineering/tdd")
        );
        assert_eq!(
            skill.upstream_dir_rel().as_deref(),
            Some("skills/upstream/skills/engineering/tdd")
        );
        assert_eq!(
            skill.upstream_file_rel(),
            "skills/upstream/skills/engineering/tdd/SKILL.md"
        );
    }

    #[test]
    fn upstream_mapping_rejects_paths_without_skill_md() {
        let skill = locked_skill("tdd", "skills/engineering/tdd/README.md", None);
        assert_eq!(skill.skill_dir_rel(), None);
        assert_eq!(skill.upstream_dir_rel(), None);
        assert_eq!(
            skill.upstream_file_rel(),
            "skills/upstream/skills/engineering/tdd/README.md"
        );
    }

    #[test]
    fn vendor_mapping_resolves_file_and_directory() {
        let skill = locked_skill(
            "show-me",
            "plugins/show-me/skills/show-me/SKILL.md",
            Some("skills/vendor/show-me"),
        );
        assert_eq!(skill.vendor_dir(), "skills/vendor/show-me");
        assert_eq!(skill.vendor_file_rel(), "skills/vendor/show-me/SKILL.md");
    }

    #[test]
    fn vendor_mapping_falls_back_to_conventional_directory() {
        let skill = locked_skill("show-me", "plugins/show-me/SKILL.md", None);
        assert_eq!(skill.vendor_dir(), "skills/vendor/show-me");
        assert_eq!(skill.vendor_file_rel(), "skills/vendor/show-me/SKILL.md");
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

    #[test]
    fn load_vendor_lock_from_temp_project() {
        let dir = tempfile::tempdir().expect("tempdir");
        let lock_json = r#"{
            "version": 1,
            "skills": {
                "show-me": {
                    "sourceType": "github",
                    "skillPath": "plugins/show-me/skills/show-me/SKILL.md",
                    "skillFolderHash": "deadbeef",
                    "vendorPath": "skills/vendor/show-me"
                }
            }
        }"#;
        std::fs::write(dir.path().join(".vendor-lock.json"), lock_json).expect("write");

        let lock = load_vendor_lock_at(dir.path()).expect("should load");
        assert_eq!(lock.skills.len(), 1);
        assert_eq!(lock.skills[0].name, "show-me");
        assert_eq!(
            lock.skills[0].vendor_path.as_deref(),
            Some("skills/vendor/show-me")
        );
    }

    #[test]
    fn load_vendor_lock_missing_file_is_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let err = load_vendor_lock_at(dir.path()).unwrap_err();
        assert!(
            err.contains(".vendor-lock.json"),
            "error should mention .vendor-lock.json, got: {}",
            err
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
