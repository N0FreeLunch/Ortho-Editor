use crate::item::Item;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlphaInventoryRoute {
    Equipment,
    Belt,
    Inventory,
    Stash,
    Cube,
}

impl AlphaInventoryRoute {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Equipment => "equipment",
            Self::Belt => "belt",
            Self::Inventory => "inventory",
            Self::Stash => "stash",
            Self::Cube => "cube",
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
    } else if item.mode == 2 {
        if is_alpha && !is_true_pot && item.location != 2 {
            match item.location {
                4 => AlphaInventoryRoute::Stash,
                7 => AlphaInventoryRoute::Cube,
                _ => AlphaInventoryRoute::Inventory,
            }
        } else {
            AlphaInventoryRoute::Belt
        }
    } else {
        match item.location {
            0 => {
                if is_true_pot && item.x == 0 && item.y == 0 {
                    AlphaInventoryRoute::Belt
                } else {
                    AlphaInventoryRoute::Inventory
                }
            }
            2 => AlphaInventoryRoute::Belt,
            4 => AlphaInventoryRoute::Stash,
            7 => AlphaInventoryRoute::Cube,
            _ => AlphaInventoryRoute::Inventory,
        }
    }
}

pub fn is_true_potion(code: &str) -> bool {
    let trimmed = code.trim().to_lowercase();
    trimmed.starts_with("hp") || trimmed.starts_with("mp") || trimmed.starts_with("rv")
}
