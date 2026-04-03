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
