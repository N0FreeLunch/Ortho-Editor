use crate::data::bit_cursor::BitCursor;
use crate::error::{ParsingResult, ParsingError};
use bitstream_io::BitRead;
use serde::{Serialize, Deserialize};
use crate::domain::item::quality::ItemQuality;
use crate::domain::item::axiom_meta::{ForensicAudit};
use crate::domain::stats::{ItemProperty, ItemStats};
use crate::domain::stats::axiom::StatsAxiom;
use crate::domain::header::entity::{ItemSegmentType, ItemHeader, HeaderAxiom, calculate_alpha_v105_checksum};
use crate::domain::forensic::v105::V105HeaderGapAxiom;
use std::ops::{Deref, DerefMut};
use std::io;

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
    pub alpha_header_gap: Option<u32>,
    pub alpha_header_gap_bits: Vec<bool>,
    pub v5_runeword_extra: Option<u8>,
    pub v105_7mgw_payload: Option<Vec<bool>>,
    pub alpha_nudge: Option<u8>,
    pub alpha_set_list_val: Option<u8>,
    pub alpha_shadow_skip_bits: Option<u64>,
    pub alpha_body_gap_bits: Vec<bool>,
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
    SemiOpaque {
        body_bits: Vec<bool>,
        reason: String,
    },
    Residue(Vec<bool>),
}

