use crate::facts::value::FactValue;
use crate::resolvers::Resolver;
use indexmap::IndexMap;
use std::fs;

pub struct DmiResolver;

impl Resolver for DmiResolver {
    fn name(&self) -> &'static str {
        "dmi"
    }

    fn resolve(&self) -> Vec<(String, FactValue)> {
        let mut facts = Vec::new();

        let bios_date = read_dmi("bios_date");
        let bios_vendor = read_dmi("bios_vendor");
        let bios_version = read_dmi("bios_version");
        let board_asset_tag = read_dmi("board_asset_tag");
        let board_name = read_dmi("board_name");
        let board_serial = read_dmi("board_serial");
        let board_vendor = read_dmi("board_vendor");
        let chassis_asset_tag = read_dmi("chassis_asset_tag");
        let chassis_type = read_dmi("chassis_type");
        let product_name = read_dmi("product_name");
        let product_serial = read_dmi("product_serial");
        let product_uuid = read_dmi("product_uuid");
        let sys_vendor = read_dmi("sys_vendor");

        // Legacy flat facts
        if let Some(v) = &bios_vendor {
            facts.push(("bios_vendor".into(), FactValue::String(v.clone())));
        }
        if let Some(v) = &bios_version {
            facts.push(("bios_version".into(), FactValue::String(v.clone())));
        }
        if let Some(v) = &bios_date {
            facts.push(("bios_release_date".into(), FactValue::String(v.clone())));
        }
        if let Some(v) = &board_asset_tag {
            facts.push(("boardassettag".into(), FactValue::String(v.clone())));
        }
        if let Some(v) = &board_vendor {
            facts.push(("boardmanufacturer".into(), FactValue::String(v.clone())));
        }
        if let Some(v) = &board_name {
            facts.push(("boardproductname".into(), FactValue::String(v.clone())));
        }
        if let Some(v) = &board_serial {
            facts.push(("boardserialnumber".into(), FactValue::String(v.clone())));
        }
        if let Some(v) = &chassis_asset_tag {
            facts.push(("chassisassettag".into(), FactValue::String(v.clone())));
        }
        if let Some(v) = &sys_vendor {
            facts.push(("manufacturer".into(), FactValue::String(v.clone())));
        }
        if let Some(v) = &product_name {
            facts.push(("productname".into(), FactValue::String(v.clone())));
        }
        if let Some(v) = &product_serial {
            facts.push(("serialnumber".into(), FactValue::String(v.clone())));
        }
        if let Some(v) = &product_uuid {
            facts.push(("uuid".into(), FactValue::String(v.clone())));
        }
        if let Some(v) = &chassis_type {
            let desc = chassis_description(v);
            facts.push(("chassistype".into(), FactValue::String(desc)));
        }

        // Structured dmi fact
        let mut dmi: IndexMap<String, FactValue> = IndexMap::new();

        let mut bios: IndexMap<String, FactValue> = IndexMap::new();
        if let Some(v) = bios_vendor { bios.insert("vendor".into(), FactValue::String(v)); }
        if let Some(v) = bios_version { bios.insert("version".into(), FactValue::String(v)); }
        if let Some(v) = bios_date { bios.insert("release_date".into(), FactValue::String(v)); }
        if !bios.is_empty() { dmi.insert("bios".into(), FactValue::Map(bios)); }

        let mut board: IndexMap<String, FactValue> = IndexMap::new();
        if let Some(v) = board_asset_tag { board.insert("asset_tag".into(), FactValue::String(v)); }
        if let Some(v) = board_vendor { board.insert("manufacturer".into(), FactValue::String(v)); }
        if let Some(v) = board_name { board.insert("product".into(), FactValue::String(v)); }
        if let Some(v) = board_serial { board.insert("serial_number".into(), FactValue::String(v)); }
        if !board.is_empty() { dmi.insert("board".into(), FactValue::Map(board)); }

        let mut chassis: IndexMap<String, FactValue> = IndexMap::new();
        if let Some(v) = chassis_asset_tag { chassis.insert("asset_tag".into(), FactValue::String(v)); }
        if let Some(ref ct) = chassis_type {
            chassis.insert("type".into(), FactValue::String(chassis_description(ct)));
        }
        if !chassis.is_empty() { dmi.insert("chassis".into(), FactValue::Map(chassis)); }

        if let Some(v) = sys_vendor { dmi.insert("manufacturer".into(), FactValue::String(v)); }

        let mut product: IndexMap<String, FactValue> = IndexMap::new();
        if let Some(v) = product_name { product.insert("name".into(), FactValue::String(v)); }
        if let Some(v) = product_serial { product.insert("serial_number".into(), FactValue::String(v)); }
        if let Some(v) = product_uuid { product.insert("uuid".into(), FactValue::String(v)); }
        if !product.is_empty() { dmi.insert("product".into(), FactValue::Map(product)); }

        if !dmi.is_empty() {
            facts.push(("dmi".into(), FactValue::Map(dmi)));
        }

        facts
    }
}

