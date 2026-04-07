use crate::facts::value::FactValue;
use crate::resolvers::Resolver;
use indexmap::IndexMap;
use std::collections::HashMap;
use std::ffi::CStr;

pub struct OsResolver;

impl Resolver for OsResolver {
    fn name(&self) -> &'static str {
        "operating system"
    }

    fn resolve(&self) -> Vec<(String, FactValue)> {
        let mut facts = Vec::new();

        let (hardware, raw_arch) = hw_and_arch();
        let os_release = crate::platform::read_os_release();

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

        // SELinux (platform-aware)
        {
            let selinux = crate::platform::collect_selinux();
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

        // macOS-specific facts from SystemVersion.plist (platform-aware)
        if let Some(sv) = crate::platform::macos_product_version() {
            if !sv.product_name.is_empty() {
                facts.push(("macosx_productname".into(), FactValue::String(sv.product_name.clone())));
            }
            if !sv.product_version.is_empty() {
                facts.push(("macosx_productversion".into(), FactValue::String(sv.product_version.clone())));
                facts.push(("macosx_productversion_major".into(), FactValue::String(sv.major.clone())));
                facts.push(("macosx_productversion_minor".into(), FactValue::String(sv.minor.clone())));
                if !sv.patch.is_empty() {
                    facts.push(("macosx_productversion_patch".into(), FactValue::String(sv.patch.clone())));
                }
            }
            if !sv.build.is_empty() {
                facts.push(("macosx_buildversion".into(), FactValue::String(sv.build.clone())));
            }

            // os.macosx structured sub-map
            if !sv.product_version.is_empty() {
                let mut ver: IndexMap<String, FactValue> = IndexMap::new();
                ver.insert("full".into(), FactValue::String(sv.product_version.clone()));
                ver.insert("major".into(), FactValue::String(sv.major.clone()));
                ver.insert("minor".into(), FactValue::String(sv.minor.clone()));
                if !sv.patch.is_empty() {
                    ver.insert("patch".into(), FactValue::String(sv.patch.clone()));
                }
                let mut macosx: IndexMap<String, FactValue> = IndexMap::new();
                macosx.insert("version".into(), FactValue::Map(ver));
                if !sv.build.is_empty() {
                    macosx.insert("build".into(), FactValue::String(sv.build.clone()));
                }
                os.insert("macosx".into(), FactValue::Map(macosx));
            }
        }

        if !os.is_empty() {
            facts.push(("os".into(), FactValue::Map(os)));
        }

        facts
    }
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

#[cfg(test)]
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
        "macos" | "darwin" => "Darwin",
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

    if n.contains("darwin") {
        return "Darwin".into();
    }

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
fn distro_release_parts(name: &str, release: &str) -> (String, String) {
    let mut it = release.splitn(3, '.');
    let a = it.next().unwrap_or("").to_string();
    let b = it.next().unwrap_or("").to_string();
    let c = it.next().unwrap_or("").to_string();

    if name == "Ubuntu" || name == "LinuxMint" {
        let major = if b.is_empty() { a } else { format!("{}.{}", a, b) };
        (major, c)
    } else {
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

    // ── macOS os_name / os_family ────────────────────────────────────────────

    #[test]
    fn os_name_macos_id() {
        let mut or = std::collections::HashMap::new();
        or.insert("ID".to_string(), "macos".to_string());
        assert_eq!(os_name(&or), "Darwin");
    }

    #[test]
    fn os_family_darwin_returns_darwin() {
        assert_eq!(os_family("Darwin", ""), "Darwin");
    }
}
