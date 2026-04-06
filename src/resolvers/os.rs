use crate::facts::value::FactValue;
use crate::resolvers::Resolver;
use indexmap::IndexMap;
use std::collections::HashMap;
use std::ffi::CStr;
use std::fs;

pub struct OsResolver;

impl Resolver for OsResolver {
    fn name(&self) -> &'static str {
        "operating system"
    }

    fn resolve(&self) -> Vec<(String, FactValue)> {
        let mut facts = Vec::new();

        let (hardware, raw_arch) = hw_and_arch();
        let os_release = read_os_release();

        let name = os_name(&os_release);
        let family = os_family(&name, os_release.get("ID_LIKE").map(String::as_str).unwrap_or(""));
        let (release_full, release_major, release_minor) = parse_release(&name, &os_release);

        // Adjust architecture label per distro (matching C++ facter behaviour)
        let arch = adjust_arch(&raw_arch, &name);

        let mut os: IndexMap<String, FactValue> = IndexMap::new();

        if !name.is_empty() {
            facts.push(("operatingsystem".into(), FactValue::String(name.clone())));
            os.insert("name".into(), FactValue::String(name.clone()));
        }
        if !family.is_empty() {
            facts.push(("osfamily".into(), FactValue::String(family.clone())));
            os.insert("family".into(), FactValue::String(family));
        }
        if !hardware.is_empty() {
            facts.push(("hardwaremodel".into(), FactValue::String(hardware.clone())));
            os.insert("hardware".into(), FactValue::String(hardware));
        }
        if !arch.is_empty() {
            facts.push(("architecture".into(), FactValue::String(arch.clone())));
            os.insert("architecture".into(), FactValue::String(arch));
        }

        if !release_full.is_empty() {
            let mut rel: IndexMap<String, FactValue> = IndexMap::new();
            facts.push(("operatingsystemrelease".into(), FactValue::String(release_full.clone())));
            rel.insert("full".into(), FactValue::String(release_full));
            if !release_major.is_empty() {
                facts.push(("operatingsystemmajrelease".into(), FactValue::String(release_major.clone())));
                rel.insert("major".into(), FactValue::String(release_major));
            }
            if !release_minor.is_empty() {
                rel.insert("minor".into(), FactValue::String(release_minor));
            }
            os.insert("release".into(), FactValue::Map(rel));
        }

        // distro sub-map from /etc/os-release fields
        let distro = build_distro(&name, &os_release);
        if !distro.is_empty() {
            os.insert("distro".into(), FactValue::Map(distro));
        }

        // SELinux
        let selinux = collect_selinux();
        if selinux.supported {
            facts.push(("selinux".into(), FactValue::Boolean(selinux.enabled)));
            let mut sel_map: IndexMap<String, FactValue> = IndexMap::new();
            sel_map.insert("enabled".into(), FactValue::Boolean(selinux.enabled));
            if selinux.enabled {
                facts.push(("selinux_enforced".into(), FactValue::Boolean(selinux.enforced)));
                sel_map.insert("enforced".into(), FactValue::Boolean(selinux.enforced));
                if !selinux.current_mode.is_empty() {
                    facts.push(("selinux_current_mode".into(), FactValue::String(selinux.current_mode.clone())));
                    sel_map.insert("current_mode".into(), FactValue::String(selinux.current_mode));
                }
                if !selinux.config_mode.is_empty() {
                    facts.push(("selinux_config_mode".into(), FactValue::String(selinux.config_mode.clone())));
                    sel_map.insert("config_mode".into(), FactValue::String(selinux.config_mode));
                }
                if !selinux.config_policy.is_empty() {
                    facts.push(("selinux_config_policy".into(), FactValue::String(selinux.config_policy.clone())));
                    sel_map.insert("config_policy".into(), FactValue::String(selinux.config_policy));
                }
                if !selinux.policy_version.is_empty() {
                    facts.push(("selinux_policyversion".into(), FactValue::String(selinux.policy_version.clone())));
                    sel_map.insert("policy_version".into(), FactValue::String(selinux.policy_version));
                }
            }
            os.insert("selinux".into(), FactValue::Map(sel_map));
        }

        // Legacy lsb* flat facts (sourced from os-release when lsb_release is absent)
        if let Some(id) = os_release.get("ID").or_else(|| os_release.get("DISTRIB_ID")) {
            facts.push(("lsbdistid".into(), FactValue::String(capitalize(id))));
        }
        if let Some(rel) = os_release.get("VERSION_ID").or_else(|| os_release.get("DISTRIB_RELEASE")) {
            facts.push(("lsbdistrelease".into(), FactValue::String(rel.clone())));
        }
        if let Some(cn) = os_release
            .get("VERSION_CODENAME")
            .or_else(|| os_release.get("DISTRIB_CODENAME"))
        {
            facts.push(("lsbdistcodename".into(), FactValue::String(cn.clone())));
        }
        if let Some(desc) = os_release
            .get("PRETTY_NAME")
            .or_else(|| os_release.get("DISTRIB_DESCRIPTION"))
        {
            facts.push(("lsbdistdescription".into(), FactValue::String(desc.clone())));
        }

        if !os.is_empty() {
            facts.push(("os".into(), FactValue::Map(os)));
        }

        facts
    }
}

