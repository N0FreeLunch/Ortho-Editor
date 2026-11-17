//! Produce base-item warnings separately from validation orchestration.

use crate::data::item_specs::get_base_item_spec;
use crate::data::DataRepository;
use crate::domain::item::type_hierarchy::get_all_item_types;
use crate::item::Item;

pub fn check_item_code_legitimacy(item: &Item) -> Vec<String> {
    let mut warnings = Vec::new();
    let trimmed = item.code.trim();
    if DataRepository::get_item_template(trimmed).is_none() {
        warnings.push(format!("Invalid item code '{}': not found in game database", trimmed));
    }
    warnings
}

pub fn check_socket_legitimacy(item: &Item) -> Vec<String> {
    let mut warnings = Vec::new();

    let ilvl = match item.level {
        Some(lvl) => lvl,
        None => return warnings,
    };

    let types = get_all_item_types(&item.code);
    let max_sockets = crate::domain::item::socket_policy::source_socket_limit(types, ilvl);

    if let Some(max) = max_sockets {
        if let Some(actual) = item.sockets {
            if actual > max {
                warnings.push(format!("Socket count {} exceeds max {} for ilvl {}", actual, max, ilvl));
            }
        }
    }

    warnings
}

pub fn check_ethereal_legitimacy(item: &Item) -> Vec<String> {
    let mut warnings = Vec::new();
    if !item.is_ethereal {
        return warnings;
    }

    let trimmed = item.code.trim();
    let spec = if let Some(s) = get_base_item_spec(trimmed) {
        s
    } else {
        return warnings;
    };

    // 1. Illegal Ethereal State Check
    let item_types = get_all_item_types(&item.code);
    if item_types.contains(&"bow") || item_types.contains(&"xbow") {
        warnings.push("Bows and crossbows cannot be ethereal".to_string());
    }

    // 2. Durability Verification
    if spec.durability > 0 {
        let eth_base_durability = (spec.durability as u32 / 2) + 1;
        let mut edur_percent = 0;
        for prop in &item.properties {
            if prop.stat_id == 75 {
                edur_percent += prop.value;
            }
        }
        let expected_max_dur = (eth_base_durability * (100 + edur_percent as u32)) / 100;
        if let Some(max_dur) = item.max_durability {
            if max_dur > 0 {
                if (max_dur as u32) < expected_max_dur {
                    warnings.push(format!(
                        "Ethereal item has lower than expected durability: {} (expected ~{})",
                        max_dur, expected_max_dur
                    ));
                } else if (max_dur as u32) > expected_max_dur + 1 {
                    warnings.push(format!(
                        "Ethereal item has higher than expected durability: {} (expected ~{})",
                        max_dur, expected_max_dur
                    ));
                }
            }
        }
    }
    warnings
}
