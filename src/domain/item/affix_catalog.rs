//! Look up affix rows and query their declared item-type eligibility.

use crate::data::affixes::{PREFIXES, SUFFIXES};
use crate::data::item_specs::Affix;

pub fn lookup_prefix(id: u16) -> Option<&'static Affix> {
    PREFIXES.iter().find(|a| a.id == id as u32)
}

pub fn lookup_suffix(id: u16) -> Option<&'static Affix> {
    SUFFIXES.iter().find(|a| a.id == id as u32)
}

pub fn is_affix_eligible_for(affix: &Affix, item_types: &[&str]) -> bool {
    // If include_types is empty, it's generally allowed on all types unless excluded
    if !affix.include_types.is_empty() {
        let has_included = affix
            .include_types
            .iter()
            .any(|&inc| item_types.contains(&inc));
        if !has_included {
            return false;
        }
    }

    // If any excluded type is in item_types, it's not eligible
    let has_excluded = affix
        .exclude_types
        .iter()
        .any(|&exc| item_types.contains(&exc));
    if has_excluded {
        return false;
    }

    true
}
