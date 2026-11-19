//! Resolve item template identity independently of validation policy.

use crate::data::item_specs::{Affix, Runeword, SetItem, UniqueItem};
use crate::data::{set_items, unique_items};
use crate::domain::item::runeword::{resolve_runeword, runeword_display_name};
use crate::item::{Item, ItemQuality};

pub enum ItemSpec {
    Unique(&'static UniqueItem),
    Set(&'static SetItem),
    Runeword(&'static Runeword),
    Affix(&'static Affix),
}

impl ItemSpec {
    pub fn name(&self) -> &str {
        match self {
            ItemSpec::Unique(ui) => ui.index,
            ItemSpec::Set(si) => si.index,
            ItemSpec::Runeword(rw) => runeword_display_name(rw),
            ItemSpec::Affix(a) => a.name,
        }
    }
}

pub(crate) fn lookup_set_item(id: u16) -> Option<&'static SetItem> {
    set_items::SET_ITEMS.iter().find(|row| row.id == u32::from(id))
}

pub(crate) fn lookup_unique_item(id: u16) -> Option<&'static UniqueItem> {
    unique_items::UNIQUE_ITEMS.iter().find(|row| row.id == u32::from(id))
}

pub fn lookup_spec(item: &Item) -> Option<ItemSpec> {
    if item.is_runeword || item.header.is_runeword {
        return resolve_runeword(item).map(|resolved| ItemSpec::Runeword(resolved.template));
    }

    match item.quality {
        Some(ItemQuality::Unique) => {
            if let Some(uid) = item.unique_id {
                return lookup_unique_item(uid).map(ItemSpec::Unique);
            }
        }
        Some(ItemQuality::Set) => {
            if let Some(sid) = item.unique_id {
                return lookup_set_item(sid).map(ItemSpec::Set);
            }
        }
        _ => {}
    }
    None
}
