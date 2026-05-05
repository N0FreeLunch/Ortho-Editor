use crate::data::bit_cursor::BitCursor;
use crate::domain::item::huffman::{HuffmanTree, read_player_name};
use crate::error::ParsingResult;
use bitstream_io::BitRead;
use crate::domain::header::entity::ItemSegmentType;
use serde::{Serialize, Deserialize};
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

use std::ops::{Deref, DerefMut};
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

impl ItemBody {
    pub fn read_from_cursor<R: BitRead>(
        cursor: &mut BitCursor<R>,
        huff: &HuffmanTree,
        header: &ItemHeader,
        alpha_mode: bool,
    ) -> ParsingResult<(Self, Option<u8>, Option<u8>, Option<String>)> {
        let is_ear = header.is_ear;
        let (code, alpha_nudge, ear_class, ear_level, ear_player_name) = if is_ear {
            cursor.begin_segment(ItemSegmentType::Unknown);
            let class = Some(cursor.read_bits::<u8>(3)? as u8);
            let level = Some(cursor.read_bits::<u8>(7)? as u8);
            let name = Some(read_player_name(cursor, alpha_mode && header.version == 5)?);
            if alpha_mode && header.version == 5 { cursor.byte_align()?; }
            cursor.end_segment();
            (String::new(), None, class, level, name)
        } else {
            cursor.begin_segment(ItemSegmentType::Code);
            let mut code = String::new();
            for _ in 0..4 {
                code.push(huff.decode_recorded(cursor)?);
            }
            let mut nudge = None;
            if alpha_mode && (header.version == 5 || header.version == 0 || header.version == 1) {
                nudge = Some(cursor.read_bits::<u8>(2)?);
            }
            cursor.end_segment();
            (code, nudge, None, None, None)
        };

        Ok((ItemBody {
            code,
            x: header.x,
            y: header.y,
            page: header.page,
            location: header.location,
            mode: header.mode,
            defense: None,
            max_durability: None,
            current_durability: None,
            quantity: None,
            alpha_header_gap: None, 
            v5_runeword_extra: None,
            v105_7mgw_payload: None,
            alpha_nudge,
            alpha_set_list_val: None,
            alpha_shadow_skip_bits: None,
            alpha_alignment_padding: Vec::new(),
        }, ear_class, ear_level, ear_player_name))
    }
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

    // --- Legacy Compatibility Fields (To be removed in Slice 3) ---
    pub code: String,
    pub defense: Option<u32>,
    pub max_durability: Option<u32>,
    pub current_durability: Option<u32>,
    pub quantity: Option<u32>,
    // -----------------------------------------------------------

    pub bits: Vec<RecordedBit>,
    pub ear_class: Option<u8>,
    pub ear_level: Option<u8>,
    pub ear_player_name: Option<String>,
    pub personalized_player_name: Option<String>,
    pub has_multiple_graphics: bool,
    pub multi_graphics_bits: Option<u8>,
    pub has_class_specific_data: bool,
    pub class_specific_bits: Option<u16>,
    pub low_high_graphic_bits: Option<u8>,
    pub magic_prefix: Option<u16>,
    pub magic_suffix: Option<u16>,
    pub rare_name_1: Option<u8>,
    pub rare_name_2: Option<u8>,
    pub rare_affixes: [Option<u16>; 6],
    pub unique_id: Option<u16>,
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
    pub sockets: Option<u8>,
    pub modules: Vec<ItemModule>,
    pub range: ItemBitRange,
    pub total_bits: u64,
    pub gap_bits: Vec<bool>,
    pub segments: Vec<BitSegment>,
    pub expected_start_bit: u64,
    pub forensic_audit: crate::domain::item::axiom_meta::ForensicAudit,
}

impl Deref for Item {
    type Target = ItemHeader;
    fn deref(&self) -> &Self::Target {
        &self.header
    }
}

impl DerefMut for Item {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.header
    }
}

impl Item {
    pub fn code(&self) -> &str { &self.body.code }
    pub fn defense(&self) -> Option<u32> { self.body.defense }
    pub fn max_durability(&self) -> Option<u32> { self.body.max_durability }
    pub fn current_durability(&self) -> Option<u32> { self.body.current_durability }
    pub fn quantity(&self) -> Option<u32> { self.body.quantity }

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
                return Some(BitSemantic { label: format!("{} -> {}", self.body.code.trim(), semantic.label) });
            }
        }
        None
    }

    pub fn empty_for_tests() -> Self {
        Self::default()
    }

    pub fn header_view(&self) -> ItemHeader {
        self.header.clone()
    }

    pub fn body_view(&self) -> ItemBody {
        self.body.clone()
    }

    /// Mutates the item using a checked placement.
    /// This clears the cached bitstream, forcing a re-encoding.
    pub fn set_placement(&mut self, placement: crate::domain::vo::InventoryPlacement) {
        self.header.x = placement.coordinate().x();
        self.header.y = placement.coordinate().y();
        self.body.x = self.header.x;
        self.body.y = self.header.y;
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
