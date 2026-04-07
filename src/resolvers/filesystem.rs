use crate::facts::value::FactValue;
use crate::resolvers::Resolver;
use indexmap::IndexMap;
use std::collections::{HashMap, HashSet};
use std::ffi::CString;
use std::fs;
use std::path::Path;
use std::process::Command;

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
    partlabel: Option<String>,
    parttype: Option<String>,
    partuuid: Option<String>,
    backing_file: Option<String>,
}

// ── Mountpoints ───────────────────────────────────────────────────────────────

/// Virtual/pseudo filesystem types excluded from mountpoints (but not from the `filesystems` fact).
/// Matches C++ facter's filtering: excludes kernel/pseudo filesystems but keeps device and FUSE mounts.
const VIRTUAL_FS: &[&str] = &[
    "autofs", "binfmt_misc", "bpf", "cgroup", "cgroup2", "configfs", "debugfs",
    "efivarfs", "fusectl", "overlay", "proc", "pstore", "rpc_pipefs",
    "securityfs", "selinuxfs", "sysfs", "tracefs",
];

#[cfg(not(target_os = "macos"))]
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

        // Skip virtual/pseudo filesystems
        if VIRTUAL_FS.contains(&fs_type.as_str()) {
            continue;
        }

        // Deduplicate: keep only first entry per mountpoint
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

/// Use getmntinfo() to enumerate mounts on macOS.
#[cfg(target_os = "macos")]
fn collect_mountpoints() -> Vec<MountInfo> {
    use std::ffi::CStr;

    const VIRTUAL_FS_MACOS: &[&str] = &[
        "autofs", "devfs", "mtmfs",
    ];

    let mut mounts = Vec::new();
    let mut mntbuf: *mut libc::statfs = std::ptr::null_mut();
    let count = unsafe { libc::getmntinfo(&mut mntbuf, libc::MNT_NOWAIT) };
    if count <= 0 {
        return mounts;
    }

    let mut seen_mps: std::collections::HashSet<String> = std::collections::HashSet::new();
    for i in 0..count as usize {
        let m = unsafe { &*mntbuf.add(i) };
        let fs_type = unsafe {
            CStr::from_ptr(m.f_fstypename.as_ptr()).to_string_lossy().into_owned()
        };
        if VIRTUAL_FS.contains(&fs_type.as_str()) || VIRTUAL_FS_MACOS.contains(&fs_type.as_str()) {
            continue;
        }
        let mountpoint = unsafe {
            CStr::from_ptr(m.f_mntonname.as_ptr()).to_string_lossy().into_owned()
        };
        if seen_mps.contains(&mountpoint) {
            continue;
        }
        seen_mps.insert(mountpoint.clone());
        let device = unsafe {
            CStr::from_ptr(m.f_mntfromname.as_ptr()).to_string_lossy().into_owned()
        };
        let frsize = m.f_bsize as u64;
        let size_bytes = m.f_blocks as u64 * frsize;
        let available_bytes = m.f_bavail as u64 * frsize;
        let used_bytes = size_bytes.saturating_sub(available_bytes);
        let options = mount_flags_to_options(m.f_flags);
        mounts.push(MountInfo {
            device, mountpoint, fs_type, options, size_bytes, available_bytes, used_bytes,
        });
    }
    mounts.sort_by(|a, b| a.mountpoint.cmp(&b.mountpoint));
    mounts
}

