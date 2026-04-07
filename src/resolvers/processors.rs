use crate::facts::value::FactValue;
use crate::resolvers::Resolver;
use indexmap::IndexMap;
use std::ffi::CStr;
#[cfg(target_os = "linux")]
use std::collections::HashSet;
#[cfg(target_os = "linux")]
use std::fs;

pub struct ProcessorsResolver;

impl Resolver for ProcessorsResolver {
    fn name(&self) -> &'static str {
        "processors"
    }

    fn resolve(&self) -> Vec<(String, FactValue)> {
        let mut facts = Vec::new();

        let info = parse_cpuinfo();
        let isa = cpu_isa();

        let logical_count = info.logical_count;
        let physical_count = info.physical_count;
        let models = info.models;

        // Legacy flat facts
        facts.push(("processorcount".into(), FactValue::Integer(logical_count as i64)));
        facts.push((
            "physicalprocessorcount".into(),
            FactValue::Integer(physical_count as i64),
        ));
        if !isa.is_empty() {
            facts.push(("hardwareisa".into(), FactValue::String(isa.clone())));
        }

        // Per-processor legacy facts (processor0, processor1, …)
        for (i, model) in models.iter().enumerate() {
            facts.push((format!("processor{}", i), FactValue::String(model.clone())));
        }

        // Structured processors fact
        let mut processors: IndexMap<String, FactValue> = IndexMap::new();
        processors.insert("count".into(), FactValue::Integer(logical_count as i64));
        if info.cores > 0 {
            processors.insert("cores".into(), FactValue::Integer(info.cores as i64));
        }
        let extensions = cpu_extensions(&isa);
        if !extensions.is_empty() {
            let ext_vals: Vec<FactValue> = extensions.into_iter().map(FactValue::String).collect();
            processors.insert("extensions".into(), FactValue::Array(ext_vals));
        }
        if !isa.is_empty() {
            processors.insert("isa".into(), FactValue::String(isa));
        }
        if !models.is_empty() {
            let model_values: Vec<FactValue> = models.into_iter().map(FactValue::String).collect();
            processors.insert("models".into(), FactValue::Array(model_values));
        }
        processors.insert(
            "physicalcount".into(),
            FactValue::Integer(physical_count as i64),
        );
        if let Some(speed) = &info.speed {
            processors.insert("speed".into(), FactValue::String(speed.clone()));
        }
        if info.threads > 0 {
            processors.insert("threads".into(), FactValue::Integer(info.threads as i64));
        }

        facts.push(("processors".into(), FactValue::Map(processors)));

        facts
    }
}

struct CpuInfo {
    logical_count: usize,
    physical_count: usize,
    cores: usize,
    threads: usize,
    models: Vec<String>,
    speed: Option<String>,
}

