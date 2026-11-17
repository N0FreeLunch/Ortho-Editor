//! Calculate qualified stack capacity without formatting or mutating item data.

use crate::data::DataRepository;
use crate::item::{Item, ItemProperty};

pub(crate) fn qualified_quantity_capacity(
    item: &Item,
    quantity: u32,
    active_set_count: usize,
) -> Option<u32> {
    const QUANTITY_MAX: u32 = (1 << 9) - 1;
    if item.header.save_is_alpha || quantity > QUANTITY_MAX {
        return None;
    }
    let template = DataRepository::get_item_template(item.code.trim())?;
    if !template.is_stackable {
        return None;
    }
    let base = template
        .max_stack
        .filter(|value| (1..=QUANTITY_MAX).contains(value))?;
    if item.header.is_socketed
        || item.sockets.is_some_and(|value| value > 0)
        || !item.socketed_items.is_empty()
    {
        return None;
    }
    let unsafe_capacity = |property: &ItemProperty| {
        property.stat_id == 254 || property.is_opaque || property.opaque_bits.is_some()
    };
    if !item.is_identified {
        let visible_unsafe = item
            .properties
            .iter()
            .chain(item.runeword_attributes.iter())
            .chain(item.set_attributes.iter().flatten())
            .any(unsafe_capacity);
        let concealed_unsafe = item
            .body
            .d2r_v3_auxiliary
            .as_ref()
            .and_then(|auxiliary| auxiliary.concealed_stats.as_ref())
            .is_some_and(|stats| {
                stats
                    .properties
                    .iter()
                    .chain(stats.runeword_attributes.iter())
                    .chain(stats.set_attributes.iter().flatten())
                    .any(unsafe_capacity)
            });
        return (!visible_unsafe && !concealed_unsafe).then_some(base);
    }
    if (!item.is_runeword && item.runeword_attributes.iter().any(unsafe_capacity))
        || (active_set_count > 1 && item.set_attributes.iter().flatten().any(unsafe_capacity))
    {
        return None;
    }
    let mut capacity = i64::from(base);
    for property in item
        .properties
        .iter()
        .chain(item.runeword_attributes.iter().filter(|_| item.is_runeword))
    {
        if property.is_opaque || property.opaque_bits.is_some() {
            return None;
        }
        if property.stat_id == 254 {
            if !property.is_verified()
                || property.param != 0
                || property.raw_value != property.value
                || !(0..=255).contains(&property.value)
            {
                return None;
            }
            capacity = capacity.checked_add(i64::from(property.value))?;
        }
    }
    u32::try_from(capacity.min(i64::from(QUANTITY_MAX))).ok()
}
