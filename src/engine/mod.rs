pub mod defense;
pub mod character_sheet;
pub(crate) mod durability;
pub(crate) mod quantity;
pub mod formatter;
pub mod validation;
pub mod checksum;
pub mod config;
pub mod item_parallel;
pub mod socket_effects;

pub mod mercenary_items;

pub(crate) fn is_explicit_v105(item: &crate::item::Item) -> bool {
    !item.header.save_is_alpha
        && item
            .body
            .d2r_v3_auxiliary
            .as_ref()
            .is_some_and(|a| a.save_version == 105)
}
