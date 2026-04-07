use crate::facts::value::FactValue;
use crate::resolvers::Resolver;
use indexmap::IndexMap;
use std::fs;
use std::time::Duration;

pub struct CloudResolver;

const TIMEOUT: Duration = Duration::from_secs(2);

impl Resolver for CloudResolver {
    fn name(&self) -> &'static str {
        "cloud"
    }

    fn resolve(&self) -> Vec<(String, FactValue)> {
        let mut facts = Vec::new();

        let vendor = dmi_vendor();
        let v = vendor.to_lowercase();

        // Only probe endpoints likely to exist based on DMI vendor.
        // This avoids multi-second timeouts on bare-metal non-cloud machines.
        let try_ec2 = v.contains("amazon") || v.contains("xen") || v.contains("kvm") || vendor.is_empty();
        let try_gce = v.contains("google");
        let try_azure = v.contains("microsoft");

        // cloud structured fact
        let mut cloud: IndexMap<String, FactValue> = IndexMap::new();

        if try_ec2 {
            if let Some(meta) = fetch_ec2() {
                facts.push(("ec2_metadata".into(), FactValue::Map(meta.clone())));
                cloud.insert("provider".into(), FactValue::String("aws".into()));
            }
        }

        if try_gce {
            if let Some(meta) = fetch_gce() {
                facts.push(("gce".into(), FactValue::Map(meta)));
                cloud.insert("provider".into(), FactValue::String("gce".into()));
            }
        }

        if try_azure {
            if let Some(meta) = fetch_azure() {
                facts.push(("az_metadata".into(), FactValue::Map(meta)));
                cloud.insert("provider".into(), FactValue::String("azure".into()));
            }
        }

        if !cloud.is_empty() {
            facts.push(("cloud".into(), FactValue::Map(cloud)));
        }

        facts
    }
}

#[cfg(target_os = "linux")]
fn dmi_vendor() -> String {
    fs::read_to_string("/sys/class/dmi/id/sys_vendor")
        .unwrap_or_default()
        .trim()
        .to_string()
}

#[cfg(target_os = "macos")]
fn dmi_vendor() -> String {
    use std::process::Command;
    // ioreg gives us the platform expert node which contains the manufacturer
    let out = Command::new("ioreg")
        .args(["-rd1", "-c", "IOPlatformExpertDevice"])
        .output()
        .unwrap_or_default();
    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines() {
        if line.contains("manufacturer") {
            if let Some(val) = line.split('"').nth(3) {
                return val.to_string();
            }
        }
    }
    String::new()
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn dmi_vendor() -> String {
    String::new()
}

// ── EC2 (IMDSv1) ─────────────────────────────────────────────────────────────

fn fetch_ec2() -> Option<IndexMap<String, FactValue>> {
    let agent = ureq::AgentBuilder::new().timeout(TIMEOUT).build();
    // Quick reachability check
    agent
        .get("http://169.254.169.254/latest/meta-data/")
        .call()
        .ok()?;

    // Fetch a flat set of the most useful metadata fields
    let fields = [
        "instance-id",
        "instance-type",
        "ami-id",
        "local-ipv4",
        "local-hostname",
        "public-ipv4",
        "public-hostname",
        "security-groups",
        "placement/region",
        "placement/availability-zone",
    ];

    let mut meta: IndexMap<String, FactValue> = IndexMap::new();
    for field in &fields {
        let url = format!("http://169.254.169.254/latest/meta-data/{}", field);
        if let Ok(resp) = agent.get(&url).call() {
            if let Ok(text) = resp.into_string() {
                let text = text.trim().to_string();
                if !text.is_empty() {
                    // Use the last path component as key, replacing / with _
                    let key = field.replace('/', "_");
                    meta.insert(key, FactValue::String(text));
                }
            }
        }
    }

    if meta.is_empty() { None } else { Some(meta) }
}

// ── GCE ──────────────────────────────────────────────────────────────────────

fn fetch_gce() -> Option<IndexMap<String, FactValue>> {
    let agent = ureq::AgentBuilder::new().timeout(TIMEOUT).build();
    let resp = agent
        .get("http://metadata.google.internal/computeMetadata/v1/?recursive=true&alt=json")
        .set("Metadata-Flavor", "Google")
        .call()
        .ok()?;

    let json: serde_json::Value = resp.into_json().ok()?;
    Some(json_to_fact_map(&json))
}

// ── Azure ─────────────────────────────────────────────────────────────────────

fn fetch_azure() -> Option<IndexMap<String, FactValue>> {
    let agent = ureq::AgentBuilder::new().timeout(TIMEOUT).build();
    let resp = agent
        .get("http://169.254.169.254/metadata/instance?api-version=2021-02-01")
        .set("Metadata", "true")
        .call()
        .ok()?;

    let json: serde_json::Value = resp.into_json().ok()?;
    Some(json_to_fact_map(&json))
}

// ── JSON → FactValue conversion ───────────────────────────────────────────────

fn json_to_fact_map(v: &serde_json::Value) -> IndexMap<String, FactValue> {
    match v {
        serde_json::Value::Object(obj) => obj
            .iter()
            .map(|(k, v)| (k.clone(), json_to_fact(v)))
            .collect(),
        _ => IndexMap::new(),
    }
}

fn json_to_fact(v: &serde_json::Value) -> FactValue {
    match v {
        serde_json::Value::Null => FactValue::String(String::new()),
        serde_json::Value::Bool(b) => FactValue::Boolean(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                FactValue::Integer(i)
            } else {
                FactValue::Float(n.as_f64().unwrap_or(0.0))
            }
        }
        serde_json::Value::String(s) => FactValue::String(s.clone()),
        serde_json::Value::Array(arr) => {
            FactValue::Array(arr.iter().map(json_to_fact).collect())
        }
        serde_json::Value::Object(obj) => {
            let map: IndexMap<String, FactValue> =
                obj.iter().map(|(k, v)| (k.clone(), json_to_fact(v))).collect();
            FactValue::Map(map)
        }
    }
}
