use crate::facts::value::FactValue;
use crate::resolvers::Resolver;
use crate::platform::types::{MountInfo, PartitionInfo};
use indexmap::IndexMap;
use std::ffi::CString;

pub struct FilesystemResolver;

impl Resolver for FilesystemResolver {
    fn name(&self) -> &'static str {
        "filesystems"
    }

    fn resolve(&self) -> Vec<(String, FactValue)> {
        let mut facts = Vec::new();

        let mounts = crate::platform::collect_mountpoints();
        let fs_types = crate::platform::collect_filesystem_types();
        let partitions = crate::platform::collect_partitions(&mounts);

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

// statvfs_info is kept in the resolver since it's a cross-platform POSIX call
// used by the resolver itself, not part of platform-specific collection.
#[allow(dead_code)]
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
