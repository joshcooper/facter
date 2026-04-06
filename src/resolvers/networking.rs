use crate::facts::value::FactValue;
use crate::resolvers::Resolver;
use indexmap::IndexMap;
use std::collections::HashMap;
use std::ffi::CStr;
use std::fs;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::path::Path;

pub struct NetworkingResolver;

impl Resolver for NetworkingResolver {
    fn name(&self) -> &'static str {
        "networking"
    }

    fn resolve(&self) -> Vec<(String, FactValue)> {
        let mut facts = Vec::new();

        let hostname = get_hostname();
        let domain = get_domain();
        let fqdn = if domain.is_empty() {
            hostname.clone()
        } else {
            format!("{}.{}", hostname, domain)
        };

        let interfaces = collect_interfaces();
        let primary = get_primary_interface().unwrap_or_default();
        let dhcp_servers = collect_dhcp_servers();

        // Legacy flat facts
        if !hostname.is_empty() {
            facts.push(("hostname".into(), FactValue::String(hostname.clone())));
        }
        if !domain.is_empty() {
            facts.push(("domain".into(), FactValue::String(domain.clone())));
        }
        if !fqdn.is_empty() {
            facts.push(("fqdn".into(), FactValue::String(fqdn.clone())));
        }

        // Determine primary interface data for top-level networking facts
        let primary_iface = interfaces.iter().find(|i| i.name == primary);
        let primary_ip = primary_iface
            .and_then(|i| i.ipv4.first())
            .map(|b| b.address.to_string())
            .unwrap_or_default();
        let primary_ip6 = primary_iface
            .and_then(|i| i.ipv6.first())
            .map(|b| b.address.to_string())
            .unwrap_or_default();
        let primary_mac = primary_iface
            .and_then(|i| i.mac.as_deref())
            .unwrap_or_default()
            .to_string();
        let primary_netmask = primary_iface
            .and_then(|i| i.ipv4.first())
            .and_then(|b| b.netmask)
            .map(|n| n.to_string())
            .unwrap_or_default();
        let primary_network = primary_iface
            .and_then(|i| i.ipv4.first())
            .and_then(|b| b.netmask.map(|nm| ipv4_network(b.address, nm)))
            .map(|n| n.to_string())
            .unwrap_or_default();
        let primary_netmask6 = primary_iface
            .and_then(|i| i.ipv6.first())
            .and_then(|b| b.netmask)
            .map(|n| n.to_string())
            .unwrap_or_default();
        let primary_network6 = primary_iface
            .and_then(|i| i.ipv6.first())
            .and_then(|b| b.netmask.map(|nm| ipv6_network(b.address, nm)))
            .map(|n| n.to_string())
            .unwrap_or_default();
        let primary_scope6 = primary_iface
            .and_then(|i| i.ipv6.first())
            .map(|b| ipv6_scope(&b.address))
            .unwrap_or_default();
        let primary_mtu = primary_iface.and_then(|i| i.mtu).unwrap_or(0);

        if !primary_ip.is_empty() {
            facts.push(("ipaddress".into(), FactValue::String(primary_ip.clone())));
        }
        if !primary_ip6.is_empty() {
            facts.push(("ipaddress6".into(), FactValue::String(primary_ip6.clone())));
        }
        if !primary_mac.is_empty() {
            facts.push(("macaddress".into(), FactValue::String(primary_mac.clone())));
        }
        if !primary_netmask.is_empty() {
            facts.push(("netmask".into(), FactValue::String(primary_netmask.clone())));
        }

        // Per-interface legacy flat facts
        let iface_names: Vec<String> = interfaces.iter().map(|i| i.name.clone()).collect();
        for iface in &interfaces {
            let n = &iface.name;
            if let Some(b) = iface.ipv4.first() {
                facts.push((format!("ipaddress_{}", n), FactValue::String(b.address.to_string())));
                if let Some(nm) = b.netmask {
                    facts.push((format!("netmask_{}", n), FactValue::String(nm.to_string())));
                    facts.push((
                        format!("network_{}", n),
                        FactValue::String(ipv4_network(b.address, nm).to_string()),
                    ));
                }
            }
            if let Some(b) = iface.ipv6.first() {
                facts.push((format!("ipaddress6_{}", n), FactValue::String(b.address.to_string())));
            }
            if let Some(mac) = &iface.mac {
                facts.push((format!("macaddress_{}", n), FactValue::String(mac.clone())));
            }
            if let Some(mtu) = iface.mtu {
                facts.push((format!("mtu_{}", n), FactValue::Integer(mtu as i64)));
            }
        }

        facts.push((
            "interfaces".into(),
            FactValue::String(iface_names.join(",").to_string()),
        ));

        // Structured networking fact
        let mut net: IndexMap<String, FactValue> = IndexMap::new();
        if !hostname.is_empty() {
            net.insert("hostname".into(), FactValue::String(hostname));
        }
        if !domain.is_empty() {
            net.insert("domain".into(), FactValue::String(domain));
        }
        if !fqdn.is_empty() {
            net.insert("fqdn".into(), FactValue::String(fqdn));
        }
        if !primary.is_empty() {
            net.insert("primary".into(), FactValue::String(primary.clone()));
        }
        if !primary_ip.is_empty() {
            net.insert("ip".into(), FactValue::String(primary_ip));
        }
        if !primary_ip6.is_empty() {
            net.insert("ip6".into(), FactValue::String(primary_ip6));
        }
        if !primary_mac.is_empty() {
            net.insert("mac".into(), FactValue::String(primary_mac));
        }
        if !primary_netmask.is_empty() {
            net.insert("netmask".into(), FactValue::String(primary_netmask));
        }
        if !primary_network.is_empty() {
            net.insert("network".into(), FactValue::String(primary_network));
        }
        if !primary_netmask6.is_empty() {
            net.insert("netmask6".into(), FactValue::String(primary_netmask6));
        }
        if !primary_network6.is_empty() {
            net.insert("network6".into(), FactValue::String(primary_network6));
        }
        if !primary_scope6.is_empty() {
            net.insert("scope6".into(), FactValue::String(primary_scope6));
        }
        if primary_mtu > 0 {
            net.insert("mtu".into(), FactValue::Integer(primary_mtu as i64));
        }

        // Per-interface structured map
        let mut ifaces_map: IndexMap<String, FactValue> = IndexMap::new();
        for iface in &interfaces {
            let dhcp = dhcp_servers.get(&iface.name).cloned();
            ifaces_map.insert(iface.name.clone(), build_interface_map(iface, dhcp));
        }
        if !ifaces_map.is_empty() {
            net.insert("interfaces".into(), FactValue::Map(ifaces_map));
        }

        facts.push(("networking".into(), FactValue::Map(net)));

        facts
    }
}

