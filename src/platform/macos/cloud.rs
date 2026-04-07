use std::process::Command;

pub fn dmi_vendor() -> String {
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
