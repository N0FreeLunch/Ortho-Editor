use crate::data::bit_cursor::BitCursor;
use crate::error::{ParsingResult, ParsingError};
use bitstream_io::BitRead;
use serde::{Serialize, Deserialize};
use crate::domain::item::quality::ItemQuality;
use crate::domain::item::axiom_meta::ForensicAudit;
use crate::domain::stats::{ItemProperty, ItemStats};
use crate::domain::stats::axiom::StatsAxiom;
use crate::domain::header::entity::{ItemSegmentType, ItemHeader, HeaderAxiom};
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
    pub fn defense(&self) -> Option<u32> { self.body.defense }
    pub fn max_durability(&self) -> Option<u32> { self.body.max_durability }
    pub fn current_durability(&self) -> Option<u32> { self.body.current_durability }
    pub fn quantity(&self) -> Option<u32> { self.body.quantity }

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
        {
            let mut lists = Vec::new();
            lists.push(&mut self.properties);
            for list in &mut self.set_attributes { lists.push(list); }
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
        if found { self.bits.clear(); }
        found
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

    pub fn to_bytes(&self, huffman: &crate::domain::item::serialization::HuffmanTree, alpha_mode: bool) -> io::Result<Vec<u8>> {
        use crate::domain::item::serialization::{BitEmitter, write_player_name};
        let mut emitter = BitEmitter::new();
        emitter.write_bits(self.header.flags, 32)?;
        emitter.write_bits(self.header.version as u32, 3)?;
        emitter.write_bits(self.header.mode as u32, 3)?;
        emitter.write_bits(self.header.location as u32, 3)?;
        emitter.write_bits(self.header.x as u32, 4)?;
        
        let s_axiom = StatsAxiom::new(self.header.version, self.header.quality.unwrap_or(ItemQuality::Normal), alpha_mode)
            .with_personalization(self.header.is_personalized)
            .with_code(&self.code);
        
        let h_axiom = HeaderAxiom::new(self.header.version, alpha_mode);
        let geometry = h_axiom.header_geometry(self.header.flags, self.header.is_compact, self.header.is_personalized);

        if geometry.has_header_gap {
            if h_axiom.is_alpha() {
                let is_v105_shadow = h_axiom.is_v105_shadow(self.header.flags);
                let is_rw = h_axiom.is_runeword(self.header.flags);

                if is_v105_shadow || is_rw {
                    let is_v105_shadow_local = (self.header.flags & (1 << 26)) != 0 || (self.header.flags & (1 << 27)) != 0;
                    let gap_bits = if is_v105_shadow_local { 8 } else { 24 };
                    let gap = self.body.alpha_header_gap.unwrap_or(0);
                    emitter.write_bits(gap, gap_bits)?;
                    
                    if !self.header.is_compact {
                        // y, page, socket_hint are embedded in the gap for shadows/runewords
                    }
                } else {
                    if !self.header.is_compact {
                        emitter.write_bits(self.header.y as u32, geometry.y_bits)?;
                        emitter.write_bits(self.header.page as u32, geometry.page_bits)?;
                        emitter.write_bits(self.header.socket_hint as u32, geometry.socket_hint_bits)?;
                    }
                    emitter.write_bits(self.body.alpha_header_gap.unwrap_or(0), 8)?;
                }
            } else {
                if !self.header.is_compact {
                    emitter.write_bits(self.header.y as u32, geometry.y_bits)?;
                    emitter.write_bits(self.header.page as u32, geometry.page_bits)?;
                    emitter.write_bits(self.header.socket_hint as u32, geometry.socket_hint_bits)?;
                }
                emitter.write_bits(self.body.alpha_header_gap.unwrap_or(0), 8)?;
            }
        } else if !geometry.skip_geometry {
            emitter.write_bits(self.header.y as u32, geometry.y_bits)?;
            emitter.write_bits(self.header.page as u32, geometry.page_bits)?;
            emitter.write_bits(self.header.socket_hint as u32, geometry.socket_hint_bits)?;
        }

        if self.header.is_ear {
            emitter.write_bits(self.ear_class.unwrap_or(0) as u32, 3)?;
            emitter.write_bits(self.ear_level.unwrap_or(0) as u32, 7)?;
            write_player_name(&mut emitter, self.ear_player_name.as_deref().unwrap_or(""), alpha_mode && self.header.version == 5)?;
            if alpha_mode && self.header.version == 5 { emitter.byte_align()?; }
        } else {
            let encoded_code = huffman.encode(&self.code)?;
            emitter.extend_bits(encoded_code)?;
            if h_axiom.is_alpha() && (self.header.version == 5 || self.header.version == 0 || self.header.version == 1) {
                let nudge = self.body.alpha_nudge.unwrap_or(0);
                emitter.write_bits(nudge as u32, 2)?;
            }
        }

        if !self.header.is_compact {
            let quality_val = self.header.quality.unwrap_or(ItemQuality::Normal);
            let is_item_alpha = s_axiom.is_alpha();

            if is_item_alpha {
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

            if !(is_item_alpha && (self.header.version == 1 || self.header.version == 4)) {
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
                    write_player_name(&mut emitter, self.personalized_player_name.as_deref().unwrap_or(""), alpha_mode && (self.header.version == 5 || self.header.version == 0 || self.header.version == 1))?;
                }
                if self.code.trim() == "tbk" || self.code.trim() == "ibk" { emitter.write_bits(self.tbk_ibk_teleport.unwrap_or(0) as u32, 5)?; }
                emitter.write_bit(self.timestamp_flag)?;
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
                if self.header.version != 5 || is_shadow || self.header.is_runeword || (alpha_mode && self.header.is_compact) || !self.properties.is_empty() {
                    crate::domain::item::serialization::write_property_list(&mut emitter, &self.code, &self.properties, self.header.version, self.header.is_runeword, self.terminator_bit, quality_val, is_shadow, &s_axiom)?;
                    for set_props in &self.set_attributes {
                        crate::domain::item::serialization::write_property_list(&mut emitter, &self.code, set_props, self.header.version, false, false, quality_val, false, &s_axiom)?;
                    }
                }
            }
        }
        if self.header.version != 5 && self.header.version != 7 { emitter.write_bit(false)?; }
        let current_bits = emitter.written_bits();
        let final_bits = s_axiom.calculate_alignment(current_bits, self.header.is_compact, &self.code, self.header.flags);
        if final_bits > current_bits {
            let padding_needed = (final_bits - current_bits) as u32;
            if !self.body.alpha_alignment_padding.is_empty() { for &bit in &self.body.alpha_alignment_padding { emitter.write_bit(bit)?; } }
            else { emitter.write_bits(0, padding_needed)?; }
        }
        Ok(emitter.into_bytes())
    }

    pub fn serialize_section(items: &[Item], huffman: &crate::domain::item::serialization::HuffmanTree, alpha_mode: bool) -> io::Result<Vec<u8>> {
        use crate::domain::item::serialization::BitEmitter;
        let mut emitter = BitEmitter::new();
        for item in items {
            emitter.extend_bits(item.gap_bits.iter().cloned())?;
            let item_bytes = item.to_bytes(huffman, alpha_mode)?;
            for byte in item_bytes { emitter.write_bits(byte as u32, 8)?; }
            for child in &item.socketed_items {
                if alpha_mode { emitter.write_bits(2, 2)?; }
                let child_bytes = child.to_bytes(huffman, alpha_mode)?;
                for byte in child_bytes { emitter.write_bits(byte as u32, 8)?; }
            }
        }
        Ok(emitter.into_bytes())
    }
}

