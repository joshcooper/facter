use crate::platform::types::MemData;
use super::sysctl::{sysctl_u64, sysctl_i32};
use std::collections::HashMap;

pub fn collect_mem() -> Option<MemData> {
    let total = sysctl_u64("hw.memsize")?;
    let page_size = sysctl_i32("hw.pagesize").unwrap_or(4096) as u64;

    let vm_out = std::process::Command::new("vm_stat").output().ok()?;
    let vm_text = String::from_utf8_lossy(&vm_out.stdout);
    let pages = parse_vm_stat(&vm_text);
    let free_pages = pages.get("Pages free").copied().unwrap_or(0)
        + pages.get("Pages inactive").copied().unwrap_or(0)
        + pages.get("Pages speculative").copied().unwrap_or(0)
        + pages.get("Pages purgeable").copied().unwrap_or(0);
    let available = free_pages * page_size;

    let (swap_total, swap_free) = collect_swap_macos();

    Some(MemData { total, available, swap_total, swap_free })
}

/// Parse `vm_stat` output into a map of metric name → page count.
pub fn parse_vm_stat(content: &str) -> HashMap<String, u64> {
    let mut map = HashMap::new();
    for line in content.lines() {
        if let Some((key, val)) = line.split_once(':') {
            let val = val.trim().trim_end_matches('.');
            if let Ok(n) = val.trim().parse::<u64>() {
                map.insert(key.trim().to_string(), n);
            }
        }
    }
    map
}

/// Parse `sysctl vm.swapusage` text output, returning (total_bytes, free_bytes).
pub fn parse_swapusage(content: &str) -> Option<(u64, u64)> {
    // Format: "vm.swapusage: total = 2048.00M  used = 512.00M  free = 1536.00M  (encrypted)"
    let rest = content.split_once(':')?.1;
    let parse_val = |label: &str| -> Option<u64> {
        let after = rest.split(label).nth(1)?.trim();
        let num_str: String = after.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
        let num: f64 = num_str.parse().ok()?;
        let unit = after[num_str.len()..].trim_start().chars().next()?;
        let bytes = match unit {
            'G' => (num * 1024.0 * 1024.0 * 1024.0) as u64,
            'M' => (num * 1024.0 * 1024.0) as u64,
            'K' => (num * 1024.0) as u64,
            _ => num as u64,
        };
        Some(bytes)
    };
    let total = parse_val("total =")?;
    let free = parse_val("free =")?;
    Some((total, free))
}

fn collect_swap_macos() -> (u64, u64) {
    let out = std::process::Command::new("sysctl")
        .arg("vm.swapusage")
        .output()
        .unwrap_or_default();
    let text = String::from_utf8_lossy(&out.stdout);
    parse_swapusage(&text).unwrap_or((0, 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_vm_stat_basic() {
        let input = "Mach Virtual Memory Statistics: (page size of 4096 bytes)\n\
            Pages free:                           123456.\n\
            Pages active:                         789012.\n\
            Pages inactive:                        45678.\n\
            Pages speculative:                      1234.\n\
            Pages purgeable:                         567.\n";
        let map = parse_vm_stat(input);
        assert_eq!(map.get("Pages free").copied(), Some(123456));
        assert_eq!(map.get("Pages active").copied(), Some(789012));
        assert_eq!(map.get("Pages inactive").copied(), Some(45678));
        assert_eq!(map.get("Pages speculative").copied(), Some(1234));
        assert_eq!(map.get("Pages purgeable").copied(), Some(567));
    }

    #[test]
    fn parse_vm_stat_skips_non_numeric() {
        let input = "Header line: (ignored)\nPages free:                           42.\n";
        let map = parse_vm_stat(input);
        assert_eq!(map.get("Pages free").copied(), Some(42));
        assert!(!map.contains_key("Header line"));
    }

    #[test]
    fn parse_vm_stat_empty() {
        let map = parse_vm_stat("");
        assert!(map.is_empty());
    }

    #[test]
    fn parse_swapusage_mib() {
        let input = "vm.swapusage: total = 2048.00M  used = 512.00M  free = 1536.00M  (encrypted)";
        let (total, free) = parse_swapusage(input).unwrap();
        assert_eq!(total, 2048 * 1024 * 1024);
        assert_eq!(free, 1536 * 1024 * 1024);
    }

    #[test]
    fn parse_swapusage_gib() {
        let input = "vm.swapusage: total = 4.00G  used = 1.00G  free = 3.00G";
        let (total, free) = parse_swapusage(input).unwrap();
        assert_eq!(total, 4 * 1024 * 1024 * 1024);
        assert_eq!(free, 3 * 1024 * 1024 * 1024);
    }

    #[test]
    fn parse_swapusage_no_swap() {
        let input = "vm.swapusage: total = 0.00K  used = 0.00K  free = 0.00K";
        let (total, free) = parse_swapusage(input).unwrap();
        assert_eq!(total, 0);
        assert_eq!(free, 0);
    }

    #[test]
    fn parse_swapusage_invalid_returns_none() {
        assert!(parse_swapusage("").is_none());
        assert!(parse_swapusage("no colon here").is_none());
    }
}