// ── SELinux ───────────────────────────────────────────────────────────────────

struct SelinuxData {
    supported: bool,
    enabled: bool,
    enforced: bool,
    current_mode: String,
    config_mode: String,
    config_policy: String,
    policy_version: String,
}

fn collect_selinux() -> SelinuxData {
    let mut d = SelinuxData {
        supported: true,
        enabled: false,
        enforced: false,
        current_mode: String::new(),
        config_mode: String::new(),
        config_policy: String::new(),
        policy_version: String::new(),
    };

    // Find selinuxfs mount point from /proc/self/mounts
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

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Returns (hardware_model, architecture) from uname.
fn hw_and_arch() -> (String, String) {
    let mut u: libc::utsname = unsafe { std::mem::zeroed() };
    if unsafe { libc::uname(&mut u) } != 0 {
        return (String::new(), String::new());
    }
    let machine = unsafe { CStr::from_ptr(u.machine.as_ptr()).to_string_lossy().into_owned() };
    (machine.clone(), machine)
}

/// Adjust architecture label to match facter's distro-specific mappings.
fn adjust_arch(arch: &str, distro: &str) -> String {
    let distro_lower = distro.to_lowercase();
    let debian_like = ["ubuntu", "debian", "linuxmint", "linux mint", "kali", "raspbian", "gentoo"];
    if arch == "x86_64" && debian_like.iter().any(|d| distro_lower.contains(d)) {
        return "amd64".to_string();
    }
    if arch.starts_with('i') && arch.ends_with("86") {
        let is_gentoo = distro_lower.contains("gentoo");
        return if is_gentoo { "x86" } else { "i386" }.to_string();
    }
    arch.to_string()
}

/// Parse /etc/os-release (and /etc/lsb-release as fallback) into a key→value map.
fn read_os_release() -> HashMap<String, String> {
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

fn strip_quotes(s: &str) -> String {
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

/// Determine the canonical OS name (matches facter's `operatingsystem` fact).
fn os_name(or: &HashMap<String, String>) -> String {
    let id = or.get("ID").map(String::as_str).unwrap_or("");
    let name = or.get("NAME").map(String::as_str).unwrap_or("");

    // Map common IDs to the names facter uses
    match id.to_lowercase().as_str() {
        "ubuntu" => "Ubuntu",
        "debian" => "Debian",
        "linuxmint" => "LinuxMint",
        "raspbian" => "Raspbian",
        "kali" => "Kali",
        "fedora" => "Fedora",
        "centos" | "centos-stream" => "CentOS",
        "rhel" | "rhcos" => "RedHat",
        "almalinux" => "AlmaLinux",
        "rocky" => "Rocky",
        "ol" | "oracle" => "OracleLinux",
        "amzn" => "Amazon",
        "cloudlinux" => "CloudLinux",
        "sles" | "sled" => "SLES",
        "opensuse" | "opensuse-leap" | "opensuse-tumbleweed" => "OpenSuSE",
        "arch" | "archlinux" => "Arch",
        "gentoo" => "Gentoo",
        "alpine" => "Alpine",
        "photon" => "PhotonOS",
        "nixos" => "NixOS",
        _ => {
            // Fall back to the NAME field with spaces removed, or the raw ID
            if !name.is_empty() {
                return name
                    .split_whitespace()
                    .map(capitalize)
                    .collect::<Vec<_>>()
                    .join("");
            }
            id
        }
    }
    .to_string()
}

/// Map OS name to facter's `osfamily` fact value.
fn os_family(name: &str, id_like: &str) -> String {
    let n = name.to_lowercase();
    let il = id_like.to_lowercase();

    let debian = ["ubuntu", "debian", "linuxmint", "raspbian", "kali", "devuan"];
    let redhat = [
        "redhat",
        "centos",
        "fedora",
        "almalinux",
        "rocky",
        "oraclelinux",
        "amazon",
        "cloudlinux",
        "scientific",
    ];
    let suse = ["sles", "opensuse", "suse"];
    let arch = ["arch"];
    let gentoo = ["gentoo"];
    let mandrake = ["mandriva", "mandrake", "mageia"];

    for d in debian {
        if n.contains(d) || il.contains(d) {
            return "Debian".into();
        }
    }
    for r in redhat {
        if n.contains(r) || il.contains(r) {
            return "RedHat".into();
        }
    }
    for s in suse {
        if n.contains(s) || il.contains(s) {
            return "Suse".into();
        }
    }
    for a in arch {
        if n.contains(a) || il.contains(a) {
            return "Arch".into();
        }
    }
    for g in gentoo {
        if n.contains(g) {
            return "Gentoo".into();
        }
    }
    for m in mandrake {
        if n.contains(m) {
            return "Mandrake".into();
        }
    }

    name.to_string()
}

/// Returns (full, major, minor) release strings.
fn parse_release(name: &str, or: &HashMap<String, String>) -> (String, String, String) {
    let full = or
        .get("VERSION_ID")
        .or_else(|| or.get("DISTRIB_RELEASE"))
        .cloned()
        .unwrap_or_default();

    if full.is_empty() {
        return (String::new(), String::new(), String::new());
    }

    let (major, minor) = distro_release_parts(name, &full);

    (full, major, minor)
}

/// Build the `os.distro` sub-map.
fn build_distro(name: &str, or: &HashMap<String, String>) -> IndexMap<String, FactValue> {
    let mut distro: IndexMap<String, FactValue> = IndexMap::new();

    let id = or
        .get("ID")
        .map(|s| capitalize(s))
        .unwrap_or_else(|| name.to_string());

    if !id.is_empty() {
        distro.insert("id".into(), FactValue::String(id));
    }
    if let Some(cn) = or.get("VERSION_CODENAME").or_else(|| or.get("DISTRIB_CODENAME")) {
        if !cn.is_empty() {
            distro.insert("codename".into(), FactValue::String(cn.clone()));
        }
    }
    if let Some(desc) = or.get("PRETTY_NAME").or_else(|| or.get("DISTRIB_DESCRIPTION")) {
        if !desc.is_empty() {
            distro.insert("description".into(), FactValue::String(desc.clone()));
        }
    }

    let rel_full = or
        .get("VERSION_ID")
        .or_else(|| or.get("DISTRIB_RELEASE"))
        .cloned()
        .unwrap_or_default();

    if !rel_full.is_empty() {
        let mut rel: IndexMap<String, FactValue> = IndexMap::new();
        let (major, minor) = distro_release_parts(name, &rel_full);
        rel.insert("full".into(), FactValue::String(rel_full));
        if !major.is_empty() {
            rel.insert("major".into(), FactValue::String(major));
        }
        if !minor.is_empty() {
            rel.insert("minor".into(), FactValue::String(minor));
        }
        distro.insert("release".into(), FactValue::Map(rel));
    }

    distro
}

/// Split a release string into (major, minor) matching C++ facter's parse_distro logic.
/// For Ubuntu/LinuxMint: major = "M.mm" (e.g. "20.04"), minor = patch if present (e.g. "1").
/// For others: major = first component, minor = second component (e.g. "7", "9").
fn distro_release_parts(name: &str, release: &str) -> (String, String) {
    let mut it = release.splitn(3, '.');
    let a = it.next().unwrap_or("").to_string();
    let b = it.next().unwrap_or("").to_string();
    let c = it.next().unwrap_or("").to_string();

    if name == "Ubuntu" || name == "LinuxMint" {
        // major = "M.mm", minor = patch (usually empty)
        let major = if b.is_empty() { a } else { format!("{}.{}", a, b) };
        (major, c)
    } else {
        // major = before first dot, minor = between first and second dot
        (a, b)
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── strip_quotes ─────────────────────────────────────────────────────────

    #[test]
    fn strip_quotes_double() {
        assert_eq!(strip_quotes(r#""Ubuntu 22.04""#), "Ubuntu 22.04");
    }

    #[test]
    fn strip_quotes_single() {
        assert_eq!(strip_quotes("'focal'"), "focal");
    }

    #[test]
    fn strip_quotes_no_quotes() {
        assert_eq!(strip_quotes("Ubuntu"), "Ubuntu");
    }

    #[test]
    fn strip_quotes_mismatched_leaves_unchanged() {
        assert_eq!(strip_quotes(r#""Ubuntu'"#), r#""Ubuntu'"#);
    }

    // ── capitalize ───────────────────────────────────────────────────────────

    #[test]
    fn capitalize_lowercase() {
        assert_eq!(capitalize("ubuntu"), "Ubuntu");
    }

    #[test]
    fn capitalize_already_capital() {
        assert_eq!(capitalize("Ubuntu"), "Ubuntu");
    }

    #[test]
    fn capitalize_empty() {
        assert_eq!(capitalize(""), "");
    }

    // ── adjust_arch ──────────────────────────────────────────────────────────

    #[test]
    fn adjust_arch_ubuntu_x86_64_becomes_amd64() {
        assert_eq!(adjust_arch("x86_64", "Ubuntu"), "amd64");
    }

    #[test]
    fn adjust_arch_debian_x86_64_becomes_amd64() {
        assert_eq!(adjust_arch("x86_64", "Debian"), "amd64");
    }

    #[test]
    fn adjust_arch_fedora_x86_64_unchanged() {
        assert_eq!(adjust_arch("x86_64", "Fedora"), "x86_64");
    }

    #[test]
    fn adjust_arch_i686_non_gentoo_becomes_i386() {
        assert_eq!(adjust_arch("i686", "Ubuntu"), "i386");
    }

    #[test]
    fn adjust_arch_i686_gentoo_becomes_x86() {
        assert_eq!(adjust_arch("i686", "Gentoo"), "x86");
    }

    // ── os_family ────────────────────────────────────────────────────────────

    #[test]
    fn os_family_ubuntu_is_debian() {
        assert_eq!(os_family("Ubuntu", ""), "Debian");
    }

    #[test]
    fn os_family_centos_is_redhat() {
        assert_eq!(os_family("CentOS", ""), "RedHat");
    }

    #[test]
    fn os_family_fedora_is_redhat() {
        assert_eq!(os_family("Fedora", ""), "RedHat");
    }

    #[test]
    fn os_family_opensuse_is_suse() {
        assert_eq!(os_family("OpenSuSE", ""), "Suse");
    }

    #[test]
    fn os_family_id_like_debian_wins() {
        assert_eq!(os_family("SomeDistro", "debian"), "Debian");
    }

    #[test]
    fn os_family_unknown_returns_name() {
        assert_eq!(os_family("CustomOS", ""), "CustomOS");
    }

    // ── distro_release_parts ─────────────────────────────────────────────────

    #[test]
    fn distro_release_parts_ubuntu_two_component() {
        let (major, minor) = distro_release_parts("Ubuntu", "20.04");
        assert_eq!(major, "20.04");
        assert_eq!(minor, "");
    }

    #[test]
    fn distro_release_parts_ubuntu_three_component() {
        let (major, minor) = distro_release_parts("Ubuntu", "20.04.1");
        assert_eq!(major, "20.04");
        assert_eq!(minor, "1");
    }

    #[test]
    fn distro_release_parts_centos_two_component() {
        let (major, minor) = distro_release_parts("CentOS", "7.9");
        assert_eq!(major, "7");
        assert_eq!(minor, "9");
    }

    #[test]
    fn distro_release_parts_fedora_single_component() {
        let (major, minor) = distro_release_parts("Fedora", "36");
        assert_eq!(major, "36");
        assert_eq!(minor, "");
    }

    #[test]
    fn distro_release_parts_linuxmint_three_component() {
        let (major, minor) = distro_release_parts("LinuxMint", "20.3.1");
        assert_eq!(major, "20.3");
        assert_eq!(minor, "1");
    }
}