#[derive(Debug, Clone, Default)]
pub struct ExtendedStatsData {
    pub id: Option<u32>,
    pub level: Option<u8>,
    pub quality: Option<ItemQuality>,
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
    pub personalized_player_name: Option<String>,
    pub tbk_ibk_teleport: Option<u8>,
    pub timestamp_flag: bool,
    pub defense: Option<u32>,
    pub max_durability: Option<u32>,
    pub current_durability: Option<u32>,
    pub quantity: Option<u32>,
    pub sockets: Option<u8>,
    pub set_list_count: u8,
    pub alpha_quality_raw: Option<u8>,
    pub alpha_unique_id_raw: Option<u16>,
    pub v5_runeword_extra: Option<u8>,
    pub alpha_set_list_val: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Item {
    pub header: ItemHeader,
    pub body: ItemBody,
    pub stats: ItemStats,

    // --- Legacy Compatibility Fields ---
    pub code: String,
    pub defense: Option<u32>,
    pub max_durability: Option<u32>,
    pub current_durability: Option<u32>,
    pub quantity: Option<u32>,
    // ------------------------------------

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
    pub forensic_audit: ForensicAudit,
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
    pub fn is_opaque(&self) -> bool {
        self.modules.iter().any(|m| matches!(m, ItemModule::Opaque(_) | ItemModule::Residue(_)))
    }
    pub fn is_semi_opaque(&self) -> bool {
        self.modules.iter().any(|m| matches!(m, ItemModule::SemiOpaque { .. }))
    }
    pub fn is_residue(&self) -> bool {
        self.modules.iter().any(|m| matches!(m, ItemModule::Residue(_)))
    }
    pub fn defense(&self) -> Option<u32> { 
        if let Some(d) = self.body.defense { return Some(d); }
        if self.header.save_is_alpha {
            let axiom = StatsAxiom::new(self.header.version, self.header.quality.unwrap_or(ItemQuality::Normal), true);
            return self.properties.iter()
                .find(|p| axiom.map_alpha_id(p.stat_id) == 31)
                .map(|p| p.value as u32);
        }
        None
    }
    pub fn max_durability(&self) -> Option<u32> { 
        if let Some(d) = self.body.max_durability { return Some(d); }
        if self.header.save_is_alpha {
            let axiom = StatsAxiom::new(self.header.version, self.header.quality.unwrap_or(ItemQuality::Normal), true);
            return self.properties.iter()
                .find(|p| axiom.map_alpha_id(p.stat_id) == 73)
                .map(|p| p.value as u32);
        }
        None
    }
    pub fn current_durability(&self) -> Option<u32> { 
        if let Some(d) = self.body.current_durability { return Some(d); }
        if self.header.save_is_alpha {
            let axiom = StatsAxiom::new(self.header.version, self.header.quality.unwrap_or(ItemQuality::Normal), true);
            return self.properties.iter()
                .find(|p| axiom.map_alpha_id(p.stat_id) == 72)
                .map(|p| p.value as u32);
        }
        None
    }
    pub fn quantity(&self) -> Option<u32> { 
        if let Some(d) = self.body.quantity { return Some(d); }
        if self.header.save_is_alpha {
            let axiom = StatsAxiom::new(self.header.version, self.header.quality.unwrap_or(ItemQuality::Normal), true);
            return self.properties.iter()
                .find(|p| axiom.map_alpha_id(p.stat_id) == 70)
                .map(|p| p.value as u32);
        }
        None
    }

    pub fn query_bit(&self, offset: u64) -> Option<BitSemantic> {
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

        let mut best_segment: Option<&BitSegment> = None;
        for seg in &self.segments {
            if offset >= seg.start && offset < seg.end {
                if let Some(best) = best_segment {
                    if seg.depth > best.depth { best_segment = Some(seg); }
                } else { best_segment = Some(seg); }
            }
        }
        if let Some(seg) = best_segment { return Some(BitSemantic { label: seg.label.clone() }); }
        for child in &self.socketed_items {
            if let Some(semantic) = child.query_bit(offset) {
                return Some(BitSemantic { label: format!("{} -> {}", self.body.code.trim(), semantic.label) });
            }
        }
        None
    }

    pub fn set_placement(&mut self, placement: crate::domain::vo::InventoryPlacement) {
        self.header.x = placement.coordinate().x();
        self.header.y = placement.coordinate().y();
        self.body.x = self.header.x;
        self.body.y = self.header.y;
        self.bits.clear();
    }

    pub fn set_property_value(&mut self, stat_id: u32, value: crate::domain::vo::ItemStatValue) -> bool {
        let mut found = false;
        // Alpha-aware stat mapping
        let is_alpha = self.header.version == 5 || self.header.version == 6 || self.header.version == 1;
        let axiom = crate::domain::stats::axiom::StatsAxiom::new(self.header.version, self.header.quality.unwrap_or(crate::domain::item::ItemQuality::Normal), is_alpha);

        {
            let mut lists = Vec::new();
            lists.push(&mut self.properties);
            for list in &mut self.set_attributes { lists.push(list); }
            lists.push(&mut self.runeword_attributes);
            for list in lists.into_iter() {
                for prop in list {
                    let effective_id = axiom.map_alpha_id(prop.stat_id);
                    if effective_id == stat_id {
                        let cost = crate::data::stat_costs::STAT_COSTS.iter().find(|s| s.id == effective_id);
                        if let Some(c) = cost {
                            prop.value = value.value();
                            prop.raw_value = value.value().wrapping_add(c.save_add);
                            found = true;
                        }
                    }
                }
            }
        }
        if found { self.bits.clear(); }
        found
    }

    pub fn empty_for_tests() -> Self {
        let mut item = Self::default();
        item.body.code = "    ".to_string();
        item.code = "    ".to_string();
        item
    }

    pub fn set_defense(&mut self, val: Option<u32>) {
        self.body.defense = val;
        self.defense = val;
        self.bits.clear();
    }

    pub fn set_durability(&mut self, current: Option<u32>, max: Option<u32>) {
        self.body.current_durability = current;
        self.current_durability = current;
        self.body.max_durability = max;
        self.max_durability = max;
        self.bits.clear();
    }

    pub fn set_quantity(&mut self, val: Option<u32>) {
        self.body.quantity = val;
        self.quantity = val;
        self.bits.clear();
    }

    pub fn set_id(&mut self, val: Option<u32>) {
        self.header.id = val;
        self.bits.clear();
    }

    pub fn set_level(&mut self, val: Option<u8>) {
        self.header.level = val;
        self.bits.clear();
    }

    /// Synchronizes the item's internal state with its socketed children.
    /// This ensures that num_socketed_items, the socketed flag, and relevant property markers stay coherent.
    pub fn sync_socket_payload(&mut self) {
        self.num_socketed_items = self.socketed_items.len() as u8;
        
        // Ensure the total socket count is at least as large as the number of items currently in them.
        if let Some(s) = self.sockets {
            if s < self.num_socketed_items {
                self.sockets = Some(self.num_socketed_items);
            }
        } else if self.num_socketed_items > 0 {
            self.sockets = Some(self.num_socketed_items);
        }
        
        // The is_socketed flag in the header should reflect whether the item HAS sockets,
        // regardless of whether they are filled.
        let has_sockets = self.sockets.unwrap_or(0) > 0;
        self.header.is_socketed = has_sockets;
        
        // Alpha-aware flag synchronization
        let is_alpha = self.header.version == 5 || self.header.version == 6 || self.header.version == 1;

        // Synchronize flags bit
        if has_sockets {
            if is_alpha {
                if self.header.version == 5 { 
                    self.header.flags |= 1 << 11; 
                    self.header.flags &= !(1 << 23); // Ensure NOT compact
                }
                else { self.header.flags |= 1 << 11; }
            } else {
                self.header.flags |= 1 << 11;
            }
            self.header.flags |= 1 << 4; // Identified
        } else {
            if is_alpha && self.header.version == 5 { self.header.flags &= !(1 << 11); }
            else { self.header.flags &= !(1 << 11); }
        }

        // Ensure we have enough Stat 317/320 properties to hold the socketed items.
        // For simplicity, we'll use Stat 317 (recursive) as the default for added items.
        let mut nested_prop_count = 0;
        let axiom = crate::domain::stats::axiom::StatsAxiom::new(self.header.version, self.header.quality.unwrap_or(crate::domain::item::ItemQuality::Normal), is_alpha);

        for prop in &self.properties {
            let effective_id = axiom.map_alpha_id(prop.stat_id);
            if effective_id == 317 || effective_id == 320 {
                nested_prop_count += 1;
            }
        }

        while nested_prop_count < self.num_socketed_items {
            self.properties.push(ItemProperty {
                stat_id: 317, // Use 317 for recursive
                name: "item_socket_child".to_string(),
                param: 0,
                raw_value: 0,
                value: 0,
                range: ItemBitRange::default(),
            });
            nested_prop_count += 1;
        }

        // Sync with stats field
        self.stats.properties = self.properties.clone();

        // In Alpha v105, nested items in properties (Stat 317/320) often require 
        // a 1:1 mapping with the socketed_items collection during serialization.
        // Clearing bits ensures that the re-serializer will rebuild the bitstream 
        // from the current properties and child collection.
        self.bits.clear();
    }

    /// Sets the maximum number of sockets for the item and updates the socketed flag.
    pub fn set_sockets(&mut self, count: u8) {
        self.sockets = Some(count);
        self.header.is_socketed = count > 0;
        self.bits.clear();
    }

    /// Adds a child item to the sockets and synchronizes the payload state.
    pub fn add_socketed_item(&mut self, child: Item) {
        self.socketed_items.push(child);
        self.sync_socket_payload();
    }

    pub fn prefixes(&self) -> Vec<&'static crate::data::item_specs::Affix> {
        let mut result = Vec::new();
        if let Some(id) = self.magic_prefix {
            if let Some(affix) = crate::data::affixes::PREFIXES.iter().find(|a| a.id == id as u32) { result.push(affix); }
        }
        for i in [0, 2, 4] {
            if let Some(id) = self.rare_affixes[i] {
                if let Some(affix) = crate::data::affixes::PREFIXES.iter().find(|a| a.id == id as u32) { result.push(affix); }
            }
        }
        result
    }

