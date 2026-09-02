use sha2::{Digest, Sha256};
use std::env;
use std::hash::{Hash, Hasher};
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(crate) fn slugify(value: &str) -> String {
    let mut slug = String::new();
    let mut last_dash = false;
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            slug.push('-');
            last_dash = true;
        }
    }
    slug.trim_matches('-').to_string()
}

pub(crate) fn stable_hash(content: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    content.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

pub(crate) fn create_run_id(session_id: &str) -> String {
    let millis = current_timestamp_millis();
    let slug: String = session_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .take(24)
        .collect();
    format!("run-{slug}-{millis}")
}

pub(crate) fn current_timestamp_millis() -> u128 {
    if let Ok(value) = env::var("DISTILL_FIXED_TIMESTAMP_MILLIS") {
        if let Ok(parsed) = value.parse() {
            return parsed;
        }
    }
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

pub(crate) fn require_non_empty(value: Option<String>, name: &str) -> Result<String, String> {
    let value = value.ok_or_else(|| format!("{name} is required"))?;
    if value.trim().is_empty() {
        return Err(format!("{name} must not be empty"));
    }
    Ok(value)
}

pub(crate) fn require_run_id(value: Option<String>, name: &str) -> Result<String, String> {
    let value = require_non_empty(value, name)?;
    crate::storage::validate_run_id(&value)?;
    Ok(value)
}

pub(crate) fn parse_revision(value: Option<String>, name: &str) -> Result<u64, String> {
    require_non_empty(value, name)?
        .parse()
        .map_err(|_| format!("{name} must be an integer"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard};

    /// Serializes tests that pin DISTILL_FIXED_TIMESTAMP_MILLIS: they run in
    /// one process, so unsynchronized set_var calls race. The guard removes
    /// the var on drop so state never leaks into other tests.
    static TIMESTAMP_ENV_LOCK: Mutex<()> = Mutex::new(());

    struct FixedTimestampEnv {
        _guard: MutexGuard<'static, ()>,
    }

    impl FixedTimestampEnv {
        fn set(value: &str) -> Self {
            let guard = TIMESTAMP_ENV_LOCK
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            std::env::set_var("DISTILL_FIXED_TIMESTAMP_MILLIS", value);
            Self { _guard: guard }
        }
    }

    impl Drop for FixedTimestampEnv {
        fn drop(&mut self) {
            std::env::remove_var("DISTILL_FIXED_TIMESTAMP_MILLIS");
        }
    }

    // --- sha256_hex ---

    #[test]
    fn test_sha256_hex_known_vector() {
        // SHA-256 of empty string
        let result = sha256_hex(b"");
        assert_eq!(
            result,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn test_sha256_hex_hello() {
        let result = sha256_hex(b"hello");
        assert_eq!(
            result,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn test_sha256_hex_consistent() {
        let a = sha256_hex(b"distill");
        let b = sha256_hex(b"distill");
        assert_eq!(a, b);
    }

    // --- slugify ---

    #[test]
    fn test_slugify_simple() {
        assert_eq!(slugify("Hello World"), "hello-world");
    }

    #[test]
    fn test_slugify_already_slug() {
        assert_eq!(slugify("hello-world"), "hello-world");
    }

    #[test]
    fn test_slugify_multiple_spaces() {
        assert_eq!(slugify("hello   world"), "hello-world");
    }

    #[test]
    fn test_slugify_special_chars() {
        assert_eq!(slugify("hello!@#world"), "hello-world");
    }

    #[test]
    fn test_slugify_leading_trailing_dash() {
        assert_eq!(slugify("  hello world  "), "hello-world");
    }

    #[test]
    fn test_slugify_empty() {
        assert_eq!(slugify(""), "");
    }

    #[test]
    fn test_slugify_only_special() {
        assert_eq!(slugify("!@#$%"), "");
    }

    #[test]
    fn test_slugify_mixed_case() {
        assert_eq!(slugify("HelloWORLD"), "helloworld");
    }

    #[test]
    fn test_slugify_numbers() {
        assert_eq!(slugify("test 123 abc"), "test-123-abc");
    }

    #[test]
    fn test_slugify_underscore() {
        // underscore is not alphanumeric, becomes dash
        assert_eq!(slugify("hello_world"), "hello-world");
    }

    // --- stable_hash ---

    #[test]
    fn test_stable_hash_deterministic() {
        let a = stable_hash("hello");
        let b = stable_hash("hello");
        assert_eq!(a, b);
    }

    #[test]
    fn test_stable_hash_different_inputs() {
        let a = stable_hash("hello");
        let b = stable_hash("world");
        assert_ne!(a, b);
    }

    #[test]
    fn test_stable_hash_format() {
        let hash = stable_hash("test");
        // 16 hex chars
        assert_eq!(hash.len(), 16);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    // --- create_run_id ---

    #[test]
    fn test_create_run_id_format() {
        let _fixed = FixedTimestampEnv::set("1234567890");
        let run_id = create_run_id("my-session");
        assert!(run_id.starts_with("run-"));
        assert!(run_id.ends_with("-1234567890"));
        assert!(run_id.contains("my-session"));
    }

    #[test]
    fn test_create_run_id_truncates_long_session() {
        let _fixed = FixedTimestampEnv::set("999");
        let long = "a".repeat(50);
        let run_id = create_run_id(&long);
        // session part truncated to 24 chars
        assert!(run_id.starts_with("run-"));
        // after "run-", the session part is max 24 chars, then "-999"
        let after_run: String = run_id.chars().skip(4).collect();
        let parts: Vec<&str> = after_run.rsplitn(2, '-').collect();
        assert_eq!(parts[0], "999");
        assert!(parts[1].len() <= 24);
    }

    #[test]
    fn test_create_run_id_filters_special_chars() {
        let _fixed = FixedTimestampEnv::set("0");
        let run_id = create_run_id("hello world!@#");
        // spaces and special chars filtered out, only alphanumeric and dash
        assert!(!run_id.contains(' '));
        assert!(!run_id.contains('!'));
        assert!(!run_id.contains('@'));
        assert!(!run_id.contains('#'));
    }

    // --- require_non_empty ---

    #[test]
    fn test_require_non_empty_some() {
        let result = require_non_empty(Some("value".to_string()), "test");
        assert_eq!(result, Ok("value".to_string()));
    }

    #[test]
    fn test_require_non_empty_none() {
        let result = require_non_empty(None, "test-field");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("test-field"));
    }

    #[test]
    fn test_require_non_empty_whitespace() {
        let result = require_non_empty(Some("   ".to_string()), "test-field");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must not be empty"));
    }

    // --- parse_revision ---

    #[test]
    fn test_parse_revision_valid() {
        let result = parse_revision(Some("42".to_string()), "--revision");
        assert_eq!(result, Ok(42));
    }

    #[test]
    fn test_parse_revision_zero() {
        let result = parse_revision(Some("0".to_string()), "--revision");
        assert_eq!(result, Ok(0));
    }

    #[test]
    fn test_parse_revision_not_integer() {
        let result = parse_revision(Some("abc".to_string()), "--revision");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must be an integer"));
    }

    #[test]
    fn test_parse_revision_missing() {
        let result = parse_revision(None, "--revision");
        assert!(result.is_err());
    }

    // --- current_timestamp_millis ---

    #[test]
    fn test_current_timestamp_millis_uses_env() {
        let _fixed = FixedTimestampEnv::set("7777");
        assert_eq!(current_timestamp_millis(), 7777);
    }

    #[test]
    fn test_current_timestamp_millis_ignores_invalid_env() {
        let _fixed = FixedTimestampEnv::set("not-a-number");
        // Should fall back to system time (just verify it returns something)
        let ts = current_timestamp_millis();
        assert!(ts > 0);
    }
}
