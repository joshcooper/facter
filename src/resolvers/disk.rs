use crate::facts::value::FactValue;
use crate::resolvers::Resolver;
use indexmap::IndexMap;

pub struct DiskResolver;

impl Resolver for DiskResolver {
    fn name(&self) -> &'static str {
        "disks"
    }

    fn resolve(&self) -> Vec<(String, FactValue)> {
        let mut facts = Vec::new();

        let disks = crate::platform::collect_disks();
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
            facts.push((
                format!("blockdevice_{}_size", disk.name),
                FactValue::Integer(disk.size_bytes as i64),
            ));

            // Structured entry
            let mut entry: IndexMap<String, FactValue> = IndexMap::new();
            if let Some(v) = &disk.vendor {
                entry.insert("vendor".into(), FactValue::String(v.clone()));
            }
            if let Some(v) = &disk.model {
                entry.insert("model".into(), FactValue::String(v.clone()));
            }
            if let Some(s) = &disk.serial {
                entry.insert("serial".into(), FactValue::String(s.clone()));
            }
            entry.insert(
                "size".into(),
                FactValue::String(fmt_bytes(disk.size_bytes)),
            );
            entry.insert(
                "size_bytes".into(),
                FactValue::Integer(disk.size_bytes as i64),
            );
            if let Some(t) = &disk.disk_type {
                entry.insert("type".into(), FactValue::String(t.clone()));
            }
            if let Some(w) = &disk.wwn {
                entry.insert("wwn".into(), FactValue::String(w.clone()));
            }
            disks_map.insert(disk.name.clone(), FactValue::Map(entry));
        }

        facts.push(("disks".into(), FactValue::Map(disks_map)));
        facts
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    // ── fmt_bytes ────────────────────────────────────────────────────────────

    #[test]
    fn fmt_bytes_gib() {
        assert_eq!(fmt_bytes(500 * GIB), "500.00 GiB");
        assert_eq!(fmt_bytes(GIB), "1.00 GiB");
    }

    #[test]
    fn fmt_bytes_mib() {
        assert_eq!(fmt_bytes(512 * MIB), "512.00 MiB");
    }

    #[test]
    fn fmt_bytes_kib() {
        assert_eq!(fmt_bytes(4 * KIB), "4.00 KiB");
    }

    #[test]
    fn fmt_bytes_raw() {
        assert_eq!(fmt_bytes(0), "0 bytes");
        assert_eq!(fmt_bytes(512), "512 bytes");
    }
}
