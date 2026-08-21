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

pub fn is_true_potion(code: &str) -> bool {
    let trimmed = code.trim().to_lowercase();
    trimmed.starts_with("hp") || trimmed.starts_with("mp") || trimmed.starts_with("rv")
}
