use crate::facts::value::FactValue;
use crate::resolvers::Resolver;
use indexmap::IndexMap;
use std::collections::{HashMap, HashSet};
use std::ffi::CString;
use std::fs;
use std::path::Path;

pub struct FilesystemResolver;

impl Resolver for FilesystemResolver {
    fn name(&self) -> &'static str {
        "filesystems"
    }

    fn resolve(&self) -> Vec<(String, FactValue)> {
        let mut facts = Vec::new();

        let mounts = collect_mountpoints();
        let fs_types = collect_filesystem_types();
        let partitions = collect_partitions(&mounts);

        // filesystems flat fact: sorted comma-separated list of supported fs types
        if !fs_types.is_empty() {
            let mut types: Vec<String> = fs_types.into_iter().collect();
            types.sort();
            facts.push(("filesystems".into(), FactValue::String(types.join(","))));
        }

        // mountpoints structured fact
        if !mounts.is_empty() {
            let mut mp_map: IndexMap<String, FactValue> = IndexMap::new();
            for m in &mounts {
                mp_map.insert(m.mountpoint.clone(), build_mount_map(m));
            }
            facts.push(("mountpoints".into(), FactValue::Map(mp_map)));
        }

        // partitions structured fact
        if !partitions.is_empty() {
            let mut part_map: IndexMap<String, FactValue> = IndexMap::new();
            for p in &partitions {
                part_map.insert(p.name.clone(), build_partition_map(p));
            }
            facts.push(("partitions".into(), FactValue::Map(part_map)));
        }

        facts
    }
}

// ── Data types ────────────────────────────────────────────────────────────────

struct MountInfo {
    device: String,
    mountpoint: String,
    fs_type: String,
    options: Vec<String>,
    size_bytes: u64,
    available_bytes: u64,
    used_bytes: u64,
}

struct PartitionInfo {
    name: String,
    size_bytes: u64,
    fs_type: String,
    mountpoint: String,
    uuid: Option<String>,
    label: Option<String>,
}

// ── Mountpoints ───────────────────────────────────────────────────────────────

const SKIP_FS: &[&str] = &[
    "autofs", "binfmt_misc", "cgroup", "cgroup2", "configfs", "debugfs",
    "devpts", "devtmpfs", "efivarfs", "fusectl", "hugetlbfs", "mqueue",
    "nfsd", "nsfs", "overlay", "proc", "pstore", "ramfs", "rpc_pipefs",
    "securityfs", "selinuxfs", "squashfs", "sysfs", "tracefs",
];

fn collect_mountpoints() -> Vec<MountInfo> {
    let content = match fs::read_to_string("/proc/mounts") {
        Ok(c) => c,
        Err(_) => return vec![],
    };

    let mut seen_mps: HashSet<String> = HashSet::new();
    let mut mounts = Vec::new();

    for line in content.lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 4 {
            continue;
        }
        let device = cols[0].to_string();
        let mountpoint = cols[1].to_string();
        let fs_type = cols[2].to_string();
        let options_str = cols[3];

        // Skip virtual/kernel filesystems
        if SKIP_FS.contains(&fs_type.as_str()) {
            continue;
        }
        // Skip mounts under /proc/ or /sys/ (but keep /proc and /sys themselves if present)
        if (mountpoint.starts_with("/proc/") || mountpoint.starts_with("/sys/"))
            && fs_type != "tmpfs"
        {
            continue;
        }
        // Deduplicate: prefer /dev/* entries over other entries for same mountpoint
        if seen_mps.contains(&mountpoint) {
            continue;
        }
        seen_mps.insert(mountpoint.clone());

        let options: Vec<String> = options_str.split(',').map(String::from).collect();
        let (size_bytes, available_bytes) = statvfs_info(&mountpoint);
        let used_bytes = size_bytes.saturating_sub(available_bytes);

        mounts.push(MountInfo {
            device,
            mountpoint,
            fs_type,
            options,
            size_bytes,
            available_bytes,
            used_bytes,
        });
    }

    mounts.sort_by(|a, b| a.mountpoint.cmp(&b.mountpoint));
    mounts
}

fn statvfs_info(path: &str) -> (u64, u64) {
    let cpath = match CString::new(path) {
        Ok(p) => p,
        Err(_) => return (0, 0),
    };
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(cpath.as_ptr(), &mut stat) } != 0 {
        return (0, 0);
    }
    let frsize = stat.f_frsize as u64;
    let total = stat.f_blocks as u64 * frsize;
    let avail = stat.f_bavail as u64 * frsize;
    (total, avail)
}

fn build_mount_map(m: &MountInfo) -> FactValue {
    let mut map: IndexMap<String, FactValue> = IndexMap::new();
    map.insert("available".into(), FactValue::String(fmt_bytes(m.available_bytes)));
    map.insert("available_bytes".into(), FactValue::Integer(m.available_bytes as i64));
    if m.size_bytes > 0 {
        let cap = format!(
            "{:.2}%",
            (m.used_bytes as f64 / m.size_bytes as f64) * 100.0
        );
        map.insert("capacity".into(), FactValue::String(cap));
    }
    map.insert("device".into(), FactValue::String(m.device.clone()));
    map.insert("filesystem".into(), FactValue::String(m.fs_type.clone()));
    let opts: Vec<FactValue> = m.options.iter().cloned().map(FactValue::String).collect();
    map.insert("options".into(), FactValue::Array(opts));
    map.insert("size".into(), FactValue::String(fmt_bytes(m.size_bytes)));
    map.insert("size_bytes".into(), FactValue::Integer(m.size_bytes as i64));
    map.insert("used".into(), FactValue::String(fmt_bytes(m.used_bytes)));
    map.insert("used_bytes".into(), FactValue::Integer(m.used_bytes as i64));
    FactValue::Map(map)
}

