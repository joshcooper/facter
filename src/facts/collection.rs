use crate::facts::value::{FactValue, HashDisplay};
use crate::resolvers::Resolver;
use indexmap::IndexMap;
use std::collections::{HashMap, HashSet};

pub struct Collection {
    facts: IndexMap<String, FactValue>,
}

impl Collection {
    pub fn new() -> Self {
        Self { facts: IndexMap::new() }
    }

    pub fn add(&mut self, key: impl Into<String>, value: FactValue) {
        self.facts.insert(key.into(), value);
    }

    /// Run every resolver, honouring blocklist and TTL cache settings.
    /// When `timing` is true, each resolver's wall time is printed to stderr.
    pub fn resolve_all(
        &mut self,
        resolvers: &[Box<dyn Resolver>],
        blocklist: &HashSet<String>,
        ttls: &HashMap<String, u64>,
        ignore_cache: bool,
        timing: bool,
    ) {
        for resolver in resolvers {
            let name = resolver.name();

            if blocklist.contains(name) {
                log::debug!("resolver blocked: {}", name);
                continue;
            }

            // Check TTL cache
            let cached = if !ignore_cache {
                ttls.get(name).and_then(|&ttl| crate::cache::load(name, ttl))
            } else {
                None
            };

            let t0 = std::time::Instant::now();
            let result = if let Some(cached_facts) = cached {
                log::debug!("resolver cache hit: {}", name);
                cached_facts
            } else {
                log::debug!("resolving: {}", name);
                let facts = resolver.resolve();
                // Persist to cache if a TTL is configured
                if !ignore_cache && ttls.contains_key(name) {
                    crate::cache::save(name, &facts);
                }
                facts
            };
            if timing {
                eprintln!("{}: {:.6} seconds", name, t0.elapsed().as_secs_f64());
            }

            for (k, v) in result {
                self.facts.insert(k, v);
            }
        }
        self.facts.sort_keys();
    }

    /// Remove a value at a dotted path from the collection.
    /// - A single segment (e.g. `"gce"`) removes the top-level key entirely.
    /// - A dotted path (e.g. `"os.distro.release"`) removes that key from within
    ///   the nested map, leaving the rest of `os` intact.
    pub fn exclude(&mut self, path: &str) {
        match path.split_once('.') {
            None => { self.facts.shift_remove(path); }
            Some((head, tail)) => {
                if let Some(value) = self.facts.get_mut(head) {
                    exclude_path(value, tail);
                }
            }
        }
    }

    /// Re-sort facts alphabetically (call after adding external/custom facts).
    pub fn sort_keys(&mut self) {
        self.facts.sort_keys();
    }

    /// Remove pre-facter-3 legacy flat facts. Call unless `--show-legacy` is set.
    pub fn remove_legacy(&mut self) {
        const STATIC_LEGACY: &[&str] = &[
            // OS
            "architecture", "hardwaremodel", "operatingsystem",
            "operatingsystemmajrelease", "operatingsystemrelease", "osfamily",
            "lsbdistcodename", "lsbdistdescription", "lsbdistid",
            "lsbdistrelease", "lsbmajdistrelease", "lsbminordistrelease",
            // SELinux flat
            "selinux", "selinux_enforced", "selinux_policyversion",
            "selinux_current_mode", "selinux_config_mode", "selinux_config_policy",
            // Identity
            "id", "gid",
            // Networking flat
            "domain", "fqdn", "hostname",
            "interfaces", "ipaddress", "ipaddress6",
            "macaddress", "netmask", "netmask6", "network", "network6",
            // Uptime flat
            "uptime", "uptime_days", "uptime_hours", "uptime_seconds",
            // Memory flat
            "memoryfree", "memoryfree_mb", "memorysize", "memorysize_mb",
            "swapencrypted", "swapfree", "swapfree_mb", "swapsize", "swapsize_mb",
            // Processors flat
            "hardwareisa", "physicalprocessorcount", "processorcount",
            // DMI flat
            "bios_release_date", "bios_vendor", "bios_version",
            "boardassettag", "boardmanufacturer", "boardproductname", "boardserialnumber",
            "chassisassettag", "chassistype",
            "manufacturer", "productname", "serialnumber", "uuid",
            // Disk flat
            "blockdevices",
            // Xen flat
            "xendomains",
            // SSH flat
            "sshdsakey", "sshecdsakey", "sshed25519key", "sshrsakey",
            "sshfp_dsa", "sshfp_ecdsa", "sshfp_ed25519", "sshfp_rsa",
        ];
        for name in STATIC_LEGACY {
            self.facts.shift_remove(*name);
        }
        // Pattern-based: processor0..N, blockdevice_*_*, ipaddress_*, mtu_*, etc.
        let legacy_keys: Vec<String> = self
            .facts
            .keys()
            .filter(|k| is_legacy_pattern(k))
            .cloned()
            .collect();
        for key in legacy_keys {
            self.facts.shift_remove(&key);
        }
    }