#[cfg(target_os = "linux")]
fn parse_cpuinfo() -> CpuInfo {
    let content = fs::read_to_string("/proc/cpuinfo").unwrap_or_default();

    let mut logical_count: usize = 0;
    let mut physical_ids: HashSet<String> = HashSet::new();
    let mut models: Vec<String> = Vec::new();
    let mut cpu_cores: usize = 0;
    let mut siblings: usize = 0;
    let mut mhz: f64 = 0.0;
    let mut mhz_seen = false;

    for line in content.lines() {
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim();
            let value = value.trim().to_string();

            match key {
                "processor" => {
                    logical_count += 1;
                }
                "model name" => {
                    models.push(value);
                }
                "physical id" => {
                    physical_ids.insert(value);
                }
                "cpu cores" => {
                    if let Ok(n) = value.parse::<usize>() {
                        cpu_cores = n;
                    }
                }
                "siblings" => {
                    if let Ok(n) = value.parse::<usize>() {
                        siblings = n;
                    }
                }
                "cpu MHz" => {
                    if !mhz_seen {
                        if let Ok(v) = value.parse::<f64>() {
                            mhz = v;
                            mhz_seen = true;
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // On single-socket systems, "physical id" may be absent
    let physical_count = if physical_ids.is_empty() {
        if logical_count > 0 { 1 } else { 0 }
    } else {
        physical_ids.len()
    };

    // threads per core = siblings / cpu_cores
    let threads = if cpu_cores > 0 && siblings > 0 {
        siblings / cpu_cores
    } else {
        0
    };

    // Ensure models list is as long as logical_count
    while models.len() < logical_count {
        models.push("Unknown".to_string());
    }

    // Format speed: prefer scaling_max_freq (stable value), fall back to cpu MHz
    let speed = cpu_speed(mhz);

    CpuInfo {
        logical_count,
        physical_count,
        cores: cpu_cores,
        threads,
        models,
        speed,
    }
}

/// Format CPU speed as "X.XX GHz" or "XXX MHz".
#[cfg(target_os = "linux")]
fn cpu_speed(mhz: f64) -> Option<String> {
    if mhz <= 0.0 {
        // Try scaling_max_freq (in kHz)
        if let Ok(s) = fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/scaling_max_freq") {
            if let Ok(khz) = s.trim().parse::<u64>() {
                let mhz_val = khz as f64 / 1000.0;
                return Some(format_mhz(mhz_val));
            }
        }
        return None;
    }
    Some(format_mhz(mhz))
}

#[cfg(target_os = "macos")]
fn parse_cpuinfo() -> CpuInfo {
    use crate::platform::macos::{sysctl_i32, sysctl_string, sysctl_u64};

    let logical_count = sysctl_i32("hw.logicalcpu").unwrap_or(0).max(0) as usize;
    let physical_count = sysctl_i32("hw.physicalcpu").unwrap_or(0).max(0) as usize;
    // threads per core
    let threads = if physical_count > 0 { logical_count / physical_count } else { 0 };

    // Brand string exists on Intel Macs; fall back to hw.model on Apple Silicon
    let model = sysctl_string("machdep.cpu.brand_string")
        .filter(|s| !s.is_empty())
        .or_else(|| sysctl_string("hw.model"))
        .unwrap_or_default();
    let models = if model.is_empty() {
        Vec::new()
    } else {
        vec![model; logical_count.max(1)]
    };

    // hw.cpufrequency / hw.cpufrequency_max are absent on Apple Silicon (returns None)
    let speed = sysctl_u64("hw.cpufrequency_max")
        .or_else(|| sysctl_u64("hw.cpufrequency"))
        .map(|hz| format_mhz(hz as f64 / 1_000_000.0));

    CpuInfo {
        logical_count,
        physical_count,
        cores: physical_count,
        threads,
        models,
        speed,
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn parse_cpuinfo() -> CpuInfo {
    // POSIX fallback: at least get the logical processor count
    let logical = unsafe { libc::sysconf(libc::_SC_NPROCESSORS_ONLN) };
    let logical_count = if logical > 0 { logical as usize } else { 1 };
    CpuInfo {
        logical_count,
        physical_count: logical_count,
        cores: 0,
        threads: 0,
        models: Vec::new(),
        speed: None,
    }
}

fn format_mhz(mhz: f64) -> String {
    if mhz >= 1000.0 {
        format!("{:.2} GHz", mhz / 1000.0)
    } else {
        format!("{:.0} MHz", mhz)
    }
}

/// Get the ISA from uname(machine) — e.g. "x86_64", "aarch64".
fn cpu_isa() -> String {
    let mut u: libc::utsname = unsafe { std::mem::zeroed() };
    if unsafe { libc::uname(&mut u) } != 0 {
        return String::new();
    }
    unsafe { CStr::from_ptr(u.machine.as_ptr()).to_string_lossy().into_owned() }
}

/// Compute x86_64 microarchitecture level extensions from /proc/cpuinfo flags.
#[cfg(not(target_os = "linux"))]
fn cpu_extensions(_isa: &str) -> Vec<String> {
    Vec::new()
}

#[cfg(target_os = "linux")]
fn cpu_extensions(isa: &str) -> Vec<String> {
    if isa != "x86_64" {
        return Vec::new();
    }
    let content = fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    let flags: HashSet<&str> = content
        .lines()
        .find(|l| l.starts_with("flags"))
        .and_then(|l| l.split_once(':'))
        .map(|(_, v)| v.split_whitespace().collect())
        .unwrap_or_default();

    // x86_64-v1: lm, cmov, cx8, fpu, fxsr, mmx, syscall, sse, sse2
    // x86_64-v2: cx16, lahf_lm, popcnt, pni (sse3), sse4_1, sse4_2, ssse3
    // x86_64-v3: avx, avx2, bmi1, bmi2, f16c, fma, abm (lzcnt), movbe
    // x86_64-v4: avx512f, avx512bw, avx512cd, avx512dq, avx512vl
    const V1: &[&str] = &["lm", "cmov", "cx8", "fpu", "fxsr", "mmx", "syscall", "sse", "sse2"];
    const V2: &[&str] = &["cx16", "lahf_lm", "popcnt", "pni", "sse4_1", "sse4_2", "ssse3"];
    const V3: &[&str] = &["avx", "avx2", "bmi1", "bmi2", "f16c", "fma", "abm", "movbe"];
    const V4: &[&str] = &["avx512f", "avx512bw", "avx512cd", "avx512dq", "avx512vl"];

    let mut exts = vec!["x86_64".to_string()];
    if V1.iter().all(|f| flags.contains(f)) {
        exts.push("x86_64-v1".to_string());
        if V2.iter().all(|f| flags.contains(f)) {
            exts.push("x86_64-v2".to_string());
            if V3.iter().all(|f| flags.contains(f)) {
                exts.push("x86_64-v3".to_string());
                if V4.iter().all(|f| flags.contains(f)) {
                    exts.push("x86_64-v4".to_string());
                }
            }
        }
    }
    exts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_mhz_ghz() {
        assert_eq!(format_mhz(3600.0), "3.60 GHz");
        assert_eq!(format_mhz(1000.0), "1.00 GHz");
        assert_eq!(format_mhz(2400.5), "2.40 GHz");
    }

    #[test]
    fn format_mhz_mhz() {
        assert_eq!(format_mhz(800.0), "800 MHz");
        assert_eq!(format_mhz(999.9), "1000 MHz");
        assert_eq!(format_mhz(400.0), "400 MHz");
    }

    #[test]
    fn cpu_extensions_non_x86_returns_empty() {
        assert!(cpu_extensions("aarch64").is_empty());
        assert!(cpu_extensions("").is_empty());
    }

    #[test]
    fn cpu_extensions_x86_64_always_starts_with_x86_64() {
        // x86_64 host: first element is always "x86_64"
        let exts = cpu_extensions("x86_64");
        assert!(!exts.is_empty());
        assert_eq!(exts[0], "x86_64");
    }

    #[test]
    fn cpu_extensions_levels_are_cumulative() {
        // If v3 is present, v1 and v2 must also be present
        let exts = cpu_extensions("x86_64");
        if exts.contains(&"x86_64-v3".to_string()) {
            assert!(exts.contains(&"x86_64-v1".to_string()));
            assert!(exts.contains(&"x86_64-v2".to_string()));
        }
    }
}
