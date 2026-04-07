use crate::platform::types::DiskInfo;
use std::process::Command;

pub fn collect_disks() -> Vec<DiskInfo> {
    let mut disks = Vec::new();

    let out = match Command::new("diskutil").args(["list", "-plist"]).output() {
        Ok(o) if o.status.success() => o.stdout,
        _ => return disks,
    };
    let text = String::from_utf8_lossy(&out);

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
pub fn parse_diskutil_list_plist(plist: &str) -> Vec<String> {
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
                if val.starts_with("disk") && val.len() > 4 && val[4..].chars().all(|c| c.is_ascii_digit()) {
                    disks.push(val.to_string());
                }
            }
        }
    }
    disks
}

/// Parse `diskutil info -plist <disk>` to extract disk metadata.
pub fn parse_diskutil_info_plist(plist: &str, disk_id: &str) -> Option<DiskInfo> {
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

#[cfg(test)]
mod tests {
    use super::*;

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