    pub fn suffixes(&self) -> Vec<&'static crate::data::item_specs::Affix> {
        let mut result = Vec::new();
        if let Some(id) = self.magic_suffix {
            if let Some(affix) = crate::data::affixes::SUFFIXES.iter().find(|a| a.id == id as u32) { result.push(affix); }
        }
        for i in [1, 3, 5] {
            if let Some(id) = self.rare_affixes[i] {
                if let Some(affix) = crate::data::affixes::SUFFIXES.iter().find(|a| a.id == id as u32) { result.push(affix); }
            }
        }
        result
    }

    pub fn to_bytes(&self, idx: usize, huffman: &crate::domain::item::serialization::HuffmanTree, alpha_mode: bool) -> io::Result<Vec<u8>> {
        use crate::domain::item::serialization::BitEmitter;
        let mut emitter = BitEmitter::new();
        self.to_emitter(idx, &mut emitter, huffman, alpha_mode)?;
        Ok(emitter.into_bytes())
    }

    pub fn to_bits(&self, idx: usize, huffman: &crate::domain::item::serialization::HuffmanTree, alpha_mode: bool) -> io::Result<Vec<bool>> {
        use crate::domain::item::serialization::BitEmitter;
        let mut emitter = BitEmitter::new();
        self.to_emitter(idx, &mut emitter, huffman, alpha_mode)?;
        Ok(emitter.into_bits())
    }

    pub fn to_emitter(&self, idx: usize, emitter: &mut crate::domain::item::serialization::BitEmitter, huffman: &crate::domain::item::serialization::HuffmanTree, alpha_mode: bool) -> io::Result<()> {
        let start_bit = emitter.written_bits();
        // Slice 2: Opaque pass-through
        for module in &self.modules {
            match module {
                ItemModule::Opaque(bits) | ItemModule::Residue(bits) => {
                    emitter.extend_bits(bits.iter().cloned())?;
                    return Ok(());
                }
                _ => {}
            }
        }

        use crate::domain::item::serialization::write_player_name;
        emitter.write_bits(self.header.flags, 32)?;
        if alpha_mode && self.header.has_checksum {
            let checksum = calculate_alpha_v105_checksum(self.header.flags, self.header.version);
            emitter.write_bits(checksum as u32, 8)?;
        }
        emitter.write_bits(self.header.version as u32, 3)?;
        emitter.write_bits(self.header.mode as u32, 3)?;
        emitter.write_bits(self.header.location as u32, 3)?;
        emitter.write_bits(self.header.x as u32, 4)?;
        
        let s_axiom = StatsAxiom::new(self.header.version, self.header.quality.unwrap_or(ItemQuality::Normal), alpha_mode)
            .with_index(idx)
            .with_personalization(self.header.is_personalized)
            .with_code(&self.code)
            .with_compact(self.header.is_compact);

        let h_axiom = HeaderAxiom::new(self.header.version, alpha_mode);
        let geometry = h_axiom.header_geometry(self.header.flags, Some(&self.code));

        if alpha_mode && self.header.save_is_alpha {
            let is_v105_shadow = h_axiom.is_v105_shadow(self.header.flags);
            let is_rw = h_axiom.is_runeword(self.header.flags, Some(&self.code));

            // Alpha v105 Forensic: Always write geometry bits for standard Alpha items
            if !is_v105_shadow && !is_rw && !geometry.skip_geometry {
                emitter.write_bits(self.header.y as u32, geometry.y_bits)?;
                emitter.write_bits(self.header.page as u32, geometry.page_bits)?;
                emitter.write_bits(self.header.socket_hint as u32, geometry.socket_hint_bits)?;
            }

            if geometry.target_width > 0 {
                let current_bits = emitter.written_bits() - start_bit;
                if current_bits < geometry.target_width as u64 {
                    let to_write = (geometry.target_width as u64 - current_bits) as u32;
                    if !self.body.alpha_header_gap_bits.is_empty() {
                         for &bit in &self.body.alpha_header_gap_bits { emitter.write_bit(bit)?; }
                    } else {
                         emitter.write_bits(self.body.alpha_header_gap.unwrap_or(0), to_write)?;
                    }
                }
            } else if geometry.has_header_gap || (h_axiom.is_alpha() && !self.header.has_checksum && self.header.version == 5) {
                if !self.body.alpha_header_gap_bits.is_empty() {
                    for &bit in &self.body.alpha_header_gap_bits { emitter.write_bit(bit)?; }
                } else {
                    let gap_len = V105HeaderGapAxiom::default().resolve_gap(self.header.version, Some(&self.code), self.header.flags, idx == 0, self.header.is_compact, self.header.has_checksum);
                    if gap_len > 0 {
                        emitter.write_bits(self.body.alpha_header_gap.unwrap_or(0), gap_len as u32)?;
                    }
                }
            }
        } else {
             if !geometry.skip_geometry {
                emitter.write_bits(self.header.y as u32, geometry.y_bits)?;
                emitter.write_bits(self.header.page as u32, geometry.page_bits)?;
                emitter.write_bits(self.header.socket_hint as u32, geometry.socket_hint_bits)?;
            }
        }

        // Slice 4: Check for SemiOpaque body preservation
        for module in &self.modules {
            if let ItemModule::SemiOpaque { body_bits, .. } = module {
                emitter.extend_bits(body_bits.iter().cloned())?;
                return Ok(());
            }
        }

        // Alpha v105 forensic: Shadow and blank items are header-only. (Exit after gap)
        // EXCEPT for Alpha equipment which might have property residue/nudges. (Axiom 0365)
        let is_header_only = s_axiom.is_header_only(self.header.flags, &self.code);
        let is_v105_blank = alpha_mode && self.code.trim().is_empty();

        if is_header_only && (is_v105_blank || !(alpha_mode && (self.header.version == 0 || self.header.version == 1 || self.header.version == 2 || self.header.version == 5))) {
            let current_bits = emitter.written_bits();
            let mut final_bits = s_axiom.calculate_alignment(current_bits - start_bit, &self.code, self.header.flags);
            if self.total_bits > final_bits { final_bits = self.total_bits; }
            

            if final_bits > (current_bits - start_bit) {
                let padding_needed = (final_bits - (current_bits - start_bit)) as u32;
                if !self.body.alpha_alignment_padding.is_empty() { 
                    for &bit in &self.body.alpha_alignment_padding { emitter.write_bit(bit)?; } 
                } else { 
                    emitter.write_bits(0, padding_needed)?; 
                }
            }
            return Ok(());
        }

        if self.header.is_ear {
            emitter.write_bits(self.ear_class.unwrap_or(0) as u32, 3)?;
            emitter.write_bits(self.ear_level.unwrap_or(0) as u32, 7)?;
            write_player_name(emitter, self.ear_player_name.as_deref().unwrap_or(""), alpha_mode && self.header.version == 5)?;
            if alpha_mode && self.header.version == 5 { emitter.byte_align()?; }
        } else if s_axiom.code_encoding() == crate::domain::stats::axiom::CodeEncoding::Ascii3x8 {
            let trimmed = self.code.trim();
            let chars: Vec<char> = trimmed.chars().collect();
            for i in 0..3 {
                let ch = if i < chars.len() { chars[i] as u32 } else { 0 };
                for bit in 0..8 {
                    emitter.write_bit((ch & (1 << bit)) != 0)?;
                }
            }
        } else {
            let encoded_code = huffman.encode(&self.code)?;
            emitter.extend_bits(encoded_code)?;
            if h_axiom.is_alpha() && (self.header.version == 5 || self.header.version == 0 || self.header.version == 1) && !s_axiom.is_compact {
                let nudge = self.body.alpha_nudge.unwrap_or(0);
                emitter.write_bits(nudge as u32, 2)?;
            }
        }

        if !s_axiom.is_compact || (alpha_mode && (self.header.version == 0 || self.header.version == 1 || self.header.version == 2)) {
            let quality_val = self.header.quality.unwrap_or(ItemQuality::Normal);
            let is_item_alpha = s_axiom.is_alpha();

            if is_item_alpha && !s_axiom.is_compact {
                let quality_to_write = self.alpha_quality_raw.unwrap_or(quality_val as u8);
                emitter.write_bits(quality_to_write as u32, 3)?;
                if (self.header.version == 5 || self.header.version == 6 || self.header.version == 7) && (s_axiom.is_runeword(self.header.flags) || h_axiom.is_v105_shadow(self.header.flags)) {
                    emitter.write_bits(self.body.v5_runeword_extra.unwrap_or(0) as u32, 2)?;
                }
            }

            if !is_item_alpha {
                emitter.write_bits(self.id.unwrap_or(0), 32)?;
                emitter.write_bits(self.level.unwrap_or(0) as u32, 7)?;
                emitter.write_bits(quality_val as u32, 4)?;
            }

            if !(is_item_alpha && (self.header.version == 4 || self.header.version == 6 || self.header.version == 7)) {
                if self.has_multiple_graphics { emitter.write_bits(self.multi_graphics_bits.unwrap_or(0) as u32, 3)?; }
                if self.has_class_specific_data { emitter.write_bits(self.class_specific_bits.unwrap_or(0) as u16 as u32, 11)?; }
                match quality_val {
                    ItemQuality::Low | ItemQuality::High => { emitter.write_bits(self.low_high_graphic_bits.unwrap_or(0) as u32, 3)?; }
                    ItemQuality::Magic => {
                        emitter.write_bits(self.magic_prefix.unwrap_or(0) as u32, 11)?;
                        emitter.write_bits(self.magic_suffix.unwrap_or(0) as u32, 11)?;
                    }
                    ItemQuality::Rare | ItemQuality::Crafted => {
                        emitter.write_bits(self.rare_name_1.unwrap_or(0) as u32, 8)?;
                        emitter.write_bits(self.rare_name_2.unwrap_or(0) as u32, 8)?;
                        for i in 0..6 {
                            if let Some(affix) = self.rare_affixes[i] {
                                emitter.write_bit(true)?; emitter.write_bits(affix as u32, 11)?;
                            } else { emitter.write_bit(false)?; }
                        }
                    }
                    ItemQuality::Set | ItemQuality::Unique => {
                        let uid = if alpha_mode { self.alpha_unique_id_raw.unwrap_or(self.unique_id.unwrap_or(0)) } else { self.unique_id.unwrap_or(0) };
                        emitter.write_bits(uid as u32, 12)?;
                    }
                    _ => {}
                }
                if s_axiom.is_runeword(self.header.flags) && !is_item_alpha && self.header.version != 5 {
                    emitter.write_bits(self.runeword_id.unwrap_or(0) as u32, 12)?;
                    emitter.write_bits(self.runeword_level.unwrap_or(0) as u32, 12)?;
                    emitter.write_bits(0, 4)?;
                }
                if self.header.is_personalized {
                    if alpha_mode && (self.header.version == 5 || self.header.version == 0 || self.header.version == 1) { emitter.byte_align()?; }
                    write_player_name(emitter, self.personalized_player_name.as_deref().unwrap_or(""), alpha_mode && (self.header.version == 5 || self.header.version == 0 || self.header.version == 1))?;
                }
                if !s_axiom.is_compact {
                    if self.code.trim() == "tbk" || self.code.trim() == "ibk" { emitter.write_bits(self.tbk_ibk_teleport.unwrap_or(0) as u32, 5)?; }
                    emitter.write_bit(self.timestamp_flag)?;
                }
                let template = crate::domain::item::serialization::item_template(&self.code);
                let (reads_def, reads_dur, reads_qty) = if let Some(t) = template { (t.is_armor, t.has_durability, t.is_stackable) } else { (false, false, false) };
                if reads_def && s_axiom.reads_defense() { emitter.write_bits(self.defense.unwrap_or(0), 11)?; }
                if reads_dur && s_axiom.reads_durability() {
                    let m_dur = self.max_durability.unwrap_or(0);
                    emitter.write_bits(m_dur, 8)?;
                    if m_dur > 0 { emitter.write_bits(self.current_durability.unwrap_or(0), 9)?; emitter.write_bit(false)?; }
                }
                if reads_qty && s_axiom.reads_quantity() { emitter.write_bits(self.quantity.unwrap_or(0), 9)?; }
                if self.header.is_socketed { emitter.write_bits(self.sockets.unwrap_or(0) as u32, 4)?; }
                if quality_val == ItemQuality::Set {
                    let val = self.body.alpha_set_list_val.unwrap_or(match self.set_list_count { 1 => 1, 2 => 3, 3 => 7, 4 => 15, 5 => 31, _ => 0 });
                    emitter.write_bits(val as u32, 5)?;
                }
                let is_shadow = s_axiom.is_v105_shadow(self.header.flags);
                if is_shadow {
                    if let Some(bits) = self.body.alpha_shadow_skip_bits { emitter.write_bits_u64(bits, 47)?; } else { emitter.write_bits(0, 47)?; }
                }
                let is_summary = crate::domain::item::serialization::is_v105_summary_code(&self.code);
                if self.header.version != 5 || is_shadow || self.header.is_runeword || (alpha_mode && s_axiom.is_compact && !is_summary) || !self.properties.is_empty() {
                    // Slice 11: Write JM-to-Body alignment gap
                    let gap_len = s_axiom.header_gap(&self.code, self.header.flags);
                    if gap_len > 0 {
                        if !self.body.alpha_body_gap_bits.is_empty() {
                            for &bit in &self.body.alpha_body_gap_bits {
                                emitter.write_bit(bit)?;
                            }
                        } else {
                            emitter.write_bits(0, gap_len)?;
                        }
                    }
                    crate::domain::item::serialization::write_property_list(emitter, &self.code, &self.properties, &self.socketed_items, huffman, self.header.version, self.header.is_runeword, self.terminator_bit, self.properties_complete, quality_val, is_shadow, &s_axiom)?;
                    for set_props in &self.set_attributes {
                        crate::domain::item::serialization::write_property_list(emitter, &self.code, set_props, &[], huffman, self.header.version, false, false, true, quality_val, false, &s_axiom)?;
                    }
                }
            }
        }
        if !alpha_mode && self.header.version != 5 && self.header.version != 7 { emitter.write_bit(false)?; }
        let current_bits = emitter.written_bits();
        let mut final_bits = s_axiom.calculate_alignment(
            current_bits - start_bit,
            &self.code,
            self.header.flags,
        );
        
        // Slice 8: Targeted Length Oracle Support
        // If the item's recorded total_bits is greater than the calculated alignment (via D2R_FORCE_LENGTH),
        // respect the physical evidence and pad to the recorded length.
        if self.total_bits > final_bits {
            final_bits = self.total_bits;
        }

        if final_bits > (current_bits - start_bit) {
            let padding_needed = (final_bits - (current_bits - start_bit)) as u32;
            if !self.body.alpha_alignment_padding.is_empty() { for &bit in &self.body.alpha_alignment_padding { emitter.write_bit(bit)?; } }
            else { emitter.write_bits(0, padding_needed)?; }
        }
        Ok(())
    }

    pub fn serialize_section(items: &[Item], huffman: &crate::domain::item::serialization::HuffmanTree, alpha_mode: bool) -> io::Result<Vec<u8>> {
        use crate::domain::item::serialization::BitEmitter;
        let mut emitter = BitEmitter::new();
        for (i, item) in items.iter().enumerate() {
            if !alpha_mode {
                emitter.extend_bits(item.gap_bits.iter().cloned())?;
            }
            if alpha_mode {
                // Alpha v105 Forensic: Items are bit-packed without byte alignment between them.
                // The gap bits are already included in the item's own bitstream via to_emitter.
                let item_bits = item.to_bits(i, huffman, alpha_mode)?;
                emitter.extend_bits(item_bits)?;
            } else {
                let item_bytes = item.to_bytes(i, huffman, alpha_mode)?;
                for byte in item_bytes { emitter.write_bits(byte as u32, 8)?; }
            }
            let axiom = StatsAxiom::new(item.header.version, item.header.quality.unwrap_or(ItemQuality::Normal), alpha_mode);
            for child in &item.socketed_items {
                if alpha_mode && axiom.is_alpha() {
                    // Alpha v105 socketed items (Runewords) are embedded in properties (Stat 317/320).
                    // Avoid double-writing here.
                    continue;
                }
                if alpha_mode { 
                    emitter.write_bits(2, 2)?; 
                    let child_bits = child.to_bits(0, huffman, alpha_mode)?; // Sockets use idx 0 as per parsing
                    emitter.extend_bits(child_bits)?;
                } else {
                    let child_bytes = child.to_bytes(0, huffman, alpha_mode)?;
                    for byte in child_bytes { emitter.write_bits(byte as u32, 8)?; }
                }
            }
        }
        Ok(emitter.into_bytes())
    }
}

