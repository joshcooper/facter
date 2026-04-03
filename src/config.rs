//! Facter configuration file loader.
//!
//! Reads `/etc/puppetlabs/facter/facter.conf` (or a custom path) in JSON5 format,
//! which is a superset of JSON and compatible with facter's HOCON config files.
//!
//! Supported structure:
//! ```json5
//! {
//!   global: {
//!     external-dir: ["/path/to/facts.d"],
//!     custom-dir: [],
//!     no-external-facts: false,
//!     no-custom-facts: false,
//!   },
//!   cli: {
//!     debug: false,
//!     verbose: false,
//!     log-level: "warn",
//!   },
//!   facts: {
//!     blocklist: ["EC2", "file system"],
//!     ttls: [
//!       { "timezone": "30 days" },
//!     ],
//!   },
//! }
//! ```

use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Default, Clone)]
pub struct Config {
    pub external_dirs: Vec<String>,
    pub custom_dirs: Vec<String>,
    pub no_external_facts: bool,
    pub no_custom_facts: bool,
    pub blocklist: Vec<String>,
    /// Resolver-name → TTL in seconds
    pub ttls: HashMap<String, u64>,
    /// Dotted paths to exclude from fact output (e.g. "os.distro.release", "gce")
    pub exclude: Vec<String>,
    /// CLI overrides from config file
    pub debug: bool,
    pub verbose: bool,
    pub log_level: Option<String>,
}

pub fn default_config_path() -> &'static str {
    "/etc/puppetlabs/facter/facter.conf"
}

/// Load config from a file path. Returns `None` if the file doesn't exist.
/// Logs a warning and returns `None` on parse errors.
pub fn load(path: &str) -> Option<Config> {
    if !Path::new(path).is_file() {
        return None;
    }
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            log::warn!("config: cannot read {}: {}", path, e);
            return None;
        }
    };
    match parse(&content) {
        Ok(c) => Some(c),
        Err(e) => {
            log::warn!("config: failed to parse {}: {}", path, e);
            None
        }
    }
}

