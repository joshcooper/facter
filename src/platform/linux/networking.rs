use crate::platform::types::Interface;
use std::collections::HashMap;
use std::fs;
use std::net::Ipv6Addr;

pub fn augment_interfaces(map: &mut HashMap<String, Interface>) {
    use std::path::Path;
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
}

pub fn augment_ipv6_flags(map: &mut HashMap<String, Interface>) {
    let ifa_flags = read_if_inet6_flags();
    for iface in map.values_mut() {
        for binding in iface.ipv6.iter_mut() {
            let key = (iface.name.clone(), binding.address);
            if let Some(flags) = ifa_flags.get(&key) {
                binding.flags = flags.clone();
            }
        }
    }
}

pub fn get_primary_interface() -> Option<String> {
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

pub fn collect_dhcp_servers() -> HashMap<String, String> {
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

pub fn parse_dhclient_leases(content: &str, result: &mut HashMap<String, String>) {
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
