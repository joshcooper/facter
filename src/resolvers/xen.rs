use crate::facts::value::FactValue;
use crate::resolvers::Resolver;
use std::path::Path;
use std::process::Command;

pub struct XenResolver;

impl Resolver for XenResolver {
    fn name(&self) -> &'static str {
        "xen"
    }

    fn resolve(&self) -> Vec<(String, FactValue)> {
        let mut facts = Vec::new();

        // Only relevant on Xen dom0 (privileged domain)
        let caps_path = "/proc/xen/capabilities";
        if !Path::new(caps_path).exists() {
            return facts;
        }
        let caps = std::fs::read_to_string(caps_path).unwrap_or_default();
        if !caps.contains("control_d") {
            return facts; // dom0 only
        }

        // Find the xen toolstack command
        let xen_cmd = find_xen_command();
        if xen_cmd.is_empty() {
            return facts;
        }

        // Run `xl list` (or `xm list`) to get domain list
        let domains = list_xen_domains(&xen_cmd);
        facts.push(("xen".into(), FactValue::String(xen_cmd)));
        if !domains.is_empty() {
            let arr: Vec<FactValue> = domains.into_iter().map(FactValue::String).collect();
            facts.push(("xendomains".into(), FactValue::Array(arr)));
        }

        facts
    }
}

fn find_xen_command() -> String {
    // Xen 4.6+: /usr/lib/xen-common/bin/xen-toolstack
    if let Ok(out) = Command::new("/usr/lib/xen-common/bin/xen-toolstack").output() {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !s.is_empty() {
                return s;
            }
        }
    }
    // Fallback: xl or xm
    for cmd in &["/usr/sbin/xl", "/usr/sbin/xm"] {
        if Path::new(cmd).exists() {
            return cmd.to_string();
        }
    }
    String::new()
}

fn list_xen_domains(cmd: &str) -> Vec<String> {
    let out = match Command::new(cmd).arg("list").output() {
        Ok(o) => o,
        Err(_) => return vec![],
    };
    let text = String::from_utf8_lossy(&out.stdout);
    // Skip header line; first field of each subsequent line is the domain name
    text.lines()
        .skip(1)
        .filter_map(|line| line.split_whitespace().next())
        .map(String::from)
        .collect()
}