/// Convert macOS mount flags bitmask to option strings (ro/rw, nosuid, etc.).
#[cfg(target_os = "macos")]
fn mount_flags_to_options(flags: u32) -> Vec<String> {
    let mut opts = Vec::new();
    if flags & libc::MNT_RDONLY != 0 { opts.push("ro".to_string()); } else { opts.push("rw".to_string()); }
    if flags & libc::MNT_NOSUID != 0 { opts.push("nosuid".to_string()); }
    if flags & libc::MNT_NODEV != 0 { opts.push("nodev".to_string()); }
    if flags & libc::MNT_NOEXEC != 0 { opts.push("noexec".to_string()); }
    opts
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
    let cap = if m.size_bytes == 0 {
        "100%".to_string()
    } else if m.used_bytes == 0 {
        "0%".to_string()
    } else {
        fmt_percent((m.used_bytes as f64 / m.size_bytes as f64) * 100.0)
    };
    map.insert("capacity".into(), FactValue::String(cap));
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

#[cfg(not(target_os = "macos"))]
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

/// On macOS, collect filesystem types from the current mounts (getmntinfo).
#[cfg(target_os = "macos")]
fn collect_filesystem_types() -> HashSet<String> {
    use std::ffi::CStr;
    let mut types = HashSet::new();
    let mut mntbuf: *mut libc::statfs = std::ptr::null_mut();
    let count = unsafe { libc::getmntinfo(&mut mntbuf, libc::MNT_NOWAIT) };
    if count <= 0 {
        return types;
    }
    for i in 0..count as usize {
        let m = unsafe { &*mntbuf.add(i) };
        let fs_type = unsafe {
            CStr::from_ptr(m.f_fstypename.as_ptr()).to_string_lossy().into_owned()
        };
        if !fs_type.is_empty() {
            types.insert(fs_type);
        }
    }
    types
}

// ── Partitions ────────────────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
fn collect_partitions(_mounts: &[MountInfo]) -> Vec<PartitionInfo> {
    Vec::new()
}

#[cfg(not(target_os = "macos"))]
fn collect_partitions(mounts: &[MountInfo]) -> Vec<PartitionInfo> {
    // Build a mount lookup: device -> (mountpoint, fstype); also include swap devices
    let swap_devices = collect_swap_devices();
    let mut mount_lookup_owned: HashMap<String, (String, String)> = mounts
        .iter()
        .map(|m| (m.device.clone(), (m.mountpoint.clone(), m.fs_type.clone())))
        .collect();
    for dev in &swap_devices {
        mount_lookup_owned.entry(dev.clone()).or_insert_with(|| ("".to_string(), "swap".to_string()));
    }
    let mount_lookup: HashMap<&str, (&str, &str)> = mount_lookup_owned
        .iter()
        .map(|(k, (mp, fs))| (k.as_str(), (mp.as_str(), fs.as_str())))
        .collect();

    // Run lsblk once to get filesystem types and partition type GUIDs
    let lsblk_info = run_lsblk();

    // Build UUID lookup from /dev/disk/by-uuid symlinks
    let uuid_map = build_uuid_map();
    // Build label lookup from /dev/disk/by-label symlinks
    let label_map = build_label_map();
    // Build partition metadata lookups
    let partlabel_map = read_disk_by_dir("/dev/disk/by-partlabel");
    let parttype_map = read_disk_by_dir("/dev/disk/by-parttype");
    let partuuid_map = read_disk_by_dir("/dev/disk/by-partuuid");
    // Build a dev-path → uuid map for mapper devices
    let devpath_uuid_map = build_devpath_uuid_map();

    let block_dir = Path::new("/sys/block");
    let mut partitions = Vec::new();

    let entries = match fs::read_dir(block_dir) {
        Ok(e) => e,
        Err(_) => return vec![],
    };

    for disk_entry in entries.flatten() {
        let disk_name = disk_entry.file_name().to_string_lossy().into_owned();
        let disk_path = disk_entry.path();

        // Handle device-mapper devices (LVM, LUKS, etc.)
        let dm_name_path = disk_path.join("dm/name");
        if dm_name_path.exists() {
            if let Ok(dm_name) = fs::read_to_string(&dm_name_path) {
                let dm_name = dm_name.trim().to_string();
                if !dm_name.is_empty() {
                    let dev_path = format!("/dev/mapper/{}", dm_name);
                    let size_sectors = fs::read_to_string(disk_path.join("size"))
                        .ok()
                        .and_then(|s| s.trim().parse::<u64>().ok())
                        .unwrap_or(0);
                    let size_bytes = size_sectors * 512;
                    let (raw_mp, raw_fs) = mount_lookup
                        .get(dev_path.as_str())
                        .copied()
                        .unwrap_or_else(|| {
                            // Also try the /dev/dm-N path
                            let alt = format!("/dev/{}", disk_name);
                            mount_lookup.get(alt.as_str()).copied().unwrap_or(("", ""))
                        });
                    let mountpoint = raw_mp.to_string();
                    // For filesystem type: prefer lsblk (most accurate), fall back to mount/uevent
                    let fs_type = lsblk_info.get(&dev_path).and_then(|l| l.fstype.clone())
                        .unwrap_or_else(|| if !raw_fs.is_empty() {
                            raw_fs.to_string()
                        } else {
                            read_dm_fstype(&disk_path).unwrap_or_default()
                        });
                    // UUID: prefer lsblk (covers LVM PV UUIDs), fall back to by-uuid symlinks
                    let uuid = lsblk_info.get(&dev_path).and_then(|l| l.uuid.clone())
                        .or_else(|| fs::canonicalize(&dev_path).ok()
                            .and_then(|canon| devpath_uuid_map.get(canon.to_string_lossy().as_ref()).cloned()))
                        .or_else(|| uuid_map.get(&disk_name).cloned());
                    partitions.push(PartitionInfo {
                        uuid,
                        label: label_map.get(&disk_name).cloned(),
                        partlabel: None,
                        parttype: None,
                        partuuid: None,
                        backing_file: None,
                        name: dev_path,
                        size_bytes,
                        fs_type,
                        mountpoint,
                    });
                }
            }
            continue;
        }

        // Handle loop devices — include all that have a backing file
        if disk_name.starts_with("loop") {
            let backing_file = fs::read_to_string(disk_path.join("loop/backing_file"))
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            // Include loop if it has a backing file (even if not mounted)
            if backing_file.is_some() || mount_lookup.contains_key(format!("/dev/{}", disk_name).as_str()) {
                let dev_path = format!("/dev/{}", disk_name);
                let size_sectors = fs::read_to_string(disk_path.join("size"))
                    .ok()
                    .and_then(|s| s.trim().parse::<u64>().ok())
                    .unwrap_or(0);
                let size_bytes = size_sectors * 512;
                let (mountpoint, fs_type) = mount_lookup
                    .get(dev_path.as_str())
                    .copied()
                    .unwrap_or(("", ""));
                let fs_type = if !fs_type.is_empty() {
                    fs_type.to_string()
                } else {
                    lsblk_info.get(&dev_path).and_then(|l| l.fstype.clone()).unwrap_or_default()
                };
                partitions.push(PartitionInfo {
                    uuid: uuid_map.get(&disk_name).cloned(),
                    label: label_map.get(&disk_name).cloned(),
                    partlabel: None,
                    parttype: None,
                    partuuid: None,
                    backing_file,
                    name: dev_path,
                    size_bytes,
                    fs_type,
                    mountpoint: mountpoint.to_string(),
                });
            }
            continue;
        }

        // Standard disk: scan sub-entries for partition directories (e.g. sda1, nvme0n1p1)
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

            let lsblk = lsblk_info.get(&dev_path);
            let fs_type = if !fs_type.is_empty() {
                fs_type.to_string()
            } else {
                lsblk.and_then(|l| l.fstype.clone()).unwrap_or_default()
            };
            let parttype = parttype_map.get(&part_name).cloned()
                .or_else(|| lsblk.and_then(|l| l.parttype.clone()));
            let uuid = uuid_map.get(&part_name).cloned()
                .or_else(|| lsblk.and_then(|l| l.uuid.clone()));
            partitions.push(PartitionInfo {
                uuid,
                label: label_map.get(&part_name).cloned(),
                partlabel: partlabel_map.get(&part_name).cloned(),
                parttype,
                partuuid: partuuid_map.get(&part_name).cloned(),
                backing_file: None,
                name: dev_path,
                size_bytes,
                fs_type,
                mountpoint: mountpoint.to_string(),
            });
        }
    }

    partitions.sort_by(|a, b| a.name.cmp(&b.name));
    partitions
}

