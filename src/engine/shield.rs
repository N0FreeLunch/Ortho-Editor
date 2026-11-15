//! Project existing shield-tooltip values independently of localization.

use crate::data::item_specs::get_base_item_spec;
use crate::data::shields::get_shield_base_block;
use crate::item::Item;

pub(crate) struct SmiteDamage {
    pub min: u16,
    pub max: u16,
}

pub(crate) struct ShieldDisplay {
    pub chance_to_block: i32,
    pub smite: Option<SmiteDamage>,
}

pub(crate) fn displayed_shield(item: &Item) -> Option<ShieldDisplay> {
    let base_block = get_shield_base_block(&item.code)?;
    let bonus_block: i32 = item
        .properties
        .iter()
        .filter(|p| p.stat_id == 20)
        .map(|p| p.value)
        .sum();
    let chance_to_block = (base_block as i32 + bonus_block).max(0);
    let smite = get_base_item_spec(&item.code)
        .filter(|spec| spec.min_dmg > 0 && spec.max_dmg > 0)
        .map(|spec| SmiteDamage {
            min: spec.min_dmg,
            max: spec.max_dmg,
        });

    Some(ShieldDisplay {
        chance_to_block,
        smite,
    })
}
