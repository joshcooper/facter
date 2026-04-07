use super::sysctl::sysctl_string;

pub fn detect_virtualization() -> (String, bool) {
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
