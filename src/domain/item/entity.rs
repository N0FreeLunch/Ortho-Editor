use serde::{Serialize, Deserialize};
use super::quality::ItemQuality;
use crate::domain::stats::{ItemProperty, ItemStats};

#[derive(Debug, Clone, Serialize)]
pub struct BitSemantic {
    pub label: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RecordedBit {
    pub bit: bool,
    pub offset: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ItemBitRange {
    pub start: u64,
    pub end: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct BitSegment {
    pub start: u64,
    pub end: u64,
    pub label: String,
    pub depth: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharmBagData {
    pub size: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CursedItemData {
    pub curse_id: u32,
}

pub use crate::domain::header::entity::ItemHeader;


#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ItemBody {
    pub code: String,
    pub x: u8,
    pub y: u8,
    pub page: u8,
    pub location: u8,
    pub mode: u8,
    pub defense: Option<u32>,
    pub max_durability: Option<u32>,
    pub current_durability: Option<u32>,
    pub quantity: Option<u32>,
    // Alpha Forensic Fields
    pub alpha_header_gap: Option<u8>,
    pub v5_runeword_extra: Option<u8>,
    pub v105_7mgw_payload: Option<Vec<bool>>,
    pub alpha_nudge: Option<u8>,
    pub alpha_set_list_val: Option<u8>,
    pub alpha_shadow_skip_bits: Option<u64>,
    pub alpha_alignment_padding: Vec<bool>,
}


#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemModule {
    MagicAffixes { prefix: Option<u16>, suffix: Option<u16> },
    RareAffixes { names: [Option<u8>; 2], affixes: [Option<u16>; 6] },
    UniqueAffix { unique_id: Option<u16> },
    Sockets { count: u8, items: Vec<Item> },
    Personalization(Option<String>),
    Runeword { id: Option<u16>, level: Option<u8> },
    CharmBag(CharmBagData),
    Cursed(CursedItemData),
    Augmentation(u32),
    Opaque(Vec<bool>),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Item {
    pub header: ItemHeader,
    pub body: ItemBody,
    pub stats: ItemStats,

    pub bits: Vec<RecordedBit>,
    pub code: String,
    pub flags: u32,
    pub version: u8,
    pub is_ear: bool,
    pub ear_class: Option<u8>,
    pub ear_level: Option<u8>,
    pub ear_player_name: Option<String>,
    pub personalized_player_name: Option<String>,
    pub mode: u8,
    pub x: u8,
    pub y: u8,
    pub page: u8,
    pub location: u8,
    pub header_socket_hint: u8,
    pub has_multiple_graphics: bool,
    pub multi_graphics_bits: Option<u8>,
    pub has_class_specific_data: bool,
    pub class_specific_bits: Option<u16>,
    pub id: Option<u32>,
    pub level: Option<u8>,
    pub quality: Option<ItemQuality>,
    pub low_high_graphic_bits: Option<u8>,
    pub is_compact: bool,
    pub is_socketed: bool,
    pub is_identified: bool,
    pub is_personalized: bool,
    pub is_runeword: bool,
    pub is_ethereal: bool,
    pub magic_prefix: Option<u16>,
    pub magic_suffix: Option<u16>,
    pub rare_name_1: Option<u8>,
    pub rare_name_2: Option<u8>,
    pub rare_affixes: [Option<u16>; 6],
    pub unique_id: Option<u16>,
    pub alpha_unique_id_raw: Option<u16>,
    pub runeword_id: Option<u16>,
    pub runeword_level: Option<u8>,
    pub properties: Vec<ItemProperty>,
    pub set_attributes: Vec<Vec<ItemProperty>>,
    pub runeword_attributes: Vec<ItemProperty>,
    pub num_socketed_items: u8,
    pub socketed_items: Vec<Item>,
    pub timestamp_flag: bool,
    pub properties_complete: bool,
    pub terminator_bit: bool,
    pub set_list_count: u8,
    pub tbk_ibk_teleport: Option<u8>,
    pub defense: Option<u32>,
    pub max_durability: Option<u32>,
    pub current_durability: Option<u32>,
    pub quantity: Option<u32>,
    pub sockets: Option<u8>,
    pub modules: Vec<ItemModule>,
    pub range: ItemBitRange,
    pub total_bits: u64,
    pub gap_bits: Vec<bool>,
    pub segments: Vec<BitSegment>,
    pub expected_start_bit: u64,
    pub forensic_audit: crate::domain::item::axiom_meta::ForensicAudit,
}

impl Item {
    pub fn query_bit(&self, offset: u64) -> Option<BitSemantic> {
        // 1. Check properties for more semantic context
        for prop in &self.properties {
            if offset >= prop.range.start && offset < prop.range.end {
                 let name = if prop.name.is_empty() {
                     crate::domain::stats::lookup_alpha_map_by_raw(prop.stat_id).map(|m| m.name.to_string()).unwrap_or_else(|| format!("Stat({})", prop.stat_id))
                 } else {
                     prop.name.clone()
                 };
                 return Some(BitSemantic { label: format!("Stats -> {}", name) });
            }
        }
        
        for (i, list) in self.set_attributes.iter().enumerate() {
            for prop in list {
                if offset >= prop.range.start && offset < prop.range.end {
                    let name = if prop.name.is_empty() {
                        crate::domain::stats::lookup_alpha_map_by_raw(prop.stat_id).map(|m| m.name.to_string()).unwrap_or_else(|| format!("Stat({})", prop.stat_id))
                    } else {
                        prop.name.clone()
                    };
                    return Some(BitSemantic { label: format!("SetAttributes[{}] -> {}", i, name) });
                }
            }
        }
        
        for prop in &self.runeword_attributes {
            if offset >= prop.range.start && offset < prop.range.end {
                let name = if prop.name.is_empty() {
                    crate::domain::stats::lookup_alpha_map_by_raw(prop.stat_id).map(|m| m.name.to_string()).unwrap_or_else(|| format!("Stat({})", prop.stat_id))
                } else {
                    prop.name.clone()
                };
                return Some(BitSemantic { label: format!("RunewordAttributes -> {}", name) });
            }
        }

        // 2. Find the deepest segment that contains this offset
        let mut best_segment: Option<&BitSegment> = None;
        
        for seg in &self.segments {
            if offset >= seg.start && offset < seg.end {
                if let Some(best) = best_segment {
                    if seg.depth > best.depth {
                        best_segment = Some(seg);
                    }
                } else {
                    best_segment = Some(seg);
                }
            }
        }

        if let Some(seg) = best_segment {
            return Some(BitSemantic { label: seg.label.clone() });
        }

        // Check children recursively
        for child in &self.socketed_items {
            if let Some(semantic) = child.query_bit(offset) {
                return Some(BitSemantic { label: format!("{} -> {}", self.code.trim(), semantic.label) });
            }
        }
        None
    }

    pub fn empty_for_tests() -> Self {
        Self::default()
    }

    pub fn header_view(&self) -> ItemHeader {
        ItemHeader {
            flags: self.flags,
            version: self.version,
            mode: self.mode,
            location: self.location,
            x: self.x,
            y: self.y,
            page: self.page,
            socket_hint: self.header_socket_hint,
            id: self.id,
            quality: self.quality,
            is_compact: self.is_compact,
            is_identified: self.is_identified,
            is_socketed: self.is_socketed,
            is_personalized: self.is_personalized,
            is_runeword: self.is_runeword,
            is_ethereal: self.is_ethereal,
            is_ear: self.is_ear,
            alpha_quality_raw: self.header.alpha_quality_raw,
            alpha_v5_runeword_extra: self.header.alpha_v5_runeword_extra,
            alpha_unique_id_raw: self.alpha_unique_id_raw,
        }
    }

    pub fn body_view(&self) -> ItemBody {
        ItemBody {
            code: self.body.code.clone(),
            x: self.body.x,
            y: self.body.y,
            page: self.body.page,
            location: self.body.location,
            mode: self.body.mode,
            defense: self.body.defense,
            max_durability: self.body.max_durability,
            current_durability: self.body.current_durability,
            quantity: self.body.quantity,
            alpha_header_gap: self.body.alpha_header_gap,
            v5_runeword_extra: self.body.v5_runeword_extra,
            v105_7mgw_payload: self.body.v105_7mgw_payload.clone(),
            alpha_nudge: self.body.alpha_nudge,
            alpha_set_list_val: self.body.alpha_set_list_val,
            alpha_shadow_skip_bits: self.body.alpha_shadow_skip_bits,
            alpha_alignment_padding: self.body.alpha_alignment_padding.clone(),
        }
    }

    /// Mutates the item using a checked placement.
    /// This clears the cached bitstream, forcing a re-encoding.
    pub fn set_placement(&mut self, placement: crate::domain::vo::InventoryPlacement) {
        self.x = placement.coordinate().x();
        self.y = placement.coordinate().y();
        // Clear bits to force re-calculation from fields
        self.bits.clear();
    }

    /// Mutates a specific property value.
    /// Returns true if the property was found and updated.
    pub fn set_property_value(&mut self, stat_id: u32, value: crate::domain::vo::ItemStatValue) -> bool {
        let mut found = false;
        
        {
            let mut lists = Vec::new();
            lists.push(&mut self.properties);
            for list in &mut self.set_attributes {
                lists.push(list);
            }
            lists.push(&mut self.runeword_attributes);

            for list in lists.into_iter() {
                for prop in list {
                    if prop.stat_id == stat_id {
                        let cost = crate::data::stat_costs::STAT_COSTS.iter().find(|s| s.id == stat_id);
                        if let Some(c) = cost {
                            prop.value = value.value();
                            prop.raw_value = value.value().wrapping_add(c.save_add);
                            found = true;
                        }
                    }
                }
            }
        }
        
        if found {
            self.bits.clear();
        }
        found
    }


    pub fn stats_view(&self) -> ItemStats {
        ItemStats {
            properties: self.properties.clone(),
            set_attributes: self.set_attributes.clone(),
            runeword_attributes: self.runeword_attributes.clone(),
        }
    }

    pub fn prefixes(&self) -> Vec<&'static crate::data::item_specs::Affix> {
        let mut result = Vec::new();
        if let Some(id) = self.magic_prefix {
            if let Some(affix) = crate::data::affixes::PREFIXES.iter().find(|a| a.id == id as u32) {
                result.push(affix);
            }
        }
        // Rare prefixes are in slots 0, 2, 4
        for i in [0, 2, 4] {
            if let Some(id) = self.rare_affixes[i] {
                if let Some(affix) = crate::data::affixes::PREFIXES.iter().find(|a| a.id == id as u32) {
                    result.push(affix);
                }
            }
        }
        result
    }



    pub fn suffixes(&self) -> Vec<&'static crate::data::item_specs::Affix> {
        let mut result = Vec::new();
        if let Some(id) = self.magic_suffix {
            if let Some(affix) = crate::data::affixes::SUFFIXES.iter().find(|a| a.id == id as u32) {
                result.push(affix);
            }
        }
        // Rare suffixes are in slots 1, 3, 5
        for i in [1, 3, 5] {
            if let Some(id) = self.rare_affixes[i] {
                if let Some(affix) = crate::data::affixes::SUFFIXES.iter().find(|a| a.id == id as u32) {
                    result.push(affix);
                }
            }
        }
        result
    }
}
