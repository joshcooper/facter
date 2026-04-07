use crate::facts::value::FactValue;
use crate::resolvers::Resolver;
use indexmap::IndexMap;
use std::fs;

pub struct MemoryResolver;

impl Resolver for MemoryResolver {
    fn name(&self) -> &'static str {
        "memory"
    }

    fn resolve(&self) -> Vec<(String, FactValue)> {
        let mut facts = Vec::new();

        let mem = match collect_mem() {
            Some(m) => m,
            None => return facts,
        };

        let total = mem.total;
        let available = mem.available;
        let used = total.saturating_sub(available);
        let swap_total = mem.swap_total;
        let swap_free = mem.swap_free;
        let swap_used = swap_total.saturating_sub(swap_free);

        // Legacy flat facts
        facts.push(("memorysize".into(), FactValue::String(fmt_bytes(total))));
        facts.push(("memoryfree".into(), FactValue::String(fmt_bytes(available))));
        facts.push(("memorysize_mb".into(), FactValue::Float(bytes_to_mb(total))));
        facts.push(("memoryfree_mb".into(), FactValue::Float(bytes_to_mb(available))));

        if swap_total > 0 {
            facts.push(("swapsize".into(), FactValue::String(fmt_bytes(swap_total))));
            facts.push(("swapfree".into(), FactValue::String(fmt_bytes(swap_free))));
            facts.push(("swapsize_mb".into(), FactValue::Float(bytes_to_mb(swap_total))));
            facts.push(("swapfree_mb".into(), FactValue::Float(bytes_to_mb(swap_free))));
        }

        // Structured memory fact
        let mut system: IndexMap<String, FactValue> = IndexMap::new();
        system.insert("available".into(), FactValue::String(fmt_bytes(available)));
        system.insert("available_bytes".into(), FactValue::Integer(available as i64));
        system.insert(
            "capacity".into(),
            FactValue::String(fmt_capacity(used, total)),
        );
        system.insert("total".into(), FactValue::String(fmt_bytes(total)));
        system.insert("total_bytes".into(), FactValue::Integer(total as i64));
        system.insert("used".into(), FactValue::String(fmt_bytes(used)));
        system.insert("used_bytes".into(), FactValue::Integer(used as i64));

        let mut memory: IndexMap<String, FactValue> = IndexMap::new();
        memory.insert("system".into(), FactValue::Map(system));

        if swap_total > 0 {
            let mut swap: IndexMap<String, FactValue> = IndexMap::new();
            swap.insert("available".into(), FactValue::String(fmt_bytes(swap_free)));
            swap.insert("available_bytes".into(), FactValue::Integer(swap_free as i64));
            swap.insert(
                "capacity".into(),
                FactValue::String(fmt_capacity(swap_used, swap_total)),
            );
            swap.insert("total".into(), FactValue::String(fmt_bytes(swap_total)));
            swap.insert("total_bytes".into(), FactValue::Integer(swap_total as i64));
            swap.insert("used".into(), FactValue::String(fmt_bytes(swap_used)));
            swap.insert("used_bytes".into(), FactValue::Integer(swap_used as i64));
            memory.insert("swap".into(), FactValue::Map(swap));
        }

        facts.push(("memory".into(), FactValue::Map(memory)));

        facts
    }
}

// ── Platform-specific memory collection ──────────────────────────────────────

struct MemData {
    total: u64,
    available: u64,
    swap_total: u64,
    swap_free: u64,
}

#[cfg(target_os = "linux")]
fn collect_mem() -> Option<MemData> {
    let content = fs::read_to_string("/proc/meminfo").ok()?;
    let mut map = std::collections::HashMap::new();
    for line in content.lines() {
        if let Some((key, rest)) = line.split_once(':') {
            let kb: u64 = rest.split_whitespace().next()?.parse().ok()?;
            map.insert(key.trim().to_string(), kb * 1024);
        }
    }
    let total = *map.get("MemTotal")?;
    let free = map.get("MemFree").copied().unwrap_or(0);
    let buffers = map.get("Buffers").copied().unwrap_or(0);
    let cached = map.get("Cached").copied().unwrap_or(0);
    let available = map.get("MemAvailable").copied().unwrap_or(free + buffers + cached);
    let swap_total = map.get("SwapTotal").copied().unwrap_or(0);
    let swap_free = map.get("SwapFree").copied().unwrap_or(0);
    Some(MemData { total, available, swap_total, swap_free })
}