// ── Data types ───────────────────────────────────────────────────────────────

struct Ipv4Binding {
    address: Ipv4Addr,
    netmask: Option<Ipv4Addr>,
}

struct Ipv6Binding {
    address: Ipv6Addr,
    netmask: Option<Ipv6Addr>,
    flags: Vec<String>,
}

struct Interface {
    name: String,
    ipv4: Vec<Ipv4Binding>,
    ipv6: Vec<Ipv6Binding>,
    mac: Option<String>,
    mtu: Option<u64>,
    physical: bool,
    operational_state: Option<String>,
    duplex: Option<String>,
    speed: Option<i64>,
}

// ── Interface collection via getifaddrs ──────────────────────────────────────

fn collect_interfaces() -> Vec<Interface> {
    let mut map: HashMap<String, Interface> = HashMap::new();

    unsafe {
        let mut ifaddrs: *mut libc::ifaddrs = std::ptr::null_mut();
        if libc::getifaddrs(&mut ifaddrs) != 0 {
            return Vec::new();
        }

        let mut ptr = ifaddrs;
        while !ptr.is_null() {
            let ifa = &*ptr;
            let name = CStr::from_ptr(ifa.ifa_name).to_string_lossy().into_owned();

            let entry = map.entry(name.clone()).or_insert_with(|| Interface {
                name: name.clone(),
                ipv4: Vec::new(),
                ipv6: Vec::new(),
                mac: None,
                mtu: None,
                physical: false,
                operational_state: None,
                duplex: None,
                speed: None,
            });

            if !ifa.ifa_addr.is_null() {
                let family = (*ifa.ifa_addr).sa_family as i32;

                if family == libc::AF_INET {
                    let sa = ifa.ifa_addr as *const libc::sockaddr_in;
                    let nm = ifa.ifa_netmask as *const libc::sockaddr_in;

                    let address = Ipv4Addr::from((*sa).sin_addr.s_addr.to_ne_bytes());
                    let netmask = if !nm.is_null() {
                        Some(Ipv4Addr::from((*nm).sin_addr.s_addr.to_ne_bytes()))
                    } else {
                        None
                    };
                    entry.ipv4.push(Ipv4Binding { address, netmask });
                } else if family == libc::AF_INET6 {
                    let sa = ifa.ifa_addr as *const libc::sockaddr_in6;
                    let nm = ifa.ifa_netmask as *const libc::sockaddr_in6;

                    let address = Ipv6Addr::from((*sa).sin6_addr.s6_addr);
                    let netmask = if !nm.is_null() {
                        Some(Ipv6Addr::from((*nm).sin6_addr.s6_addr))
                    } else {
                        None
                    };
                    entry.ipv6.push(Ipv6Binding { address, netmask, flags: Vec::new() });
                }
            }

            ptr = ifa.ifa_next;
        }

        libc::freeifaddrs(ifaddrs);
    }

    // Augment with MAC, MTU, and additional fields from sysfs
    for iface in map.values_mut() {
        let base = format!("/sys/class/net/{}", iface.name);

        if let Ok(mac) = fs::read_to_string(format!("{}/address", base)) {
            let mac = mac.trim().to_string();
            if !mac.is_empty() && mac != "00:00:00:00:00:00" {
                iface.mac = Some(mac);
            }
        }
        if let Ok(mtu_str) = fs::read_to_string(format!("{}/mtu", base)) {
            if let Ok(mtu) = mtu_str.trim().parse::<u64>() {
                iface.mtu = Some(mtu);
            }
        }
        iface.physical = Path::new(&format!("{}/device", base)).exists();
        if let Ok(s) = fs::read_to_string(format!("{}/operstate", base)) {
            let s = s.trim().to_string();
            if !s.is_empty() {
                iface.operational_state = Some(s);
            }
        }
        if let Ok(s) = fs::read_to_string(format!("{}/duplex", base)) {
            let s = s.trim().to_string();
            if !s.is_empty() {
                iface.duplex = Some(s);
            }
        }
        if let Ok(s) = fs::read_to_string(format!("{}/speed", base)) {
            if let Ok(v) = s.trim().parse::<i64>() {
                if v > 0 {
                    iface.speed = Some(v);
                }
            }
        }
    }

    // Augment IPv6 bindings with IFA_F_* flags from /proc/net/if_inet6
    let ifa_flags = read_if_inet6_flags();
    for iface in map.values_mut() {
        for binding in iface.ipv6.iter_mut() {
            let key = (iface.name.clone(), binding.address);
            if let Some(flags) = ifa_flags.get(&key) {
                binding.flags = flags.clone();
            }
        }
    }

    let mut result: Vec<Interface> = map.into_values().collect();
    result.sort_by(|a, b| a.name.cmp(&b.name));
    result
}

