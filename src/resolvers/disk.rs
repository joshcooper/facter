use crate::facts::value::FactValue;
use crate::resolvers::Resolver;
use indexmap::IndexMap;
use std::fs;
use std::path::Path;

pub struct DiskResolver;

impl Resolver for DiskResolver {
    fn name(&self) -> &'static str {
        "disks"
    }

    fn resolve(&self) -> Vec<(String, FactValue)> {
        let mut facts = Vec::new();

        let disks = collect_disks();
        if disks.is_empty() {
            return facts;
        }

        let mut names: Vec<String> = disks.iter().map(|d| d.name.clone()).collect();
        names.sort();
        facts.push(("blockdevices".into(), FactValue::String(names.join(","))));

        let mut disks_map: IndexMap<String, FactValue> = IndexMap::new();
        for disk in &disks {
            // Legacy flat facts
            if let Some(v) = &disk.vendor {
                facts.push((
                    format!("blockdevice_{}_vendor", disk.name),
                    FactValue::String(v.clone()),
                ));
            }
            if let Some(v) = &disk.model {
                facts.push((
                    format!("blockdevice_{}_model", disk.name),
                    FactValue::String(v.clone()),
                ));
            }
            if disk.size_bytes > 0 {
                facts.push((
                    format!("blockdevice_{}_size", disk.name),
                    FactValue::Integer(disk.size_bytes as i64),
                ));
            }

            // Structured entry
            let mut entry: IndexMap<String, FactValue> = IndexMap::new();
            if let Some(v) = &disk.vendor {
                entry.insert("vendor".into(), FactValue::String(v.clone()));
            }
            if let Some(v) = &disk.model {
                entry.insert("model".into(), FactValue::String(v.clone()));
            }
            if disk.size_bytes > 0 {
                entry.insert(
                    "size".into(),
                    FactValue::String(fmt_bytes(disk.size_bytes)),
                );
                entry.insert(
                    "size_bytes".into(),
                    FactValue::Integer(disk.size_bytes as i64),
                );
            }
            disks_map.insert(disk.name.clone(), FactValue::Map(entry));
        }

        facts.push(("disks".into(), FactValue::Map(disks_map)));
        facts
    }
}

struct DiskInfo {
    name: String,
    vendor: Option<String>,
    model: Option<String>,
    size_bytes: u64,
}

fn collect_disks() -> Vec<DiskInfo> {
    let mut disks = Vec::new();
    let block_dir = Path::new("/sys/block");
    let entries = match fs::read_dir(block_dir) {
        Ok(e) => e,
        Err(_) => return disks,
    };

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let dev_path = entry.path();

        // Skip virtual/mapped devices: must have a device/ subdirectory
        if !dev_path.join("device").exists() {
            continue;
        }

        let size_bytes = read_u64(&dev_path.join("size")).unwrap_or(0) * 512;
        let vendor = read_trimmed(&dev_path.join("device/vendor"));
        let model = read_trimmed(&dev_path.join("device/model"));

        disks.push(DiskInfo { name, vendor, model, size_bytes });
    }

    disks.sort_by(|a, b| a.name.cmp(&b.name));
    disks
}

fn read_u64(path: &Path) -> Option<u64> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

fn read_trimmed(path: &Path) -> Option<String> {
    let s = fs::read_to_string(path).ok()?.trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

const GIB: u64 = 1 << 30;
const MIB: u64 = 1 << 20;
const KIB: u64 = 1 << 10;

fn fmt_bytes(bytes: u64) -> String {
    if bytes >= GIB {
        format!("{:.2} GiB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.2} MiB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.2} KiB", bytes as f64 / KIB as f64)
    } else {
        format!("{} bytes", bytes)
    }
}
