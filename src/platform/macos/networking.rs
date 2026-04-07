use crate::platform::types::Interface;
use std::collections::HashMap;
use std::ffi::CStr;

pub fn augment_interfaces(map: &mut HashMap<String, Interface>) {
    // Collect AF_LINK MAC addresses via a second getifaddrs pass
    unsafe {
        let mut ifaddrs: *mut libc::ifaddrs = std::ptr::null_mut();
        if libc::getifaddrs(&mut ifaddrs) == 0 {
            let mut ptr = ifaddrs;
            while !ptr.is_null() {
                let ifa = &*ptr;
                if !ifa.ifa_addr.is_null() {
                    let family = (*ifa.ifa_addr).sa_family as i32;
                    if family == libc::AF_LINK as i32 {
                        let name = CStr::from_ptr(ifa.ifa_name).to_string_lossy().into_owned();
                        if let Some(iface) = map.get_mut(&name) {
                            if iface.mac.is_none() {
                                let sdl = ifa.ifa_addr as *const libc::sockaddr_dl;
                                let nlen = (*sdl).sdl_nlen as usize;
                                let alen = (*sdl).sdl_alen as usize;
                                if alen == 6 {
                                    let data = (*sdl).sdl_data.as_ptr();
                                    let mac_bytes =
                                        std::slice::from_raw_parts(data.add(nlen) as *const u8, alen);
                                    let mac = format!(
                                        "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                                        mac_bytes[0], mac_bytes[1], mac_bytes[2],
                                        mac_bytes[3], mac_bytes[4], mac_bytes[5]
                                    );
                                    if mac != "00:00:00:00:00:00" {
                                        iface.mac = Some(mac);
                                    }
                                }
                            }
                        }
                    }
                }
                ptr = ifa.ifa_next;
            }
            libc::freeifaddrs(ifaddrs);
        }
    }

    // Set MTU and physical flag
    for iface in map.values_mut() {
        iface.mtu = macos_get_mtu(&iface.name);
        iface.physical = iface.name.starts_with("en");
    }
}

pub fn augment_ipv6_flags(_map: &mut HashMap<String, Interface>) {
    // No-op on macOS: no /proc/net/if_inet6
}

pub fn get_primary_interface() -> Option<String> {
    let out = std::process::Command::new("route")
        .args(["-n", "get", "default"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines() {
        let line = line.trim();
        if let Some(iface) = line.strip_prefix("interface:") {
            let iface = iface.trim().to_string();
            if !iface.is_empty() {
                return Some(iface);
            }
        }
    }
    None
}

pub fn collect_dhcp_servers() -> HashMap<String, String> {
    let mut result: HashMap<String, String> = HashMap::new();

    if let Ok(entries) = std::fs::read_dir("/var/db/dhclient") {
        for entry in entries.flatten() {
            if let Ok(content) = std::fs::read_to_string(entry.path()) {
                parse_dhclient_leases(&content, &mut result);
            }
        }
    }

    result
}

fn macos_get_mtu(name: &str) -> Option<u64> {
    use std::ffi::CString;
    let cname = CString::new(name).ok()?;
    unsafe {
        let sock = libc::socket(libc::AF_INET, libc::SOCK_DGRAM, 0);
        if sock < 0 {
            return None;
        }
        let mut ifr: libc::ifreq = std::mem::zeroed();
        let src = cname.as_bytes_with_nul();
        let len = src.len().min(libc::IFNAMSIZ);
        std::ptr::copy_nonoverlapping(
            src.as_ptr(),
            ifr.ifr_name.as_mut_ptr() as *mut u8,
            len,
        );
        let ret = libc::ioctl(sock, libc::SIOCGIFMTU, &mut ifr as *mut _);
        libc::close(sock);
        if ret == 0 {
            Some(ifr.ifr_ifru.ifru_mtu as u64)
        } else {
            None
        }
    }
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
