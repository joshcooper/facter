use crate::facts::value::FactValue;
use crate::resolvers::Resolver;
use indexmap::IndexMap;
use std::fs;

pub struct UptimeResolver;

impl Resolver for UptimeResolver {
    fn name(&self) -> &'static str {
        "uptime"
    }

    fn resolve(&self) -> Vec<(String, FactValue)> {
        let mut facts = Vec::new();

        let seconds = match read_uptime_seconds() {
            Some(s) => s,
            None => return facts,
        };

        let days = seconds / 86400;
        let hours = (seconds % 86400) / 3600;
        let minutes = (seconds % 3600) / 60;

        let uptime_str = format_uptime(days, hours, minutes);

        // Legacy flat facts
        facts.push(("uptime_seconds".into(), FactValue::Integer(seconds as i64)));
        facts.push(("uptime_hours".into(), FactValue::Integer((seconds / 3600) as i64)));
        facts.push(("uptime_days".into(), FactValue::Integer(days as i64)));

        // Legacy string fact
        facts.push(("uptime".into(), FactValue::String(uptime_str.clone())));

        // Structured fact
        let map = build_map(seconds, days, hours, minutes, &uptime_str);
        facts.push(("system_uptime".into(), FactValue::Map(map)));

        facts
    }
}

#[cfg(target_os = "linux")]
fn read_uptime_seconds() -> Option<u64> {
    let content = fs::read_to_string("/proc/uptime").ok()?;
    let secs_str = content.split_whitespace().next()?;
    let secs: f64 = secs_str.parse().ok()?;
    Some(secs as u64)
}

#[cfg(target_os = "macos")]
fn read_uptime_seconds() -> Option<u64> {
    use std::ffi::CString;
    let name = CString::new("kern.boottime").ok()?;
    let mut boottime: libc::timeval = unsafe { std::mem::zeroed() };
    let mut size = std::mem::size_of::<libc::timeval>();
    let ret = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            &mut boottime as *mut _ as *mut _,
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if ret != 0 {
        return None;
    }
    let now = unsafe { libc::time(std::ptr::null_mut()) };
    let uptime = now - boottime.tv_sec;
    if uptime < 0 { None } else { Some(uptime as u64) }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn read_uptime_seconds() -> Option<u64> {
    None
}

fn format_uptime(days: u64, hours: u64, minutes: u64) -> String {
    if days == 1 {
        "1 day".to_string()
    } else if days > 1 {
        format!("{} days", days)
    } else if hours > 0 {
        format!("{}:{:02} hours", hours, minutes)
    } else {
        format!("0:{:02} minutes", minutes)
    }
}

fn build_map(
    seconds: u64,
    days: u64,
    hours: u64,
    _minutes: u64,
    uptime_str: &str,
) -> IndexMap<String, FactValue> {
    let mut map: IndexMap<String, FactValue> = IndexMap::new();
    map.insert("days".into(), FactValue::Integer(days as i64));
    map.insert("hours".into(), FactValue::Integer((days * 24 + hours) as i64));
    map.insert("seconds".into(), FactValue::Integer(seconds as i64));
    map.insert("uptime".into(), FactValue::String(uptime_str.to_string()));
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_uptime_one_day() {
        assert_eq!(format_uptime(1, 0, 0), "1 day");
    }

    #[test]
    fn format_uptime_multiple_days() {
        assert_eq!(format_uptime(3, 5, 20), "3 days");
    }

    #[test]
    fn format_uptime_hours_and_minutes() {
        assert_eq!(format_uptime(0, 2, 7), "2:07 hours");
        assert_eq!(format_uptime(0, 10, 30), "10:30 hours");
    }

    #[test]
    fn format_uptime_minutes_only() {
        assert_eq!(format_uptime(0, 0, 5), "0:05 minutes");
        assert_eq!(format_uptime(0, 0, 42), "0:42 minutes");
    }

    #[test]
    fn build_map_hours_span_days() {
        // 2 days + 3 hours = 51 total hours
        let map = build_map(2 * 86400 + 3 * 3600, 2, 3, 0, "2 days");
        assert!(matches!(map["days"], FactValue::Integer(2)));
        assert!(matches!(map["hours"], FactValue::Integer(51)));
        assert!(matches!(map["seconds"], FactValue::Integer(s) if s == (2 * 86400 + 3 * 3600) as i64));
        assert!(matches!(&map["uptime"], FactValue::String(s) if s == "2 days"));
    }
}
