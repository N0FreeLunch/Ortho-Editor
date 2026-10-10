use crate::item::Item;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlphaInventoryRoute {
    Equipment,
    Belt,
    Inventory,
    Stash,
    Cube,
    Unknown,
}

impl AlphaInventoryRoute {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Equipment => "equipment",
            Self::Belt => "belt",
            Self::Inventory => "inventory",
            Self::Stash => "stash",
            Self::Cube => "cube",
            Self::Unknown => "unknown",
        }
    }
}

pub fn alpha_inventory_route(item: &Item, is_alpha: bool) -> AlphaInventoryRoute {
    let candidate = alpha_inventory_candidate_route(item, is_alpha);
    if candidate == AlphaInventoryRoute::Belt && !is_belt_consumable(&item.code) {
        AlphaInventoryRoute::Unknown
    } else {
        candidate
    }
}

/// Unvalidated placement claim, retained for diagnostics before routing isolation.
pub fn alpha_inventory_candidate_route(item: &Item, is_alpha: bool) -> AlphaInventoryRoute {
    let is_true_pot = is_true_potion(&item.code);

    if item.mode == 1 {
        if is_true_pot {
            AlphaInventoryRoute::Belt
        } else {
            AlphaInventoryRoute::Equipment
        }
    } else if item.mode == 2 || item.location == 2 || item.location == 8 {
        AlphaInventoryRoute::Belt
    } else if item.location == 4 || (is_alpha && item.location == 12) {
        AlphaInventoryRoute::Stash
    } else if item.location == 7 {
        AlphaInventoryRoute::Cube
    } else {
        // Location 0 or 10 or other grid-based placements
        if item.location == 0 || item.location == 10 {
            if is_true_pot && item.x < 4 && item.y < 4 {
                AlphaInventoryRoute::Belt
            } else if item.x < 10 && item.y < 4 {
                AlphaInventoryRoute::Inventory
            } else if item.x < 10 && item.y < 10 {
                AlphaInventoryRoute::Stash
            } else {
                AlphaInventoryRoute::Unknown
            }
        } else if !is_alpha {
            AlphaInventoryRoute::Inventory
        } else {
            AlphaInventoryRoute::Unknown
        }
    }
}

/// Belt-admissible consumables; exact codes prevent malformed prefix matches.
/// This checks item kind only, not placement, capacity, or slot collisions.
pub fn is_belt_consumable(code: &str) -> bool {
    matches!(
        code.trim().to_ascii_lowercase().as_str(),
        "hp1"
            | "hp2"
            | "hp3"
            | "hp4"
            | "hp5"
            | "hpf"
            | "hpo"
            | "mp1"
            | "mp2"
            | "mp3"
            | "mp4"
            | "mp5"
            | "mpf"
            | "mpo"
            | "rvs"
            | "rvl"
            | "vps"
            | "yps"
            | "wms"
            | "tsc"
            | "isc"
            | "hrb"
    )
}

pub fn is_true_potion(code: &str) -> bool {
    let trimmed = code.trim().to_lowercase();
    trimmed.starts_with("hp")
        || trimmed.starts_with("mp")
        || trimmed.starts_with("rv")
        || trimmed.starts_with("gp")
        || trimmed.starts_with("op")
        || trimmed == "vps"
        || trimmed == "yps"
        || trimmed == "wms"
}

#[cfg(test)]
mod tests {
    use super::*;
    use d2r_macros::settled_contract_test;

    #[settled_contract_test]
    fn belt_admissibility_rejects_equipment_tomes_and_throwing_potions() {
        for code in [
            "rin", "cm2", "jew", "7pw", "tbk", "ibk", "gpl", "opl", "gp1", "hp9", "mpjunk", "rvx",
            "Opaque", "oesw", "",
        ] {
            assert!(!is_belt_consumable(code), "{code}");
        }
        for code in [
            "hp1", "hp2", "hp3", "hp4", "hp5", "hpf", "hpo", "mp1", "mp2", "mp3", "mp4", "mp5",
            "mpf", "mpo", "rvs", "rvl", "vps", "yps", "wms", "tsc", "isc", "hrb", " HP4 ",
        ] {
            assert!(is_belt_consumable(code), "{code}");
        }
    }

    #[settled_contract_test]
    fn all_belt_branches_isolate_invalid_claims_without_changing_equipment() {
        for (mode, location, code) in [
            (2, 0, "rin"),
            (0, 2, "cm2"),
            (0, 8, "jew"),
            (1, 0, "hp9"),
            (0, 0, "gp1"),
        ] {
            let mut item = Item {
                code: code.into(),
                ..Item::default()
            };
            item.mode = mode;
            item.location = location;
            assert_eq!(
                alpha_inventory_candidate_route(&item, true),
                AlphaInventoryRoute::Belt
            );
            assert_eq!(
                alpha_inventory_route(&item, true),
                AlphaInventoryRoute::Unknown
            );
        }
        let mut item = Item {
            code: "rin".into(),
            ..Item::default()
        };
        item.mode = 1;
        assert_eq!(
            alpha_inventory_route(&item, true),
            AlphaInventoryRoute::Equipment
        );
        item.mode = 2;
        for code in ["hp4", "rvs", "tsc", "isc"] {
            item.code = code.into();
            assert_eq!(
                alpha_inventory_route(&item, true),
                AlphaInventoryRoute::Belt
            );
        }
    }
}
