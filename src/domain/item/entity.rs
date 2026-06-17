use crate::data::bit_cursor::BitCursor;
use crate::domain::forensic::v105::{
    get_v105_target_width, V105HeaderGapAxiom, V105PropertyWidthAxiom,
};
use crate::domain::header::entity::{
    calculate_alpha_v105_checksum, HeaderAxiom, ItemHeader, ItemSegmentType,
};
use crate::domain::item::axiom_meta::ForensicAudit;
use crate::domain::item::quality::ItemQuality;
use crate::domain::item::subdomains::affix::{
    AffixCombinator, MagicAffixSegment, RareAffixSegment, UniqueAffixSegment,
};
use crate::domain::item::subdomains::gap::{AlphaHeaderGap, GapCombinator};
use crate::domain::stats::axiom::StatsAxiom;
use crate::domain::stats::{ItemProperty, ItemStats};
use crate::error::{ParsingError, ParsingResult};
use bitstream_io::BitRead;
use d2r_macros::serialization_symmetry;
use serde::{Deserialize, Serialize};
use std::io;
use std::ops::{Deref, DerefMut};

/// Category B: Alpha v105 bitstream symmetry point for ear name alignment.
#[serialization_symmetry(align = true)]
pub struct AlphaV105EarAlignment;

/// Category B: Alpha v105 bitstream symmetry point for personalized name alignment.
#[serialization_symmetry(align = true)]
pub struct AlphaV105PersonalizedAlignment;

/// Category B: Alpha v105 bitstream symmetry point for post-body alignment.
#[serialization_symmetry(align = true)]
pub struct AlphaV105PostBodyAlignment;

