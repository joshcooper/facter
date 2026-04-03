use crate::facts::value::{FactValue, HashDisplay};
use crate::resolvers::Resolver;
use indexmap::IndexMap;

pub struct Collection {
    facts: IndexMap<String, FactValue>,
}

impl Collection {
    pub fn new() -> Self {
        Self {
            facts: IndexMap::new(),
        }
    }

    pub fn add(&mut self, key: impl Into<String>, value: FactValue) {
        self.facts.insert(key.into(), value);
    }

    /// Run every resolver and sort all keys alphabetically.
    pub fn resolve_all(&mut self, resolvers: &[Box<dyn Resolver>]) {
        for resolver in resolvers {
            log::debug!("resolving: {}", resolver.name());
            for (k, v) in resolver.resolve() {
                self.facts.insert(k, v);
            }
        }
        self.facts.sort_keys();
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

    // ── Output formatters ────────────────────────────────────────────────────

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