pub fn parse_item_header<R: BitRead>(
    cursor: &mut BitCursor<R>,
    alpha_mode: bool,
    code_hint: Option<&str>,
    gap_override: Option<usize>,
    is_first_item: bool,
    forced_compact: Option<bool>,
) -> ParsingResult<(ItemHeader, Option<u32>, Vec<bool>)> {
    let start_bit = cursor.pos();
    cursor.begin_segment(ItemSegmentType::Header);
    let flags = cursor.read_bits::<u32>(32)?;
    if !alpha_mode && (flags & 0xFFFF) != 0x4D4A {
         return Err(cursor.fail(ParsingError::MissingMarker { marker: "JM".to_string(), bit_offset: start_bit }));
    }
    let (version, has_checksum) = if alpha_mode {
        let saved_pos = cursor.checkpoint();
        let checksum_res = cursor.read_bits::<u8>(8);
        let v_res = cursor.read_bits::<u8>(3);

        if let (Ok(checksum), Ok(v)) = (checksum_res, v_res) {
            let expected = calculate_alpha_v105_checksum(flags, v);
            if checksum == expected {
                (v, true)
            } else {
                cursor.rollback(saved_pos);
                let v = cursor.read_bits::<u8>(3)? as u8;
                // Forensic: Support Version 5 items without checksum (e.g. in amazon_empty.d2s)
                (v, false)
            }
        } else {
            cursor.rollback(saved_pos);
            (cursor.read_bits::<u8>(3)? as u8, false)
        }
    } else {
        (cursor.read_bits::<u8>(3)? as u8, false)
    };
    let mode = cursor.read_bits::<u8>(3)? as u8;
    let location = cursor.read_bits::<u8>(3)? as u8;
    let x = cursor.read_bits::<u8>(4)? as u8;

    let h_axiom = HeaderAxiom::new(version, alpha_mode);
    let s_axiom = StatsAxiom::new(version, ItemQuality::Normal, alpha_mode);
    let is_compact = forced_compact.unwrap_or_else(|| h_axiom.is_compact(flags, code_hint));
    let is_personalized = s_axiom.is_personalized(flags);
    let is_rw_initial = h_axiom.is_runeword(flags, code_hint);
    
    let mut y = 0; let mut page = 0; let mut socket_hint = 0;
    let geometry = h_axiom.header_geometry(flags, code_hint);
    let mut alpha_header_gap = None;
    let mut alpha_header_gap_bits = Vec::new();
    if geometry.has_header_gap {
        if h_axiom.is_alpha() {
            let is_v105_shadow = h_axiom.is_v105_shadow(flags);
            let is_rw = h_axiom.is_runeword(flags, code_hint);
            
            let gap_bits = if is_rw || is_v105_shadow {
                let is_v105_shadow_local = (flags & (1 << 26)) != 0 || (flags & (1 << 27)) != 0;
                if is_v105_shadow_local { 8 } else { 24 }
            } else {
                let geom_bits = (geometry.y_bits + geometry.page_bits + geometry.socket_hint_bits) as usize;
                if let Some(go) = gap_override {
                    if go >= geom_bits { go - geom_bits } else { 8 }
                } else {
                    V105HeaderGapAxiom::default().resolve_gap(version, code_hint, flags, is_first_item, is_compact, has_checksum)
                }
            };

            alpha_header_gap_bits = cursor.with_context("AlphaHeaderGap", |c| c.read_bits_as_vec(gap_bits as u32))?;
            
            // Forensic: Byte-align after header gap for Version 5 to fix body parsing desync
            if version == 5 {
                cursor.byte_align()?;
            }

            let mut gap = 0u32;
            for (i, &bit) in alpha_header_gap_bits.iter().enumerate() {
                if i < 32 && bit { gap |= 1 << i; }
            }
            alpha_header_gap = Some(gap);

            // Alpha v105 Forensic: Equipment coordinates (y, page, socket_hint) 
            // are packed into the header gap rather than being separate bitfields.
            if !is_compact && gap_bits >= 8 {
                y = (gap & 0x0F) as u8; 
                page = ((gap >> 4) & 0x07) as u8; 
                socket_hint = ((gap >> 7) & 0x01) as u8; 
            }
        } else {
            if !is_compact { y = cursor.read_bits::<u8>(geometry.y_bits)? as u8; page = cursor.read_bits::<u8>(geometry.page_bits)? as u8; socket_hint = cursor.read_bits::<u8>(geometry.socket_hint_bits)? as u8; }
            alpha_header_gap_bits = cursor.with_context("AlphaHeaderGap", |c| c.read_bits_as_vec(8))?;
            let mut val = 0u32;
            for (i, &bit) in alpha_header_gap_bits.iter().enumerate() { if bit { val |= 1 << i; } }
            alpha_header_gap = Some(val);
        }
    } else if !geometry.skip_geometry {
        y = cursor.read_bits::<u8>(geometry.y_bits)? as u8; page = cursor.read_bits::<u8>(geometry.page_bits)? as u8; socket_hint = cursor.read_bits::<u8>(geometry.socket_hint_bits)? as u8;
    }

    if alpha_mode && geometry.target_width > 0 {
        let current_bits = (cursor.pos() - start_bit) as u32;
        if current_bits < geometry.target_width {
            let to_read = geometry.target_width - current_bits;
            // Forensic: Ensure we don't read past the available bits if target_width is overestimated.
            let available = cursor.remaining() as u32;
            let actual_read = std::cmp::min(to_read, available);
            if actual_read > 0 {
                let bits = cursor.with_context("AlphaHeaderGapPadding", |c| c.read_bits_as_vec(actual_read))?;
                for b in bits {
                    alpha_header_gap_bits.push(b);
                }
                let mut val = 0u32;
                for (i, &bit) in alpha_header_gap_bits.iter().enumerate() { 
                    if i < 32 && bit { val |= 1 << i; } 
                }
                alpha_header_gap = Some(val);
            }
        }
    }
    cursor.end_segment();
    Ok((ItemHeader {
        flags, version, mode, location, x, y, page, socket_hint, id: None, level: None, quality: None, is_compact,
        is_identified: s_axiom.is_identified(flags), is_socketed: s_axiom.is_socketed(flags, is_compact), is_personalized,
        is_runeword: s_axiom.is_runeword(flags), is_ethereal: s_axiom.is_ethereal(flags), is_ear: (flags & (1 << 24)) != 0,
        has_checksum,
        alpha_quality_raw: None, alpha_v5_runeword_extra: None, alpha_unique_id_raw: None,
        save_is_alpha: alpha_mode,
    }, alpha_header_gap, alpha_header_gap_bits))
}

