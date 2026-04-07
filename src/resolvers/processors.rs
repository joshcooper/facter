use crate::facts::value::FactValue;
use crate::resolvers::Resolver;
use indexmap::IndexMap;
use std::ffi::CStr;

pub struct ProcessorsResolver;

impl Resolver for ProcessorsResolver {
    fn name(&self) -> &'static str {
        "processors"
    }

    fn resolve(&self) -> Vec<(String, FactValue)> {
        let mut facts = Vec::new();

        let info = crate::platform::collect_cpu();
        let isa = cpu_isa();

        let logical_count = info.logical_count;
        let physical_count = info.physical_count;
        let models = info.models;

        // Legacy flat facts
        facts.push(("processorcount".into(), FactValue::Integer(logical_count as i64)));
        facts.push((
            "physicalprocessorcount".into(),
            FactValue::Integer(physical_count as i64),
        ));
        if !isa.is_empty() {
            facts.push(("hardwareisa".into(), FactValue::String(isa.clone())));
        }

        // Per-processor legacy facts (processor0, processor1, …)
        for (i, model) in models.iter().enumerate() {
            facts.push((format!("processor{}", i), FactValue::String(model.clone())));
        }

        // Structured processors fact
        let mut processors: IndexMap<String, FactValue> = IndexMap::new();
        processors.insert("count".into(), FactValue::Integer(logical_count as i64));
        if info.cores > 0 {
            processors.insert("cores".into(), FactValue::Integer(info.cores as i64));
        }
        let extensions = crate::platform::cpu_extensions(&isa);
        if !extensions.is_empty() {
            let ext_vals: Vec<FactValue> = extensions.into_iter().map(FactValue::String).collect();
            processors.insert("extensions".into(), FactValue::Array(ext_vals));
        }
        if !isa.is_empty() {
            processors.insert("isa".into(), FactValue::String(isa));
        }
        if !models.is_empty() {
            let model_values: Vec<FactValue> = models.into_iter().map(FactValue::String).collect();
            processors.insert("models".into(), FactValue::Array(model_values));
        }
        processors.insert(
            "physicalcount".into(),
            FactValue::Integer(physical_count as i64),
        );
        if let Some(speed) = &info.speed {
            processors.insert("speed".into(), FactValue::String(speed.clone()));
        }
        if info.threads > 0 {
            processors.insert("threads".into(), FactValue::Integer(info.threads as i64));
        }

        facts.push(("processors".into(), FactValue::Map(processors)));

        facts
    }
}

/// Get the ISA from uname(machine) — e.g. "x86_64", "aarch64".
fn cpu_isa() -> String {
    let mut u: libc::utsname = unsafe { std::mem::zeroed() };
    if unsafe { libc::uname(&mut u) } != 0 {
        return String::new();
    }
    unsafe { CStr::from_ptr(u.machine.as_ptr()).to_string_lossy().into_owned() }
}

#[cfg(test)]
mod tests {
    #[test]
    fn cpu_extensions_non_x86_returns_empty() {
        assert!(crate::platform::cpu_extensions("aarch64").is_empty());
        assert!(crate::platform::cpu_extensions("").is_empty());
    }

    #[test]
    fn cpu_extensions_x86_64_always_starts_with_x86_64() {
        // x86_64 host: first element is always "x86_64"
        let exts = crate::platform::cpu_extensions("x86_64");
        assert!(!exts.is_empty());
        assert_eq!(exts[0], "x86_64");
    }

    #[test]
    fn cpu_extensions_levels_are_cumulative() {
        // If v3 is present, v1 and v2 must also be present
        let exts = crate::platform::cpu_extensions("x86_64");
        if exts.contains(&"x86_64-v3".to_string()) {
            assert!(exts.contains(&"x86_64-v1".to_string()));
            assert!(exts.contains(&"x86_64-v2".to_string()));
        }
    }
}
