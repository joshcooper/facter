use std::fs;

pub fn read_dmi(field: &str) -> Option<String> {
    let path = format!("/sys/class/dmi/id/{}", field);
    fs::read_to_string(&path)
        .ok()
        .map(|s| sanitize(s.trim()))
        .filter(|s| !s.is_empty() && s != "Not Specified" && s != "To Be Filled By O.E.M.")
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| if (c as u32) < 32 || (c as u32) == 127 { '.' } else { c })
        .collect()
}