#[cfg(not(target_os = "macos"))]
struct LsblkInfo {
    fstype: Option<String>,
    parttype: Option<String>,
    uuid: Option<String>,
}

/// Run `lsblk -o PATH,FSTYPE,PARTTYPE,UUID --json` and return a map of device path → info.
#[cfg(not(target_os = "macos"))]
fn run_lsblk() -> HashMap<String, LsblkInfo> {
    let output = match Command::new("lsblk")
        .args(["-o", "PATH,FSTYPE,PARTTYPE,UUID", "--json"])
        .output()
    {
        Ok(o) if o.status.success() => o.stdout,
        _ => return HashMap::new(),
    };
    let text = String::from_utf8_lossy(&output);
    let v: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(_) => return HashMap::new(),
    };
    let mut map = HashMap::new();
    fn walk(node: &serde_json::Value, map: &mut HashMap<String, LsblkInfo>) {
        if let Some(path) = node.get("path").and_then(|v| v.as_str()) {
            let fstype = node.get("fstype").and_then(|v| v.as_str()).filter(|s| !s.is_empty()).map(String::from);
            let parttype = node.get("parttype").and_then(|v| v.as_str()).filter(|s| !s.is_empty()).map(String::from);
            let uuid = node.get("uuid").and_then(|v| v.as_str()).filter(|s| !s.is_empty()).map(String::from);
            if fstype.is_some() || parttype.is_some() || uuid.is_some() {
                map.insert(path.to_string(), LsblkInfo { fstype, parttype, uuid });
            }
        }
        if let Some(children) = node.get("children").and_then(|v| v.as_array()) {
            for child in children {
                walk(child, map);
            }
        }
    }
    if let Some(devs) = v.get("blockdevices").and_then(|v| v.as_array()) {
        for dev in devs {
            walk(dev, &mut map);
        }
    }
    map
}