#[cfg(target_os = "macos")]
fn collect_mem() -> Option<MemData> {
    use crate::platform::macos::{sysctl_u64, sysctl_i32};
    let total = sysctl_u64("hw.memsize")?;
    let page_size = sysctl_i32("hw.pagesize").unwrap_or(4096) as u64;

    // Parse vm_stat for page counts
    let vm_out = std::process::Command::new("vm_stat").output().ok()?;
    let vm_text = String::from_utf8_lossy(&vm_out.stdout);
    let pages = parse_vm_stat(&vm_text);
    let free_pages = pages.get("Pages free").copied().unwrap_or(0)
        + pages.get("Pages inactive").copied().unwrap_or(0)
        + pages.get("Pages speculative").copied().unwrap_or(0)
        + pages.get("Pages purgeable").copied().unwrap_or(0);
    let available = free_pages * page_size;

    // Parse swap from `sysctl vm.swapusage`
    let (swap_total, swap_free) = collect_swap_macos();

    Some(MemData { total, available, swap_total, swap_free })
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn collect_mem() -> Option<MemData> {
    None
}

/// Parse `vm_stat` output into a map of metric name → page count.
#[allow(dead_code)]
fn parse_vm_stat(content: &str) -> std::collections::HashMap<&str, u64> {
    let mut map = std::collections::HashMap::new();
    for line in content.lines() {
        if let Some((key, val)) = line.split_once(':') {
            let val = val.trim().trim_end_matches('.');
            if let Ok(n) = val.trim().parse::<u64>() {
                map.insert(key.trim(), n);
            }
        }
    }
    map
}

/// Parse `sysctl vm.swapusage` text output, returning (total_bytes, free_bytes).
#[allow(dead_code)]
fn parse_swapusage(content: &str) -> Option<(u64, u64)> {
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

#[cfg(target_os = "macos")]
fn collect_swap_macos() -> (u64, u64) {
    let out = std::process::Command::new("sysctl")
        .arg("vm.swapusage")
        .output()
        .unwrap_or_default();
    let text = String::from_utf8_lossy(&out.stdout);
    parse_swapusage(&text).unwrap_or((0, 0))
}

const TIB: u64 = 1 << 40;
const GIB: u64 = 1 << 30;
const MIB: u64 = 1 << 20;
const KIB: u64 = 1 << 10;

fn fmt_bytes(bytes: u64) -> String {
    if bytes >= TIB {
        format!("{:.2} TiB", bytes as f64 / TIB as f64)
    } else if bytes >= GIB {
        format!("{:.2} GiB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.2} MiB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.2} KiB", bytes as f64 / KIB as f64)
    } else {
        format!("{} bytes", bytes)
    }
}

fn bytes_to_mb(bytes: u64) -> f64 {
    (bytes as f64) / (MIB as f64)
}

fn fmt_capacity(used: u64, total: u64) -> String {
    if total == 0 {
        return "0%".to_string();
    }
    format!("{:.2}%", (used as f64 / total as f64) * 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_vm_stat ────────────────────────────────────────────────────────

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

    // ── parse_swapusage ──────────────────────────────────────────────────────

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

    // ── fmt_capacity ─────────────────────────────────────────────────────────

    #[test]
    fn fmt_capacity_zero_total() {
        assert_eq!(fmt_capacity(0, 0), "0%");
    }

    #[test]
    fn fmt_capacity_half_used() {
        assert_eq!(fmt_capacity(512, 1024), "50.00%");
    }

    #[test]
    fn fmt_capacity_fully_used() {
        assert_eq!(fmt_capacity(1024, 1024), "100.00%");
    }
}