pub fn parse_item_body<R: BitRead>(
    cursor: &mut BitCursor<R>,
    huff: &crate::domain::item::serialization::HuffmanTree,
    header: &ItemHeader,
    alpha_mode: bool,
) -> ParsingResult<(ItemBody, Option<u8>, Option<u8>, Option<String>)> {
    let h_axiom = HeaderAxiom::new(header.version, alpha_mode);
    let is_ear = header.is_ear;
    let (code, alpha_nudge, ear_class, ear_level, ear_player_name) = if is_ear {
        cursor.begin_segment(ItemSegmentType::Unknown);
        let class = Some(cursor.read_bits::<u8>(3)? as u8);
        let level = Some(cursor.read_bits::<u8>(7)? as u8);
        let name = Some(crate::domain::item::serialization::read_player_name(cursor, alpha_mode && header.version == 5)?);
        if alpha_mode && header.version == 5 { cursor.byte_align()?; }
        cursor.end_segment();
        (String::new(), None, class, level, name)
    } else {
        cursor.begin_segment(ItemSegmentType::Code);
        let mut code = String::new();
        let s_axiom = StatsAxiom::new(header.version, ItemQuality::Normal, alpha_mode).with_compact(header.is_compact);
        
        if alpha_mode && header.is_compact && s_axiom.code_encoding() == crate::domain::stats::axiom::CodeEncoding::Ascii3x8 {
            // Forensic (Axiom 0344): Compact items in Alpha v105 
            // use 3x8-bit fixed width characters for the item code.
            for _ in 0..3 {
                let mut ch = 0u8;
                for bit in 0..8 {
                    if cursor.read_bit()? { ch |= 1 << bit; }
                }
                code.push(ch as char);
            }
        } else {
            if alpha_mode {
                let saved_pos = cursor.pos();
                if let Ok(bits) = cursor.read_bits_as_vec(24) {
                    if let Some(stealth) = crate::domain::forensic::v105::axioms::V105StealthCodeAxiom::default().resolve_stealth_code(&bits) {
                        code = stealth.to_string();
                    } else {
                        cursor.rollback(saved_pos);
                    }
                } else {
                    cursor.rollback(saved_pos);
                }
            }

            if code.is_empty() {
                for i in 0..4 {
                    match huff.decode_recorded(cursor) {
                        Ok(ch) => code.push(ch),
                    Err(e) => {
                        if alpha_mode && i >= 1 {
                            // Trial: 1-bit and 2-bit lookahead nudges (Axiom 0340) for Alpha v105 bitstream drift
                            let saved_pos = cursor.pos();
                            // Try 1-bit nudge
                            if let Ok(_) = cursor.read_bit() {
                                if let Ok(ch) = huff.decode_recorded(cursor) {
                                    code.push(ch);
                                    continue;
                                }
                            }
                            // Try 2-bit nudge
                            cursor.rollback(saved_pos);
                            if let Ok(_) = cursor.read_bits::<u8>(2) {
                                if let Ok(ch) = huff.decode_recorded(cursor) {
                                    code.push(ch);
                                    continue;
                                }
                            }
                            cursor.rollback(saved_pos);
                        }
                        return Err(e);
                    }
                }
            }
        }
    }
    let mut alpha_nudge = None;
    if alpha_mode {
        // Forensic: Apply 2-bit alignment nudge for Version 5 item bodies
        if h_axiom.is_alpha() && !header.is_compact {
            if header.version == 5 {
                let nudge_val = cursor.read_bits::<u8>(2)?;
                alpha_nudge = Some(nudge_val);
            } else if header.version == 0 || header.version == 1 { 
                alpha_nudge = Some(cursor.with_context("AlphaNudge", |c| c.read_bits::<u8>(2))?); 
            }
        }
    }
        
        let _props_start = cursor.pos();

        // Forensic: Ensure byte-alignment after body properties for Version 5 to resolve drift
        if header.version == 5 && !header.is_compact {
            cursor.byte_align()?;
        }

        cursor.end_segment();
        (code, alpha_nudge, None, None, None)
    };
Ok((ItemBody {
code, x: header.x, y: header.y, page: header.page, location: header.location, mode: header.mode,
defense: None, max_durability: None, current_durability: None, quantity: None, alpha_header_gap: None, 
alpha_header_gap_bits: Vec::new(),
v5_runeword_extra: None, v105_7mgw_payload: None, alpha_nudge, alpha_set_list_val: None, alpha_shadow_skip_bits: None, 
alpha_body_gap_bits: Vec::new(), alpha_alignment_padding: Vec::new(),
}, ear_class, ear_level, ear_player_name))
}
impl ExtendedStatsData {
    pub fn read_from_cursor<R: BitRead>(
        cursor: &mut BitCursor<R>,
        code: &str,
        header: &ItemHeader,
        alpha_mode: bool,
        axiom: &StatsAxiom,
    ) -> ParsingResult<Self> {
        cursor.begin_segment(ItemSegmentType::ExtendedStats);
        let mut data = Self::default();
        let trimmed_code = code.trim();
        let version = header.version;
        let is_compact = header.is_compact;
        let is_socketed_flag = header.is_socketed;
        let is_runeword = header.is_runeword;
        let is_personalized = header.is_personalized;
        let h_axiom = HeaderAxiom::new(version, alpha_mode);
        let is_fragment = h_axiom.is_alpha() && (version == 5 || version == 2 || version == 1) && ((header.flags & (1 << 26)) != 0 || (header.flags & (1 << 27)) != 0) ;
        let is_alpha_early_exit = h_axiom.is_alpha() && (version == 4 || version == 6 || version == 7);
        if axiom.is_alpha() {
            if !is_compact {
                let quality_raw = cursor.read_bits::<u8>(3)?;
                let quality = ItemQuality::from(quality_raw);
                data.alpha_quality_raw = Some(quality_raw);
                data.quality = Some(quality);

                if (version == 5 || version == 6 || version == 7) && (is_runeword || is_fragment || h_axiom.is_v105_shadow(header.flags)) {
                    data.v5_runeword_extra = Some(cursor.with_context("AlphaV5RunewordExtra", |c| c.read_bits::<u8>(2))?);
                    data.id = Some(0);
                } else if version == 5 && crate::domain::item::serialization::is_v105_summary_code(trimmed_code) {
                    cursor.end_segment();
                    return Ok(data);
                } else { 
                    data.id = Some(0); 
                }
            } else { data.id = Some(0); }
        } else {
            data.id = Some(cursor.read_bits::<u32>(32)?);
            data.level = Some(cursor.read_bits::<u8>(7)?);
            let quality_raw = cursor.read_bits::<u8>(4)?;
            data.quality = Some(ItemQuality::from(quality_raw));
        }
        // Version 2 remains as early exit for now if confirmed. 
        // Version 1 and 4 are removed from early exit to allow stats/sockets parsing.
        if is_alpha_early_exit { cursor.end_segment(); return Ok(data); }
        if data.has_multiple_graphics { data.multi_graphics_bits = Some(cursor.read_bits::<u8>(3)? as u8); }
        if data.has_class_specific_data { data.class_specific_bits = Some(cursor.read_bits::<u16>(11)? as u16); }
        let quality_val = data.quality.unwrap_or(ItemQuality::Normal);
        match quality_val {
            ItemQuality::Low | ItemQuality::High => { data.low_high_graphic_bits = Some(cursor.read_bits::<u8>(3)? as u8); }
            ItemQuality::Magic => { data.magic_prefix = Some(cursor.read_bits::<u16>(11)? as u16); data.magic_suffix = Some(cursor.read_bits::<u16>(11)? as u16); }
            ItemQuality::Rare | ItemQuality::Crafted => {
                data.rare_name_1 = Some(cursor.read_bits::<u8>(8)? as u8); data.rare_name_2 = Some(cursor.read_bits::<u8>(8)? as u8);
                for i in 0..6 { if cursor.read_bit()? { data.rare_affixes[i] = Some(cursor.read_bits::<u16>(11)? as u16); } }
            }
            ItemQuality::Set | ItemQuality::Unique => { 
                let uid = cursor.read_bits::<u16>(12)? as u16;
                if alpha_mode { data.alpha_unique_id_raw = Some(uid); }
                data.unique_id = Some(uid); 
            }
            _ => {}
        }
        if is_runeword && !is_fragment && !axiom.is_alpha() && version != 5 { data.runeword_id = Some(cursor.read_bits::<u16>(12)? as u16); data.runeword_level = Some(cursor.read_bits::<u8>(4)? as u8); }
        if is_personalized { 
            if alpha_mode && (version == 5 || version == 0 || version == 1) { cursor.byte_align()?; }
            data.personalized_player_name = Some(crate::domain::item::serialization::read_player_name(cursor, alpha_mode && (version == 5 || version == 0 || version == 1))?); 
        }
        if trimmed_code == "tbk" || trimmed_code == "ibk" { data.tbk_ibk_teleport = Some(cursor.read_bits::<u8>(5)? as u8) }
        data.timestamp_flag = cursor.read_bit()?;
        let template = crate::domain::item::serialization::item_template(trimmed_code);
        let (reads_defense, reads_durability, reads_quantity) = if let Some(template) = template { (template.is_armor, template.has_durability, template.is_stackable) } else {
            let is_scroll = trimmed_code == "tsc" || trimmed_code == "isc";
            let armor_like_unknown = data.has_class_specific_data || trimmed_code.contains(' ');
            (armor_like_unknown, armor_like_unknown, is_scroll)
        };
        if reads_defense && axiom.reads_defense() { data.defense = Some(cursor.read_bits::<u32>(crate::domain::stats::stat_save_bits(31).unwrap_or(11))?); }
        if reads_durability && axiom.reads_durability() {
            let max_bits = crate::domain::stats::stat_save_bits(73).unwrap_or(8);
            let cur_bits = crate::domain::stats::stat_save_bits(72).unwrap_or(9);
            let m_dur = cursor.read_bits::<u32>(max_bits)?;
            data.max_durability = Some(m_dur);
            if m_dur > 0 { data.current_durability = Some(cursor.read_bits::<u32>(cur_bits)?); let _extra = cursor.read_bit()?; }
        }
        if reads_quantity && axiom.reads_quantity() { data.quantity = Some(cursor.read_bits::<u32>(9)?); }
        if is_socketed_flag { data.sockets = Some(cursor.read_bits::<u8>(4)? as u8); }
        if quality_val == ItemQuality::Set {
            let val = cursor.read_bits::<u8>(5)?;
            data.alpha_set_list_val = Some(val);
            data.set_list_count = match val { 1 => 1, 3 => 2, 7 => 3, 15 => 4, 31 => 5, _ => 0 };
        }
        cursor.end_segment();
        Ok(data)
    }
}