/// Parse /proc/net/if_inet6 and return a map of (ifname, addr) → flag name list.
fn read_if_inet6_flags() -> HashMap<(String, Ipv6Addr), Vec<String>> {
    const IFA_FLAGS: &[(u32, &str)] = &[
        (0x01, "secondary"),
        (0x02, "nodad"),
        (0x04, "optimistic"),
        (0x08, "dadfailed"),
        (0x10, "homeaddress"),
        (0x20, "deprecated"),
        (0x40, "tentative"),
        (0x80, "permanent"),
        (0x100, "managedaddr"),
        (0x200, "noprefixroute"),
        (0x400, "mcautojoin"),
        (0x800, "stable_privacy"),
    ];

    let mut map = HashMap::new();
    let content = match fs::read_to_string("/proc/net/if_inet6") {
        Ok(c) => c,
        Err(_) => return map,
    };
    for line in content.lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 6 {
            continue;
        }
        let addr_hex = cols[0];
        let flags_hex = u32::from_str_radix(cols[4], 16).unwrap_or(0);
        let ifname = cols[5].to_string();

        // Parse 32 hex chars into 16 bytes
        if addr_hex.len() != 32 {
            continue;
        }
        let mut bytes = [0u8; 16];
        let mut ok = true;
        for i in 0..16 {
            match u8::from_str_radix(&addr_hex[i * 2..i * 2 + 2], 16) {
                Ok(b) => bytes[i] = b,
                Err(_) => { ok = false; break; }
            }
        }
        if !ok {
            continue;
        }
        let addr = Ipv6Addr::from(bytes);
        let flags: Vec<String> = IFA_FLAGS
            .iter()
            .filter(|(bit, _)| flags_hex & bit != 0)
            .map(|(_, name)| name.to_string())
            .collect();
        map.insert((ifname, addr), flags);
    }
    map
}

