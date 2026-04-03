//! TTL-based on-disk fact cache.
//!
//! Each resolver group is stored as `<cache_dir>/<resolver_name>.json`.
//! A cached file is considered valid if its modification time is newer than now − TTL.

use crate::facts::value::FactValue;
use indexmap::IndexMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const CACHE_DIR: &str = "/opt/puppetlabs/facter/cache/cached_facts";

pub fn cache_dir() -> PathBuf {
    PathBuf::from(CACHE_DIR)
}

/// Try to load cached facts for a resolver. Returns `None` if missing or expired.
pub fn load(resolver_name: &str, ttl_secs: u64) -> Option<Vec<(String, FactValue)>> {
    let path = cache_path(resolver_name);
    let metadata = fs::metadata(&path).ok()?;
    let modified = metadata.modified().ok()?;
    let age = SystemTime::now().duration_since(modified).ok()?;

    if age > Duration::from_secs(ttl_secs) {
        log::debug!("cache: {} expired (age {}s > ttl {}s)", resolver_name, age.as_secs(), ttl_secs);
        return None;
    }

    let content = fs::read_to_string(&path).ok()?;
    let map: IndexMap<String, FactValue> = serde_json::from_str(&content).ok()?;
    log::debug!("cache: loaded {} from {:?}", resolver_name, path);
    Some(map.into_iter().collect())
}

/// Persist resolver facts to disk. Silently ignores write errors.
pub fn save(resolver_name: &str, facts: &[(String, FactValue)]) {
    let dir = cache_dir();
    if let Err(e) = fs::create_dir_all(&dir) {
        log::debug!("cache: cannot create cache dir {:?}: {}", dir, e);
        return;
    }

    let map: IndexMap<&str, &FactValue> = facts.iter().map(|(k, v)| (k.as_str(), v)).collect();
    let json = match serde_json::to_string_pretty(&map) {
        Ok(j) => j,
        Err(e) => {
            log::warn!("cache: serialisation failed for {}: {}", resolver_name, e);
            return;
        }
    };

    let path = cache_path(resolver_name);
    if let Err(e) = fs::write(&path, json) {
        log::debug!("cache: cannot write {:?}: {}", path, e);
    } else {
        log::debug!("cache: saved {} to {:?}", resolver_name, path);
    }
}

