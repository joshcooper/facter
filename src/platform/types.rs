/// Shared data structs used across resolvers and platform implementations.

pub struct MemData {
    pub total: u64,
    pub available: u64,
    pub swap_total: u64,
    pub swap_free: u64,
}

pub struct CpuInfo {
    pub logical_count: usize,
    pub physical_count: usize,
    pub cores: usize,
    pub threads: usize,
    pub models: Vec<String>,
    pub speed: Option<String>,
}

pub struct Ipv4Binding {
    pub address: std::net::Ipv4Addr,
    pub netmask: Option<std::net::Ipv4Addr>,
}

pub struct Ipv6Binding {
    pub address: std::net::Ipv6Addr,
    pub netmask: Option<std::net::Ipv6Addr>,
    pub flags: Vec<String>,
}

pub struct Interface {
    pub name: String,
    pub ipv4: Vec<Ipv4Binding>,
    pub ipv6: Vec<Ipv6Binding>,
    pub mac: Option<String>,
    pub mtu: Option<u64>,
    pub physical: bool,
    pub operational_state: Option<String>,
    pub duplex: Option<String>,
    pub speed: Option<i64>,
}

pub struct MountInfo {
    pub device: String,
    pub mountpoint: String,
    pub fs_type: String,
    pub options: Vec<String>,
    pub size_bytes: u64,
    pub available_bytes: u64,
    pub used_bytes: u64,
}

pub struct PartitionInfo {
    pub name: String,
    pub size_bytes: u64,
    pub fs_type: String,
    pub mountpoint: String,
    pub uuid: Option<String>,
    pub label: Option<String>,
    pub partlabel: Option<String>,
    pub parttype: Option<String>,
    pub partuuid: Option<String>,
    pub backing_file: Option<String>,
}

pub struct DiskInfo {
    pub name: String,
    pub vendor: Option<String>,
    pub model: Option<String>,
    pub serial: Option<String>,
    pub disk_type: Option<String>,
    pub wwn: Option<String>,
    pub size_bytes: u64,
}

pub struct SelinuxData {
    pub supported: bool,
    pub enabled: bool,
    pub enforced: bool,
    pub current_mode: String,
    pub config_mode: String,
    pub config_policy: String,
    pub policy_version: String,
}

pub struct MacosVersion {
    pub product_name: String,
    pub product_version: String,
    pub major: String,
    pub minor: String,
    pub patch: String,
    pub build: String,
}
