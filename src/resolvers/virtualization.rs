use crate::facts::value::FactValue;
use crate::resolvers::Resolver;
use indexmap::IndexMap;
use std::fs;
use std::path::Path;

pub struct VirtualizationResolver;

impl Resolver for VirtualizationResolver {
    fn name(&self) -> &'static str {
        "virtualization"
    }

    fn resolve(&self) -> Vec<(String, FactValue)> {
        let mut facts = Vec::new();

        let (virt, is_virtual) = detect();

        facts.push(("virtual".into(), FactValue::String(virt.clone())));
        facts.push(("is_virtual".into(), FactValue::Boolean(is_virtual)));

        // Structured hypervisors fact (present only when virtual)
        if is_virtual && virt != "physical" {
            let mut hypervisors: IndexMap<String, FactValue> = IndexMap::new();
            hypervisors.insert(virt.clone(), FactValue::Map(IndexMap::new()));
            facts.push(("hypervisors".into(), FactValue::Map(hypervisors)));
        }

        facts
    }
}

#[cfg(target_os = "linux")]
fn detect() -> (String, bool) {
    // 1. DMI / SMBIOS vendor (most reliable on bare-metal VMs)
    if let Some((virt, is_v)) = check_dmi() {
        return (virt, is_v);
    }

    // 2. Xen — check for /proc/xen/capabilities (privileged dom0 = not virtual)
    if Path::new("/proc/xen").exists() {
        if let Ok(caps) = fs::read_to_string("/proc/xen/capabilities") {
            if caps.contains("control_d") {
                return ("xen".to_string(), false); // dom0 is not considered virtual
            }
        }
        return ("xen".to_string(), true);
    }

    // 3. Container detection (before /proc/cpuinfo hypervisor flag)
    if Path::new("/.dockerenv").exists() {
        return ("docker".to_string(), true);
    }
    if let Ok(cg) = fs::read_to_string("/proc/1/cgroup") {
        if cg.contains("docker") || cg.contains("containerd") {
            return ("docker".to_string(), true);
        }
        if cg.contains("lxc") {
            return ("lxc".to_string(), true);
        }
        if cg.contains("kubepods") {
            return ("docker".to_string(), true);
        }
    }
    if Path::new("/proc/vz").exists() && !Path::new("/proc/bc").exists() {
        return ("openvz".to_string(), true);
    }

    // 4. /proc/cpuinfo hypervisor flag (generic VM)
    if let Ok(cpuinfo) = fs::read_to_string("/proc/cpuinfo") {
        for line in cpuinfo.lines() {
            if (line.starts_with("flags") || line.starts_with("Features"))
                && line.contains("hypervisor")
            {
                // Could be KVM, VMware, Hyper-V — we already checked DMI above.
                // If DMI was inconclusive, report as generic "virtual".
                return ("virtual".to_string(), true);
            }
        }
    }

    ("physical".to_string(), false)
}

#[cfg(target_os = "macos")]
fn detect() -> (String, bool) {
    use crate::platform::macos::sysctl_string;
    // hw.model is "VMwareX,Y" on VMware Fusion, "Parallels..." on Parallels,
    // otherwise a real Mac model like "MacBookPro18,1".
    if let Some(model) = sysctl_string("hw.model") {
        let m = model.to_lowercase();
        if m.contains("vmware") {
            return ("vmware".to_string(), true);
        }
        if m.contains("parallels") {
            return ("parallels".to_string(), true);
        }
        if m.contains("virtualbox") {
            return ("virtualbox".to_string(), true);
        }
    }
    ("physical".to_string(), false)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn detect() -> (String, bool) {
    ("physical".to_string(), false)
}

fn check_dmi() -> Option<(String, bool)> {
    let vendor = read_dmi("sys_vendor")?;
    let v = vendor.to_lowercase();

    if v.contains("vmware") {
        return Some(("vmware".to_string(), true));
    }
    if v.contains("virtualbox") || v.contains("innotek") {
        return Some(("virtualbox".to_string(), true));
    }
    if v.contains("qemu") {
        return Some(("kvm".to_string(), true));
    }
    if v.contains("microsoft corporation") {
        if let Some(product) = read_dmi("product_name") {
            if product.to_lowercase().contains("virtual") {
                return Some(("hyperv".to_string(), true));
            }
        }
    }
    if v.contains("xen") {
        return Some(("xen".to_string(), true));
    }
    if v.contains("bochs") || v.contains("kvm") {
        return Some(("kvm".to_string(), true));
    }
    if v.contains("google") {
        return Some(("gce".to_string(), true));
    }
    if v.contains("amazon") || v.contains("ec2") {
        return Some(("kvm".to_string(), true)); // AWS Nitro/KVM
    }

    None
}

fn read_dmi(field: &str) -> Option<String> {
    fs::read_to_string(format!("/sys/class/dmi/id/{}", field))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}