    /// Look up a fact by exact key.
    pub fn get(&self, key: &str) -> Option<&FactValue> {
        self.facts.get(key)
    }

    /// Look up a fact by dotted path, e.g. "os.release.major" or "processors.models.0".
    pub fn query(&self, path: &str) -> Option<&FactValue> {
        let mut parts = path.splitn(2, '.');
        let head = parts.next()?;
        let tail = parts.next();
        let mut current = self.facts.get(head)?;
        if let Some(tail) = tail {
            for part in tail.split('.') {
                current = match current {
                    FactValue::Map(m) => m.get(part)?,
                    FactValue::Array(a) => {
                        let idx: usize = part.parse().ok()?;
                        a.get(idx)?
                    }
                    _ => return None,
                };
            }
        }
        Some(current)
    }

    // ── Resolver group lists ──────────────────────────────────────────────────

    /// Names of all resolver groups (for --list-block-groups / --list-cache-groups).
    pub fn resolver_group_names(resolvers: &[Box<dyn Resolver>]) -> Vec<&'static str> {
        resolvers.iter().map(|r| r.name()).collect()
    }

    // ── Output formatters ─────────────────────────────────────────────────────

    /// Facter's default "hash" output format.
    pub fn write_hash(&self, queries: &[String]) -> String {
        use std::fmt::Write;
        let mut out = String::new();
        if queries.is_empty() {
            for (k, v) in &self.facts {
                let _ = writeln!(out, "{} => {}", k, HashDisplay { value: v, indent: 0 });
            }
        } else {
            for query in queries {
                if let Some(value) = self.query(query) {
                    if queries.len() == 1 {
                        let _ = writeln!(out, "{}", HashDisplay { value, indent: 0 });
                    } else {
                        let _ = writeln!(out, "{} => {}", query, HashDisplay { value, indent: 0 });
                    }
                }
            }
        }
        out
    }

    /// JSON output.
    pub fn write_json(&self, queries: &[String]) -> anyhow::Result<String> {
        if queries.is_empty() {
            return Ok(serde_json::to_string_pretty(&self.facts)?);
        }
        if queries.len() == 1 {
            return match self.query(&queries[0]) {
                Some(v) => Ok(serde_json::to_string_pretty(v)?),
                None => Ok("null".to_string()),
            };
        }
        let mut map: IndexMap<&str, &FactValue> = IndexMap::new();
        for q in queries {
            if let Some(v) = self.query(q) {
                map.insert(q.as_str(), v);
            }
        }
        Ok(serde_json::to_string_pretty(&map)?)
    }

    /// YAML output.
    pub fn write_yaml(&self, queries: &[String]) -> anyhow::Result<String> {
        if queries.is_empty() {
            return Ok(serde_yaml::to_string(&self.facts)?);
        }
        if queries.len() == 1 {
            return match self.query(&queries[0]) {
                Some(v) => Ok(serde_yaml::to_string(v)?),
                None => Ok("null\n".to_string()),
            };
        }
        let mut map: IndexMap<&str, &FactValue> = IndexMap::new();
        for q in queries {
            if let Some(v) = self.query(q) {
                map.insert(q.as_str(), v);
            }
        }
        Ok(serde_yaml::to_string(&map)?)
    }
}

/// Remove the value at a dotted path, e.g. `"os.distro.release"` or `"gce.instance"`.
/// Silently does nothing if the path doesn't exist.
pub fn exclude_path(value: &mut FactValue, path: &str) {
    let (head, tail) = match path.split_once('.') {
        Some((h, t)) => (h, Some(t)),
        None => (path, None),
    };
    if let FactValue::Map(map) = value {
        match tail {
            None => { map.shift_remove(head); }
            Some(rest) => {
                if let Some(child) = map.get_mut(head) {
                    exclude_path(child, rest);
                }
            }
        }
    }
}

