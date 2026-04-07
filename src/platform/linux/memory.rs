use crate::platform::types::MemData;
use std::fs;

pub fn collect_mem() -> Option<MemData> {
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
