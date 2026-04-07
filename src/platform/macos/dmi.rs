use super::sysctl::sysctl_string;

pub fn read_dmi(field: &str) -> Option<String> {
    match field {
        "product_name" => {
            sysctl_string("hw.model")
        }
        "product_uuid" => {
            let out = std::process::Command::new("ioreg")
                .args(["-rd1", "-c", "IOPlatformExpertDevice"])
                .output()
                .ok()?;
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines() {
                if line.contains("IOPlatformUUID") {
                    if let Some(val) = line.split('"').nth(3) {
                        return Some(val.to_string());
                    }
                }
            }
            None
        }
        "sys_vendor" => {
            Some("Apple Inc.".to_string())
        }
        "bios_vendor" => Some("Apple Inc.".to_string()),
        "bios_version" => {
            let out = std::process::Command::new("ioreg")
                .args(["-rd1", "-c", "IOPlatformExpertDevice"])
                .output()
                .ok()?;
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines() {
                if line.contains("version") || line.contains("Version") {
                    if let Some(val) = line.split('"').nth(3) {
                        return Some(val.to_string());
                    }
                }
            }
            None
        }
        "product_serial" | "board_serial" => {
            let out = std::process::Command::new("ioreg")
                .args(["-rd1", "-c", "IOPlatformExpertDevice"])
                .output()
                .ok()?;
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines() {
                if line.contains("IOPlatformSerialNumber") {
                    if let Some(val) = line.split('"').nth(3) {
                        return Some(val.to_string());
                    }
                }
            }
            None
        }
        _ => None,
    }
}

/// Parse key: value lines from `system_profiler SPHardwareDataType` output.
/// Pure function — no I/O, testable on any platform.
pub fn parse_system_profiler_hardware(output: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    for line in output.lines() {
        if let Some((key, val)) = line.split_once(':') {
            let key = key.trim().to_string();
            let val = val.trim().to_string();
            if !key.is_empty() && !val.is_empty() {
                map.insert(key, val);
            }
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_system_profiler_hardware_basic() {
        let output = "      Model Name: MacBook Pro\n      Serial Number (system): C02XG0JHJGH6\n";
        let map = parse_system_profiler_hardware(output);
        assert_eq!(map.get("Model Name").map(String::as_str), Some("MacBook Pro"));
        assert_eq!(map.get("Serial Number (system)").map(String::as_str), Some("C02XG0JHJGH6"));
    }

    #[test]
    fn parse_system_profiler_hardware_empty() {
        let map = parse_system_profiler_hardware("");
        assert!(map.is_empty());
    }

    #[test]
    fn parse_system_profiler_hardware_skips_empty_values() {
        let output = "      Model Name: MacBook Pro\n      Empty Field:\n";
        let map = parse_system_profiler_hardware(output);
        assert_eq!(map.get("Model Name").map(String::as_str), Some("MacBook Pro"));
        assert!(!map.contains_key("Empty Field"));
    }
}
