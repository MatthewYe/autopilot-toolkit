//! Shared infrastructure for the autopilot-toolkit workspace.
//!
//! Provides:
//! - `project_root()` — unified project root derivation
//! - `SkillLock` / `LockedSkill` — strong types for `.skill-lock.json` and `.vendor-lock.json`
//! - `load_skill_lock_at()` / `load_vendor_lock_at()` — parse entrypoints for a given project root
//! - `load_skill_lock()` / `load_vendor_lock()` — ambient-root conveniences (test-only in this repo)

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

// ── Lock file names ────────────────────────────────────────────────────────

/// Upstream skill lock (mattpocock/skills snapshot).
pub const SKILL_LOCK_FILE: &str = ".skill-lock.json";
/// Vendor skill lock (third-party skills under `skills/vendor/`).
pub const VENDOR_LOCK_FILE: &str = ".vendor-lock.json";

// ── .skill-lock.json types ─────────────────────────────────────────────────

/// Which lock file flavor a `SkillLock` was parsed from (or should be
/// written as). The two flavors use different key orders on disk:
///
/// - `Skill` (`.skill-lock.json`): insertion-order entry fields
///   (`source`, `sourceType`, `sourceUrl`, `skillPath`, `skillFolderHash`,
///   `pluginName`, `installedAt`, `updatedAt`) and top-level
///   `version, skills, dismissed`.
/// - `Vendor` (`.vendor-lock.json`): strictly alphabetical keys everywhere
///   (top level `skills, version`), matching `serde_json`'s BTreeMap output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LockFlavor {
    /// `.skill-lock.json` flavor (default for ad-hoc deserialization).
    #[default]
    Skill,
    /// `.vendor-lock.json` flavor.
    Vendor,
}

/// A single locked skill entry from `.skill-lock.json` / `.vendor-lock.json`.
///
/// Full-fidelity model: every field seen in either real lock file is
/// represented. Vendor-only fields (`license`, `pluginVersion`,
/// `sourceCommit`, `vendorPath`) are `Option`, as is every field that any
/// real or synthetic entry may omit. Field declaration order matches the
/// `.skill-lock.json` on-disk order; `.vendor-lock.json`'s alphabetical
/// order is produced by `SkillLock::to_bytes` via `LockFlavor`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct LockedSkill {
    /// The skill name (key in the `skills` JSON object).
    /// Populated by the custom deserializer — not present in the JSON value.
    #[serde(default, skip_serializing)]
    pub name: String,
    /// The source repo (e.g. `"mattpocock/skills"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// The source type for this skill (e.g. `"github"`, `"local"`).
    /// Deserialized from the per-skill `sourceType` JSON field.
    #[serde(rename = "sourceType", default, skip_serializing_if = "String::is_empty")]
    pub source_type: String,
    /// Clone URL of the source repo.
    #[serde(rename = "sourceUrl", default, skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    /// Relative path to the skill's SKILL.md within the upstream repo
    /// (e.g. `skills/engineering/tdd/SKILL.md`).
    #[serde(rename = "skillPath")]
    pub skill_path: String,
    /// Git tree hash of the skill's directory at lock time.
    #[serde(rename = "skillFolderHash")]
    pub skill_folder_hash: String,
    /// Plugin the skill belongs to (e.g. `"mattpocock-skills"`).
    #[serde(rename = "pluginName", default, skip_serializing_if = "Option::is_none")]
    pub plugin_name: Option<String>,
    /// License identifier (vendor skills only, e.g. `"MIT"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    /// Version of the source plugin (vendor skills only, e.g. `"1.0.1"`).
    #[serde(rename = "pluginVersion", default, skip_serializing_if = "Option::is_none")]
    pub plugin_version: Option<String>,
    /// Commit the vendor skill was pinned at (vendor skills only).
    #[serde(rename = "sourceCommit", default, skip_serializing_if = "Option::is_none")]
    pub source_commit: Option<String>,
    /// Local directory for a vendor skill, relative to the project root
    /// (e.g. `skills/vendor/show-me`). Present only in `.vendor-lock.json`.
    #[serde(rename = "vendorPath", default, skip_serializing_if = "Option::is_none")]
    pub vendor_path: Option<String>,
    /// Timestamp when the skill was first installed (ISO 8601).
    #[serde(rename = "installedAt", default, skip_serializing_if = "Option::is_none")]
    pub installed_at: Option<String>,
    /// Timestamp when the lock entry was last updated (ISO 8601).
    #[serde(rename = "updatedAt", default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
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

