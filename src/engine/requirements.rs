//! Project existing item requirements independently of localization.

use crate::data::item_specs::get_base_item_spec;
use crate::item::Item;

pub(crate) struct RequirementsDisplay {
    // An omitted requirement is distinct from a displayed zero.
    pub strength: Option<i32>,
    pub dexterity: Option<i32>,
    pub level: Option<u8>,
}

fn scaled_requirement(mut base: i32, ethereal: bool, percent: i32) -> i32 {
    if ethereal {
        base = (base - 10).max(0);
    }
    ((base * (100 + percent)) / 100).max(0)
}

pub(crate) fn displayed_requirements(item: &Item) -> Option<RequirementsDisplay> {
    let spec = get_base_item_spec(&item.code)?;
    let percent: i32 = item
        .properties
        .iter()
        .filter(|p| p.stat_id == 91)
        .map(|p| p.value)
        .sum();
    let strength = (spec.req_str > 0)
        .then(|| scaled_requirement(spec.req_str as i32, item.is_ethereal, percent));
    let dexterity = (spec.req_dex > 0)
        .then(|| scaled_requirement(spec.req_dex as i32, item.is_ethereal, percent));
    let level = item.required_level();
    Some(RequirementsDisplay {
        strength,
        dexterity,
        level: (level > 1).then_some(level),
    })
}
