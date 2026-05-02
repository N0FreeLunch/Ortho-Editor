use serde::Serialize;
use crate::domain::item::entity::ItemBitRange;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct ItemStats {
    pub properties: Vec<ItemProperty>,
    pub set_attributes: Vec<Vec<ItemProperty>>,
    pub runeword_attributes: Vec<ItemProperty>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ItemProperty {
    pub stat_id: u32,
    pub name: String,
    pub param: u32,
    pub raw_value: i32,
    pub value: i32, // After applying save_add if needed
    pub range: ItemBitRange,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct AlphaStatMap {
    pub raw_id: u32,
    pub effective_id: u32,
    pub name: &'static str,
    pub save_bits: Option<u32>,
    pub save_add: Option<i32>,
}

pub const ALPHA_STAT_MAPS: &[AlphaStatMap] = &[
    AlphaStatMap { raw_id: 256, effective_id: 127, name: "item_allskills",           save_bits: None,     save_add: None },
    AlphaStatMap { raw_id: 496, effective_id: 99,  name: "item_fastergethitrate",    save_bits: None,     save_add: None },
    AlphaStatMap { raw_id: 499, effective_id: 16,  name: "item_enandefense_percent", save_bits: None,     save_add: None },
    AlphaStatMap { raw_id: 289, effective_id: 9,   name: "maxmana",                  save_bits: Some(14), save_add: Some(0) },
    AlphaStatMap { raw_id: 26,  effective_id: 31,  name: "item_defense_percent",     save_bits: None,     save_add: None },
    AlphaStatMap { raw_id: 312, effective_id: 72,  name: "item_durability",          save_bits: Some(14), save_add: Some(0) },
    AlphaStatMap { raw_id: 207, effective_id: 73,  name: "item_maxdurability",       save_bits: Some(14), save_add: Some(0) },
    AlphaStatMap { raw_id: 380, effective_id: 194, name: "item_indestructible",      save_bits: None,     save_add: None },
    AlphaStatMap { raw_id: 25,  effective_id: 194, name: "item_numsockets_alpha",    save_bits: None,     save_add: None },
    AlphaStatMap { raw_id: 3,   effective_id: 119, name: "item_tohit_percent_alpha", save_bits: None,     save_add: None },
    AlphaStatMap { raw_id: 114, effective_id: 7,   name: "maxlife",                  save_bits: Some(14), save_add: Some(0) },
    AlphaStatMap { raw_id: 287, effective_id: 9,   name: "maxmana_alt",              save_bits: Some(14), save_add: Some(0) }, 
    AlphaStatMap { raw_id: 106, effective_id: 127, name: "item_allskills_alt",       save_bits: None,     save_add: None },
    AlphaStatMap { raw_id: 309, effective_id: 9,   name: "maxmana_alpha_309",        save_bits: Some(14), save_add: Some(0) },
    AlphaStatMap { raw_id: 310, effective_id: 7,   name: "maxlife_alpha_310",        save_bits: Some(14), save_add: Some(0) },
    AlphaStatMap { raw_id: 311, effective_id: 7,   name: "maxlife_alpha_311",        save_bits: Some(14), save_add: Some(0) },
];