pub(crate) fn parse(text: &str) -> Result<Config, String> {
    let v: serde_json::Value =
        json5::from_str(text).map_err(|e| e.to_string())?;

    let mut cfg = Config::default();

    if let Some(global) = v.get("global") {
        cfg.external_dirs = str_array(global, "external-dir");
        cfg.custom_dirs = str_array(global, "custom-dir");
        cfg.no_external_facts = bool_val(global, "no-external-facts");
        cfg.no_custom_facts = bool_val(global, "no-custom-facts");
    }

    if let Some(cli) = v.get("cli") {
        cfg.debug = bool_val(cli, "debug");
        cfg.verbose = bool_val(cli, "verbose");
        if let Some(l) = cli.get("log-level").and_then(|v| v.as_str()) {
            cfg.log_level = Some(l.to_string());
        }
    }

    if let Some(facts) = v.get("facts") {
        cfg.blocklist = str_array(facts, "blocklist");
        cfg.exclude = str_array(facts, "exclude");

        if let Some(ttls) = facts.get("ttls").and_then(|v| v.as_array()) {
            for entry in ttls {
                if let Some(obj) = entry.as_object() {
                    for (resolver, dur) in obj {
                        let secs = match dur {
                            serde_json::Value::Number(n) => n.as_u64().unwrap_or(0),
                            serde_json::Value::String(s) => parse_duration(s),
                            _ => 0,
                        };
                        if secs > 0 {
                            cfg.ttls.insert(resolver.clone(), secs);
                        }
                    }
                }
            }
        }
    }

    Ok(cfg)
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn str_array(v: &serde_json::Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

fn bool_val(v: &serde_json::Value, key: &str) -> bool {
    v.get(key).and_then(|v| v.as_bool()).unwrap_or(false)
}

/// Parse a HOCON-style duration string into seconds.
/// Supports: "N seconds", "N minutes", "N hours", "N days", "N weeks"
pub(crate) fn parse_duration(s: &str) -> u64 {
    let s = s.trim();
    let parts: Vec<&str> = s.splitn(2, ' ').collect();
    let n: u64 = match parts.first() {
        Some(n) => n.parse().unwrap_or(0),
        None => return 0,
    };
    let unit = parts.get(1).copied().unwrap_or("seconds").trim();
    match unit {
        u if u.starts_with("second") => n,
        u if u.starts_with("minute") => n * 60,
        u if u.starts_with("hour") => n * 3600,
        u if u.starts_with("day") => n * 86400,
        u if u.starts_with("week") => n * 604800,
        _ => n, // assume seconds
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_seconds() {
        assert_eq!(parse_duration("30 seconds"), 30);
        assert_eq!(parse_duration("1 second"), 1);
    }

    #[test]
    fn duration_minutes() {
        assert_eq!(parse_duration("5 minutes"), 300);
        assert_eq!(parse_duration("1 minute"), 60);
    }

    #[test]
    fn duration_hours() {
        assert_eq!(parse_duration("2 hours"), 7200);
    }

    #[test]
    fn duration_days() {
        assert_eq!(parse_duration("30 days"), 2_592_000);
    }

    #[test]
    fn duration_weeks() {
        assert_eq!(parse_duration("1 week"), 604_800);
        assert_eq!(parse_duration("2 weeks"), 1_209_600);
    }

    #[test]
    fn duration_bare_number_treated_as_seconds() {
        assert_eq!(parse_duration("42"), 42);
    }

    #[test]
    fn parse_empty_object() {
        let cfg = parse("{}").unwrap();
        assert!(cfg.blocklist.is_empty());
        assert!(cfg.ttls.is_empty());
        assert!(cfg.exclude.is_empty());
    }

    #[test]
    fn parse_blocklist() {
        let cfg = parse(r#"{ facts: { blocklist: ["kernel", "ssh"] } }"#).unwrap();
        assert_eq!(cfg.blocklist, vec!["kernel", "ssh"]);
    }

    #[test]
    fn parse_ttl_string_duration() {
        let cfg = parse(r#"{ facts: { ttls: [{ "kernel": "30 days" }] } }"#).unwrap();
        assert_eq!(cfg.ttls.get("kernel"), Some(&2_592_000));
    }

    #[test]
    fn parse_ttl_numeric() {
        let cfg = parse(r#"{ facts: { ttls: [{ "timezone": 3600 }] } }"#).unwrap();
        assert_eq!(cfg.ttls.get("timezone"), Some(&3600));
    }

    #[test]
    fn parse_exclude() {
        let cfg = parse(r#"{ facts: { exclude: ["os.distro.release", "gce"] } }"#).unwrap();
        assert_eq!(cfg.exclude, vec!["os.distro.release", "gce"]);
    }

    #[test]
    fn parse_global_dirs() {
        let cfg = parse(r#"{ global: { "external-dir": ["/tmp/facts"], "custom-dir": ["/tmp/custom"] } }"#).unwrap();
        assert_eq!(cfg.external_dirs, vec!["/tmp/facts"]);
        assert_eq!(cfg.custom_dirs, vec!["/tmp/custom"]);
    }

    #[test]
    fn parse_global_disable_flags() {
        let cfg = parse(r#"{ global: { "no-external-facts": true, "no-custom-facts": true } }"#).unwrap();
        assert!(cfg.no_external_facts);
        assert!(cfg.no_custom_facts);
    }

    #[test]
    fn parse_cli_section() {
        let cfg = parse(r#"{ cli: { debug: true, verbose: false, "log-level": "info" } }"#).unwrap();
        assert!(cfg.debug);
        assert!(!cfg.verbose);
        assert_eq!(cfg.log_level.as_deref(), Some("info"));
    }

    #[test]
    fn parse_invalid_json5_returns_err() {
        assert!(parse("{ not valid json5 !!!").is_err());
    }

    // ── load() file I/O ───────────────────────────────────────────────────────

    #[test]
    fn load_returns_none_for_missing_file() {
        assert!(load("/nonexistent/path/facter.conf").is_none());
    }

    #[test]
    fn load_returns_none_for_directory() {
        assert!(load("/tmp").is_none());
    }

    #[test]
    fn load_parses_real_file() {
        let dir = std::env::temp_dir().join("facter_config_test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("facter.conf");
        std::fs::write(&path, r#"{ facts: { blocklist: ["ssh"] } }"#).unwrap();
        let cfg = load(path.to_str().unwrap()).unwrap();
        assert_eq!(cfg.blocklist, vec!["ssh"]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn load_returns_none_on_bad_syntax() {
        let dir = std::env::temp_dir().join("facter_config_bad");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("facter.conf");
        std::fs::write(&path, "{ this is not json5 !!!").unwrap();
        assert!(load(path.to_str().unwrap()).is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn default_config_path_is_nonempty() {
        assert!(!default_config_path().is_empty());
    }

    #[test]
    fn load_returns_none_for_unreadable_file() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join("facter_config_unreadable");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("facter.conf");
        std::fs::write(&path, "{}").unwrap();
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o000);
        std::fs::set_permissions(&path, perms).unwrap();
        // Only meaningful when not running as root
        if std::env::var("USER").as_deref() != Ok("root") {
            assert!(load(path.to_str().unwrap()).is_none());
        }
        // Restore so cleanup can remove the file
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o644);
        std::fs::set_permissions(&path, perms).unwrap();
        let _ = std::fs::remove_dir_all(dir);
    }

    // ── parse_duration edge cases ─────────────────────────────────────────────

    #[test]
    fn duration_unknown_unit_treated_as_seconds() {
        // The fallback `_ => n` branch
        assert_eq!(parse_duration("5 fortnights"), 5);
    }

    #[test]
    fn duration_empty_string_returns_zero() {
        assert_eq!(parse_duration(""), 0);
    }
}
