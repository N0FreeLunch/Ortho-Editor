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
    #[serde(default)]
    pub is_opaque: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opaque_bits: Option<Vec<bool>>,
}

impl ItemProperty {
    pub fn new(
        stat_id: u32,
        name: String,
        param: u32,
        raw_value: i32,
        value: i32,
        range: ItemBitRange,
    ) -> Self {
        Self {
            stat_id,
            name,
            param,
            raw_value,
            value,
            range,
            is_opaque: false,
            opaque_bits: None,
        }
    }

    pub fn new_opaque(
        stat_id_candidate: u32,
        raw_bits: Vec<bool>,
        range: ItemBitRange,
    ) -> Self {
        Self {
            stat_id: stat_id_candidate,
            name: "opaque_property".to_string(),
            param: 0,
            raw_value: 0,
            value: 0,
            range,
            is_opaque: true,
            opaque_bits: Some(raw_bits),
        }
    }
}



