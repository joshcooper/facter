use crate::platform::types::{MacosVersion, SelinuxData};
use std::collections::HashMap;

pub fn read_os_release() -> HashMap<String, String> {
    let mut map = HashMap::new();
    map.insert("ID".to_string(), "macos".to_string());
    map.insert("NAME".to_string(), "Darwin".to_string());
    let mut u: libc::utsname = unsafe { std::mem::zeroed() };
    if unsafe { libc::uname(&mut u) } == 0 {
        use std::ffi::CStr;
        let release = unsafe { CStr::from_ptr(u.release.as_ptr()).to_string_lossy().into_owned() };
        if !release.is_empty() {
            map.insert("VERSION_ID".to_string(), release);
        }
    }
    map
}

pub fn collect_selinux() -> SelinuxData {
    SelinuxData {
        supported: false,
        enabled: false,
        enforced: false,
        current_mode: String::new(),
        config_mode: String::new(),
        config_policy: String::new(),
        policy_version: String::new(),
    }
}

pub fn macos_product_version() -> Option<MacosVersion> {
    let content = std::fs::read_to_string(
        "/System/Library/CoreServices/SystemVersion.plist",
    )
    .unwrap_or_default();
    let (product_name, product_version, build) = parse_system_version_plist(&content);

    if product_version.is_empty() && product_name.is_empty() && build.is_empty() {
        return None;
    }

    let mut parts = product_version.splitn(3, '.');
    let major = parts.next().unwrap_or("").to_string();
    let minor = parts.next().unwrap_or("").to_string();
    let patch = parts.next().unwrap_or("").to_string();

    Some(MacosVersion { product_name, product_version, major, minor, patch, build })
}

/// Parse the XML plist content of SystemVersion.plist.
/// Pure function — no I/O, testable on any platform.
pub fn parse_system_version_plist(content: &str) -> (String, String, String) {
    let mut map: std::collections::HashMap<&str, &str> = std::collections::HashMap::new();
    let mut lines = content.lines().peekable();
    while let Some(line) = lines.next() {
        let line = line.trim();
        if let Some(key) = line.strip_prefix("<key>").and_then(|s| s.strip_suffix("</key>")) {
            if let Some(val_line) = lines.next() {
                let val_line = val_line.trim();
                if let Some(val) = val_line.strip_prefix("<string>").and_then(|s| s.strip_suffix("</string>")) {
                    map.insert(key, val);
                }
            }
        }
    }
    let name = map.get("ProductName").copied().unwrap_or("").to_string();
    let version = map.get("ProductVersion").copied()
        .or_else(|| map.get("ProductUserVisibleVersion").copied())
        .unwrap_or("")
        .to_string();
    let build = map.get("ProductBuildVersion").copied().unwrap_or("").to_string();
    (name, version, build)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_system_version_plist_ventura() {
        let plist = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>ProductBuildVersion</key>
    <string>22G90</string>
    <key>ProductName</key>
    <string>macOS</string>
    <key>ProductUserVisibleVersion</key>
    <string>13.5.1</string>
    <key>ProductVersion</key>
    <string>13.5.1</string>
</dict>
</plist>"#;
        let (name, version, build) = parse_system_version_plist(plist);
        assert_eq!(name, "macOS");
        assert_eq!(version, "13.5.1");
        assert_eq!(build, "22G90");
    }

    #[test]
    fn parse_system_version_plist_empty() {
        let (name, version, build) = parse_system_version_plist("");
        assert_eq!(name, "");
        assert_eq!(version, "");
        assert_eq!(build, "");
    }

    #[test]
    fn parse_system_version_plist_missing_build() {
        let plist = r#"<plist><dict>
    <key>ProductName</key>
    <string>macOS</string>
    <key>ProductVersion</key>
    <string>14.0</string>
</dict></plist>"#;
        let (name, version, build) = parse_system_version_plist(plist);
        assert_eq!(name, "macOS");
        assert_eq!(version, "14.0");
        assert_eq!(build, "");
    }
}
