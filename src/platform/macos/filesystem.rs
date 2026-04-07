use crate::platform::types::{MountInfo, PartitionInfo};
use std::collections::HashSet;
use std::ffi::CStr;

const VIRTUAL_FS: &[&str] = &[
    "autofs", "binfmt_misc", "bpf", "cgroup", "cgroup2", "configfs", "debugfs",
    "efivarfs", "fusectl", "overlay", "proc", "pstore", "rpc_pipefs",
    "securityfs", "selinuxfs", "sysfs", "tracefs",
];

const VIRTUAL_FS_MACOS: &[&str] = &[
    "autofs", "devfs", "mtmfs",
];

pub fn collect_mountpoints() -> Vec<MountInfo> {
    let mut mounts = Vec::new();
    let mut mntbuf: *mut libc::statfs = std::ptr::null_mut();
    let count = unsafe { libc::getmntinfo(&mut mntbuf, libc::MNT_NOWAIT) };
    if count <= 0 {
        return mounts;
    }

    let mut seen_mps: HashSet<String> = HashSet::new();
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

pub fn collect_filesystem_types() -> HashSet<String> {
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

pub fn collect_partitions(_mounts: &[MountInfo]) -> Vec<PartitionInfo> {
    Vec::new()
}

fn mount_flags_to_options(flags: u32) -> Vec<String> {
    let mut opts = Vec::new();
    if flags & libc::MNT_RDONLY != 0 { opts.push("ro".to_string()); } else { opts.push("rw".to_string()); }
    if flags & libc::MNT_NOSUID != 0 { opts.push("nosuid".to_string()); }
    if flags & libc::MNT_NODEV != 0 { opts.push("nodev".to_string()); }
    if flags & libc::MNT_NOEXEC != 0 { opts.push("noexec".to_string()); }
    opts
}
