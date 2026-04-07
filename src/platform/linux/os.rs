use crate::platform::types::{MacosVersion, SelinuxData};
use std::collections::HashMap;
use std::fs;

pub fn read_os_release() -> HashMap<String, String> {
    let mut map = HashMap::new();
    for path in &["/etc/os-release", "/usr/lib/os-release", "/etc/lsb-release"] {
        if let Ok(content) = fs::read_to_string(path) {
            for line in content.lines() {
                let line = line.trim();
                if line.starts_with('#') || line.is_empty() {
                    continue;
                }
                if let Some((k, v)) = line.split_once('=') {
                    map.insert(k.trim().to_string(), strip_quotes(v.trim()));
                }
            }
            // Prefer os-release over lsb-release; stop after first found
            if path.ends_with("os-release") && !map.is_empty() {
                break;
            }
        }
    }
    map
}

pub fn collect_selinux() -> SelinuxData {
    let mut d = SelinuxData {
        supported: true,
        enabled: false,
        enforced: false,
        current_mode: String::new(),
        config_mode: String::new(),
        config_policy: String::new(),
        policy_version: String::new(),
    };

    let mountpoint = find_selinux_mountpoint();
    let config_exists = std::path::Path::new("/etc/selinux/config").exists();

    d.enabled = !mountpoint.is_empty() && config_exists;
    if !d.enabled {
        return d;
    }

    d.policy_version = fs::read_to_string(format!("{}/policyvers", mountpoint))
        .unwrap_or_default()
        .trim()
        .to_string();

    let enforce = fs::read_to_string(format!("{}/enforce", mountpoint))
        .unwrap_or_default();
    let enforce = enforce.trim();
    if enforce == "1" {
        d.enforced = true;
        d.current_mode = "enforcing".into();
    } else if !enforce.is_empty() {
        d.current_mode = "permissive".into();
    }

    if let Ok(config) = fs::read_to_string("/etc/selinux/config") {
        for line in config.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("SELINUX=") {
                d.config_mode = rest.to_string();
            } else if let Some(rest) = line.strip_prefix("SELINUXTYPE=") {
                d.config_policy = rest.to_string();
            }
        }
    }

    d
}

/// Linux does not have macOS product version.
pub fn macos_product_version() -> Option<MacosVersion> {
    None
}

fn find_selinux_mountpoint() -> String {
    let mounts = match fs::read_to_string("/proc/self/mounts") {
        Ok(m) => m,
        Err(_) => return String::new(),
    };
    for line in mounts.lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() >= 3 && cols[2] == "selinuxfs" {
            return cols[1].to_string();
        }
    }
    String::new()
}

fn strip_quotes(s: &str) -> String {
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}