pub fn parse_item_header<R: BitRead>(
    cursor: &mut BitCursor<R>,
    alpha_mode: bool,
) -> ParsingResult<(ItemHeader, Option<u32>)> {
    let start_bit = cursor.pos();
    cursor.begin_segment(ItemSegmentType::Header);
    let flags = cursor.read_bits::<u32>(32)?;
    if !alpha_mode && (flags & 0xFFFF) != 0x4D4A {
         return Err(cursor.fail(ParsingError::MissingMarker { marker: "JM".to_string(), bit_offset: start_bit }));
    }
    let version = cursor.read_bits::<u8>(3)? as u8;
    let mode = cursor.read_bits::<u8>(3)? as u8;
    let location = cursor.read_bits::<u8>(3)? as u8;
    let x = cursor.read_bits::<u8>(4)? as u8;
    
    let h_axiom = HeaderAxiom::new(version, alpha_mode);
    let s_axiom = StatsAxiom::new(version, ItemQuality::Normal, alpha_mode);
    let is_compact = s_axiom.is_compact(flags);
    let is_personalized = s_axiom.is_personalized(flags);
    let mut y = 0; let mut page = 0; let mut socket_hint = 0;
    let geometry = h_axiom.header_geometry(flags, is_compact, is_personalized);
    let mut alpha_header_gap = None;
    if geometry.has_header_gap {
        if h_axiom.is_alpha() {
            let is_v105_shadow = h_axiom.is_v105_shadow(flags);
            let is_rw = h_axiom.is_runeword(flags);
            if is_rw || is_v105_shadow {
                let is_v105_shadow_local = (flags & (1 << 26)) != 0 || (flags & (1 << 27)) != 0;
                let gap_bits = if is_v105_shadow_local { 8 } else { 24 };
                let gap = cursor.read_bits::<u32>(gap_bits)?;
                alpha_header_gap = Some(gap);
                if !is_compact { y = (gap & 0x0F) as u8; page = ((gap >> 4) & 0x07) as u8; socket_hint = ((gap >> 7) & 0x01) as u8; }
            } else {
                if !is_compact { y = cursor.read_bits::<u8>(geometry.y_bits)? as u8; page = cursor.read_bits::<u8>(geometry.page_bits)? as u8; socket_hint = cursor.read_bits::<u8>(geometry.socket_hint_bits)? as u8; }
                alpha_header_gap = Some(cursor.read_bits::<u32>(8)?);
            }
        } else {
            if !is_compact { y = cursor.read_bits::<u8>(geometry.y_bits)? as u8; page = cursor.read_bits::<u8>(geometry.page_bits)? as u8; socket_hint = cursor.read_bits::<u8>(geometry.socket_hint_bits)? as u8; }
            alpha_header_gap = Some(cursor.read_bits::<u32>(8)?);
        }
    } else if !geometry.skip_geometry {
        y = cursor.read_bits::<u8>(geometry.y_bits)? as u8; page = cursor.read_bits::<u8>(geometry.page_bits)? as u8; socket_hint = cursor.read_bits::<u8>(geometry.socket_hint_bits)? as u8;
    }
    cursor.end_segment();
    Ok((ItemHeader {
        flags, version, mode, location, x, y, page, socket_hint, id: None, level: None, quality: None, is_compact,
        is_identified: s_axiom.is_identified(flags), is_socketed: s_axiom.is_socketed(flags, is_compact), is_personalized,
        is_runeword: s_axiom.is_runeword(flags), is_ethereal: s_axiom.is_ethereal(flags), is_ear: (flags & (1 << 24)) != 0,
        alpha_quality_raw: None, alpha_v5_runeword_extra: None, alpha_unique_id_raw: None,
    }, alpha_header_gap))
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
        for _ in 0..4 { code.push(huff.decode_recorded(cursor)?); }
        let mut nudge = None;
        if h_axiom.is_alpha() && (header.version == 5 || header.version == 0 || header.version == 1) { nudge = Some(cursor.read_bits::<u8>(2)?); }
        cursor.end_segment();
        (code, nudge, None, None, None)
    };
    Ok((ItemBody {
        code, x: header.x, y: header.y, page: header.page, location: header.location, mode: header.mode,
        defense: None, max_durability: None, current_durability: None, quantity: None, alpha_header_gap: None, 
        v5_runeword_extra: None, v105_7mgw_payload: None, alpha_nudge, alpha_set_list_val: None, alpha_shadow_skip_bits: None, alpha_alignment_padding: Vec::new(),
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
        let is_alpha_early_exit = h_axiom.is_alpha() && (version == 1 || version == 4);
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
                } else { data.id = Some(0); }
            } else { data.id = Some(0); }
        } else {
            data.id = Some(cursor.read_bits::<u32>(32)?);
            data.level = Some(cursor.read_bits::<u8>(7)?);
            let quality_raw = cursor.read_bits::<u8>(4)?;
            data.quality = Some(ItemQuality::from(quality_raw));
        }
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
