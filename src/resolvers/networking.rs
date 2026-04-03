use crate::facts::value::FactValue;
use crate::resolvers::Resolver;
use indexmap::IndexMap;
use std::collections::HashMap;
use std::ffi::CStr;
use std::fs;
use std::net::{Ipv4Addr, Ipv6Addr};

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
        if primary_mtu > 0 {
            net.insert("mtu".into(), FactValue::Integer(primary_mtu as i64));
        }

        // Per-interface structured map
        let mut ifaces_map: IndexMap<String, FactValue> = IndexMap::new();
        for iface in &interfaces {
            ifaces_map.insert(iface.name.clone(), build_interface_map(iface));
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
    scope_id: u32,
}

struct Interface {
    name: String,
    ipv4: Vec<Ipv4Binding>,
    ipv6: Vec<Ipv6Binding>,
    mac: Option<String>,
    mtu: Option<u64>,
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
                    let scope_id = (*sa).sin6_scope_id;
                    let netmask = if !nm.is_null() {
                        Some(Ipv6Addr::from((*nm).sin6_addr.s6_addr))
                    } else {
                        None
                    };
                    entry.ipv6.push(Ipv6Binding { address, netmask, scope_id });
                }
            }

            ptr = ifa.ifa_next;
        }

        libc::freeifaddrs(ifaddrs);
    }

    // Augment with MAC and MTU from sysfs
    for iface in map.values_mut() {
        let mac_path = format!("/sys/class/net/{}/address", iface.name);
        if let Ok(mac) = fs::read_to_string(&mac_path) {
            let mac = mac.trim().to_string();
            if !mac.is_empty() && mac != "00:00:00:00:00:00" {
                iface.mac = Some(mac);
            }
        }
        let mtu_path = format!("/sys/class/net/{}/mtu", iface.name);
        if let Ok(mtu_str) = fs::read_to_string(&mtu_path) {
            if let Ok(mtu) = mtu_str.trim().parse::<u64>() {
                iface.mtu = Some(mtu);
            }
        }
    }

    let mut result: Vec<Interface> = map.into_values().collect();
    result.sort_by(|a, b| a.name.cmp(&b.name));
    result
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

fn build_interface_map(iface: &Interface) -> FactValue {
    let mut m: IndexMap<String, FactValue> = IndexMap::new();

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
        }
        m.insert(
            "scope6".into(),
            FactValue::String(ipv6_scope(first.scope_id)),
        );

        let bindings6: Vec<FactValue> = iface
            .ipv6
            .iter()
            .map(|b| {
                let mut bm: IndexMap<String, FactValue> = IndexMap::new();
                bm.insert("address".into(), FactValue::String(b.address.to_string()));
                if let Some(nm) = b.netmask {
                    bm.insert("netmask".into(), FactValue::String(nm.to_string()));
                }
                bm.insert("scope".into(), FactValue::String(ipv6_scope(b.scope_id)));
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

    FactValue::Map(m)
}

// ── Network calculation helpers ───────────────────────────────────────────────

fn ipv4_network(addr: Ipv4Addr, netmask: Ipv4Addr) -> Ipv4Addr {
    let a = u32::from(addr);
    let m = u32::from(netmask);
    Ipv4Addr::from(a & m)
}

fn ipv6_scope(scope_id: u32) -> String {
    // scope_id 0 means global; non-zero is link-local
    if scope_id == 0 {
        "global".to_string()
    } else {
        "link".to_string()
    }
}
