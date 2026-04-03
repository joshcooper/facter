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
            "operatingsystemrelease", "osfamily",
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
