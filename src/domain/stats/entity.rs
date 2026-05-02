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
}

pub const ALPHA_STAT_MAPS: &[AlphaStatMap] = &[
    AlphaStatMap { raw_id: 256, effective_id: 127, name: "item_allskills" },
    AlphaStatMap { raw_id: 496, effective_id: 99,  name: "item_fastergethitrate" },
    AlphaStatMap { raw_id: 499, effective_id: 16,  name: "item_enandefense_percent" },
    AlphaStatMap { raw_id: 289, effective_id: 9,   name: "maxmana" },
    AlphaStatMap { raw_id: 26,  effective_id: 31,  name: "item_defense_percent" },
    AlphaStatMap { raw_id: 312, effective_id: 72,  name: "item_durability" },
    AlphaStatMap { raw_id: 207, effective_id: 73,  name: "item_maxdurability" },
    AlphaStatMap { raw_id: 380, effective_id: 194, name: "item_indestructible" },
    AlphaStatMap { raw_id: 256, effective_id: 127, name: "item_allskills" },
    AlphaStatMap { raw_id: 496, effective_id: 99,  name: "item_fastergethitrate" },
    AlphaStatMap { raw_id: 499, effective_id: 16,  name: "item_enandefense_percent" },
    AlphaStatMap { raw_id: 289, effective_id: 9,   name: "maxmana" },

    AlphaStatMap { raw_id: 25,  effective_id: 194, name: "item_numsockets_alpha" },
    AlphaStatMap { raw_id: 3,   effective_id: 119, name: "item_tohit_percent_alpha" },
    AlphaStatMap { raw_id: 114, effective_id: 7,   name: "maxlife" },
    AlphaStatMap { raw_id: 287, effective_id: 9,   name: "maxmana_alt" }, 
    AlphaStatMap { raw_id: 106, effective_id: 127, name: "item_allskills_alt" },
];
