use std::fs;

pub fn read_uptime_seconds() -> Option<u64> {
    let content = fs::read_to_string("/proc/uptime").ok()?;
    let secs_str = content.split_whitespace().next()?;
    let secs: f64 = secs_str.parse().ok()?;
    Some(secs as u64)
}
