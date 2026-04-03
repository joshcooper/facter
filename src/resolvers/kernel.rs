use crate::facts::value::FactValue;
use crate::resolvers::Resolver;
use std::ffi::CStr;

pub struct KernelResolver;

impl Resolver for KernelResolver {
    fn name(&self) -> &'static str {
        "kernel"
    }

    fn resolve(&self) -> Vec<(String, FactValue)> {
        let mut facts = Vec::new();

        let mut u: libc::utsname = unsafe { std::mem::zeroed() };
        if unsafe { libc::uname(&mut u) } != 0 {
            log::warn!("uname failed: kernel facts unavailable");
            return facts;
        }

        let sysname = utsname_str(&u.sysname);
        let release = utsname_str(&u.release);

        // kernelversion: strip everything from the first '-' onward (e.g. "5.15.0-139-generic" -> "5.15.0")
        let version: String = release.split('-').next().unwrap_or("").to_string();

        // kernelmajversion: first two dot-separated components (e.g. "5.15.0" -> "5.15")
        let major_version = kernel_major(&version);

        facts.push(("kernel".into(), FactValue::String(sysname)));
        facts.push(("kernelrelease".into(), FactValue::String(release)));
        if !version.is_empty() {
            facts.push(("kernelversion".into(), FactValue::String(version)));
        }
        if let Some(mv) = major_version {
            facts.push(("kernelmajversion".into(), FactValue::String(mv)));
        }

        facts
    }
}

fn utsname_str(chars: &[libc::c_char]) -> String {
    unsafe { CStr::from_ptr(chars.as_ptr()).to_string_lossy().into_owned() }
}

fn kernel_major(version: &str) -> Option<String> {
    let mut parts = version.splitn(3, '.');
    let major = parts.next()?;
    let minor = parts.next()?;
    if major.is_empty() {
        return None;
    }
    Some(format!("{}.{}", major, minor))
}
