//! Look up affix rows without applying caller-specific item or naming policy.

use crate::data::affixes::{PREFIXES, SUFFIXES};
use crate::data::item_specs::Affix;

pub fn lookup_prefix(id: u16) -> Option<&'static Affix> {
    PREFIXES.iter().find(|a| a.id == id as u32)
}

pub fn lookup_suffix(id: u16) -> Option<&'static Affix> {
    SUFFIXES.iter().find(|a| a.id == id as u32)
}
