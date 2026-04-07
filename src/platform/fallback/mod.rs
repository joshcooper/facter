use std::collections::{HashMap, HashSet};
use crate::platform::types::*;

pub fn collect_mem() -> Option<MemData> {
    None
}

pub fn collect_cpu() -> CpuInfo {
    let logical = unsafe { libc::sysconf(libc::_SC_NPROCESSORS_ONLN) };
    let count = if logical > 0 { logical as usize } else { 1 };
    CpuInfo {
        logical_count: count,
        physical_count: count,
        cores: 0,
        threads: 0,
        models: Vec::new(),
        speed: None,
    }
}

pub fn cpu_extensions(_isa: &str) -> Vec<String> {
    Vec::new()
}

pub fn read_os_release() -> HashMap<String, String> {
    HashMap::new()
}

pub fn collect_selinux() -> SelinuxData {
    SelinuxData {
        supported: false,
        enabled: false,
        enforced: false,
        current_mode: String::new(),
        config_mode: String::new(),
        config_policy: String::new(),
        policy_version: String::new(),
    }
}

pub fn macos_product_version() -> Option<MacosVersion> {
    None
}

pub fn augment_interfaces(_map: &mut HashMap<String, Interface>) {}

pub fn augment_ipv6_flags(_map: &mut HashMap<String, Interface>) {}

pub fn get_primary_interface() -> Option<String> {
    None
}

pub fn collect_dhcp_servers() -> HashMap<String, String> {
    HashMap::new()
}

pub fn collect_mountpoints() -> Vec<MountInfo> {
    Vec::new()
}

pub fn collect_filesystem_types() -> HashSet<String> {
    HashSet::new()
}

pub fn collect_partitions(_mounts: &[MountInfo]) -> Vec<PartitionInfo> {
    Vec::new()
}

pub fn collect_disks() -> Vec<DiskInfo> {
    Vec::new()
}

pub fn read_dmi(_field: &str) -> Option<String> {
    None
}

pub fn read_uptime_seconds() -> Option<u64> {
    None
}

pub fn dmi_vendor() -> String {
    String::new()
}

pub fn detect_virtualization() -> (String, bool) {
    ("physical".to_string(), false)
}
