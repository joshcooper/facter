use crate::facts::value::FactValue;
use crate::resolvers::Resolver;
use indexmap::IndexMap;
#[cfg(target_os = "macos")]
use std::process::Command;
#[cfg(target_os = "linux")]
use std::fs;
#[cfg(target_os = "linux")]
use std::path::Path;

pub struct DiskResolver;

impl Resolver for DiskResolver {
    fn name(&self) -> &'static str {
        "disks"
    }

    fn resolve(&self) -> Vec<(String, FactValue)> {
        let mut facts = Vec::new();

        let disks = collect_disks();
        if disks.is_empty() {
            return facts;
        }

        let mut names: Vec<String> = disks.iter().map(|d| d.name.clone()).collect();
        names.sort();
        facts.push(("blockdevices".into(), FactValue::String(names.join(","))));

        let mut disks_map: IndexMap<String, FactValue> = IndexMap::new();
        for disk in &disks {
            // Legacy flat facts
            if let Some(v) = &disk.vendor {
                facts.push((
                    format!("blockdevice_{}_vendor", disk.name),
                    FactValue::String(v.clone()),
                ));
            }
            if let Some(v) = &disk.model {
                facts.push((
                    format!("blockdevice_{}_model", disk.name),
                    FactValue::String(v.clone()),
                ));
            }
            facts.push((
                format!("blockdevice_{}_size", disk.name),
                FactValue::Integer(disk.size_bytes as i64),
            ));

            // Structured entry
            let mut entry: IndexMap<String, FactValue> = IndexMap::new();
            if let Some(v) = &disk.vendor {
                entry.insert("vendor".into(), FactValue::String(v.clone()));
            }
            if let Some(v) = &disk.model {
                entry.insert("model".into(), FactValue::String(v.clone()));
            }
            if let Some(s) = &disk.serial {
                entry.insert("serial".into(), FactValue::String(s.clone()));
            }
            entry.insert(
                "size".into(),
                FactValue::String(fmt_bytes(disk.size_bytes)),
            );
            entry.insert(
                "size_bytes".into(),
                FactValue::Integer(disk.size_bytes as i64),
            );
            if let Some(t) = &disk.disk_type {
                entry.insert("type".into(), FactValue::String(t.clone()));
            }
            if let Some(w) = &disk.wwn {
                entry.insert("wwn".into(), FactValue::String(w.clone()));
            }
            disks_map.insert(disk.name.clone(), FactValue::Map(entry));
        }

        facts.push(("disks".into(), FactValue::Map(disks_map)));
        facts
    }
}