/// Read /proc/swaps and return the list of swap device paths.
#[cfg(not(target_os = "macos"))]
fn collect_swap_devices() -> Vec<String> {
    let content = match fs::read_to_string("/proc/swaps") {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    content
        .lines()
        .skip(1) // skip header
        .filter_map(|line| line.split_whitespace().next().map(String::from))
        .filter(|s| s.starts_with('/'))
        .collect()
}

/// Read filesystem type for a DM device from its udev uevent file.
#[cfg(not(target_os = "macos"))]
fn read_dm_fstype(sysfs_path: &Path) -> Option<String> {
    let uevent = fs::read_to_string(sysfs_path.join("uevent")).ok()?;
    for line in uevent.lines() {
        if let Some(val) = line.strip_prefix("ID_FS_TYPE=") {
            let s = val.trim().to_string();
            if !s.is_empty() {
                return Some(s);
            }
        }
    }
    // Also check dm/uuid for LUKS pattern to detect crypto_LUKS
    if let Ok(dm_uuid) = fs::read_to_string(sysfs_path.join("dm/uuid")) {
        let dm_uuid = dm_uuid.trim();
        if dm_uuid.starts_with("CRYPT-LUKS") {
            return Some("crypto_LUKS".to_string());
        }
    }
    None
}

#[cfg(not(target_os = "macos"))]
fn build_uuid_map() -> HashMap<String, String> {
    read_disk_by_dir("/dev/disk/by-uuid")
}

#[cfg(not(target_os = "macos"))]
fn build_label_map() -> HashMap<String, String> {
    read_disk_by_dir("/dev/disk/by-label")
}

/// Maps resolved device path (e.g. `/dev/mapper/foo`) → uuid identifier.
#[cfg(not(target_os = "macos"))]
fn build_devpath_uuid_map() -> HashMap<String, String> {
    let mut map = HashMap::new();
    if let Ok(entries) = fs::read_dir("/dev/disk/by-uuid") {
        for entry in entries.flatten() {
            if let Ok(target) = fs::read_link(entry.path()) {
                // Resolve relative symlink against /dev/disk/by-uuid/
                let resolved = if target.is_absolute() {
                    target
                } else {
                    std::path::Path::new("/dev/disk/by-uuid").join(&target)
                };
                // Canonicalize to resolve ../../ components
                if let Ok(canon) = fs::canonicalize(&resolved) {
                    let uuid = entry.file_name().to_string_lossy().into_owned();
                    map.insert(canon.to_string_lossy().into_owned(), uuid);
                }
            }
        }
    }
    map
}

/// Read a /dev/disk/by-<X>/ directory, mapping device short name → identifier.
/// Symlink names may use `\xNN` udev escaping (e.g. `\x20` for space); these are unescaped.
#[cfg(not(target_os = "macos"))]
fn read_disk_by_dir(dir: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Ok(target) = fs::read_link(entry.path()) {
                if let Some(dev_name) = target.file_name() {
                    let raw = entry.file_name().to_string_lossy().into_owned();
                    let identifier = udev_unescape(&raw);
                    map.insert(dev_name.to_string_lossy().into_owned(), identifier);
                }
            }
        }
    }
    map
}

