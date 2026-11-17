//! Project unmodified boot kick damage independently of localized text.

use crate::data::DataRepository;
use crate::item::Item;

pub(crate) struct BootKickDamage {
    pub min: u16,
    pub max: u16,
}

pub(crate) fn displayed_boot_kick_damage(item: &Item) -> Option<BootKickDamage> {
    let is_boot = DataRepository::get_item_template(item.code.trim())
        .map_or(false, |t| t.item_type == Some("boot"));

    if is_boot {
        if let Some(spec) = crate::data::item_specs::get_base_item_spec(&item.code) {
            if spec.min_dmg > 0 && spec.max_dmg > 0 {
                return Some(BootKickDamage {
                    min: spec.min_dmg,
                    max: spec.max_dmg,
                });
            }
        }
    }
    None
}