struct DiskInfo {
    name: String,
    vendor: Option<String>,
    model: Option<String>,
    serial: Option<String>,
    disk_type: Option<String>,
    wwn: Option<String>,
    size_bytes: u64,
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn collect_disks() -> Vec<DiskInfo> {
    Vec::new()
}

/// On macOS, use `diskutil list -plist` to enumerate physical disks.
#[cfg(target_os = "macos")]
fn collect_disks() -> Vec<DiskInfo> {
    collect_disks_macos()
}

#[cfg(target_os = "macos")]
fn collect_disks_macos() -> Vec<DiskInfo> {
    use std::process::Command;
    let mut disks = Vec::new();

    let out = match Command::new("diskutil").args(["list", "-plist"]).output() {
        Ok(o) if o.status.success() => o.stdout,
        _ => return disks,
    };
    let text = String::from_utf8_lossy(&out);

    // Parse AllDisksAndPartitions plist: extract whole disk identifiers (disk0, disk1, …)
    let whole_disks = parse_diskutil_list_plist(&text);

    for disk_id in &whole_disks {
        let info_out = match Command::new("diskutil")
            .args(["info", "-plist", disk_id])
            .output()
        {
            Ok(o) if o.status.success() => o.stdout,
            _ => continue,
        };
        let info_text = String::from_utf8_lossy(&info_out);
        if let Some(info) = parse_diskutil_info_plist(&info_text, disk_id) {
            disks.push(info);
        }
    }

    disks.sort_by(|a, b| a.name.cmp(&b.name));
    disks
}

/// Extract whole-disk identifiers from `diskutil list -plist` output.
/// Returns e.g. ["disk0", "disk1", "disk2"].
/// Uses the flat `AllDisks` array and filters to whole disks (diskN, not diskNsM).
#[allow(dead_code)]
fn parse_diskutil_list_plist(plist: &str) -> Vec<String> {
    let mut disks = Vec::new();
    let mut in_all_disks = false;
    for line in plist.lines() {
        let line = line.trim();
        if line == "<key>AllDisks</key>" {
            in_all_disks = true;
            continue;
        }
        if in_all_disks {
            if line.starts_with("<key>") || line == "</array>" {
                break;
            }
            if let Some(val) = line.strip_prefix("<string>").and_then(|s| s.strip_suffix("</string>")) {
                // Whole disk: diskN (only digits after "disk"); partition: diskNsM
                if val.starts_with("disk") && val.len() > 4 && val[4..].chars().all(|c| c.is_ascii_digit()) {
                    disks.push(val.to_string());
                }
            }
        }
    }
    disks
}

/// Parse `diskutil info -plist <disk>` to extract disk metadata.
#[allow(dead_code)]
fn parse_diskutil_info_plist(plist: &str, disk_id: &str) -> Option<DiskInfo> {
    let mut map: std::collections::HashMap<&str, &str> = std::collections::HashMap::new();
    let mut lines = plist.lines().peekable();
    while let Some(line) = lines.next() {
        let line = line.trim();
        if let Some(key) = line.strip_prefix("<key>").and_then(|s| s.strip_suffix("</key>")) {
            if let Some(val_line) = lines.next() {
                let val_line = val_line.trim();
                if let Some(val) = val_line.strip_prefix("<string>").and_then(|s| s.strip_suffix("</string>")) {
                    map.insert(key, val);
                } else if let Some(val) = val_line.strip_prefix("<integer>").and_then(|s| s.strip_suffix("</integer>")) {
                    map.insert(key, val);
                }
            }
        }
    }

    let size_bytes: u64 = map.get("Size")
        .or_else(|| map.get("TotalSize"))
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    let model = map.get("MediaName")
        .or_else(|| map.get("IORegistryEntryName"))
        .map(|s| s.to_string());
    let vendor = map.get("VendorName")
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    Some(DiskInfo {
        name: format!("/dev/{}", disk_id),
        vendor,
        model,
        serial: None,
        disk_type: None,
        wwn: None,
        size_bytes,
    })
}

#[cfg(target_os = "linux")]
fn collect_disks() -> Vec<DiskInfo> {
    let mut disks = Vec::new();
    let block_dir = Path::new("/sys/block");
    let entries = match fs::read_dir(block_dir) {
        Ok(e) => e,
        Err(_) => return disks,
    };

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let dev_path = entry.path();

        // Skip virtual/mapped devices: must have a device/ subdirectory
        if !dev_path.join("device").exists() {
            continue;
        }

        let size_bytes = read_u64(&dev_path.join("size")).unwrap_or(0) * 512;
        let vendor = read_trimmed(&dev_path.join("device/vendor"));
        let model = read_trimmed(&dev_path.join("device/model"));
        let serial = read_trimmed(&dev_path.join("device/serial"))
            .or_else(|| read_vpd_serial(&dev_path.join("device/vpd_pg80")))
            // Only walk up for devices that have media (size > 0), to avoid false serials from USB hubs
            .or_else(|| if size_bytes > 0 { read_ancestor_serial(&dev_path.join("device")) } else { None });
        let disk_type = read_trimmed(&dev_path.join("queue/rotational")).map(|r| {
            if r == "0" { "ssd".to_string() } else { "hdd".to_string() }
        });
        // NVMe exposes wwid directly under the block device; SCSI/SAS via device/wwid
        let wwn = read_trimmed(&dev_path.join("wwid"))
            .or_else(|| read_trimmed(&dev_path.join("device/wwid")));

        disks.push(DiskInfo { name, vendor, model, serial, disk_type, wwn, size_bytes });
    }

    disks.sort_by(|a, b| a.name.cmp(&b.name));
    disks
}

#[cfg(target_os = "linux")]
fn read_u64(path: &Path) -> Option<u64> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

/// Walk up the sysfs device tree to find a `serial` file (e.g. for USB drives).
#[cfg(target_os = "linux")]
fn read_ancestor_serial(start: &Path) -> Option<String> {
    let real = fs::canonicalize(start).ok()?;
    let mut dir = real.as_path();
    // Walk up at most 6 levels to avoid going too far
    for _ in 0..6 {
        dir = dir.parent()?;
        let serial_path = dir.join("serial");
        if let Some(s) = read_trimmed(&serial_path) {
            return Some(s);
        }
    }
    None
}

/// Read SCSI VPD page 0x80 (Unit Serial Number) from a binary sysfs file.
/// The serial number is a string starting at byte 4.
#[cfg(target_os = "linux")]
fn read_vpd_serial(path: &Path) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    if bytes.len() < 4 {
        return None;
    }
    let len = bytes[3] as usize;
    let end = 4 + len;
    if end > bytes.len() {
        return None;
    }
    let s = std::str::from_utf8(&bytes[4..end]).ok()?.trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