/// Unescape udev's `\xNN` encoding in partition label/type symlink names.
fn udev_unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            let mut peek = chars.clone();
            if peek.next() == Some('x') {
                let h1 = peek.next();
                let h2 = peek.next();
                if let (Some(a), Some(b)) = (h1, h2) {
                    if let Ok(byte) = u8::from_str_radix(&format!("{}{}", a, b), 16) {
                        out.push(byte as char);
                        chars.next(); // consume 'x'
                        chars.next(); // consume first hex digit
                        chars.next(); // consume second hex digit
                        continue;
                    }
                }
            }
        }
        out.push(c);
    }
    out
}

fn build_partition_map(p: &PartitionInfo) -> FactValue {
    let mut map: IndexMap<String, FactValue> = IndexMap::new();
    if let Some(bf) = &p.backing_file {
        map.insert("backing_file".into(), FactValue::String(bf.clone()));
    }
    if !p.fs_type.is_empty() {
        map.insert("filesystem".into(), FactValue::String(p.fs_type.clone()));
    }
    if let Some(label) = &p.label {
        map.insert("label".into(), FactValue::String(label.clone()));
    }
    if !p.mountpoint.is_empty() {
        map.insert("mount".into(), FactValue::String(p.mountpoint.clone()));
    }
    if let Some(pl) = &p.partlabel {
        map.insert("partlabel".into(), FactValue::String(pl.clone()));
    }
    if let Some(pt) = &p.parttype {
        map.insert("parttype".into(), FactValue::String(pt.clone()));
    }
    if let Some(pu) = &p.partuuid {
        map.insert("partuuid".into(), FactValue::String(pu.clone()));
    }
    map.insert("size".into(), FactValue::String(fmt_bytes(p.size_bytes)));
    map.insert("size_bytes".into(), FactValue::Integer(p.size_bytes as i64));
    if let Some(uuid) = &p.uuid {
        map.insert("uuid".into(), FactValue::String(uuid.clone()));
    }
    FactValue::Map(map)
}

// ── Formatting ────────────────────────────────────────────────────────────────

fn fmt_percent(pct: f64) -> String {
    let s = format!("{:.2}", pct);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    format!("{}%", s)
}

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

#[cfg(test)]
mod tests {
    use super::*;

    // ── udev_unescape ────────────────────────────────────────────────────────

    #[test]
    fn udev_unescape_no_escapes() {
        assert_eq!(udev_unescape("EFISystem"), "EFISystem");
    }

    #[test]
    fn udev_unescape_space() {
        assert_eq!(udev_unescape("EFI\\x20System\\x20Partition"), "EFI System Partition");
    }

    #[test]
    fn udev_unescape_multiple() {
        assert_eq!(udev_unescape("a\\x2fb"), "a/b");
    }

    #[test]
    fn udev_unescape_incomplete_sequence_left_as_is() {
        // \x with fewer than 2 hex digits is not a valid escape
        assert_eq!(udev_unescape("\\x2"), "\\x2");
    }

    #[test]
    fn udev_unescape_backslash_not_x() {
        assert_eq!(udev_unescape("foo\\nbar"), "foo\\nbar");
    }

    // ── fmt_percent ──────────────────────────────────────────────────────────

    #[test]
    fn fmt_percent_strips_trailing_zeros() {
        assert_eq!(fmt_percent(50.0), "50%");
        assert_eq!(fmt_percent(33.33), "33.33%");
        assert_eq!(fmt_percent(10.10), "10.1%");
    }

    #[test]
    fn fmt_percent_zero() {
        assert_eq!(fmt_percent(0.0), "0%");
    }

    #[test]
    fn fmt_percent_hundred() {
        assert_eq!(fmt_percent(100.0), "100%");
    }

    // ── fmt_bytes ────────────────────────────────────────────────────────────

    #[test]
    fn fmt_bytes_tib() {
        assert_eq!(fmt_bytes(2 * TIB), "2.00 TiB");
    }

    #[test]
    fn fmt_bytes_gib() {
        assert_eq!(fmt_bytes(GIB), "1.00 GiB");
    }

    #[test]
    fn fmt_bytes_mib() {
        assert_eq!(fmt_bytes(MIB), "1.00 MiB");
    }

    #[test]
    fn fmt_bytes_kib() {
        assert_eq!(fmt_bytes(KIB), "1.00 KiB");
    }

    #[test]
    fn fmt_bytes_small() {
        assert_eq!(fmt_bytes(1), "1 bytes");
    }
}
