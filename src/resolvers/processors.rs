use crate::facts::value::FactValue;
use crate::resolvers::Resolver;
use indexmap::IndexMap;
use std::collections::HashSet;
use std::ffi::CStr;
use std::fs;

pub struct ProcessorsResolver;

impl Resolver for ProcessorsResolver {
    fn name(&self) -> &'static str {
        "processors"
    }

    fn resolve(&self) -> Vec<(String, FactValue)> {
        let mut facts = Vec::new();

        let info = parse_cpuinfo();
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
        processors.insert(
            "physicalcount".into(),
            FactValue::Integer(physical_count as i64),
        );
        if !isa.is_empty() {
            processors.insert("isa".into(), FactValue::String(isa));
        }
        if !models.is_empty() {
            // Deduplicate while preserving order for the models array
            let mut seen = HashSet::new();
            let unique_models: Vec<FactValue> = models
                .into_iter()
                .filter(|m| seen.insert(m.clone()))
                .map(FactValue::String)
                .collect();
            processors.insert("models".into(), FactValue::Array(unique_models));
        }

        facts.push(("processors".into(), FactValue::Map(processors)));

        facts
    }
}

struct CpuInfo {
    logical_count: usize,
    physical_count: usize,
    models: Vec<String>,
}

fn parse_cpuinfo() -> CpuInfo {
    let content = fs::read_to_string("/proc/cpuinfo").unwrap_or_default();

    let mut logical_count: usize = 0;
    let mut physical_ids: HashSet<String> = HashSet::new();
    let mut models: Vec<String> = Vec::new();
    for line in content.lines() {
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim();
            let value = value.trim().to_string();

            match key {
                "processor" => {
                    logical_count += 1;
                }
                "model name" => {
                    models.push(value);
                }
                "physical id" => {
                    physical_ids.insert(value);
                }
                _ => {}
            }
        }
    }

    // On single-socket systems, "physical id" may be absent
    let physical_count = if physical_ids.is_empty() {
        if logical_count > 0 { 1 } else { 0 }
    } else {
        physical_ids.len()
    };

    // Ensure models list is as long as logical_count
    while models.len() < logical_count {
        models.push("Unknown".to_string());
    }

    CpuInfo {
        logical_count,
        physical_count,
        models,
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