/// Top-level structure of `.skill-lock.json` / `.vendor-lock.json`.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct SkillLock {
    /// Lockfile format version (`4` for `.skill-lock.json`, `1` for
    /// `.vendor-lock.json`).
    pub version: u32,
    /// The source type shared by all locked skills (e.g. `"github"`).
    /// Deserialized from the top-level `source_type` JSON field when present;
    /// `None` otherwise (neither real lock file carries it). Modeled as
    /// `Option` so serialization can omit it and stay byte-stable.
    #[serde(default)]
    pub source_type: Option<String>,
    /// All locked skills, flattened from the `skills` JSON object.
    #[serde(rename = "skills", deserialize_with = "deserialize_skills_map")]
    pub skills: Vec<LockedSkill>,
    /// The `dismissed` JSON object from `.skill-lock.json`, kept verbatim.
    /// `None` when the file has no `dismissed` key (`.vendor-lock.json`).
    #[serde(default)]
    pub dismissed: Option<serde_json::Value>,
    /// Which lock flavor this file was parsed as; controls the key order
    /// emitted by `to_bytes()`. Set by `load_vendor_lock_at`; defaults to
    /// `LockFlavor::Skill` for direct `serde_json` deserialization.
    #[serde(skip)]
    pub flavor: LockFlavor,
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

// ── Typed write path ─────────────────────────────────────────────────────

impl SkillLock {
    /// Byte-stable serialization of this lock, including the trailing
    /// newline the on-disk files carry.
    ///
    /// "Byte-stable" means: for a lock file parsed and re-serialized without
    /// modification, `to_bytes()` returns exactly the original file bytes.
    /// The key order is chosen per `flavor` (see [`LockFlavor`]); `skills`
    /// entries are emitted in `Vec` order, which for a freshly parsed lock
    /// is alphabetical (the deserializer flattens a `BTreeMap`).
    pub fn to_bytes(&self) -> Vec<u8> {
        let skills_obj = J::Obj(
            self.skills
                .iter()
                .map(|s| (s.name.clone(), s.to_ordered_json(self.flavor)))
                .collect(),
        );
        let mut top: Vec<(String, J)> = Vec::new();
        match self.flavor {
            // .skill-lock.json: version, skills, dismissed (insertion order).
            LockFlavor::Skill => {
                top.push(("version".to_string(), J::Num(u64::from(self.version))));
                if let Some(st) = &self.source_type {
                    top.push(("source_type".to_string(), J::Str(st.clone())));
                }
                top.push(("skills".to_string(), skills_obj));
                if let Some(d) = &self.dismissed {
                    top.push(("dismissed".to_string(), J::Raw(d.clone())));
                }
            }
            // .vendor-lock.json: strictly alphabetical (skills, version).
            LockFlavor::Vendor => {
                if let Some(d) = &self.dismissed {
                    top.push(("dismissed".to_string(), J::Raw(d.clone())));
                }
                top.push(("skills".to_string(), skills_obj));
                if let Some(st) = &self.source_type {
                    top.push(("source_type".to_string(), J::Str(st.clone())));
                }
                top.push(("version".to_string(), J::Num(u64::from(self.version))));
            }
        }
        let mut out = String::new();
        J::Obj(top).render(0, &mut out);
        out.push('\n');
        out.into_bytes()
    }