fn is_legacy_pattern(k: &str) -> bool {
    // processorN (e.g. processor0, processor1) but not "processors"
    if let Some(suffix) = k.strip_prefix("processor") {
        if !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit()) {
            return true;
        }
    }
    k.starts_with("blockdevice_")
        || k.starts_with("ipaddress_")
        || k.starts_with("ipaddress6_")
        || k.starts_with("macaddress_")
        || k.starts_with("netmask_")
        || k.starts_with("netmask6_")
        || k.starts_with("network_")
        || k.starts_with("network6_")
        || k.starts_with("mtu_")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make() -> Collection {
        let mut c = Collection::new();
        // structured os map
        let mut release = IndexMap::new();
        release.insert("full".to_string(), FactValue::String("22.04".into()));
        release.insert("major".to_string(), FactValue::String("22".into()));
        let mut distro = IndexMap::new();
        distro.insert("id".to_string(), FactValue::String("Ubuntu".into()));
        distro.insert("release".to_string(), FactValue::Map(release.clone()));
        let mut os = IndexMap::new();
        os.insert("name".to_string(), FactValue::String("Ubuntu".into()));
        os.insert("release".to_string(), FactValue::Map(release));
        os.insert("distro".to_string(), FactValue::Map(distro));
        c.add("os", FactValue::Map(os));
        c.add("kernel", FactValue::String("Linux".into()));
        // processors with models array
        let mut proc_map = IndexMap::new();
        proc_map.insert("count".to_string(), FactValue::Integer(4));
        proc_map.insert("models".to_string(), FactValue::Array(vec![
            FactValue::String("Intel Core i7".into()),
            FactValue::String("Intel Core i7".into()),
        ]));
        c.add("processors", FactValue::Map(proc_map));
        // legacy facts
        c.add("operatingsystem", FactValue::String("Ubuntu".into()));
        c.add("processor0", FactValue::String("Intel Core i7".into()));
        c.add("blockdevice_sda_model", FactValue::String("Samsung SSD".into()));
        c.add("ipaddress_eth0", FactValue::String("10.0.0.1".into()));
        c.add("mtu_eth0", FactValue::Integer(1500));
        c
    }

    // ── query ─────────────────────────────────────────────────────────────────

    #[test]
    fn query_top_level_string() {
        let c = make();
        assert!(matches!(c.query("kernel"), Some(FactValue::String(s)) if s == "Linux"));
    }

    #[test]
    fn query_dotted_one_level() {
        let c = make();
        assert!(matches!(c.query("os.name"), Some(FactValue::String(s)) if s == "Ubuntu"));
    }

    #[test]
    fn query_dotted_two_levels() {
        let c = make();
        assert!(matches!(c.query("os.release.full"), Some(FactValue::String(s)) if s == "22.04"));
    }

    #[test]
    fn query_array_index() {
        let c = make();
        assert!(matches!(
            c.query("processors.models.0"),
            Some(FactValue::String(s)) if s == "Intel Core i7"
        ));
    }

    #[test]
    fn query_missing_top_level_returns_none() {
        assert!(make().query("nonexistent").is_none());
    }

    #[test]
    fn query_missing_nested_returns_none() {
        assert!(make().query("os.nonexistent").is_none());
    }

    #[test]
    fn query_out_of_bounds_index_returns_none() {
        assert!(make().query("processors.models.99").is_none());
    }

    #[test]
    fn query_index_on_non_array_returns_none() {
        assert!(make().query("os.name.0").is_none());
    }

    // ── exclude ───────────────────────────────────────────────────────────────

    #[test]
    fn exclude_top_level_key() {
        let mut c = make();
        c.exclude("kernel");
        assert!(c.query("kernel").is_none());
        assert!(c.query("os.name").is_some()); // others unaffected
    }

    #[test]
    fn exclude_nested_key_leaves_siblings() {
        let mut c = make();
        c.exclude("os.release");
        assert!(c.query("os.release").is_none());
        assert!(c.query("os.name").is_some());
        assert!(c.query("os.distro").is_some());
    }

    #[test]
    fn exclude_deep_nested_leaves_siblings() {
        let mut c = make();
        c.exclude("os.release.major");
        assert!(c.query("os.release.major").is_none());
        assert!(c.query("os.release.full").is_some());
    }

    #[test]
    fn exclude_nonexistent_path_is_noop() {
        let mut c = make();
        c.exclude("does.not.exist"); // must not panic
        assert!(c.query("kernel").is_some());
    }

    // ── remove_legacy ─────────────────────────────────────────────────────────

    #[test]
    fn remove_legacy_removes_static_names() {
        let mut c = make();
        c.remove_legacy();
        assert!(c.query("operatingsystem").is_none());
    }

    #[test]
    fn remove_legacy_removes_processor_n_pattern() {
        let mut c = make();
        c.remove_legacy();
        assert!(c.query("processor0").is_none());
    }

    #[test]
    fn remove_legacy_removes_blockdevice_pattern() {
        let mut c = make();
        c.remove_legacy();
        assert!(c.query("blockdevice_sda_model").is_none());
    }

    #[test]
    fn remove_legacy_removes_ipaddress_pattern() {
        let mut c = make();
        c.remove_legacy();
        assert!(c.query("ipaddress_eth0").is_none());
    }

    #[test]
    fn remove_legacy_removes_mtu_pattern() {
        let mut c = make();
        c.remove_legacy();
        assert!(c.query("mtu_eth0").is_none());
    }

    #[test]
    fn remove_legacy_keeps_modern_facts() {
        let mut c = make();
        c.remove_legacy();
        assert!(c.query("kernel").is_some());
        assert!(c.query("os.name").is_some());
        assert!(c.query("processors.count").is_some());
    }

    #[test]
    fn remove_legacy_keeps_processors_map() {
        // "processors" must not be removed by the processorN pattern
        let mut c = make();
        c.remove_legacy();
        assert!(c.query("processors").is_some());
    }

    #[test]
    fn remove_legacy_removes_operatingsystemmajrelease() {
        let mut c = Collection::new();
        c.add("operatingsystemmajrelease", FactValue::String("22".into()));
        c.remove_legacy();
        assert!(c.query("operatingsystemmajrelease").is_none());
    }

    #[test]
    fn remove_legacy_removes_uptime_facts() {
        let mut c = Collection::new();
        c.add("uptime", FactValue::String("1 day".into()));
        c.add("uptime_days", FactValue::Integer(1));
        c.add("uptime_hours", FactValue::Integer(24));
        c.add("uptime_seconds", FactValue::Integer(86400));
        c.remove_legacy();
        assert!(c.query("uptime").is_none());
        assert!(c.query("uptime_days").is_none());
        assert!(c.query("uptime_hours").is_none());
        assert!(c.query("uptime_seconds").is_none());
    }

    #[test]
    fn remove_legacy_keeps_system_uptime() {
        let mut c = Collection::new();
        let mut map = IndexMap::new();
        map.insert("seconds".to_string(), FactValue::Integer(86400));
        c.add("system_uptime", FactValue::Map(map));
        c.add("uptime_seconds", FactValue::Integer(86400));
        c.remove_legacy();
        assert!(c.query("system_uptime").is_some());
        assert!(c.query("uptime_seconds").is_none());
    }

    // ── sort_keys ─────────────────────────────────────────────────────────────

    #[test]
    fn sort_keys_orders_alphabetically() {
        let mut c = Collection::new();
        c.add("zebra", FactValue::String("z".into()));
        c.add("apple", FactValue::String("a".into()));
        c.add("mango", FactValue::String("m".into()));
        c.sort_keys();
        let out = c.write_hash(&[]);
        let pa = out.find("apple").unwrap();
        let pm = out.find("mango").unwrap();
        let pz = out.find("zebra").unwrap();
        assert!(pa < pm && pm < pz);
    }

    // ── is_legacy_pattern ─────────────────────────────────────────────────────

    #[test]
    fn legacy_pattern_processor_digits() {
        assert!(is_legacy_pattern("processor0"));
        assert!(is_legacy_pattern("processor12"));
    }

    #[test]
    fn legacy_pattern_processor_map_not_matched() {
        assert!(!is_legacy_pattern("processors"));
        assert!(!is_legacy_pattern("processorcount"));
    }

    #[test]
    fn legacy_pattern_blockdevice() {
        assert!(is_legacy_pattern("blockdevice_sda_model"));
    }

    #[test]
    fn legacy_pattern_interface_variants() {
        assert!(is_legacy_pattern("ipaddress_eth0"));
        assert!(is_legacy_pattern("ipaddress6_eth0"));
        assert!(is_legacy_pattern("macaddress_eth0"));
        assert!(is_legacy_pattern("netmask_eth0"));
        assert!(is_legacy_pattern("mtu_eth0"));
    }

    // ── write_hash ────────────────────────────────────────────────────────────

    #[test]
    fn write_hash_all_facts_when_no_queries() {
        let c = make();
        let out = c.write_hash(&[]);
        assert!(out.contains("kernel =>"));
        assert!(out.contains("os =>"));
    }

    #[test]
    fn write_hash_single_query_omits_key_prefix() {
        let c = make();
        let out = c.write_hash(&["kernel".to_string()]);
        assert!(!out.contains("=>"));
        assert!(out.contains("Linux"));
    }

    #[test]
    fn write_hash_multiple_queries_include_key_prefix() {
        let c = make();
        let out = c.write_hash(&["kernel".to_string(), "os.name".to_string()]);
        assert!(out.contains("kernel =>"));
        assert!(out.contains("os.name =>"));
    }

    #[test]
    fn write_hash_missing_query_omitted_silently() {
        let c = make();
        let out = c.write_hash(&["nonexistent".to_string()]);
        assert!(out.is_empty());
    }

    #[test]
    fn write_hash_map_value_uses_braces() {
        let c = make();
        let out = c.write_hash(&["os".to_string()]);
        assert!(out.starts_with('{'));
        assert!(out.trim_end().ends_with('}'));
    }

    // ── write_json ────────────────────────────────────────────────────────────

    #[test]
    fn write_json_no_queries_returns_all_facts() {
        let c = make();
        let json = c.write_json(&[]).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(v.get("kernel").is_some());
        assert!(v.get("os").is_some());
    }

    #[test]
    fn write_json_single_query_returns_bare_value() {
        let c = make();
        let json = c.write_json(&["kernel".to_string()]).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v.as_str().unwrap(), "Linux");
    }

    #[test]
    fn write_json_single_missing_query_returns_null() {
        let c = make();
        let json = c.write_json(&["nonexistent".to_string()]).unwrap();
        assert_eq!(json.trim(), "null");
    }

    #[test]
    fn write_json_multiple_queries_returns_object() {
        let c = make();
        let json = c.write_json(&["kernel".to_string(), "os.name".to_string()]).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(v.is_object());
        assert_eq!(v.get("kernel").and_then(|v| v.as_str()), Some("Linux"));
        assert_eq!(v.get("os.name").and_then(|v| v.as_str()), Some("Ubuntu"));
    }

    #[test]
    fn write_json_multiple_queries_omits_missing() {
        let c = make();
        let json = c.write_json(&["kernel".to_string(), "nonexistent".to_string()]).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(v.get("kernel").is_some());
        assert!(v.get("nonexistent").is_none());
    }

    // ── write_yaml ────────────────────────────────────────────────────────────

    #[test]
    fn write_yaml_no_queries_contains_all_facts() {
        let c = make();
        let yaml = c.write_yaml(&[]).unwrap();
        assert!(yaml.contains("kernel:"));
        assert!(yaml.contains("os:"));
    }

    #[test]
    fn write_yaml_single_query_returns_bare_value() {
        let c = make();
        let yaml = c.write_yaml(&["kernel".to_string()]).unwrap();
        assert!(yaml.contains("Linux"));
        assert!(!yaml.contains("kernel:"));
    }

    #[test]
    fn write_yaml_single_missing_query_returns_null() {
        let c = make();
        let yaml = c.write_yaml(&["nonexistent".to_string()]).unwrap();
        assert_eq!(yaml.trim(), "null");
    }

    #[test]
    fn write_yaml_multiple_queries_returns_mapping() {
        let c = make();
        let yaml = c.write_yaml(&["kernel".to_string(), "os.name".to_string()]).unwrap();
        assert!(yaml.contains("kernel:"));
        assert!(yaml.contains("os.name:"));
    }

    // ── resolver_group_names ──────────────────────────────────────────────────

    #[test]
    fn resolver_group_names_reflects_resolver_list() {
        let resolvers = crate::resolvers::default_resolvers();
        let names = Collection::resolver_group_names(&resolvers);
        assert!(names.contains(&"kernel"));
        assert!(names.contains(&"networking"));
        assert!(!names.is_empty());
    }
}
