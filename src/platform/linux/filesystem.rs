use crate::platform::types::{MountInfo, PartitionInfo};
use std::collections::{HashMap, HashSet};
use std::ffi::CString;
use std::fs;
use std::path::Path;
use std::process::Command;

/// Virtual/pseudo filesystem types excluded from mountpoints.
const VIRTUAL_FS: &[&str] = &[
    "autofs", "binfmt_misc", "bpf", "cgroup", "cgroup2", "configfs", "debugfs",
    "efivarfs", "fusectl", "overlay", "proc", "pstore", "rpc_pipefs",
    "securityfs", "selinuxfs", "sysfs", "tracefs",
];

pub fn collect_mountpoints() -> Vec<MountInfo> {
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

        if VIRTUAL_FS.contains(&fs_type.as_str()) {
            continue;
        }

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

pub fn collect_filesystem_types() -> HashSet<String> {
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

pub fn collect_partitions(mounts: &[MountInfo]) -> Vec<PartitionInfo> {
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

    let lsblk_info = run_lsblk();

    let uuid_map = build_uuid_map();
    let label_map = build_label_map();
    let partlabel_map = read_disk_by_dir("/dev/disk/by-partlabel");
    let parttype_map = read_disk_by_dir("/dev/disk/by-parttype");
    let partuuid_map = read_disk_by_dir("/dev/disk/by-partuuid");
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
                            let alt = format!("/dev/{}", disk_name);
                            mount_lookup.get(alt.as_str()).copied().unwrap_or(("", ""))
                        });
                    let mountpoint = raw_mp.to_string();
                    let fs_type = lsblk_info.get(&dev_path).and_then(|l| l.fstype.clone())
                        .unwrap_or_else(|| if !raw_fs.is_empty() {
                            raw_fs.to_string()
                        } else {
                            read_dm_fstype(&disk_path).unwrap_or_default()
                        });
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

        // Handle loop devices
        if disk_name.starts_with("loop") {
            let backing_file = fs::read_to_string(disk_path.join("loop/backing_file"))
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
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

        // Standard disk: scan sub-entries for partition directories
        let sub_entries = match fs::read_dir(&disk_path) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for part_entry in sub_entries.flatten() {
            let part_name = part_entry.file_name().to_string_lossy().into_owned();
            let part_path = part_entry.path();
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

struct LsblkInfo {
    fstype: Option<String>,
    parttype: Option<String>,
    uuid: Option<String>,
}

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

fn collect_swap_devices() -> Vec<String> {
    let content = match fs::read_to_string("/proc/swaps") {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    content
        .lines()
        .skip(1)
        .filter_map(|line| line.split_whitespace().next().map(String::from))
        .filter(|s| s.starts_with('/'))
        .collect()
}

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
    if let Ok(dm_uuid) = fs::read_to_string(sysfs_path.join("dm/uuid")) {
        let dm_uuid = dm_uuid.trim();
        if dm_uuid.starts_with("CRYPT-LUKS") {
            return Some("crypto_LUKS".to_string());
        }
    }
    None
}

fn build_uuid_map() -> HashMap<String, String> {
    read_disk_by_dir("/dev/disk/by-uuid")
}

fn build_label_map() -> HashMap<String, String> {
    read_disk_by_dir("/dev/disk/by-label")
}

fn build_devpath_uuid_map() -> HashMap<String, String> {
    let mut map = HashMap::new();
    if let Ok(entries) = fs::read_dir("/dev/disk/by-uuid") {
        for entry in entries.flatten() {
            if let Ok(target) = fs::read_link(entry.path()) {
                let resolved = if target.is_absolute() {
                    target
                } else {
                    std::path::Path::new("/dev/disk/by-uuid").join(&target)
                };
                if let Ok(canon) = fs::canonicalize(&resolved) {
                    let uuid = entry.file_name().to_string_lossy().into_owned();
                    map.insert(canon.to_string_lossy().into_owned(), uuid);
                }
            }
        }
    }
    map
}

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

pub fn udev_unescape(s: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(udev_unescape("\\x2"), "\\x2");
    }

    #[test]
    fn udev_unescape_backslash_not_x() {
        assert_eq!(udev_unescape("foo\\nbar"), "foo\\nbar");
    }
}