    /// Update only the `skillFolderHash` of the named skill.
    ///
    /// `updatedAt` and every other field are left untouched (unlike
    /// [`SkillLock::replace_skills`]). Returns `Err` when no skill with
    /// that name exists in the lock.
    pub fn set_folder_hash(&mut self, name: &str, hash: &str) -> Result<(), String> {
        match self.skills.iter_mut().find(|s| s.name == name) {
            Some(skill) => {
                skill.skill_folder_hash = hash.to_string();
                Ok(())
            }
            None => Err(format!("skill '{}' not found in lock file", name)),
        }
    }

    /// Replace the skill set with a freshly discovered one, merging
    /// timestamps per the lock-update rules:
    ///
    /// - Entries whose hash is unchanged are preserved verbatim
    ///   (`installedAt` and `updatedAt` carry over from the old entry).
    /// - Entries whose hash changed keep their old `installedAt` but get
    ///   `updatedAt = now`.
    /// - New entries get `installedAt = updatedAt = now`.
    /// - Entries present in the lock but absent from `discovered` (orphans)
    ///   are removed.
    /// - `dismissed` is never touched.
    pub fn replace_skills(&mut self, discovered: Vec<LockedSkill>, now: &str) {
        let previous: std::collections::HashMap<String, LockedSkill> = self
            .skills
            .drain(..)
            .map(|s| (s.name.clone(), s))
            .collect();
        let mut merged = Vec::with_capacity(discovered.len());
        for mut skill in discovered {
            match previous.get(&skill.name) {
                Some(prev) => {
                    skill.installed_at = prev.installed_at.clone();
                    if prev.skill_folder_hash == skill.skill_folder_hash {
                        skill.updated_at = prev.updated_at.clone();
                    } else {
                        skill.updated_at = Some(now.to_string());
                    }
                }
                None => {
                    skill.installed_at = Some(now.to_string());
                    skill.updated_at = Some(now.to_string());
                }
            }
            merged.push(skill);
        }
        self.skills = merged;
    }
}

impl LockedSkill {
    /// Ordered JSON fields for one lock entry, in the key order of the
    /// given flavor. Absent (`None` / empty) fields are omitted.
    fn to_ordered_json(&self, flavor: LockFlavor) -> J {
        /// Push `key: value` when the optional string is present.
        fn opt(fields: &mut Vec<(String, J)>, key: &str, val: &Option<String>) {
            if let Some(v) = val {
                fields.push((key.to_string(), J::Str(v.clone())));
            }
        }
        /// Push `key: value` when the string is non-empty.
        fn non_empty(fields: &mut Vec<(String, J)>, key: &str, val: &str) {
            if !val.is_empty() {
                fields.push((key.to_string(), J::Str(val.to_string())));
            }
        }
        let mut fields: Vec<(String, J)> = Vec::new();
        match flavor {
            // .skill-lock.json insertion order; vendor-only extras slot in
            // after pluginName (they never occur in this flavor on disk).
            LockFlavor::Skill => {
                opt(&mut fields, "source", &self.source);
                non_empty(&mut fields, "sourceType", &self.source_type);
                opt(&mut fields, "sourceUrl", &self.source_url);
                fields.push(("skillPath".to_string(), J::Str(self.skill_path.clone())));
                fields.push((
                    "skillFolderHash".to_string(),
                    J::Str(self.skill_folder_hash.clone()),
                ));
                opt(&mut fields, "pluginName", &self.plugin_name);
                opt(&mut fields, "license", &self.license);
                opt(&mut fields, "pluginVersion", &self.plugin_version);
                opt(&mut fields, "sourceCommit", &self.source_commit);
                opt(&mut fields, "vendorPath", &self.vendor_path);
                opt(&mut fields, "installedAt", &self.installed_at);
                opt(&mut fields, "updatedAt", &self.updated_at);
            }
            // .vendor-lock.json: strictly alphabetical.
            LockFlavor::Vendor => {
                opt(&mut fields, "installedAt", &self.installed_at);
                opt(&mut fields, "license", &self.license);
                opt(&mut fields, "pluginName", &self.plugin_name);
                opt(&mut fields, "pluginVersion", &self.plugin_version);
                fields.push((
                    "skillFolderHash".to_string(),
                    J::Str(self.skill_folder_hash.clone()),
                ));
                fields.push(("skillPath".to_string(), J::Str(self.skill_path.clone())));
                opt(&mut fields, "source", &self.source);
                opt(&mut fields, "sourceCommit", &self.source_commit);
                non_empty(&mut fields, "sourceType", &self.source_type);
                opt(&mut fields, "sourceUrl", &self.source_url);
                opt(&mut fields, "updatedAt", &self.updated_at);
                opt(&mut fields, "vendorPath", &self.vendor_path);
            }
        }
        J::Obj(fields)
    }
}

