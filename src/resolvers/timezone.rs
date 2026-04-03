use crate::facts::value::FactValue;
use crate::resolvers::Resolver;
use std::ffi::CStr;

pub struct TimezoneResolver;

impl Resolver for TimezoneResolver {
    fn name(&self) -> &'static str {
        "timezone"
    }

    fn resolve(&self) -> Vec<(String, FactValue)> {
        let mut facts = Vec::new();

        if let Some(tz) = get_timezone() {
            facts.push(("timezone".into(), FactValue::String(tz)));
        }

        facts
    }
}

/// Get the current timezone abbreviation (e.g. "UTC", "EST", "PDT").
fn get_timezone() -> Option<String> {
    // Use localtime_r to get the current local time struct, which contains tm_zone.
    let mut now: libc::time_t = 0;
    unsafe { libc::time(&mut now) };

    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&now, &mut tm) };

    if tm.tm_zone.is_null() {
        // Fallback: read TZ abbreviation via strftime
        return timezone_from_strftime();
    }

    let tz = unsafe { CStr::from_ptr(tm.tm_zone).to_string_lossy().into_owned() };
    if tz.is_empty() {
        return timezone_from_strftime();
    }
    Some(tz)
}

fn timezone_from_strftime() -> Option<String> {
    // Last resort: read /etc/timezone (Debian/Ubuntu) or parse localtime symlink
    if let Ok(tz) = std::fs::read_to_string("/etc/timezone") {
        let tz = tz.trim();
        // /etc/timezone contains the IANA name like "America/New_York"; we want
        // the abbreviation, but this is the best we can do without libc support.
        if !tz.is_empty() {
            return Some(tz.rsplit('/').next().unwrap_or(tz).to_string());
        }
    }
    None
}
