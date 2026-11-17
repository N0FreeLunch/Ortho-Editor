//! Runeword base and ordered socket warning policies.

use super::model::{ValidationWarning, WarningConfidenceTier};
use crate::domain::item::runeword::{
    runeword_display_name, runeword_expected_item_types, runeword_expected_runes,
};
use crate::domain::item::specification::{ItemSpec, lookup_spec};
use crate::domain::item::type_hierarchy::get_all_item_types;
use crate::item::{Item, ItemQuality};

pub fn check_runeword_legitimacy(item: &Item) -> Vec<String> {
    check_runeword_legitimacy_detailed(item)
        .into_iter()
        .map(|w| w.message)
        .collect()
}

pub fn check_runeword_legitimacy_detailed(item: &Item) -> Vec<ValidationWarning> {
    let mut warnings = Vec::new();

    if !item.is_runeword {
        return warnings;
    }

    let rw = match lookup_spec(item) {
        Some(ItemSpec::Runeword(rw)) => rw,
        _ => return warnings,
    };

    let rw_name = runeword_display_name(rw);

    // 1. Base Item Quality: Runewords can ONLY be made in Normal or Superior items (Low/Normal/High)
    if let Some(quality) = item.quality {
        match quality {
            ItemQuality::Normal | ItemQuality::Low | ItemQuality::High => {}
            _ => {
                warnings.push(ValidationWarning::new(
                    format!(
                        "Runeword '{}' cannot be made in an item of quality '{:?}'",
                        rw_name, quality
                    ),
                    WarningConfidenceTier::SettledContract,
                    "runeword_base_quality",
                ));
            }
        }
    }

    let expected_item_types = runeword_expected_item_types(rw);

    // 2. Type Eligibility
    let item_types = get_all_item_types(&item.code);
    let is_eligible = expected_item_types
        .iter()
        .any(|&item_type| item_types.contains(&item_type));
    if !is_eligible {
        warnings.push(ValidationWarning::new(
            format!(
                "Runeword '{}' is not eligible for item type '{}'",
                rw_name,
                item.code.trim()
            ),
            WarningConfidenceTier::SettledContract,
            "runeword_item_type",
        ));
    }

    let expected_runes = runeword_expected_runes(rw);

    // 3. Socket Count
    if let Some(sockets) = item.sockets {
        if !expected_runes.is_empty() && sockets as usize != expected_runes.len() {
            warnings.push(ValidationWarning::new(
                format!(
                    "Runeword '{}' requires {} sockets, but item has {}",
                    rw_name,
                    expected_runes.len(),
                    sockets
                ),
                WarningConfidenceTier::SettledContract,
                "runeword_socket_count",
            ));
        }
    }

    // 4. Rune Sequence
    if !expected_runes.is_empty() && item.socketed_items.len() == expected_runes.len() {
        for (i, socketed) in item.socketed_items.iter().enumerate() {
            if socketed.code.trim() != expected_runes[i] {
                warnings.push(ValidationWarning::new(
                    format!(
                        "Runeword '{}' has incorrect rune sequence: expected '{}' at socket position {}, found '{}'",
                        rw_name,
                        expected_runes[i],
                        i + 1,
                        socketed.code.trim()
                    ),
                    WarningConfidenceTier::SettledContract,
                    "runeword_rune_sequence",
                ));
                break;
            }
        }
    } else if !expected_runes.is_empty() && !item.socketed_items.is_empty() {
        warnings.push(ValidationWarning::new(
            format!(
                "Runeword '{}' has incorrect number of socketed items",
                rw_name
            ),
            WarningConfidenceTier::SettledContract,
            "runeword_socketed_items",
        ));
    }

    warnings
}
