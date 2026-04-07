use crate::platform::types::DiskInfo;
use std::fs;
use std::path::Path;

pub fn collect_disks() -> Vec<DiskInfo> {
    let mut disks = Vec::new();
    let block_dir = Path::new("/sys/block");
    let entries = match fs::read_dir(block_dir) {
        Ok(e) => e,
        Err(_) => return disks,
    };

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let dev_path = entry.path();

        // Skip virtual/mapped devices: must have a device/ subdirectory
        if !dev_path.join("device").exists() {
            continue;
        }

        let size_bytes = read_u64(&dev_path.join("size")).unwrap_or(0) * 512;
        let vendor = read_trimmed(&dev_path.join("device/vendor"));
        let model = read_trimmed(&dev_path.join("device/model"));
        let serial = read_trimmed(&dev_path.join("device/serial"))
            .or_else(|| read_vpd_serial(&dev_path.join("device/vpd_pg80")))
            .or_else(|| if size_bytes > 0 { read_ancestor_serial(&dev_path.join("device")) } else { None });
        let disk_type = read_trimmed(&dev_path.join("queue/rotational")).map(|r| {
            if r == "0" { "ssd".to_string() } else { "hdd".to_string() }
        });
        let wwn = read_trimmed(&dev_path.join("wwid"))
            .or_else(|| read_trimmed(&dev_path.join("device/wwid")));

        disks.push(DiskInfo { name, vendor, model, serial, disk_type, wwn, size_bytes });
    }

    disks.sort_by(|a, b| a.name.cmp(&b.name));
    disks
}

fn read_u64(path: &Path) -> Option<u64> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

fn read_ancestor_serial(start: &Path) -> Option<String> {
    let real = fs::canonicalize(start).ok()?;
    let mut dir = real.as_path();
    for _ in 0..6 {
        dir = dir.parent()?;
        let serial_path = dir.join("serial");
        if let Some(s) = read_trimmed(&serial_path) {
            return Some(s);
        }
    }
    None
}

pub fn read_vpd_serial(path: &Path) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    read_vpd_serial_bytes(&bytes)
}

pub fn read_vpd_serial_bytes(bytes: &[u8]) -> Option<String> {
    if bytes.len() < 4 {
        return None;
    }
    let len = bytes[3] as usize;
    let end = 4 + len;
    if end > bytes.len() {
        return None;
    }
    let s = std::str::from_utf8(&bytes[4..end]).ok()?.trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

fn read_trimmed(path: &Path) -> Option<String> {
    let s = fs::read_to_string(path).ok()?.trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_vpd_serial_valid() {
        let serial = b"SN12345";
        let mut data = vec![0u8, 0x80, 0x00, serial.len() as u8];
        data.extend_from_slice(serial);
        let result = read_vpd_serial_bytes(&data);
        assert_eq!(result, Some("SN12345".to_string()));
    }

    #[test]
    fn read_vpd_serial_too_short() {
        assert_eq!(read_vpd_serial_bytes(&[0x00, 0x80, 0x00]), None);
    }

    #[test]
    fn read_vpd_serial_length_overrun() {
        let data = vec![0u8, 0x80, 0x00, 10, b'A', b'B'];
        assert_eq!(read_vpd_serial_bytes(&data), None);
    }

    #[test]
    fn read_vpd_serial_empty_string() {
        let data = vec![0u8, 0x80, 0x00, 0x00];
        assert_eq!(read_vpd_serial_bytes(&data), None);
    }
}
