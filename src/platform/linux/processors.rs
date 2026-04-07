use crate::platform::types::CpuInfo;
use std::collections::HashSet;
use std::fs;

pub fn collect_cpu() -> CpuInfo {
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

pub fn cpu_extensions(isa: &str) -> Vec<String> {
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

pub fn format_mhz(mhz: f64) -> String {
    if mhz >= 1000.0 {
        format!("{:.2} GHz", mhz / 1000.0)
    } else {
        format!("{:.0} MHz", mhz)
    }
}

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
        let exts = cpu_extensions("x86_64");
        assert!(!exts.is_empty());
        assert_eq!(exts[0], "x86_64");
    }

    #[test]
    fn cpu_extensions_levels_are_cumulative() {
        let exts = cpu_extensions("x86_64");
        if exts.contains(&"x86_64-v3".to_string()) {
            assert!(exts.contains(&"x86_64-v1".to_string()));
            assert!(exts.contains(&"x86_64-v2".to_string()));
        }
    }
}
