use std::fs;

pub fn dmi_vendor() -> String {
    fs::read_to_string("/sys/class/dmi/id/sys_vendor")
        .unwrap_or_default()
        .trim()
        .to_string()
}