// ── Routing table ────────────────────────────────────────────────────────────

/// Read /proc/net/route to find the interface used for the default route.
fn get_primary_interface() -> Option<String> {
    let content = fs::read_to_string("/proc/net/route").ok()?;
    for line in content.lines().skip(1) {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 8 {
            continue;
        }
        let dest = u32::from_str_radix(cols[1], 16).ok()?;
        let flags = u32::from_str_radix(cols[3], 16).ok()?;
        const RTF_UP: u32 = 0x0001;
        const RTF_GATEWAY: u32 = 0x0002;
        if dest == 0 && flags & RTF_UP != 0 && flags & RTF_GATEWAY != 0 {
            return Some(cols[0].to_string());
        }
    }
    None
}

// ── Hostname / domain ────────────────────────────────────────────────────────

fn get_hostname() -> String {
    let mut buf = vec![0u8; 256];
    if unsafe { libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len()) } == 0 {
        // Hostname is null-terminated; strip at null byte
        let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        String::from_utf8_lossy(&buf[..end]).into_owned()
    } else {
        String::new()
    }
}

fn get_domain() -> String {
    // Try /etc/resolv.conf first
    if let Ok(content) = fs::read_to_string("/etc/resolv.conf") {
        for line in content.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("domain ") {
                let d = rest.trim().trim_end_matches('.');
                if !d.is_empty() {
                    return d.to_string();
                }
            }
            if let Some(rest) = line.strip_prefix("search ") {
                if let Some(first) = rest.split_whitespace().next() {
                    let d = first.trim_end_matches('.');
                    if !d.is_empty() {
                        return d.to_string();
                    }
                }
            }
        }
    }
    String::new()
}

// ── Structured interface map builder ─────────────────────────────────────────

fn build_interface_map(iface: &Interface, dhcp: Option<String>) -> FactValue {
    let mut m: IndexMap<String, FactValue> = IndexMap::new();
    if let Some(server) = dhcp {
        m.insert("dhcp".into(), FactValue::String(server));
    }

    if !iface.ipv4.is_empty() {
        // Primary binding fields
        let first = &iface.ipv4[0];
        m.insert("ip".into(), FactValue::String(first.address.to_string()));
        if let Some(nm) = first.netmask {
            m.insert("netmask".into(), FactValue::String(nm.to_string()));
            m.insert(
                "network".into(),
                FactValue::String(ipv4_network(first.address, nm).to_string()),
            );
        }

        // All bindings array
        let bindings: Vec<FactValue> = iface
            .ipv4
            .iter()
            .map(|b| {
                let mut bm: IndexMap<String, FactValue> = IndexMap::new();
                bm.insert("address".into(), FactValue::String(b.address.to_string()));
                if let Some(nm) = b.netmask {
                    bm.insert("netmask".into(), FactValue::String(nm.to_string()));
                    bm.insert(
                        "network".into(),
                        FactValue::String(ipv4_network(b.address, nm).to_string()),
                    );
                }
                FactValue::Map(bm)
            })
            .collect();
        m.insert("bindings".into(), FactValue::Array(bindings));
    }

    if !iface.ipv6.is_empty() {
        let first = &iface.ipv6[0];
        m.insert("ip6".into(), FactValue::String(first.address.to_string()));
        if let Some(nm) = first.netmask {
            m.insert("netmask6".into(), FactValue::String(nm.to_string()));
            m.insert(
                "network6".into(),
                FactValue::String(ipv6_network(first.address, nm).to_string()),
            );
        }
        // Note: network6 stays at interface level (matches C++ facter); bindings6 uses "network"
        m.insert(
            "scope6".into(),
            FactValue::String(ipv6_scope(&first.address)),
        );

        let bindings6: Vec<FactValue> = iface
            .ipv6
            .iter()
            .map(|b| {
                let mut bm: IndexMap<String, FactValue> = IndexMap::new();
                bm.insert("address".into(), FactValue::String(b.address.to_string()));
                if !b.flags.is_empty() {
                    let flag_vals: Vec<FactValue> = b.flags.iter().cloned().map(FactValue::String).collect();
                    bm.insert("flags".into(), FactValue::Array(flag_vals));
                }
                if let Some(nm) = b.netmask {
                    bm.insert("netmask".into(), FactValue::String(nm.to_string()));
                    bm.insert(
                        "network".into(),
                        FactValue::String(ipv6_network(b.address, nm).to_string()),
                    );
                }
                bm.insert("scope6".into(), FactValue::String(ipv6_scope(&b.address)));
                FactValue::Map(bm)
            })
            .collect();
        m.insert("bindings6".into(), FactValue::Array(bindings6));
    }

    if let Some(mac) = &iface.mac {
        m.insert("mac".into(), FactValue::String(mac.clone()));
    }
    if let Some(mtu) = iface.mtu {
        m.insert("mtu".into(), FactValue::Integer(mtu as i64));
    }
    m.insert("physical".into(), FactValue::Boolean(iface.physical));
    if let Some(ref s) = iface.operational_state {
        m.insert("operational_state".into(), FactValue::String(s.clone()));
    }
    if iface.physical {
        if let Some(ref d) = iface.duplex {
            m.insert("duplex".into(), FactValue::String(d.clone()));
        }
        if let Some(s) = iface.speed {
            m.insert("speed".into(), FactValue::Integer(s));
        }
    }

    FactValue::Map(m)
}