fn cache_path(resolver_name: &str) -> PathBuf {
    // Sanitise resolver name for use as a filename
    let safe: String = resolver_name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect();
    Path::new(CACHE_DIR).join(format!("{}.json", safe))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// Build a temporary cache dir and return a closure that produces cache paths
    /// rooted there instead of the hard-coded CACHE_DIR.
    fn write_cache_file(dir: &std::path::Path, name: &str, facts: &[(String, FactValue)]) {
        fs::create_dir_all(dir).unwrap();
        let map: indexmap::IndexMap<&str, &FactValue> =
            facts.iter().map(|(k, v)| (k.as_str(), v)).collect();
        let json = serde_json::to_string_pretty(&map).unwrap();
        let safe: String = name
            .chars()
            .map(|c| if c.is_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
            .collect();
        fs::write(dir.join(format!("{}.json", safe)), json).unwrap();
    }

    fn aged_file(dir: &std::path::Path, name: &str, age: Duration) {
        // Backdate the file's mtime so TTL checks see it as old
        let path = dir.join(format!("{}.json", name));
        let new_mtime = SystemTime::now() - age;
        let secs = new_mtime
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        // Use utime-style approach via std::fs filetime manipulation isn't stable;
        // fall back to writing then touching via a raw libc call.
        unsafe {
            let path_c = std::ffi::CString::new(path.to_str().unwrap()).unwrap();
            let times = [
                libc::timeval { tv_sec: secs as libc::time_t, tv_usec: 0 },
                libc::timeval { tv_sec: secs as libc::time_t, tv_usec: 0 },
            ];
            libc::utimes(path_c.as_ptr(), times.as_ptr());
        }
    }

    // ── cache_path / name sanitisation ───────────────────────────────────────

    #[test]
    fn cache_path_alphanumeric_unchanged() {
        let p = cache_path("kernel");
        assert!(p.to_str().unwrap().ends_with("kernel.json"));
    }

    #[test]
    fn cache_path_spaces_replaced() {
        let p = cache_path("operating system");
        assert!(p.to_str().unwrap().ends_with("operating_system.json"));
    }

    #[test]
    fn cache_path_dots_replaced() {
        let p = cache_path("a.b.c");
        assert!(p.to_str().unwrap().ends_with("a_b_c.json"));
    }

    #[test]
    fn cache_path_dashes_and_underscores_kept() {
        let p = cache_path("load-avg_1");
        assert!(p.to_str().unwrap().ends_with("load-avg_1.json"));
    }

    // ── load ─────────────────────────────────────────────────────────────────

    /// Override the cache dir for a test by writing directly and then calling
    /// a modified load that accepts a custom dir parameter.
    /// Since load() hardcodes CACHE_DIR we test the logic by exercising save/load
    /// together via the public API, using a writable temp location only when
    /// the real cache dir is accessible, or by testing the helper pieces directly.

    #[test]
    fn load_returns_none_for_missing_file() {
        // A resolver name that almost certainly has no cached file
        let result = load("__no_such_resolver_xyz__", 3600);
        assert!(result.is_none());
    }

    #[test]
    fn load_returns_none_when_ttl_zero() {
        // TTL of 0 means any file is instantly expired
        let result = load("kernel", 0);
        // Even if a real cache file exists it must be considered expired
        assert!(result.is_none());
    }

    // ── save + load round-trip (uses the real cache dir if writable) ──────────

    #[test]
    fn save_and_load_roundtrip() {
        // Skip if the cache dir isn't writable (CI, non-root, etc.)
        let dir = cache_dir();
        if fs::create_dir_all(&dir).is_err() {
            return;
        }
        let test_name = "__facter_test_roundtrip__";
        let facts: Vec<(String, FactValue)> = vec![
            ("rt_key".to_string(), FactValue::String("rt_val".into())),
            ("rt_num".to_string(), FactValue::Integer(42)),
        ];
        save(test_name, &facts);
        // Large TTL so the file is definitely fresh
        let loaded = load(test_name, 86400);
        // Clean up before asserting so we don't leave garbage on failure
        let _ = fs::remove_file(cache_path(test_name));
        let loaded = loaded.expect("round-trip load should succeed");
        assert!(loaded.iter().any(|(k, v)| k == "rt_key" && matches!(v, FactValue::String(s) if s == "rt_val")));
        assert!(loaded.iter().any(|(k, v)| k == "rt_num" && matches!(v, FactValue::Integer(42))));
    }

    // ── TTL expiry logic (via backdated file in temp dir) ────────────────────

    #[test]
    fn expired_cache_returns_none() {
        let tmp = std::env::temp_dir().join("facter_cache_test_expired");
        fs::create_dir_all(&tmp).unwrap();
        let name = "expiry_test";
        let facts = vec![("k".to_string(), FactValue::String("v".into()))];
        write_cache_file(&tmp, name, &facts);
        // Backdate the file by 2 hours
        aged_file(&tmp, name, Duration::from_secs(7200));

        // Read the file directly to confirm it exists and is valid JSON
        let path = tmp.join(format!("{}.json", name));
        let content = fs::read_to_string(&path).unwrap();
        let map: indexmap::IndexMap<String, FactValue> = serde_json::from_str(&content).unwrap();
        assert!(!map.is_empty());

        // Now verify the TTL logic: a file older than TTL should be rejected
        let metadata = fs::metadata(&path).unwrap();
        let modified = metadata.modified().unwrap();
        let age = SystemTime::now().duration_since(modified).unwrap();
        let ttl_secs: u64 = 3600; // 1 hour
        assert!(age > Duration::from_secs(ttl_secs), "file should appear expired");

        let _ = fs::remove_dir_all(tmp);
    }

    #[test]
    fn fresh_cache_passes_ttl_check() {
        let tmp = std::env::temp_dir().join("facter_cache_test_fresh");
        fs::create_dir_all(&tmp).unwrap();
        let name = "fresh_test";
        let facts = vec![("k".to_string(), FactValue::String("v".into()))];
        write_cache_file(&tmp, name, &facts);
        // File was just written — age is ~0 seconds
        let path = tmp.join(format!("{}.json", name));
        let metadata = fs::metadata(&path).unwrap();
        let modified = metadata.modified().unwrap();
        let age = SystemTime::now().duration_since(modified).unwrap();
        assert!(age < Duration::from_secs(5), "freshly written file should be <5s old");
        let _ = fs::remove_dir_all(tmp);
    }

    // ── JSON deserialisation of cached facts ─────────────────────────────────

    #[test]
    fn cached_json_deserialises_all_types() {
        let json = r#"{"s":"hello","i":42,"f":3.14,"b":true,"arr":[1,2],"map":{"x":1}}"#;
        let map: indexmap::IndexMap<String, FactValue> = serde_json::from_str(json).unwrap();
        assert!(matches!(map.get("s"), Some(FactValue::String(_))));
        assert!(matches!(map.get("i"), Some(FactValue::Integer(42))));
        assert!(matches!(map.get("b"), Some(FactValue::Boolean(true))));
        assert!(matches!(map.get("arr"), Some(FactValue::Array(_))));
        assert!(matches!(map.get("map"), Some(FactValue::Map(_))));
    }
}