/// Minimal ordered-JSON tree used by `SkillLock::to_bytes`.
///
/// serde's derived `Serialize` cannot express the two different key orders
/// of the lock flavors (and `serde_json::Map` without the `preserve_order`
/// feature sorts keys alphabetically), so the write path renders this
/// ordered tree directly with serde_json-compatible 2-space pretty
/// formatting.
enum J {
    Str(String),
    Num(u64),
    /// An opaque JSON value (the `dismissed` blob), pretty-printed via
    /// `serde_json` and re-indented to the surrounding depth.
    Raw(serde_json::Value),
    Obj(Vec<(String, J)>),
}

impl J {
    fn render(&self, indent: usize, out: &mut String) {
        match self {
            J::Str(s) => {
                out.push_str(&serde_json::to_string(s).expect("string serialization is infallible"))
            }
            J::Num(n) => out.push_str(&n.to_string()),
            J::Raw(v) => {
                let pretty = serde_json::to_string_pretty(v).unwrap_or_default();
                for (i, line) in pretty.lines().enumerate() {
                    if i > 0 {
                        out.push('\n');
                        push_indent(out, indent);
                    }
                    out.push_str(line);
                }
            }
            J::Obj(fields) => {
                if fields.is_empty() {
                    out.push_str("{}");
                    return;
                }
                out.push_str("{\n");
                for (i, (key, val)) in fields.iter().enumerate() {
                    push_indent(out, indent + 1);
                    out.push_str(
                        &serde_json::to_string(key).expect("string serialization is infallible"),
                    );
                    out.push_str(": ");
                    val.render(indent + 1, out);
                    if i + 1 < fields.len() {
                        out.push(',');
                    }
                    out.push('\n');
                }
                push_indent(out, indent);
                out.push('}');
            }
        }
    }
}