// ── Network calculation helpers ───────────────────────────────────────────────

fn ipv4_network(addr: Ipv4Addr, netmask: Ipv4Addr) -> Ipv4Addr {
    let a = u32::from(addr);
    let m = u32::from(netmask);
    Ipv4Addr::from(a & m)
}

/// Parse dhclient lease files and return a map of interface name → DHCP server address.
/// The last lease block per interface wins (most recent).
fn collect_dhcp_servers() -> HashMap<String, String> {
    const LEASE_FILES: &[&str] = &[
        "/var/lib/dhcp/dhclient.leases",
        "/var/lib/dhclient/dhclient.leases",
        "/var/lib/dhclient/dhclient6.leases",
    ];

    let mut result: HashMap<String, String> = HashMap::new();

    for path in LEASE_FILES {
        if let Ok(content) = fs::read_to_string(path) {
            parse_dhclient_leases(&content, &mut result);
        }
    }

    // Also scan /var/lib/dhclient/ for per-interface lease files (dhclient-<iface>.leases)
    if let Ok(entries) = fs::read_dir("/var/lib/dhclient") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.ends_with(".leases") && name.starts_with("dhclient") {
                if let Ok(content) = fs::read_to_string(entry.path()) {
                    parse_dhclient_leases(&content, &mut result);
                }
            }
        }
    }

    result
}

fn parse_dhclient_leases(content: &str, result: &mut HashMap<String, String>) {
    let mut current_iface: Option<String> = None;
    let mut current_server: Option<String> = None;

    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("lease {") {
            current_iface = None;
            current_server = None;
        } else if let Some(iface) = line.strip_prefix("interface ") {
            let iface = iface.trim_matches(|c| c == '"' || c == ';').to_string();
            current_iface = Some(iface);
        } else if let Some(rest) = line.strip_prefix("option dhcp-server-identifier ") {
            let server = rest.trim_end_matches(';').trim().to_string();
            current_server = Some(server);
        } else if line == "}" {
            if let (Some(iface), Some(server)) = (current_iface.take(), current_server.take()) {
                result.insert(iface, server);
            }
        }
    }
}

fn ipv6_scope(addr: &Ipv6Addr) -> String {
    if addr.is_loopback() {
        "host".to_string()
    } else {
        let o = addr.octets();
        if o[0] == 0xfe && (o[1] & 0xc0) == 0x80 {
            "link".to_string()
        } else {
            "global".to_string()
        }
    }
}

