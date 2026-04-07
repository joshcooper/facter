use crate::platform::types::CpuInfo;
use super::sysctl::{sysctl_i32, sysctl_string, sysctl_u64};

pub fn collect_cpu() -> CpuInfo {
    let logical_count = sysctl_i32("hw.logicalcpu").unwrap_or(0).max(0) as usize;
    let physical_count = sysctl_i32("hw.physicalcpu").unwrap_or(0).max(0) as usize;
    let threads = if physical_count > 0 { logical_count / physical_count } else { 0 };

    let model = sysctl_string("machdep.cpu.brand_string")
        .filter(|s| !s.is_empty())
        .or_else(|| sysctl_string("hw.model"))
        .unwrap_or_default();
    let models = if model.is_empty() {
        Vec::new()
    } else {
        vec![model; logical_count.max(1)]
    };

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

pub fn cpu_extensions(_isa: &str) -> Vec<String> {
    Vec::new()
}

fn format_mhz(mhz: f64) -> String {
    if mhz >= 1000.0 {
        format!("{:.2} GHz", mhz / 1000.0)
    } else {
        format!("{:.0} MHz", mhz)
    }
}