fn push_indent(out: &mut String, indent: usize) {
    for _ in 0..indent {
        out.push_str("  ");
    }
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
    let mut lock = load_lock_at(root, VENDOR_LOCK_FILE)?;
    lock.flavor = LockFlavor::Vendor;
    Ok(lock)
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
    use std::sync::{Mutex, MutexGuard};

    /// Serializes tests that read or pin `PROJECT_ROOT`. The variable is
    /// process-global while tests run in parallel threads, so an unsynchronized
    /// pin lets a sibling test observe a temp root where it expects the real
    /// one (`project_root()` returns the pinned dir, whose `.skill-lock.json`
    /// exists but which has no `Cargo.toml`).
    static PROJECT_ROOT_ENV_LOCK: Mutex<()> = Mutex::new(());

    fn lock_project_root() -> MutexGuard<'static, ()> {
        PROJECT_ROOT_ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Pins `PROJECT_ROOT` for one test and removes it on drop, holding the
    /// lock for as long as the pin lives.
    struct ProjectRootEnv {
        _guard: MutexGuard<'static, ()>,
    }

    impl ProjectRootEnv {
        fn pin(path: &std::path::Path) -> Self {
            let _guard = lock_project_root();
            std::env::set_var("PROJECT_ROOT", path);
            Self { _guard }
        }
    }

    impl Drop for ProjectRootEnv {
        fn drop(&mut self) {
            std::env::remove_var("PROJECT_ROOT");
        }
    }

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
        assert_eq!(lock.source_type, None, "no top-level source_type in JSON");
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
        assert_eq!(lock.source_type, None, "no top-level source_type in JSON");
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
            ..Default::default()
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

    // ── to_bytes: byte-stable round-trip on the real lock files ─────────

    #[test]
    fn to_bytes_round_trip_real_skill_lock_is_byte_stable() {
        let _guard = lock_project_root();
        let root = project_root();
        let original = std::fs::read(root.join(SKILL_LOCK_FILE)).expect("read real skill lock");
        let lock = load_skill_lock_at(&root).expect("parse real skill lock");
        assert_eq!(
            lock.to_bytes(),
            original,
            "read → to_bytes must be byte-identical for .skill-lock.json"
        );
    }

    #[test]
    fn to_bytes_round_trip_real_vendor_lock_is_byte_stable() {
        let _guard = lock_project_root();
        let root = project_root();
        let original = std::fs::read(root.join(VENDOR_LOCK_FILE)).expect("read real vendor lock");
        let lock = load_vendor_lock_at(&root).expect("parse real vendor lock");
        assert_eq!(
            lock.to_bytes(),
            original,
            "read → to_bytes must be byte-identical for .vendor-lock.json"
        );
    }

    #[test]
    fn to_bytes_ends_with_trailing_newline() {
        let lock = SkillLock {
            version: 4,
            source_type: None,
            skills: vec![],
            dismissed: Some(serde_json::json!({})),
            flavor: LockFlavor::Skill,
        };
        let bytes = lock.to_bytes();
        assert_eq!(bytes.last(), Some(&b'\n'));
        assert!(!bytes.ends_with(b"\n\n"), "exactly one trailing newline");
    }

    // ── set_folder_hash ──────────────────────────────────────────────────

    fn full_skill(name: &str, hash: &str, installed: &str, updated: &str) -> LockedSkill {
        LockedSkill {
            name: name.to_string(),
            skill_folder_hash: hash.to_string(),
            installed_at: Some(installed.to_string()),
            updated_at: Some(updated.to_string()),
            ..locked_skill(name, "skills/engineering/x/SKILL.md", None)
        }
    }

    #[test]
    fn set_folder_hash_updates_hash_and_keeps_updated_at() {
        let mut lock = SkillLock {
            version: 4,
            source_type: None,
            skills: vec![full_skill("tdd", "old-hash", "2026-01-01", "2026-01-02")],
            dismissed: None,
            flavor: LockFlavor::Skill,
        };
        lock.set_folder_hash("tdd", "new-hash").expect("should update");
        assert_eq!(lock.skills[0].skill_folder_hash, "new-hash");
        assert_eq!(
            lock.skills[0].updated_at.as_deref(),
            Some("2026-01-02"),
            "set_folder_hash must not touch updatedAt"
        );
        assert_eq!(
            lock.skills[0].installed_at.as_deref(),
            Some("2026-01-01"),
            "set_folder_hash must not touch installedAt"
        );
    }

    #[test]
    fn set_folder_hash_unknown_skill_is_error() {
        let mut lock = SkillLock {
            version: 4,
            source_type: None,
            skills: vec![full_skill("tdd", "h", "i", "u")],
            dismissed: None,
            flavor: LockFlavor::Skill,
        };
        let err = lock.set_folder_hash("nope", "x").unwrap_err();
        assert!(err.contains("nope"), "error should name the skill: {}", err);
        assert_eq!(lock.skills[0].skill_folder_hash, "h", "no mutation on error");
    }

    // ── replace_skills merge matrix ──────────────────────────────────────

    fn merge_lock() -> SkillLock {
        SkillLock {
            version: 4,
            source_type: None,
            skills: vec![
                full_skill("same", "h-same", "i-same", "u-same"),
                full_skill("changed", "h-old", "i-changed", "u-changed"),
                full_skill("orphan", "h-orphan", "i-orphan", "u-orphan"),
            ],
            dismissed: Some(serde_json::json!({"grumpy": {"reason": "no thanks"}})),
            flavor: LockFlavor::Skill,
        }
    }

    #[test]
    fn replace_skills_unchanged_entry_is_preserved_verbatim() {
        let mut lock = merge_lock();
        let incoming = full_skill("same", "h-same", "IGNORED", "IGNORED");
        lock.replace_skills(vec![incoming], "NOW");
        let s = &lock.skills[0];
        assert_eq!(s.name, "same");
        assert_eq!(s.skill_folder_hash, "h-same");
        assert_eq!(s.installed_at.as_deref(), Some("i-same"));
        assert_eq!(
            s.updated_at.as_deref(),
            Some("u-same"),
            "unchanged hash keeps the old updatedAt"
        );
    }

    #[test]
    fn replace_skills_changed_entry_updates_hash_and_timestamp_keeps_installed() {
        let mut lock = merge_lock();
        let incoming = full_skill("changed", "h-new", "IGNORED", "IGNORED");
        lock.replace_skills(vec![incoming], "NOW");
        let s = &lock.skills[0];
        assert_eq!(s.skill_folder_hash, "h-new");
        assert_eq!(
            s.installed_at.as_deref(),
            Some("i-changed"),
            "installedAt is preserved from the old entry"
        );
        assert_eq!(s.updated_at.as_deref(), Some("NOW"));
    }

    #[test]
    fn replace_skills_new_entry_gets_now_for_both_timestamps() {
        let mut lock = merge_lock();
        let incoming = full_skill("brand-new", "h-new", "IGNORED", "IGNORED");
        lock.replace_skills(vec![incoming], "NOW");
        let s = &lock.skills[0];
        assert_eq!(s.installed_at.as_deref(), Some("NOW"));
        assert_eq!(s.updated_at.as_deref(), Some("NOW"));
    }

    #[test]
    fn replace_skills_removes_orphans() {
        let mut lock = merge_lock();
        let incoming = vec![
            full_skill("same", "h-same", "x", "x"),
            full_skill("changed", "h-old", "x", "x"),
        ];
        lock.replace_skills(incoming, "NOW");
        let names: Vec<&str> = lock.skills.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["same", "changed"], "orphan must be removed");
    }

    #[test]
    fn replace_skills_preserves_dismissed() {
        let mut lock = merge_lock();
        lock.replace_skills(vec![], "NOW");
        assert_eq!(
            lock.dismissed,
            Some(serde_json::json!({"grumpy": {"reason": "no thanks"}})),
            "dismissed must survive replace_skills untouched"
        );
        assert!(lock.skills.is_empty());
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
        let _env = ProjectRootEnv::pin(dir.path());
        let result = load_skill_lock();

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

        let _env = ProjectRootEnv::pin(dir.path());
        let result = load_skill_lock();

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
        let _guard = lock_project_root();
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

        let _env = ProjectRootEnv::pin(dir.path());
        let root = project_root();

        // Compare canonical forms (macOS /var vs /private/var)
        assert_eq!(
            root.canonicalize().unwrap(),
            dir.path().canonicalize().unwrap()
        );
    }

    #[test]
    fn project_root_consistency() {
        let _guard = lock_project_root();
        // Calling twice returns the same result
        let r1 = project_root();
        let r2 = project_root();
        assert_eq!(r1, r2);
    }
}