#[cfg(target_os = "linux")]
fn read_trimmed(path: &Path) -> Option<String> {
    let s = fs::read_to_string(path).ok()?.trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

const GIB: u64 = 1 << 30;
const MIB: u64 = 1 << 20;
const KIB: u64 = 1 << 10;

fn fmt_bytes(bytes: u64) -> String {
    if bytes >= GIB {
        format!("{:.2} GiB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.2} MiB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.2} KiB", bytes as f64 / KIB as f64)
    } else {
        format!("{} bytes", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── fmt_bytes ────────────────────────────────────────────────────────────

    #[test]
    fn fmt_bytes_gib() {
        assert_eq!(fmt_bytes(500 * GIB), "500.00 GiB");
        assert_eq!(fmt_bytes(GIB), "1.00 GiB");
    }

    #[test]
    fn fmt_bytes_mib() {
        assert_eq!(fmt_bytes(512 * MIB), "512.00 MiB");
    }

    #[test]
    fn fmt_bytes_kib() {
        assert_eq!(fmt_bytes(4 * KIB), "4.00 KiB");
    }

    #[test]
    fn fmt_bytes_raw() {
        assert_eq!(fmt_bytes(0), "0 bytes");
        assert_eq!(fmt_bytes(512), "512 bytes");
    }

    // ── read_vpd_serial ──────────────────────────────────────────────────────

    #[test]
    fn read_vpd_serial_valid() {
        // Page 0x80: header (4 bytes) + serial string
        // bytes[0..4] = page header, bytes[3] = length
        let serial = b"SN12345";
        let mut data = vec![0u8, 0x80, 0x00, serial.len() as u8];
        data.extend_from_slice(serial);
        let result = read_vpd_serial_bytes(&data);
        assert_eq!(result, Some("SN12345".to_string()));
    }

    #[test]
    fn read_vpd_serial_too_short() {
        assert_eq!(read_vpd_serial_bytes(&[0x00, 0x80, 0x00]), None);
    }

    #[test]
    fn read_vpd_serial_length_overrun() {
        // Claims 10 bytes but only 2 present
        let data = vec![0u8, 0x80, 0x00, 10, b'A', b'B'];
        assert_eq!(read_vpd_serial_bytes(&data), None);
    }

    #[test]
    fn read_vpd_serial_empty_string() {
        let data = vec![0u8, 0x80, 0x00, 0x00];
        assert_eq!(read_vpd_serial_bytes(&data), None);
    }

    /// Test helper: same logic as read_vpd_serial but takes bytes directly.
    fn read_vpd_serial_bytes(bytes: &[u8]) -> Option<String> {
        if bytes.len() < 4 {
            return None;
        }
        let len = bytes[3] as usize;
        let end = 4 + len;
        if end > bytes.len() {
            return None;
        }
        let s = std::str::from_utf8(&bytes[4..end]).ok()?.trim().to_string();
        if s.is_empty() { None } else { Some(s) }
    }

    // ── parse_diskutil_list_plist ─────────────────────────────────────────────

    #[test]
    fn parse_diskutil_list_plist_extracts_whole_disks() {
        let plist = r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
    <key>AllDisks</key>
    <array>
        <string>disk0</string>
        <string>disk0s1</string>
        <string>disk0s2</string>
        <string>disk1</string>
        <string>disk1s1</string>
    </array>
</dict>
</plist>"#;
        let disks = parse_diskutil_list_plist(plist);
        // Should include disk0 and disk1 but NOT partitions
        assert!(disks.contains(&"disk0".to_string()), "disk0 missing: {:?}", disks);
        assert!(disks.contains(&"disk1".to_string()), "disk1 missing: {:?}", disks);
        assert!(!disks.contains(&"disk0s1".to_string()));
        assert!(!disks.contains(&"disk1s1".to_string()));
    }

    #[test]
    fn parse_diskutil_info_plist_extracts_size() {
        let plist = r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
    <key>Size</key>
    <integer>500107862016</integer>
    <key>MediaName</key>
    <string>APPLE SSD AP0512Q</string>
</dict>
</plist>"#;
        let info = parse_diskutil_info_plist(plist, "disk0");
        assert!(info.is_some());
        let info = info.unwrap();
        assert_eq!(info.size_bytes, 500107862016);
        assert_eq!(info.model.as_deref(), Some("APPLE SSD AP0512Q"));
    }
}
