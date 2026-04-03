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

fn parse(text: &str) -> Result<Config, String> {
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
pub fn parse_duration(s: &str) -> u64 {
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
