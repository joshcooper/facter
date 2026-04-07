use crate::facts::value::FactValue;
use crate::resolvers::Resolver;
use indexmap::IndexMap;

pub struct MemoryResolver;

impl Resolver for MemoryResolver {
    fn name(&self) -> &'static str {
        "memory"
    }

    fn resolve(&self) -> Vec<(String, FactValue)> {
        let mut facts = Vec::new();

        let mem = match crate::platform::collect_mem() {
            Some(m) => m,
            None => return facts,
        };

        let total = mem.total;
        let available = mem.available;
        let used = total.saturating_sub(available);
        let swap_total = mem.swap_total;
        let swap_free = mem.swap_free;
        let swap_used = swap_total.saturating_sub(swap_free);

        // Legacy flat facts
        facts.push(("memorysize".into(), FactValue::String(fmt_bytes(total))));
        facts.push(("memoryfree".into(), FactValue::String(fmt_bytes(available))));
        facts.push(("memorysize_mb".into(), FactValue::Float(bytes_to_mb(total))));
        facts.push(("memoryfree_mb".into(), FactValue::Float(bytes_to_mb(available))));

        if swap_total > 0 {
            facts.push(("swapsize".into(), FactValue::String(fmt_bytes(swap_total))));
            facts.push(("swapfree".into(), FactValue::String(fmt_bytes(swap_free))));
            facts.push(("swapsize_mb".into(), FactValue::Float(bytes_to_mb(swap_total))));
            facts.push(("swapfree_mb".into(), FactValue::Float(bytes_to_mb(swap_free))));
        }

        // Structured memory fact
        let mut system: IndexMap<String, FactValue> = IndexMap::new();
        system.insert("available".into(), FactValue::String(fmt_bytes(available)));
        system.insert("available_bytes".into(), FactValue::Integer(available as i64));
        system.insert(
            "capacity".into(),
            FactValue::String(fmt_capacity(used, total)),
        );
        system.insert("total".into(), FactValue::String(fmt_bytes(total)));
        system.insert("total_bytes".into(), FactValue::Integer(total as i64));
        system.insert("used".into(), FactValue::String(fmt_bytes(used)));
        system.insert("used_bytes".into(), FactValue::Integer(used as i64));

        let mut memory: IndexMap<String, FactValue> = IndexMap::new();
        memory.insert("system".into(), FactValue::Map(system));

        if swap_total > 0 {
            let mut swap: IndexMap<String, FactValue> = IndexMap::new();
            swap.insert("available".into(), FactValue::String(fmt_bytes(swap_free)));
            swap.insert("available_bytes".into(), FactValue::Integer(swap_free as i64));
            swap.insert(
                "capacity".into(),
                FactValue::String(fmt_capacity(swap_used, swap_total)),
            );
            swap.insert("total".into(), FactValue::String(fmt_bytes(swap_total)));
            swap.insert("total_bytes".into(), FactValue::Integer(swap_total as i64));
            swap.insert("used".into(), FactValue::String(fmt_bytes(swap_used)));
            swap.insert("used_bytes".into(), FactValue::Integer(swap_used as i64));
            memory.insert("swap".into(), FactValue::Map(swap));
        }

        facts.push(("memory".into(), FactValue::Map(memory)));

        facts
    }
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

fn bytes_to_mb(bytes: u64) -> f64 {
    (bytes as f64) / (MIB as f64)
}

fn fmt_capacity(used: u64, total: u64) -> String {
    if total == 0 {
        return "0%".to_string();
    }
    format!("{:.2}%", (used as f64 / total as f64) * 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── fmt_capacity ─────────────────────────────────────────────────────────

    #[test]
    fn fmt_capacity_zero_total() {
        assert_eq!(fmt_capacity(0, 0), "0%");
    }

    #[test]
    fn fmt_capacity_half_used() {
        assert_eq!(fmt_capacity(512, 1024), "50.00%");
    }

    #[test]
    fn fmt_capacity_fully_used() {
        assert_eq!(fmt_capacity(1024, 1024), "100.00%");
    }
}