fn ipv6_network(addr: Ipv6Addr, netmask: Ipv6Addr) -> Ipv6Addr {
    let a = addr.octets();
    let m = netmask.octets();
    let mut r = [0u8; 16];
    for i in 0..16 {
        r[i] = a[i] & m[i];
    }
    Ipv6Addr::from(r)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // ── ipv6_scope ───────────────────────────────────────────────────────────

    #[test]
    fn ipv6_scope_loopback() {
        let addr: Ipv6Addr = "::1".parse().unwrap();
        assert_eq!(ipv6_scope(&addr), "host");
    }

    #[test]
    fn ipv6_scope_link_local() {
        let addr: Ipv6Addr = "fe80::1".parse().unwrap();
        assert_eq!(ipv6_scope(&addr), "link");
    }

    #[test]
    fn ipv6_scope_global() {
        let addr: Ipv6Addr = "2001:db8::1".parse().unwrap();
        assert_eq!(ipv6_scope(&addr), "global");
    }

    #[test]
    fn ipv6_scope_link_local_upper_range() {
        // fe80::/10 covers fe80–febf
        let addr: Ipv6Addr = "feb0::1".parse().unwrap();
        assert_eq!(ipv6_scope(&addr), "link");
    }

    // ── ipv6_network ─────────────────────────────────────────────────────────

    #[test]
    fn ipv6_network_basic() {
        let addr: Ipv6Addr = "2001:db8::1".parse().unwrap();
        let mask: Ipv6Addr = "ffff:ffff::".parse().unwrap();
        let net = ipv6_network(addr, mask);
        assert_eq!(net, "2001:db8::".parse::<Ipv6Addr>().unwrap());
    }

    #[test]
    fn ipv6_network_full_mask() {
        let addr: Ipv6Addr = "2001:db8::42".parse().unwrap();
        let mask: Ipv6Addr = "ffff:ffff:ffff:ffff:ffff:ffff:ffff:ffff".parse().unwrap();
        assert_eq!(ipv6_network(addr, mask), addr);
    }

    #[test]
    fn ipv6_network_zero_mask() {
        let addr: Ipv6Addr = "2001:db8::1".parse().unwrap();
        let mask: Ipv6Addr = "::".parse().unwrap();
        assert_eq!(ipv6_network(addr, mask), "::".parse::<Ipv6Addr>().unwrap());
    }

    // ── parse_dhclient_leases ────────────────────────────────────────────────

    #[test]
    fn parse_dhclient_leases_single_lease() {
        let content = r#"lease {
  interface "eth0";
  option dhcp-server-identifier 192.168.1.1;
}"#;
        let mut result = HashMap::new();
        parse_dhclient_leases(content, &mut result);
        assert_eq!(result.get("eth0").map(String::as_str), Some("192.168.1.1"));
    }

    #[test]
    fn parse_dhclient_leases_last_wins() {
        let content = r#"lease {
  interface "eth0";
  option dhcp-server-identifier 10.0.0.1;
}
lease {
  interface "eth0";
  option dhcp-server-identifier 10.0.0.2;
}"#;
        let mut result = HashMap::new();
        parse_dhclient_leases(content, &mut result);
        assert_eq!(result.get("eth0").map(String::as_str), Some("10.0.0.2"));
    }

    #[test]
    fn parse_dhclient_leases_multiple_interfaces() {
        let content = r#"lease {
  interface "eth0";
  option dhcp-server-identifier 192.168.1.1;
}
lease {
  interface "wlan0";
  option dhcp-server-identifier 172.16.4.254;
}"#;
        let mut result = HashMap::new();
        parse_dhclient_leases(content, &mut result);
        assert_eq!(result.get("eth0").map(String::as_str), Some("192.168.1.1"));
        assert_eq!(result.get("wlan0").map(String::as_str), Some("172.16.4.254"));
    }

    #[test]
    fn parse_dhclient_leases_incomplete_lease_ignored() {
        // Lease with server but no interface
        let content = r#"lease {
  option dhcp-server-identifier 192.168.1.1;
}"#;
        let mut result = HashMap::new();
        parse_dhclient_leases(content, &mut result);
        assert!(result.is_empty());
    }

    #[test]
    fn parse_dhclient_leases_empty() {
        let mut result = HashMap::new();
        parse_dhclient_leases("", &mut result);
        assert!(result.is_empty());
    }
}