// ── Filesystem types ──────────────────────────────────────────────────────────

fn collect_filesystem_types() -> HashSet<String> {
    let content = match fs::read_to_string("/proc/filesystems") {
        Ok(c) => c,
        Err(_) => return HashSet::new(),
    };

    content
        .lines()
        .filter(|l| !l.starts_with("nodev") && !l.contains("fuseblk"))
        .map(|l| l.split_whitespace().last().unwrap_or("").to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

// ── Partitions ────────────────────────────────────────────────────────────────

fn collect_partitions(mounts: &[MountInfo]) -> Vec<PartitionInfo> {
    // Build a mount lookup: device -> (mountpoint, fstype)
    let mount_lookup: HashMap<&str, (&str, &str)> = mounts
        .iter()
        .map(|m| (m.device.as_str(), (m.mountpoint.as_str(), m.fs_type.as_str())))
        .collect();

    // Build UUID lookup from /dev/disk/by-uuid symlinks
    let uuid_map = build_uuid_map();
    // Build label lookup from /dev/disk/by-label symlinks
    let label_map = build_label_map();

    let block_dir = Path::new("/sys/block");
    let mut partitions = Vec::new();

    let entries = match fs::read_dir(block_dir) {
        Ok(e) => e,
        Err(_) => return vec![],
    };

    for disk_entry in entries.flatten() {
        let disk_path = disk_entry.path();
        // Scan sub-entries for partition directories (e.g. sda1, nvme0n1p1)
        let sub_entries = match fs::read_dir(&disk_path) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for part_entry in sub_entries.flatten() {
            let part_name = part_entry.file_name().to_string_lossy().into_owned();
            let part_path = part_entry.path();
            // A partition directory has a `partition` file
            if !part_path.join("partition").exists() {
                continue;
            }
            let size_sectors = fs::read_to_string(part_path.join("size"))
                .ok()
                .and_then(|s| s.trim().parse::<u64>().ok())
                .unwrap_or(0);
            let size_bytes = size_sectors * 512;

            let dev_path = format!("/dev/{}", part_name);
            let (mountpoint, fs_type) = mount_lookup
                .get(dev_path.as_str())
                .copied()
                .unwrap_or(("", ""));

            partitions.push(PartitionInfo {
                uuid: uuid_map.get(&part_name).cloned(),
                label: label_map.get(&part_name).cloned(),
                name: part_name,
                size_bytes,
                fs_type: fs_type.to_string(),
                mountpoint: mountpoint.to_string(),
            });
        }
    }

    partitions.sort_by(|a, b| a.name.cmp(&b.name));
    partitions
}

fn build_uuid_map() -> HashMap<String, String> {
    read_disk_by_dir("/dev/disk/by-uuid")
}

fn build_label_map() -> HashMap<String, String> {
    read_disk_by_dir("/dev/disk/by-label")
}

/// Read a /dev/disk/by-<X>/ directory, mapping device short name → identifier.
fn read_disk_by_dir(dir: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Ok(target) = fs::read_link(entry.path()) {
                if let Some(dev_name) = target.file_name() {
                    let identifier = entry.file_name().to_string_lossy().into_owned();
                    map.insert(dev_name.to_string_lossy().into_owned(), identifier);
                }
            }
        }
    }
    map
}

fn build_partition_map(p: &PartitionInfo) -> FactValue {
    let mut map: IndexMap<String, FactValue> = IndexMap::new();
    if !p.fs_type.is_empty() {
        map.insert("filesystem".into(), FactValue::String(p.fs_type.clone()));
    }
    if let Some(label) = &p.label {
        map.insert("label".into(), FactValue::String(label.clone()));
    }
    if !p.mountpoint.is_empty() {
        map.insert("mount".into(), FactValue::String(p.mountpoint.clone()));
    }
    map.insert("size".into(), FactValue::String(fmt_bytes(p.size_bytes)));
    map.insert("size_bytes".into(), FactValue::Integer(p.size_bytes as i64));
    if let Some(uuid) = &p.uuid {
        map.insert("uuid".into(), FactValue::String(uuid.clone()));
    }
    FactValue::Map(map)
}

// ── Formatting ────────────────────────────────────────────────────────────────

const TIB: u64 = 1 << 40;
const GIB: u64 = 1 << 30;
const MIB: u64 = 1 << 20;
const KIB: u64 = 1 << 10;

fn fmt_bytes(bytes: u64) -> String {
    if bytes >= TIB {
        format!("{:.2} TiB", bytes as f64 / TIB as f64)
    } else if bytes >= GIB {
        format!("{:.2} GiB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.2} MiB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.2} KiB", bytes as f64 / KIB as f64)
    } else {
        format!("{} bytes", bytes)
    }
}