/// Category B: Alpha v105 bitstream symmetry point for post-header-gap alignment.
#[serialization_symmetry(align = true)]
pub struct AlphaV105HeaderGapAlignment;

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
    pub alpha_code_bits: Vec<bool>,
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
    MagicAffixes {
        prefix: Option<u16>,
        suffix: Option<u16>,
    },
    RareAffixes {
        names: [Option<u8>; 2],
        affixes: [Option<u16>; 6],
    },
    UniqueAffix {
        unique_id: Option<u16>,
    },
    Sockets {
        count: u8,
        items: Vec<Item>,
    },
    Personalization(Option<String>),
    Runeword {
        id: Option<u16>,
        level: Option<u8>,
    },
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
    pub logical_width: Option<u64>,
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
    pub fn code(&self) -> &str {
        &self.body.code
    }
    pub fn is_opaque(&self) -> bool {
        self.modules
            .iter()
            .any(|m| matches!(m, ItemModule::Opaque(_) | ItemModule::Residue(_)))
    }
    pub fn is_semi_opaque(&self) -> bool {
        self.modules
            .iter()
            .any(|m| matches!(m, ItemModule::SemiOpaque { .. }))
    }
    pub fn is_residue(&self) -> bool {
        let trimmed = self
            .code
            .trim_matches(|c: char| c.is_whitespace() || c == '\0');
        if trimmed.is_empty() {
            return true;
        }

        // Alpha v105 pure fragments (`ww`, `gcw`) are scanner helpers rather than
        // semantic top-level items, even though they may still be preserved in the
        // recovered bitstream.
        self.header.save_is_alpha && matches!(trimmed, "ww" | "gcw")
    }
    pub fn defense(&self) -> Option<u32> {
        if let Some(d) = self.body.defense {
            return Some(d);
        }
        if self.header.save_is_alpha {
            let axiom = StatsAxiom::new(
                self.header.version,
                self.header.quality.unwrap_or(ItemQuality::Normal),
                true,
            );
            return self
                .properties
                .iter()
                .find(|p| axiom.map_alpha_id(p.stat_id) == 31)
                .map(|p| p.value as u32);
        }
        None
    }
    pub fn max_durability(&self) -> Option<u32> {
        if let Some(d) = self.body.max_durability {
            return Some(d);
        }
        if self.header.save_is_alpha {
            let axiom = StatsAxiom::new(
                self.header.version,
                self.header.quality.unwrap_or(ItemQuality::Normal),
                true,
            );
            return self
                .properties
                .iter()
                .find(|p| axiom.map_alpha_id(p.stat_id) == 73)
                .map(|p| p.value as u32);
        }
        None
    }
    pub fn current_durability(&self) -> Option<u32> {
        if let Some(d) = self.body.current_durability {
            return Some(d);
        }
        if self.header.save_is_alpha {
            let axiom = StatsAxiom::new(
                self.header.version,
                self.header.quality.unwrap_or(ItemQuality::Normal),
                true,
            );
            return self
                .properties
                .iter()
                .find(|p| axiom.map_alpha_id(p.stat_id) == 72)
                .map(|p| p.value as u32);
        }
        None
    }
    pub fn quantity(&self) -> Option<u32> {
        if let Some(d) = self.body.quantity {
            return Some(d);
        }
        if self.header.save_is_alpha {
            let axiom = StatsAxiom::new(
                self.header.version,
                self.header.quality.unwrap_or(ItemQuality::Normal),
                true,
            );
            return self
                .properties
                .iter()
                .find(|p| axiom.map_alpha_id(p.stat_id) == 70)
                .map(|p| p.value as u32);
        }
        None
    }

    pub fn query_bit(&self, offset: u64) -> Option<BitSemantic> {
        for prop in &self.properties {
            if offset >= prop.range.start && offset < prop.range.end {
                let name = if prop.name.is_empty() {
                    crate::domain::stats::lookup_alpha_map_by_raw(prop.stat_id)
                        .map(|m| m.name.to_string())
                        .unwrap_or_else(|| format!("Stat({})", prop.stat_id))
                } else {
                    prop.name.clone()
                };
                return Some(BitSemantic {
                    label: format!("Stats -> {}", name),
                });
            }
        }
        for (i, list) in self.set_attributes.iter().enumerate() {
            for prop in list {
                if offset >= prop.range.start && offset < prop.range.end {
                    let name = if prop.name.is_empty() {
                        crate::domain::stats::lookup_alpha_map_by_raw(prop.stat_id)
                            .map(|m| m.name.to_string())
                            .unwrap_or_else(|| format!("Stat({})", prop.stat_id))
                    } else {
                        prop.name.clone()
                    };
                    return Some(BitSemantic {
                        label: format!("SetAttributes[{}] -> {}", i, name),
                    });
                }
            }
        }
        for prop in &self.runeword_attributes {
            if offset >= prop.range.start && offset < prop.range.end {
                let name = if prop.name.is_empty() {
                    crate::domain::stats::lookup_alpha_map_by_raw(prop.stat_id)
                        .map(|m| m.name.to_string())
                        .unwrap_or_else(|| format!("Stat({})", prop.stat_id))
                } else {
                    prop.name.clone()
                };
                return Some(BitSemantic {
                    label: format!("RunewordAttributes -> {}", name),
                });
            }
        }

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
            return Some(BitSemantic {
                label: seg.label.clone(),
            });
        }
        for child in &self.socketed_items {
            if let Some(semantic) = child.query_bit(offset) {
                return Some(BitSemantic {
                    label: format!("{} -> {}", self.body.code.trim(), semantic.label),
                });
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

    pub fn set_property_value(
        &mut self,
        stat_id: u32,
        value: crate::domain::vo::ItemStatValue,
    ) -> bool {
        let mut found = false;
        // Alpha-aware stat mapping
        let is_alpha =
            self.header.version == 5 || self.header.version == 6 || self.header.version == 1;
        let axiom = crate::domain::stats::axiom::StatsAxiom::new(
            self.header.version,
            self.header
                .quality
                .unwrap_or(crate::domain::item::ItemQuality::Normal),
            is_alpha,
        );

        {
            let mut lists = Vec::new();
            lists.push(&mut self.properties);
            for list in &mut self.set_attributes {
                lists.push(list);
            }
            lists.push(&mut self.runeword_attributes);
            for list in lists.into_iter() {
                for prop in list {
                    let effective_id = axiom.map_alpha_id(prop.stat_id);
                    if effective_id == stat_id {
                        let cost = crate::data::stat_costs::STAT_COSTS
                            .iter()
                            .find(|s| s.id == effective_id);
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
        let is_alpha =
            self.header.version == 5 || self.header.version == 6 || self.header.version == 1;

        // Synchronize flags bit
        if has_sockets {
            if is_alpha {
                if self.header.version == 5 {
                    self.header.flags |= 1 << 11;
                    self.header.flags &= !(1 << 23); // Ensure NOT compact
                } else {
                    self.header.flags |= 1 << 11;
                }
            } else {
                self.header.flags |= 1 << 11;
            }
            self.header.flags |= 1 << 4; // Identified
        } else {
            if is_alpha && self.header.version == 5 {
                self.header.flags &= !(1 << 11);
            } else {
                self.header.flags &= !(1 << 11);
            }
        }

        // Ensure we have enough Stat 317/320 properties to hold the socketed items.
        // For simplicity, we'll use Stat 317 (recursive) as the default for added items.
        let mut nested_prop_count = 0;
        let axiom = crate::domain::stats::axiom::StatsAxiom::new(
            self.header.version,
            self.header
                .quality
                .unwrap_or(crate::domain::item::ItemQuality::Normal),
            is_alpha,
        );

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
            if let Some(affix) = crate::data::affixes::PREFIXES
                .iter()
                .find(|a| a.id == id as u32)
            {
                result.push(affix);
            }
        }
        for i in [0, 2, 4] {
            if let Some(id) = self.rare_affixes[i] {
                if let Some(affix) = crate::data::affixes::PREFIXES
                    .iter()
                    .find(|a| a.id == id as u32)
                {
                    result.push(affix);
                }
            }
        }
        result
    }

    pub fn suffixes(&self) -> Vec<&'static crate::data::item_specs::Affix> {
        let mut result = Vec::new();
        if let Some(id) = self.magic_suffix {
            if let Some(affix) = crate::data::affixes::SUFFIXES
                .iter()
                .find(|a| a.id == id as u32)
            {
                result.push(affix);
            }
        }
        for i in [1, 3, 5] {
            if let Some(id) = self.rare_affixes[i] {
                if let Some(affix) = crate::data::affixes::SUFFIXES
                    .iter()
                    .find(|a| a.id == id as u32)
                {
                    result.push(affix);
                }
            }
        }
        result
    }

    pub fn to_bytes(
        &self,
        idx: usize,
        huffman: &crate::domain::item::serialization::HuffmanTree,
        alpha_mode: bool,
    ) -> io::Result<Vec<u8>> {
        use crate::domain::item::serialization::BitEmitter;
        let mut emitter = BitEmitter::new();
        self.to_emitter(idx, &mut emitter, huffman, alpha_mode)?;
        Ok(emitter.into_bytes())
    }

    pub fn to_bits(
        &self,
        idx: usize,
        huffman: &crate::domain::item::serialization::HuffmanTree,
        alpha_mode: bool,
    ) -> io::Result<Vec<bool>> {
        use crate::domain::item::serialization::BitEmitter;
        let mut emitter = BitEmitter::new();
        self.to_emitter(idx, &mut emitter, huffman, alpha_mode)?;
        Ok(emitter.into_bits())
    }

    pub fn to_emitter(
        &self,
        idx: usize,
        emitter: &mut crate::domain::item::serialization::BitEmitter,
        huffman: &crate::domain::item::serialization::HuffmanTree,
        alpha_mode: bool,
    ) -> io::Result<()> {
        let start_bit = emitter.written_bits();
        let trimmed = self.code.trim();
        let reg = crate::domain::forensic::registry::get_registry();
        let mut is_authority_overlap_code =
            alpha_mode && matches!(trimmed, "xrs" | "c8xr" | "rhd" | "wa2" | "ww" | "gcw");
        let mut is_v105_shadow_override = alpha_mode && matches!(trimmed, "xrs" | "c8xr" | "rhd");
        if alpha_mode {
            if let Some(overrides) = &reg.item_overrides {
                if let Some(map) = overrides.get(trimmed) {
                    if let Some(&val) = map.get("is_authority_overlap") {
                        is_authority_overlap_code = val != 0 || is_authority_overlap_code;
                    }
                    if let Some(&val) = map.get("is_shadow") {
                        is_v105_shadow_override = val != 0 || is_v105_shadow_override;
                    }
                }
            }
        }
        // Slice 2: Opaque pass-through
        let is_placeholder_opaque = self.code.trim().is_empty() || self.code == "Opaque";
        let has_opaque_module = self
            .modules
            .iter()
            .any(|m| matches!(m, ItemModule::Opaque(_) | ItemModule::Residue(_)));

        if is_placeholder_opaque {
            for module in &self.modules {
                match module {
                    ItemModule::Opaque(bits) | ItemModule::Residue(bits) => {
                        let len = bits.len() as u64;
                        emitter.extend_bits(bits.iter().cloned())?;
                        // Slice 7: Dynamic Interval Capture. Honor total_bits even for placeholder items.
                        if self.total_bits > len {
                            let padding = (self.total_bits - len) as u32;
                            emitter.extend_bits(std::iter::repeat(false).take(padding as usize))?;
                        }
                        return Ok(());
                    }
                    _ => {}
                }
            }
        }
        let trimmed_code = self.code.trim_matches(|c: char| c.is_whitespace() || c == '\0');
        let is_target_blank = alpha_mode && trimmed_code.is_empty();
        if alpha_mode && (self.is_opaque() || self.is_semi_opaque() || is_target_blank) && !self.bits.is_empty() {
            let take = self.total_bits.min(self.bits.len() as u64) as usize;
            if take > 0 {
                emitter.extend_bits(self.bits[..take].iter().map(|rb| rb.bit))?;
            }
            // Slice 7: Dynamic Interval Capture. Honor total_bits even for Opaque/SemiOpaque/Blank items
            // to ensure bit-perfect preservation of drifted boundaries.
            if self.total_bits > take as u64 {
                let padding_needed = (self.total_bits - take as u64) as u32;
                emitter.extend_bits(std::iter::repeat(false).take(padding_needed as usize))?;
            }
            if take > 0 || self.total_bits > 0 {
                return Ok(());
            }
        }
        if alpha_mode && self.header.is_runeword && is_authority_overlap_code && !self.bits.is_empty() {
            let take = self.total_bits.min(self.bits.len() as u64) as usize;
            if take > 0 {
                emitter.extend_bits(self.bits[..take].iter().map(|rb| rb.bit))?;
            }
            // Slice 7: Dynamic Interval Capture. Honor total_bits for authority containers (xrs, wa2, etc)
            // to preserve the full 512-bit (or larger) block.
            if self.total_bits > take as u64 {
                let padding_needed = (self.total_bits - take as u64) as u32;
                emitter.extend_bits(std::iter::repeat(false).take(padding_needed as usize))?;
            }
            if take > 0 || self.total_bits > 0 {
                return Ok(());
            }
        }

        // 1. Write Header fields (Flags, Checksum, Version, Mode, Location, X).
        use crate::domain::item::serialization::write_player_name;
        let flags_to_write = self.header.flags;
        emitter.write_bits(flags_to_write, 32)?;
        let w_axiom = V105PropertyWidthAxiom::default();
        let is_v105_summary = alpha_mode && w_axiom.is_summary_item(self.header.version, &self.code);
        
        if alpha_mode && self.header.has_checksum {
            let checksum = self.header.alpha_checksum.unwrap_or_else(|| {
                calculate_alpha_v105_checksum(flags_to_write, self.header.version)
            });
            emitter.write_bits(checksum as u32, 8)?;
        }
        emitter.write_bits(self.header.version as u32, w_axiom.version_bits(alpha_mode) as u32)?;
        emitter.write_bits(self.header.mode as u32, w_axiom.mode_bits(alpha_mode) as u32)?;
        emitter.write_bits(self.header.x as u32, w_axiom.x_bits(alpha_mode, self.header.version) as u32)?;
        emitter.write_bits(self.header.location as u32, w_axiom.location_bits(alpha_mode, self.header.version) as u32)?;

        let s_axiom = StatsAxiom::new(
            self.header.version,
            self.header.quality.unwrap_or(ItemQuality::Normal),
            alpha_mode,
        )
        .with_index(idx)
        .with_personalization(self.header.is_personalized)
        .with_compact(self.header.is_compact)
        .with_code(&self.code);
        let h_axiom = HeaderAxiom::new(self.header.version, alpha_mode);
        let geometry = h_axiom.header_geometry(self.header.flags, Some(&self.code));

        // 2. Handle Alpha v105 summary items (Potions, Scrolls):
        if is_v105_summary {
            // Write Y, Page, SocketHint (3 bits for Y, 0 for others).
            emitter.write_bits(self.header.y as u32, 3)?;
            // Write Gap.
            let gap_bits = w_axiom.summary_gap_bits(&self.code);
            if gap_bits > 0 {
                let gap_val = self.body.alpha_header_gap.unwrap_or(0);
                emitter.write_bits(gap_val, gap_bits)?;
            }

            // Write 16-bit ID (Slice 2): Unified 16-bit ID field.
            // Alpha v105 summary items (wsp, potions, scrolls) MUST use 16-bit ID to maintain 73/80 bit slotted alignment.
            // If they have 24-bit raw code bits, it will overflow the slotted boundary.
            let id_val = if !self.body.alpha_code_bits.is_empty() {
                let mut val = 0u32;
                for (i, &bit) in self.body.alpha_code_bits.iter().enumerate() {
                    if i < 32 && bit {
                        val |= 1 << i;
                    }
                }
                val
            } else {
                self.id.unwrap_or(0)
            };
            emitter.write_bits(id_val, 16)?;

            // Align to target width (72 or 80 bits).
            let current_bits = emitter.written_bits() - start_bit;
            let target = get_v105_target_width(
                self.header.version,
                &self.code,
                self.header.flags,
                Some(idx),
            );
            
            let mut final_target = target as u64;
            if self.total_bits > final_target {
                final_target = self.total_bits;
            }

            if final_target > current_bits {
                let padding_needed = (final_target - current_bits) as u32;
                let padding_bits = if !self.body.alpha_alignment_padding.is_empty() {
                    let mut bits = self.body.alpha_alignment_padding.clone();
                    // Slice 7: Honor total_bits even for summary items to preserve bit-exact boundaries
                    // when they have been expanded by proximity snaps or dynamic interval capture.
                    if bits.len() < padding_needed as usize {
                        bits.resize(padding_needed as usize, false);
                    } else {
                        bits.truncate(padding_needed as usize);
                    }
                    bits
                } else {
                    vec![false; padding_needed as usize]
                };
                AlphaHeaderGap { bits: padding_bits }.emit(emitter)?;
            }

            // Summary items can still carry trailing preserved tails
            if is_authority_overlap_code {
                for module in &self.modules {
                    match module {
                        ItemModule::Opaque(bits) | ItemModule::Residue(bits) => {
                            emitter.extend_bits(bits.iter().cloned())?;
                        }
                        _ => {}
                    }
                }
            }
            return Ok(());
        }

        // 3. Handle standard items (including Ear).
        if alpha_mode && self.header.save_is_alpha {
            let preserve_compact_summary_header = self.header.is_compact
                && self.body.alpha_header_gap_bits.is_empty()
                && is_v105_shadow_override;

            if !preserve_compact_summary_header {
                if !geometry.skip_geometry {
                    emitter.write_bits(self.header.y as u32, geometry.y_bits)?;
                    emitter.write_bits(self.header.page as u32, geometry.page_bits)?;
                    emitter.write_bits(self.header.socket_hint as u32, geometry.socket_hint_bits)?;
                }
                if geometry.target_width > 0 {
                    let current_bits = emitter.written_bits() - start_bit;
                    if current_bits < geometry.target_width as u64 {
                        let to_write = (geometry.target_width as u64 - current_bits) as u32;
                        let gap_seg = AlphaHeaderGap {
                            bits: if !self.body.alpha_header_gap_bits.is_empty() {
                                self.body.alpha_header_gap_bits.clone()
                            } else {
                                let start_idx = current_bits as usize;
                                let end_idx = start_idx + to_write as usize;
                                if self.bits.len() >= end_idx {
                                    // Prefer the preserved raw slice whenever it is available.
                                    self.bits[start_idx..end_idx]
                                        .iter()
                                        .map(|rb| rb.bit)
                                        .collect()
                                } else {
                                    let mut b = Vec::new();
                                    let val = self.body.alpha_header_gap.unwrap_or(0);
                                    for i in 0..to_write {
                                        let bit = i < 32 && (val & (1u32 << i)) != 0;
                                        b.push(bit);
                                    }
                                    b
                                }
                            },
                        };
                        gap_seg.emit(emitter)?;
                    }
                } else if geometry.has_header_gap
                    || (h_axiom.is_alpha() && !self.header.has_checksum && self.header.version == 5)
                {
                    let gap_len = V105HeaderGapAxiom::default().resolve_gap(
                        self.header.version,
                        Some(&self.code),
                        self.header.flags,
                        idx == 0,
                        self.header.is_compact,
                        self.header.has_checksum,
                        None,
                    );
                    if gap_len > 0 || !self.body.alpha_header_gap_bits.is_empty() {
                        let gap_seg = AlphaHeaderGap {
                            bits: if !self.body.alpha_header_gap_bits.is_empty() {
                                self.body.alpha_header_gap_bits.clone()
                            } else {
                                let mut b = Vec::new();
                                let val = self.body.alpha_header_gap.unwrap_or(0);
                                for i in 0..gap_len as u32 {
                                    let bit = i < 32 && (val & (1u32 << i)) != 0;
                                    b.push(bit);
                                }
                                b
                            },
                        };
                        gap_seg.emit(emitter)?;
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
        let is_header_only = s_axiom.is_header_only(self.header.flags, &self.code);
        let is_v105_blank = alpha_mode && self.code.trim().is_empty();
        if is_header_only
            && (is_v105_blank
                || !(alpha_mode
                    && (self.header.version == 0
                        || self.header.version == 1
                        || self.header.version == 2
                        || self.header.version == 5)))
        {
            let current_bits = emitter.written_bits();
            let mut final_bits = s_axiom.calculate_alignment(
                current_bits - start_bit,
                &self.code,
                self.header.flags,
            );
            if self.total_bits > final_bits {
                final_bits = self.total_bits;
            }

            if final_bits > (current_bits - start_bit) {
                let padding_needed = (final_bits - (current_bits - start_bit)) as u32;
                let fallback_padding = {
                    let start_idx = (current_bits - start_bit) as usize;
                    let total_bits = self.total_bits.min(self.bits.len() as u64) as usize;
                    if total_bits > start_idx && total_bits <= self.bits.len() {
                        self.bits[start_idx..total_bits]
                            .iter()
                            .map(|rb| rb.bit)
                            .collect::<Vec<bool>>()
                    } else {
                        Vec::new()
                    }
                };
                let mut align_bits = if !fallback_padding.is_empty() {
                    fallback_padding
                } else if !self.body.alpha_alignment_padding.is_empty() {
                    self.body.alpha_alignment_padding.clone()
                } else {
                    Vec::new()
                };
                if align_bits.len() < padding_needed as usize {
                    align_bits.resize(padding_needed as usize, false);
                } else {
                    align_bits.truncate(padding_needed as usize);
                }
                let pad_seg = AlphaHeaderGap {
                    bits: align_bits,
                };
                pad_seg.emit(emitter)?;
            }
            if has_opaque_module && !is_placeholder_opaque {
                for module in &self.modules {
                    match module {
                        ItemModule::Opaque(bits) | ItemModule::Residue(bits) => {
                            emitter.extend_bits(bits.iter().cloned())?;
                        }
                        _ => {}
                    }
                }
            }
            return Ok(());
        }

        if self.header.is_ear {
            emitter.write_bits(self.ear_class.unwrap_or(0) as u32, 3)?;
            emitter.write_bits(self.ear_level.unwrap_or(0) as u32, 7)?;
            write_player_name(
                emitter,
                self.ear_player_name.as_deref().unwrap_or(""),
                alpha_mode && (self.header.version == 5 || self.header.version == 0 || self.header.version == 1),
            )?;
        } else {
            if alpha_mode && !self.body.alpha_code_bits.is_empty() {
                emitter.extend_bits(self.body.alpha_code_bits.iter().cloned())?;
            } else {
                let code_bits = huffman.encode(&self.code)?;
                emitter.extend_bits(code_bits)?;
            }

            if alpha_mode && h_axiom.is_alpha() && !self.header.is_compact {
                if let Some(nudge) = self.body.alpha_nudge {
                    emitter.write_bits(nudge as u32, w_axiom.nudge_bits(self.header.version) as u32)?;
                }
            }

            let quality_val = self.header.quality.unwrap_or(ItemQuality::Normal);
            let is_item_alpha = s_axiom.is_alpha();

            if is_item_alpha && !s_axiom.is_compact {
                let quality_to_write = if let Some(raw) = self.alpha_quality_raw {
                    raw
                } else {
                    let current_bits = emitter.written_bits() - start_bit;
                    let start_idx = current_bits as usize;
                    if self.bits.len() >= start_idx + 3 {
                        let mut raw = 0u8;
                        for (i, rb) in self.bits[start_idx..start_idx + 3].iter().enumerate() {
                            if rb.bit {
                                raw |= 1 << i;
                            }
                        }
                        raw
                    } else {
                        quality_val as u8
                    }
                };
                emitter.write_bits(quality_to_write as u32, 3)?;
                if (self.header.version == 1
                    || self.header.version == 5
                    || self.header.version == 6
                    || self.header.version == 7)
                    && (s_axiom.is_runeword(self.header.flags)
                        || h_axiom.is_v105_shadow(self.header.flags, Some(&self.code)))
                {
                    emitter.write_bits(self.body.v5_runeword_extra.unwrap_or(0) as u32, 2)?;
                }
            }

            if !is_item_alpha || (alpha_mode && (self.header.version == 0 || self.header.version == 2)) {
                emitter.write_bits(self.id.unwrap_or(0), 32)?;
                emitter.write_bits(self.level.unwrap_or(0) as u32, 7)?;
                emitter.write_bits(quality_val as u32, 4)?;
            }

            if !(is_item_alpha
                && (self.header.version == 4
                    || self.header.version == 6
                    || self.header.version == 7))
            {
                if self.has_multiple_graphics {
                    emitter.write_bits(self.multi_graphics_bits.unwrap_or(0) as u32, 3)?;
                }
                if self.has_class_specific_data {
                    emitter.write_bits(self.class_specific_bits.unwrap_or(0) as u16 as u32, 11)?;
                }
                match quality_val {
                    ItemQuality::Low | ItemQuality::High => {
                        emitter.write_bits(self.low_high_graphic_bits.unwrap_or(0) as u32, 3)?;
                    }
                    ItemQuality::Magic => {
                        let seg = MagicAffixSegment {
                            prefix: self.magic_prefix,
                            suffix: self.magic_suffix,
                        };
                        seg.emit(emitter)?;
                    }
                    ItemQuality::Rare | ItemQuality::Crafted => {
                        let seg = RareAffixSegment {
                            names: [self.rare_name_1, self.rare_name_2],
                            affixes: self.rare_affixes,
                        };
                        seg.emit(emitter)?;
                    }
                    ItemQuality::Set | ItemQuality::Unique => {
                        let uid = if alpha_mode {
                            self.alpha_unique_id_raw
                                .unwrap_or(self.unique_id.unwrap_or(0))
                        } else {
                            self.unique_id.unwrap_or(0)
                        };
                        let seg = UniqueAffixSegment {
                            unique_id: Some(uid),
                        };
                        seg.emit(emitter)?;
                    }
                    _ => {}
                }
                if s_axiom.is_runeword(self.header.flags)
                    && !is_item_alpha
                    && self.header.version != 5
                {
                    emitter.write_bits(self.runeword_id.unwrap_or(0) as u32, 12)?;
                    emitter.write_bits(self.runeword_level.unwrap_or(0) as u32, 12)?;
                    emitter.write_bits(0, 4)?;
                }
                if self.header.is_personalized {
                    if alpha_mode
                        && (self.header.version == 5
                            || self.header.version == 0
                            || self.header.version == 1)
                        && AlphaV105PersonalizedAlignment::align_required()
                    {
                        emitter.byte_align()?;
                    }
                    write_player_name(
                        emitter,
                        self.personalized_player_name.as_deref().unwrap_or(""),
                        alpha_mode
                            && (self.header.version == 5
                                || self.header.version == 0
                                || self.header.version == 1),
                    )?;
                }
                if !s_axiom.is_compact {
                    if self.code.trim() == "tbk" || self.code.trim() == "ibk" {
                        emitter.write_bits(self.tbk_ibk_teleport.unwrap_or(0) as u32, 5)?;
                    }
                    emitter.write_bit(self.timestamp_flag)?;
                }
                let template = crate::domain::item::serialization::item_template(&self.code);
                let (reads_def, reads_dur, reads_qty) = if let Some(t) = template {
                    (t.is_armor, t.has_durability, t.is_stackable)
                } else {
                    (false, false, false)
                };
                if reads_def && s_axiom.reads_defense() {
                    emitter.write_bits(self.defense.unwrap_or(0), 11)?;
                }
                if reads_dur && s_axiom.reads_durability() {
                    let m_dur = self.max_durability.unwrap_or(0);
                    emitter.write_bits(m_dur, 8)?;
                    if m_dur > 0 {
                        emitter.write_bits(self.current_durability.unwrap_or(0), 9)?;
                        emitter.write_bit(false)?;
                    }
                }
                if reads_qty && s_axiom.reads_quantity() {
                    emitter.write_bits(self.quantity.unwrap_or(0), 9)?;
                }
                if self.header.is_socketed {
                    emitter.write_bits(self.sockets.unwrap_or(0) as u32, 4)?;
                }
                if quality_val == ItemQuality::Set {
                    let val = self
                        .body
                        .alpha_set_list_val
                        .unwrap_or(match self.set_list_count {
                            1 => 1,
                            2 => 3,
                            3 => 7,
                            4 => 15,
                            5 => 31,
                            _ => 0,
                        });
                    emitter.write_bits(val as u32, 5)?;
                }
                let is_shadow = s_axiom.is_v105_shadow(self.header.flags, Some(&self.code));
                if is_shadow {
                    if let Some(bits) = self.body.alpha_shadow_skip_bits {
                        emitter.write_bits_u64(bits, 47)?;
                    } else {
                        emitter.write_bits(0, 47)?;
                    }
                }
                if !is_v105_summary
                    && (self.header.version != 5
                        || is_shadow
                        || (self.header.is_runeword && is_authority_overlap_code)
                        || (alpha_mode && s_axiom.is_compact)
                        || !self.properties.is_empty())
                {
                    // Slice 11: Write JM-to-Body alignment gap
                    let gap_len = s_axiom.header_gap(&self.code, self.header.flags);
                    if gap_len > 0 || !self.body.alpha_body_gap_bits.is_empty() {
                        let gap_seg = AlphaHeaderGap {
                            bits: if !self.body.alpha_body_gap_bits.is_empty() {
                                self.body.alpha_body_gap_bits.clone()
                            } else {
                                vec![false; gap_len as usize]
                            },
                        };
                        gap_seg.emit(emitter)?;
                    }
                    crate::domain::item::serialization::write_property_list(
                        emitter,
                        &self.code,
                        &self.properties,
                        &self.socketed_items,
                        huffman,
                        self.header.version,
                        self.header.is_runeword,
                        self.terminator_bit,
                        self.properties_complete,
                        quality_val,
                        is_shadow,
                        &s_axiom,
                    )?;
                    for set_props in &self.set_attributes {
                        crate::domain::item::serialization::write_property_list(
                            emitter,
                            &self.code,
                            set_props,
                            &[],
                            huffman,
                            self.header.version,
                            false,
                            false,
                            true,
                            quality_val,
                            false,
                            &s_axiom,
                        )?;
                    }
                }
            }
        }
        if alpha_mode {
            let current_bits = emitter.written_bits() - start_bit;
            let start_idx = current_bits as usize;
            let recorded_total = self.bits.len();
            let recorded_padding = if recorded_total > start_idx {
                // Preserve any raw tail bits captured during parsing before falling back to
                // synthesized alignment padding. This keeps compact-tail seams bit-exact.
                self.bits[start_idx..recorded_total]
                    .iter()
                    .map(|rb| rb.bit)
                    .collect::<Vec<bool>>()
            } else {
                Vec::new()
            };
            let padding_bits = if !recorded_padding.is_empty() {
                recorded_padding
            } else {
                let total_bits = self.total_bits.min(self.bits.len() as u64) as usize;
                let fallback_padding = if total_bits > start_idx && total_bits <= self.bits.len() {
                    self.bits[start_idx..total_bits]
                        .iter()
                        .map(|rb| rb.bit)
                        .collect::<Vec<bool>>()
                } else {
                    Vec::new()
                };
                if !fallback_padding.is_empty() {
                    fallback_padding
                } else {
                    self.body.alpha_alignment_padding.clone()
                }
            };

            if !padding_bits.is_empty() {
                let pad_seg = AlphaHeaderGap { bits: padding_bits };
                pad_seg.emit(emitter)?;
            }
        }

        if has_opaque_module && !is_placeholder_opaque {
            for module in &self.modules {
                match module {
                    ItemModule::Opaque(bits) | ItemModule::Residue(bits) => {
                        emitter.extend_bits(bits.iter().cloned())?;
                    }
                    _ => {}
                }
            }
        }

        if !alpha_mode && self.header.version != 5 && self.header.version != 7 {
            emitter.write_bit(false)?;
        }
        let current_bits = emitter.written_bits();
        let mut final_bits =
            s_axiom.calculate_alignment(current_bits - start_bit, &self.code, self.header.flags);

        if self.total_bits > final_bits {
            final_bits = self.total_bits;
        }

        if final_bits > (current_bits - start_bit) {
            let padding_needed = (final_bits - (current_bits - start_bit)) as u32;
            let pad_seg = AlphaHeaderGap {
                bits: vec![false; padding_needed as usize],
            };
            pad_seg.emit(emitter)?;
        }
        Ok(())
    }


    pub fn serialize_section(
        items: &[Item],
        huffman: &crate::domain::item::serialization::HuffmanTree,
        alpha_mode: bool,
    ) -> io::Result<Vec<u8>> {
        use crate::domain::item::serialization::BitEmitter;
        let mut emitter = BitEmitter::new();
        for (i, item) in items.iter().enumerate() {
            if !alpha_mode {
                emitter.extend_bits(item.gap_bits.iter().cloned())?;
            }
            if alpha_mode {
                let item_bits = item.to_bits(i, huffman, alpha_mode)?;
                emitter.extend_bits(item_bits)?;
            } else {
                let item_bytes = item.to_bytes(i, huffman, alpha_mode)?;
                for byte in item_bytes {
                    emitter.write_bits(byte as u32, 8)?;
                }
            }
            let axiom = StatsAxiom::new(
                item.header.version,
                item.header.quality.unwrap_or(ItemQuality::Normal),
                alpha_mode,
            );
            let mut is_authority_overlap_code = false;
            if alpha_mode {
                let reg = crate::domain::forensic::registry::get_registry();
                let trimmed = item.code.trim();
                if let Some(overrides) = &reg.item_overrides {
                    if let Some(map) = overrides.get(trimmed) {
                        if let Some(&val) = map.get("is_authority_overlap") { is_authority_overlap_code = val != 0; }
                    }
                }
            }

            for child in &item.socketed_items {
                let is_true_alpha_runeword = item.header.is_runeword && is_authority_overlap_code;
                if alpha_mode && axiom.is_alpha() && is_true_alpha_runeword {
                    continue;
                }
                if alpha_mode {
                    let child_bits = child.to_bits(0, huffman, alpha_mode)?;
                    emitter.extend_bits(child_bits)?;
                } else {
                    let child_bytes = child.to_bytes(0, huffman, alpha_mode)?;
                    for byte in child_bytes {
                        emitter.write_bits(byte as u32, 8)?;
                    }
                }
            }
        }
        if alpha_mode {
            let bits = emitter.into_bits();
            let full_bytes = bits.len() / 8;
            let mut out = Vec::with_capacity(full_bytes);
            for i in 0..full_bytes {
                let mut byte = 0u8;
                for bit in 0..8 {
                    if bits[i * 8 + bit] {
                        byte |= 1 << bit;
                    }
                }
                out.push(byte);
            }
            Ok(out)
        } else {
            Ok(emitter.into_bytes())
        }
    }
}

pub fn parse_item_header<R: BitRead>(
    cursor: &mut BitCursor<R>,
    alpha_mode: bool,
    code_hint: Option<&str>,
    gap_override: Option<usize>,
    is_first_item: bool,
    forced_compact: Option<bool>,
    _has_checksum_hint: Option<bool>,
    start_bit_offset: Option<u64>,
) -> ParsingResult<(ItemHeader, Option<u32>, Vec<bool>)> {
    let mut code_hint = code_hint.map(crate::item::normalize_alpha_code_hint);

    let w_axiom = V105PropertyWidthAxiom::default();
    let start_bit = cursor.pos();
    cursor.begin_segment(ItemSegmentType::Header);

    // 1. Read Flags (32 bits).
    let flags = cursor.read_bits::<u32>(32)?;
    let raw_flags = flags;
    let is_nested = cursor.context_stack().iter().any(|s| s == "nested")
        || crate::domain::header::entity::IN_NESTED_RECOVERY.with(|v| v.get());

    if !alpha_mode && !is_nested && (flags & 0xFFFF) != 0x4D4A {
        return Err(cursor.fail(ParsingError::MissingMarker {
            marker: "JM".to_string(),
            bit_offset: start_bit,
        }));
    }

    let mut alpha_checksum = None;
    let mut has_checksum = false;
    let mut version = 0;

    // 4. For Alpha v105 Summary Items (use w_axiom.is_summary_item):
    // Use version 5 as a placeholder for code checking if header version not yet known
    let is_v105_summary = alpha_mode && code_hint.map(|c| w_axiom.is_summary_item(5, c)).unwrap_or(false);

    // 2. For Alpha v105:
    if alpha_mode {
        let saved_pos = cursor.checkpoint();
        let checksum_res = cursor.read_bits::<u8>(8);
        let version_res = cursor.read_bits::<u8>(3);

        if let (Ok(ck), Ok(v)) = (checksum_res, version_res) {
            let mut matched = ck == calculate_alpha_v105_checksum(flags, v);
            
            // Forensic Override: Many Alpha v105 items have a checksum slot (8 bits)
            // even if the formula doesn't match our current understanding.
            // If it's a known summary item, we MUST consume those 8 bits to maintain rhythm.
            // Exception: hp1/mp1/tsc/isc often skip the checksum entirely (Slice 30).
            let is_known_summary = if let Some(code) = code_hint {
                matches!(code.trim(), "hp1" | "mp1" | "tsc" | "isc")
            } else {
                false
            };

            if matched && is_known_summary {
                // Checksum match for hp1/mp1/tsc/isc is often a false positive against 
                // version (3 bits) + mode (3 bits) + location (2 bits) bits.
                // Trust the raw bits instead.
                matched = false;
            }

            if !matched && is_v105_summary {
                let trimmed = code_hint.unwrap_or("").trim();
                if !matches!(trimmed, "hp1" | "mp1" | "tsc" | "isc") {
                    matched = true;
                }
            }

            if matched {
                alpha_checksum = Some(ck);
                has_checksum = true;
                version = v;
            } else {
                cursor.rollback(saved_pos);
                version = cursor.read_bits::<u8>(3).unwrap_or(0);
            }
        } else {
            cursor.rollback(saved_pos);
            version = cursor.read_bits::<u8>(3).unwrap_or(0);
        }
    } else {
        version = cursor.read_bits::<u8>(3).unwrap_or(0);
    }

    let mut flags = flags;
    if alpha_mode {
        if let Some(code) = code_hint {
            if is_potion_code(code) {
                flags |= 1 << 23;
            }
        }
    }

    // 3. Read Mode, X, Location with correct widths.
    let mode = cursor.read_bits::<u8>(w_axiom.mode_bits(alpha_mode) as u32).unwrap_or(0);
    let x = cursor.read_bits::<u8>(w_axiom.x_bits(alpha_mode, version) as u32).unwrap_or(0);
    let location = cursor.read_bits::<u8>(w_axiom.location_bits(alpha_mode, version) as u32).unwrap_or(0);

    let h_axiom = HeaderAxiom::new(version, alpha_mode);
    let s_axiom = StatsAxiom::new(version, ItemQuality::Normal, alpha_mode);
    let is_compact = forced_compact.unwrap_or_else(|| h_axiom.is_compact(flags, code_hint));
    let is_personalized = s_axiom.is_personalized(flags, is_compact);

    let mut y = 0;
    let mut page = 0;
    let mut socket_hint = 0;
    let mut alpha_header_gap = None;
    let mut alpha_header_gap_bits = Vec::new();

    // 4. For Alpha v105 Summary Items (use w_axiom.is_summary_item):
    if is_v105_summary {
        // Read Y (3 bits).
        y = cursor.read_bits::<u8>(3)?;
        // Read Gap (8 bits) and store in alpha_header_gap.
        let gap_bits = w_axiom.summary_gap_bits(code_hint.unwrap_or(""));
        if gap_bits > 0 {
            let gap_seg = cursor.with_context("AlphaHeaderGap", |c| {
                Ok(AlphaHeaderGap::parse(c, gap_bits as usize)?)
            })?;
            alpha_header_gap_bits = gap_seg.bits;
            let mut gap_val = 0u32;
            for (i, &bit) in alpha_header_gap_bits.iter().enumerate() {
                if i < 32 && bit {
                    gap_val |= 1u32 << i;
                }
            }
            alpha_header_gap = Some(gap_val);
        }
    } else {
        // 5. For others, read Y, Page, SocketHint as per geometry.
        let geometry = h_axiom.header_geometry(flags, code_hint);
        if !geometry.skip_geometry {
            y = cursor.read_bits::<u8>(geometry.y_bits).unwrap_or(0);
            page = cursor.read_bits::<u8>(geometry.page_bits).unwrap_or(0);
            socket_hint = cursor.read_bits::<u8>(geometry.socket_hint_bits).unwrap_or(0);
        }

        // Handle Gap for others if required.
        if geometry.has_header_gap {
            let gap_bits = if h_axiom.is_alpha() {
                if let Some(go) = gap_override {
                    go
                } else {
                    V105HeaderGapAxiom::default().resolve_gap(
                        version,
                        code_hint,
                        flags,
                        is_first_item,
                        is_compact,
                        has_checksum,
                        start_bit_offset,
                    )
                }
            } else if alpha_mode {
                8
            } else {
                0
            };

            if gap_bits > 0 {
                let gap_seg = cursor.with_context("AlphaHeaderGap", |c| {
                    Ok(AlphaHeaderGap::parse(c, gap_bits as usize)?)
                })?;
                alpha_header_gap_bits = gap_seg.bits;

                if version == 5 && AlphaV105HeaderGapAlignment::align_required() {
                    let _ = cursor.byte_align();
                }

                let mut val = 0u32;
                for (i, &bit) in alpha_header_gap_bits.iter().enumerate() {
                    if i < 32 && bit {
                        val |= 1u32 << i;
                    }
                }
                alpha_header_gap = Some(val);
            }
        }
    }

    if alpha_mode && !is_v105_summary {
        let geometry = h_axiom.header_geometry(flags, code_hint);
        if geometry.target_width > 0 {
            let current_bits = (cursor.pos() - start_bit) as u32;
            if current_bits < geometry.target_width {
                let to_read = geometry.target_width - current_bits;
                let available = cursor.remaining() as u32;
                let actual_read = std::cmp::min(to_read, available);
                if actual_read > 0 {
                    let pad_seg = cursor.with_context("AlphaHeaderGapPadding", |c| {
                        Ok(AlphaHeaderGap::parse(c, actual_read as usize)?)
                    })?;
                    for b in pad_seg.bits {
                        alpha_header_gap_bits.push(b);
                    }
                    let mut val = 0u32;
                    for (i, &bit) in alpha_header_gap_bits.iter().enumerate() {
                        if i < 32 && bit {
                            val |= 1u32 << i;
                        }
                    }
                    alpha_header_gap = Some(val);
                }
            }
        }
    }

    cursor.end_segment();
    Ok((
        ItemHeader {
            flags: if alpha_mode { raw_flags } else { flags },
            version,
            mode,
            location,
            x,
            y,
            page,
            socket_hint,
            id: None,
            level: None,
            quality: None,
            is_compact,
            is_identified: s_axiom.is_identified(flags),
            is_socketed: s_axiom.is_socketed(flags, is_compact, code_hint),
            is_personalized,
            is_runeword: h_axiom.is_runeword(flags, code_hint, has_checksum),
            is_ethereal: s_axiom.is_ethereal(flags),
            is_ear: !alpha_mode && (flags & (1 << 24)) != 0,
            has_checksum,
            alpha_checksum,
            alpha_quality_raw: None,
            alpha_v5_runeword_extra: None,
            alpha_unique_id_raw: None,
            save_is_alpha: alpha_mode,
        },
        alpha_header_gap,
        alpha_header_gap_bits,
    ))
}

pub fn parse_item_body<R: BitRead>(
    cursor: &mut BitCursor<R>,
    huff: &crate::domain::item::serialization::HuffmanTree,
    header: &ItemHeader,
    alpha_mode: bool,
    code_hint: Option<&str>,
) -> ParsingResult<(ItemBody, Vec<bool>, Option<u8>, Option<u8>, Option<String>)> {
    let code_hint = code_hint.map(crate::item::normalize_alpha_code_hint);
    let w_axiom = V105PropertyWidthAxiom::default();
    let h_axiom = HeaderAxiom::new(header.version, alpha_mode);
    let is_ear = header.is_ear;
    let (code, alpha_code_bits, alpha_nudge, ear_class, ear_level, ear_player_name) = if is_ear {
        let w_axiom = V105PropertyWidthAxiom::default();
        cursor.begin_segment(ItemSegmentType::Unknown);
        let class = Some(cursor.read_bits::<u8>(w_axiom.ear_class_bits() as u32)? as u8);
        let level = Some(cursor.read_bits::<u8>(w_axiom.ear_level_bits() as u32)? as u8);
        let name = Some(crate::domain::item::serialization::read_player_name(
            cursor,
            alpha_mode && w_axiom.is_ear_name_v5_style(header.version),
        )?);
        if alpha_mode
            && w_axiom.needs_ear_name_byte_alignment(header.version)
            && AlphaV105EarAlignment::align_required()
        {
            cursor.byte_align()?;
        }
        cursor.end_segment();
        (String::new(), Vec::new(), None, class, level, name)
    } else {
        cursor.begin_segment(ItemSegmentType::Code);
        let code_start = cursor.pos();
        let mut code = String::new();
        let mut alpha_code_bits = Vec::new();
        let _s_axiom = StatsAxiom::new(header.version, ItemQuality::Normal, alpha_mode)
            .with_compact(header.is_compact);

        if alpha_mode && code.is_empty() {
            if let Some(hint) = code_hint {
                let trimmed = hint.trim();
                if crate::domain::forensic::v105::axioms::is_v105_summary_code(trimmed) {
                    code = trimmed.to_string();
                    // Capture the 16-bit ID/Stealth bits
                    alpha_code_bits = cursor.read_bits_as_vec(16)?;
                }
            }
        }

        if alpha_mode && code.is_empty() {
            let saved_pos = cursor.pos();
            if let Ok(bits) = cursor.read_bits_as_vec(24) {
                if let Some(stealth) =
                    crate::domain::forensic::v105::axioms::V105StealthCodeAxiom::default()
                        .resolve_stealth_code(&bits)
                {
                    code = stealth.to_string();
                } else {
                    cursor.rollback(saved_pos);
                }
            } else {
                cursor.rollback(saved_pos);
            }
        }

        if alpha_mode && header.is_compact && code.is_empty() {
            let mut is_summary_candidate = false;
            if let Some(hint) = code_hint {
                is_summary_candidate = w_axiom.is_summary_item(header.version, hint);
                if !is_summary_candidate {
                    let trimmed = hint.trim();
                    is_summary_candidate = trimmed == "hp1"
                        || trimmed == "mp1"
                        || trimmed == "tsc"
                        || trimmed == "isc";
                }
            }

            if is_summary_candidate {
                let saved_pos = cursor.pos();
                let mut temp_code = String::new();
                let mut success = true;
                for _ in 0..3 {
                    match cursor.read_bits::<u8>(8) {
                        Ok(ch) => {
                            temp_code.push(ch as char);
                        }
                        Err(_) => {
                            success = false;
                            break;
                        }
                    }
                }

                if success && w_axiom.is_summary_item(header.version, &temp_code) {
                    code = temp_code;
                } else if success {
                    let trimmed_temp = temp_code.trim();
                    if let Some(stripped) = trimmed_temp.strip_prefix('w') {
                        if w_axiom.is_summary_item(header.version, stripped) {
                            code = stripped.to_string();
                        } else {
                            cursor.rollback(saved_pos);
                        }
                    } else {
                        cursor.rollback(saved_pos);
                    }
                } else {
                    cursor.rollback(saved_pos);
                }
            }
        }

        if alpha_mode && header.is_compact && code.is_empty() {
            if let Some(hint) = code_hint {
                let trimmed_hint = hint.trim();
                let trusted_compact_hint =
                    matches!(trimmed_hint, "buc" | "ucb8" | "bwcw" | "jav" | "xrs" | "c8xr" | "rhd");
                if !trimmed_hint.is_empty()
                    && (trusted_compact_hint
                        || h_axiom.is_plausible(
                            header.mode,
                            header.location,
                            trimmed_hint.as_bytes(),
                            header.flags,
                        ))
                {
                    let normalized_hint = match trimmed_hint {
                        "buc" => format!("{trimmed_hint} "),
                        "jav" => "us g".to_string(),
                        _ => trimmed_hint.to_string(),
                    };
                    let hint_axiom =
                        StatsAxiom::new(header.version, ItemQuality::Normal, alpha_mode)
                            .with_compact(header.is_compact)
                            .with_code(trimmed_hint);

                    let consume_len = if hint_axiom.code_encoding()
                        == crate::domain::stats::axiom::CodeEncoding::Ascii3x8
                    {
                        24
                    } else if let Ok(bits) = huff.encode(normalized_hint.as_str()) {
                        bits.len() as u32
                    } else {
                        0
                    };
                    if consume_len > 0 {
                        let saved_pos = cursor.pos();
                        if let Ok(bits) = cursor.read_bits_as_vec(consume_len) {
                            if trimmed_hint == "buc" {
                                // Buckler-style compact tails keep their consumed bits for
                                // fidelity, but the raw Huffman decode is still authoritative
                                // when it can recover a concrete code.
                                alpha_code_bits = bits;
                                cursor.rollback(saved_pos);
                            } else {
                                code = normalized_hint;
                                alpha_code_bits = bits;
                            }
                        } else {
                            cursor.rollback(saved_pos);
                            if trusted_compact_hint {
                                code = normalized_hint;
                                // Slice19 boundary guard:
                                // If trusted compact tail code ("buc") cannot consume its own code bits,
                                // drain the remaining tail to prevent a synthetic trailing residue item.
                                if trimmed_hint == "buc" {
                                    let tail_bits = cursor.remaining() as u32;
                                    if tail_bits > 0 && tail_bits < consume_len {
                                        if let Ok(bits) = cursor.read_bits_as_vec(tail_bits) {
                                            alpha_code_bits = bits;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if code.is_empty() {
            for i in 0..4 {
                if alpha_mode && i == 3 {
                    let trimmed = code.trim();
                    if trimmed == "us g"
                        || trimmed == "jav"
                        || trimmed == "buc"
                        || crate::domain::forensic::v105::axioms::is_v105_summary_code(trimmed)
                    {
                        break;
                    }
                }
                match huff.decode_recorded(cursor) {
                    Ok(ch) => code.push(ch),
                    Err(e) => {
                        if alpha_mode {
                            let saved_pos = cursor.pos();
                            if let Ok(_) = cursor.read_bit() {
                                if let Ok(ch) = huff.decode_recorded(cursor) {
                                    code.push(ch);
                                    continue;
                                }
                            }
                            cursor.rollback(saved_pos);
                            if let Ok(_) =
                                cursor.read_bits::<u8>(w_axiom.nudge_bits(header.version) as u32)
                            {
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

        let mut alpha_nudge = None;
        if alpha_mode {
            if h_axiom.is_alpha()
                && !header.is_compact
                && !w_axiom.is_summary_item(header.version, &code)
            {
                if header.version == 5 {
                    let nudge_val =
                        cursor.read_bits::<u8>(w_axiom.nudge_bits(header.version) as u32)?;
                    alpha_nudge = Some(nudge_val);
                } else if header.version == 0 || header.version == 2 {
                    alpha_nudge = Some(cursor.with_context("AlphaNudge", |c| {
                        c.read_bits::<u8>(w_axiom.nudge_bits(header.version) as u32)
                    })?);
                }
            }
        }

        if alpha_code_bits.is_empty() {
            let code_end = cursor.pos();
            if code_end > code_start {
                alpha_code_bits = cursor
                    .recorded_bits()
                    .iter()
                    .filter(|bit| bit.offset >= code_start && bit.offset < code_end)
                    .map(|bit| bit.bit)
                    .collect();
            }
        }

        if w_axiom.needs_post_body_byte_alignment(header.version, header.is_compact)
            && AlphaV105PostBodyAlignment::align_required()
        {
            cursor.byte_align()?;
        }

        cursor.end_segment();
        (code, alpha_code_bits, alpha_nudge, None, None, None)
    };

    Ok((
        ItemBody {
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
            alpha_header_gap_bits: Vec::new(),
            alpha_code_bits: alpha_code_bits.clone(),
            v5_runeword_extra: None,
            v105_7mgw_payload: None,
            alpha_nudge,
            alpha_set_list_val: None,
            alpha_shadow_skip_bits: None,
            alpha_body_gap_bits: Vec::new(),
            alpha_alignment_padding: Vec::new(),
        },
        alpha_code_bits,
        ear_class,
        ear_level,
        ear_player_name,
    ))
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
        let w_axiom = V105PropertyWidthAxiom::default();
        let is_fragment = h_axiom.is_alpha()
            && (version == 5 || version == 2 || version == 1)
            && ((header.flags & (1 << 26)) != 0 || (header.flags & (1 << 27)) != 0);
        let is_alpha_early_exit =
            h_axiom.is_alpha() && w_axiom.is_extended_stats_early_exit(version);
        let soft_truncate_on_limit =
            alpha_mode && matches!(trimmed_code, "jav" | "buc" | "ucb8" | "xrs" | "c8xr");
        macro_rules! read_or_truncate {
            ($expr:expr) => {{
                match $expr {
                    Ok(value) => value,
                    Err(e) => {
                        let failure = crate::error::ParsingFailure::from(e);
                        if soft_truncate_on_limit
                            && matches!(
                                &failure.error,
                                crate::error::ParsingError::Io(msg)
                                    if msg.contains("Bit limit exceeded")
                                        || msg.contains("failed to fill whole buffer")
                                        || msg.contains("end of bitstream")
                            )
                        {
                            cursor.end_segment();
                            return Ok(data);
                        }
                        return Err(failure);
                    }
                }
            }};
        }
        if axiom.is_alpha() {
            if h_axiom.is_alpha() && w_axiom.is_summary_item(version, trimmed_code) {
                // Alpha v105 (Slice 2): Summary items use unified 16-bit ID,
                // bits already consumed in parse_item_body.
                data.id = Some(0); 
                cursor.end_segment();
                return Ok(data);
            }

            if !is_compact {
                let quality_raw =
                    read_or_truncate!(cursor.read_bits::<u8>(w_axiom.quality_bits(true) as u32));
                let quality = ItemQuality::from(quality_raw);
                data.alpha_quality_raw = Some(quality_raw);
                data.quality = Some(quality);

                let is_authority_runeword =
                    alpha_mode && (trimmed_code == "xrs" || trimmed_code == "c8xr");
                if w_axiom.has_v5_runeword_extra(version)
                    && (is_runeword
                        || is_fragment
                        || h_axiom.is_v105_shadow(header.flags, Some(&code)))
                {
                    if is_authority_runeword {
                        data.id = Some(0);
                    } else {
                        data.v5_runeword_extra = Some(read_or_truncate!(cursor
                            .with_context("AlphaV5RunewordExtra", |c| c
                                .read_bits::<u8>(w_axiom.v5_runeword_extra_bits() as u32))));
                        data.id = Some(0);
                    }
                } else {
                    data.id = Some(read_or_truncate!(
                        cursor.read_bits::<u32>(w_axiom.item_id_bits() as u32)
                    ));
                    data.level = Some(read_or_truncate!(
                        cursor.read_bits::<u8>(w_axiom.item_level_bits() as u32)
                    ));
                }
            } else {
                data.id = Some(0);
            }
        } else {
            let is_nested = crate::domain::header::entity::IN_NESTED_RECOVERY.with(|v| v.get());
            if is_nested && is_compact {
                data.id = Some(0);
            } else {
                data.id = Some(read_or_truncate!(
                    cursor.read_bits::<u32>(w_axiom.item_id_bits() as u32)
                ));
                data.level = Some(read_or_truncate!(
                    cursor.read_bits::<u8>(w_axiom.item_level_bits() as u32)
                ));
                let quality_raw =
                    read_or_truncate!(cursor.read_bits::<u8>(w_axiom.quality_bits(false) as u32));
                data.quality = Some(ItemQuality::from(quality_raw));
            }
        }
        if is_alpha_early_exit {
            cursor.end_segment();
            return Ok(data);
        }

        let template = crate::domain::item::serialization::item_template(trimmed_code);
        data.has_multiple_graphics = matches!(trimmed_code, "rin" | "amu" | "cm1" | "cm2" | "cm3");
        data.has_class_specific_data = false;

        if data.has_multiple_graphics {
            data.multi_graphics_bits = Some(read_or_truncate!(
                cursor.read_bits::<u8>(w_axiom.multi_graphics_bits() as u32)
            ) as u8);
        }
        if data.has_class_specific_data {
            data.class_specific_bits = Some(read_or_truncate!(
                cursor.read_bits::<u16>(w_axiom.class_specific_bits() as u32)
            ) as u16);
        }
        let quality_val = data.quality.unwrap_or(ItemQuality::Normal);
        match quality_val {
            ItemQuality::Low | ItemQuality::High => {
                data.low_high_graphic_bits = Some(read_or_truncate!(
                    cursor.read_bits::<u8>(w_axiom.low_high_graphic_bits() as u32)
                ) as u8);
            }
            ItemQuality::Magic => {
                let seg = read_or_truncate!(MagicAffixSegment::parse(cursor));
                data.magic_prefix = seg.prefix;
                data.magic_suffix = seg.suffix;
            }
            ItemQuality::Rare | ItemQuality::Crafted => {
                let seg = read_or_truncate!(RareAffixSegment::parse(cursor));
                data.rare_name_1 = seg.names[0];
                data.rare_name_2 = seg.names[1];
                data.rare_affixes = seg.affixes;
            }
            ItemQuality::Set | ItemQuality::Unique => {
                let seg = read_or_truncate!(UniqueAffixSegment::parse(cursor));
                let uid = seg.unique_id.unwrap_or(0);
                if alpha_mode {
                    data.alpha_unique_id_raw = Some(uid);
                }
                data.unique_id = Some(uid);
            }
            _ => {}
        }
        if is_runeword && !is_fragment && version != 5 {
            data.runeword_id = Some(read_or_truncate!(
                cursor.read_bits::<u16>(w_axiom.runeword_id_bits() as u32)
            ) as u16);
            data.runeword_level = Some(read_or_truncate!(
                cursor.read_bits::<u8>(w_axiom.runeword_level_bits() as u32)
            ) as u8);
        }
        if is_personalized {
            if alpha_mode
                && w_axiom.needs_player_name_byte_alignment(version)
                && AlphaV105PersonalizedAlignment::align_required()
            {
                read_or_truncate!(cursor.byte_align());
            }
            data.personalized_player_name = Some(read_or_truncate!(
                crate::domain::item::serialization::read_player_name(
                    cursor,
                    alpha_mode && w_axiom.is_player_name_alpha_style(version)
                )
            ));
        }
        if trimmed_code == "tbk" || trimmed_code == "ibk" {
            data.tbk_ibk_teleport = Some(read_or_truncate!(
                cursor.read_bits::<u8>(w_axiom.teleport_bits() as u32)
            ) as u8)
        }

        let is_ear = !alpha_mode && (header.flags & (1 << 24)) != 0;
        if !is_ear && !alpha_mode && header.version != 5 && header.version != 7 {
            let has_realm_data = read_or_truncate!(cursor.read_bit());
            if has_realm_data {
                let _realm_data_1 = read_or_truncate!(cursor.read_bits::<u32>(32));
                let _realm_data_2 = read_or_truncate!(cursor.read_bits::<u32>(32));
                let _realm_data_3 = read_or_truncate!(cursor.read_bits::<u32>(32));
            }
        }
        data.timestamp_flag = read_or_truncate!(cursor.read_bit());
        let (reads_defense, reads_durability, reads_quantity) = if let Some(template) = template {
            (
                template.is_armor,
                template.has_durability,
                template.is_stackable,
            )
        } else {
            let is_scroll = trimmed_code == "tsc" || trimmed_code == "isc";
            let is_authority_xrs = alpha_mode && (trimmed_code == "xrs" || trimmed_code == "c8xr");
            let armor_like_unknown =
                data.has_class_specific_data || trimmed_code.contains(' ') || is_authority_xrs;
            (armor_like_unknown, armor_like_unknown, is_scroll)
        };
        if reads_defense && axiom.reads_defense() {
            data.defense = Some(read_or_truncate!(
                cursor.read_bits::<u32>(w_axiom.stat_bits(31) as u32)
            ));
        }
        if reads_durability && axiom.reads_durability() {
            let max_bits = w_axiom.stat_bits(73);
            let cur_bits = w_axiom.stat_bits(72);
            let m_dur = read_or_truncate!(cursor.read_bits::<u32>(max_bits as u32));
            data.max_durability = Some(m_dur);
            if m_dur > 0 {
                data.current_durability =
                    Some(read_or_truncate!(cursor.read_bits::<u32>(cur_bits as u32)));
                let _extra = read_or_truncate!(cursor.read_bit());
            }
        }
        if reads_quantity && axiom.reads_quantity() {
            data.quantity = Some(read_or_truncate!(
                cursor.read_bits::<u32>(w_axiom.quantity_bits() as u32)
            ));
        }
        if is_socketed_flag {
            data.sockets =
                Some(read_or_truncate!(cursor.read_bits::<u8>(w_axiom.socket_bits() as u32)) as u8);
        }
        if quality_val == ItemQuality::Set {
            let val = read_or_truncate!(cursor.read_bits::<u8>(w_axiom.set_list_bits() as u32));
            data.alpha_set_list_val = Some(val);
            data.set_list_count = match val {
                1 => 1,
                3 => 2,
                7 => 3,
                15 => 4,
                31 => 5,
                _ => 0,
            };
        }
        cursor.end_segment();
        Ok(data)
    }
}

fn is_potion_code(code: &str) -> bool {
    let trimmed = code.trim();
    trimmed.starts_with('h')
        || trimmed.starts_with('m')
        || (trimmed.starts_with('r') && trimmed.len() <= 3)
        || trimmed == "vps"
        || trimmed == "yps"
        || trimmed == "wms"
        || trimmed.starts_with('o')
        || trimmed.starts_with('g')
        || trimmed == "ice"
        || trimmed == "xyz"
        || trimmed == "wwsw"
        || trimmed.starts_with('7')
}