fn read_dmi(field: &str) -> Option<String> {
    let path = format!("/sys/class/dmi/id/{}", field);
    fs::read_to_string(&path)
        .ok()
        .map(|s| sanitize(s.trim()))
        .filter(|s| !s.is_empty() && s != "Not Specified" && s != "To Be Filled By O.E.M.")
}

/// Replace non-printable ASCII characters with '.', matching dmidecode behaviour.
fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| if (c as u32) < 32 || (c as u32) == 127 { '.' } else { c })
        .collect()
}

fn chassis_description(code: &str) -> String {
    let n: u8 = code.trim().parse().unwrap_or(2);
    match n {
        1 => "Other",
        2 => "Unknown",
        3 => "Desktop",
        4 => "Low Profile Desktop",
        5 => "Pizza Box",
        6 => "Mini Tower",
        7 => "Tower",
        8 => "Portable",
        9 => "Laptop",
        10 => "Notebook",
        11 => "Hand Held",
        12 => "Docking Station",
        13 => "All in One",
        14 => "Sub Notebook",
        15 => "Space-Saving",
        16 => "Lunch Box",
        17 => "Main System Chassis",
        18 => "Expansion Chassis",
        19 => "SubChassis",
        20 => "Bus Expansion Chassis",
        21 => "Peripheral Chassis",
        22 => "RAID Chassis",
        23 => "Rack Mount Chassis",
        24 => "Sealed-Case PC",
        25 => "Multi-System",
        26 => "CompactPCI",
        27 => "AdvancedTCA",
        28 => "Blade",
        29 => "Blade Enclosure",
        30 => "Tablet",
        31 => "Convertible",
        32 => "Detachable",
        33 => "IoT Gateway",
        34 => "Embedded PC",
        35 => "Mini PC",
        36 => "Stick PC",
        _ => "Unknown",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── sanitize ─────────────────────────────────────────────────────────────

    #[test]
    fn sanitize_normal_string() {
        assert_eq!(sanitize("Dell Inc."), "Dell Inc.");
    }

    #[test]
    fn sanitize_replaces_control_chars() {
        assert_eq!(sanitize("foo\x01bar"), "foo.bar");
    }

    #[test]
    fn sanitize_replaces_del() {
        assert_eq!(sanitize("foo\x7fbar"), "foo.bar");
    }

    #[test]
    fn sanitize_space_preserved() {
        assert_eq!(sanitize("Dell Inc."), "Dell Inc.");
    }

    // ── chassis_description ──────────────────────────────────────────────────

    #[test]
    fn chassis_description_known_codes() {
        assert_eq!(chassis_description("3"), "Desktop");
        assert_eq!(chassis_description("9"), "Laptop");
        assert_eq!(chassis_description("17"), "Main System Chassis");
        assert_eq!(chassis_description("23"), "Rack Mount Chassis");
    }

    #[test]
    fn chassis_description_unknown_code() {
        assert_eq!(chassis_description("99"), "Unknown");
    }

    #[test]
    fn chassis_description_unparseable_defaults_to_unknown() {
        assert_eq!(chassis_description("not-a-number"), "Unknown");
    }

    #[test]
    fn chassis_description_trims_whitespace() {
        assert_eq!(chassis_description(" 3 "), "Desktop");
    }
}
