// This software is licensed under the PolyForm Noncommercial License 1.0.0.
// Required Notice: Copyright 2026 N0FreeLunch (https://github.com/N0FreeLunch/d2r-core)

use crate::item::{HuffmanTree, Item};
pub use crate::domain::progression::axiom::PROG_START_FILE;
pub use crate::domain::header::axiom::{
    ACTIVE_ACT_OFFSET, ACTIVE_WEAPON_OFFSET, CHAR_CLASS_OFFSET, CHAR_LEVEL_OFFSET,
    CHAR_NAME_LEN, CHAR_NAME_OFFSET, CHECKSUM_OFFSET, D2S_MAGIC, FILE_SIZE_OFFSET,
    LAST_PLAYED_OFFSET, MAGIC_OFFSET, MIN_HEADER_LEN, PROGRESS_FLAG_OFFSET, VERSION_OFFSET,
};
pub use crate::domain::stats::{AttributeSection, AttributeEntry};
pub use crate::domain::character::skills::{SkillSection, SKILL_SECTION_LEN};
pub use crate::domain::progression::{QuestSection, WaypointSection};
use std::io;
use std::mem;
use crate::domain::forensic::v105::{V105JmMarkerAxiom, V105SectionMarkerAxiom};
use bitstream_io::BitRead;


#[derive(Debug, Clone)]
pub struct Header {
    pub magic: u32,
    pub version: u32,
    pub file_size: u32,
    pub checksum: u32,
    pub active_weapon: u32,
    pub char_name: String,
    pub char_class: u8,
    pub char_level: u8,
    pub active_act: u8,
    pub progress_flag: u8,
    pub last_played: u32,
    pub raw_prefix: Vec<u8>,
    pub quests: Option<QuestSection>,
    pub waypoints: Option<WaypointSection>,
    pub expansion: Option<ExpansionSection>,
}

#[derive(Debug, Clone)]
pub struct Save {
    pub header: Header,
}

pub fn class_name(class_id: u8) -> &'static str {
    match class_id {
        0 => "Amazon",
        1 => "Sorceress",
        2 => "Necromancer",
        3 => "Paladin",
        4 => "Barbarian",
        5 => "Druid",
        6 => "Assassin",
        7 => "Warlock",
        _ => "Unknown",
    }
}

/// Returns the base skill ID for a given character class.
pub fn class_skill_base_id(class_id: u8) -> Option<u32> {
    let code = match class_id {
        0 => "ama",
        1 => "sor",
        2 => "nec",
        3 => "pal",
        4 => "bar",
        5 => "dru",
        6 => "ass",
        7 => "war",
        _ => return None,
    };
    crate::domain::character::skills::find_base_skill_id(code)
}
/// A bridge helper to get skill level using class_id.
pub fn get_skill_level_by_class(skills: &SkillSection, class_id: u8, skill_id: u32) -> u8 {
    if let Some(base_id) = class_skill_base_id(class_id) {
        skills.get_level(base_id, skill_id)
    } else {
        0
    }
}

pub fn find_jm_markers(bytes: &[u8]) -> Vec<usize> {
    let axiom = V105JmMarkerAxiom::default();
    axiom.scan(bytes)
}

#[derive(Debug, Clone)]
pub struct SaveSectionMap {
    pub gf_pos: usize,
    pub if_pos: usize,
    pub jm_positions: Vec<usize>,
    // Alpha v105 progression markers
    pub woo_pos: Option<usize>,
    pub ws_pos: Option<usize>,
    pub w4_pos: Option<usize>,
    // Alpha v105 mercenary markers
    pub jf_pos: Option<usize>,
    pub kf_pos: Option<usize>,
    pub lf_pos: Option<usize>,
}

impl SaveSectionMap {
    pub fn first_jm(&self) -> usize {
        *self.jm_positions.first().expect("jm_positions is non-empty")
    }
}

pub fn map_core_sections(bytes: &[u8]) -> io::Result<SaveSectionMap> {
    let version = if bytes.len() >= 8 {
        u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]))
    } else {
        0
    };

    let section_axiom = V105SectionMarkerAxiom::default();
    let gf_pos = section_axiom.find_gf(bytes).ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "gf marker not found in save file")
    })?;
    let if_pos = section_axiom.find_if(bytes).ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "if marker not found in save file")
    })?;
    let jm_axiom = V105JmMarkerAxiom::default();
    let jm_positions = jm_axiom.scan(bytes);
    if jm_positions.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "No JM markers found in save file",
        ));
    }
    if !(gf_pos < if_pos && if_pos < jm_positions[0]) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Save markers are out of order",
        ));
    }

    let (woo_pos, ws_pos, w4_pos, jf_pos, kf_pos, lf_pos) = if version == 105 {
        (
            section_axiom.find_woo(bytes), // 'Woo!'
            section_axiom.find_ws(bytes),  // 'WS'
            section_axiom.find_w4(bytes),  // 'w4'
            section_axiom.find_jf(bytes),  // Mercenary marker
            section_axiom.find_kf(bytes),  // Mercenary data marker 1
            section_axiom.find_lf(bytes),  // Mercenary data marker 2
        )
    } else {
        (None, None, None, None, None, None)
    };

    Ok(SaveSectionMap {
        gf_pos,
        if_pos,
        jm_positions,
        woo_pos,
        ws_pos,
        w4_pos,
        jf_pos,
        kf_pos,
        lf_pos,
    })
}

pub fn gf_payload_range(map: &SaveSectionMap) -> std::ops::Range<usize> {
    let section_axiom = V105SectionMarkerAxiom::default();
    let start = map.gf_pos + section_axiom.gf_len();
    start..map.if_pos
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemSlotClass {
    InventoryLike,
    EquipmentLike,
    StashLike,
    SocketChild,
    Unknown,
}

pub fn classify_item_slot(item: &Item) -> ItemSlotClass {
    if item.mode == 6 || item.location == 6 {
        return ItemSlotClass::SocketChild;
    }
    if item.page >= 4 {
        return ItemSlotClass::StashLike;
    }
    if item.location == 0 {
        return ItemSlotClass::InventoryLike;
    }
    if (1..=3).contains(&item.location) {
        return ItemSlotClass::EquipmentLike;
    }
    ItemSlotClass::Unknown
}

pub fn collect_player_slots(
    bytes: &[u8],
    huffman: &HuffmanTree,
) -> io::Result<Vec<(Item, ItemSlotClass)>> {
    let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]));
    let alpha_mode = version == 105;
    let items = Item::read_player_items(bytes, huffman, alpha_mode)?;
    let mut slots = Vec::new();
    for item in items {
        push_with_children(&mut slots, item);
    }
    Ok(slots)
}

fn push_with_children(slots: &mut Vec<(Item, ItemSlotClass)>, mut item: Item) {
    let children = mem::take(&mut item.socketed_items);
    let class = classify_item_slot(&item);
    slots.push((item, class));
    for child in children {
        push_with_children(slots, child);
    }
}

pub fn rebuild_status_and_player_items(
    bytes: &[u8],
    attributes: Option<&AttributeSection>,
    skills: Option<&SkillSection>,
    quests: Option<&QuestSection>,
    waypoints: Option<&WaypointSection>,
    expansion: Option<&ExpansionSection>,
    items: &[Item],
    huffman: &HuffmanTree,
) -> io::Result<Vec<u8>> {
    let map = map_core_sections(bytes)?;
    let mut result = Vec::with_capacity(bytes.len() + 64);

    // 1. Prefix: Header up to 'gf' marker
    let mut header_bytes = bytes[..map.gf_pos].to_vec();
    
    // Detect version from header_bytes
    let version = u32::from_le_bytes(header_bytes[4..8].try_into().unwrap_or([0; 4]));

    let section_axiom = V105SectionMarkerAxiom::default();

    // Update QUESTS if present (Alpha v105)
    if version == 105 {
        if let Some(qs) = quests {
            section_axiom.sync_quests(&mut header_bytes, map.woo_pos, map.ws_pos, qs.as_slice());
        }
    }

    // Update WAYPOINTS if present (Alpha v105)
    if version == 105 {
        if let Some(wps) = waypoints {
            section_axiom.sync_waypoints(&mut header_bytes, map.ws_pos, map.w4_pos, wps.as_slice());
        }
    }

    // Update NPC Section if present (Alpha v105)
    if version == 105 {
        if let Some(npc) = expansion {
            section_axiom.sync_npc_section(&mut header_bytes, map.w4_pos, npc.as_slice());
        }
    }

    // Synchronize Header Level with Stat Section (id 12) to prevent engine-level reset.
    if let Some(attr) = attributes {
        if let Some(lv) = attr.actual_value(12, version == 105) {
            section_axiom.sync_char_level(&mut header_bytes, lv as u8, CHAR_LEVEL_OFFSET);
        }
    }
    
    result.extend_from_slice(&header_bytes);

    let is_alpha = version == 105;
    // 2. GF Section
    if let Some(attr) = attributes {
        result.extend_from_slice(&attr.to_bytes(is_alpha)?);
    } else {
        result.extend_from_slice(&bytes[map.gf_pos..map.if_pos]);
    }

    // 3. IF Section (Marker + skills)
    let section_axiom = V105SectionMarkerAxiom::default();
    if let Some(skills) = skills {
        result.extend_from_slice(section_axiom.if_bytes());
        result.extend_from_slice(skills.as_slice());
    } else {
        let skill_end = if version == 105 {
            map.jm_positions[0].min(map.if_pos + section_axiom.if_len() + SKILL_SECTION_LEN)
        } else {
            map.if_pos + section_axiom.if_len() + SKILL_SECTION_LEN
        };
        result.extend_from_slice(&bytes[map.if_pos..skill_end]);
    }

    // 4. Quest/Progression Section (Gap between IF end and first JM)
    let jm0 = map.jm_positions[0];
    let skill_end_original = if version == 105 {
        jm0.min(map.if_pos + section_axiom.if_len() + SKILL_SECTION_LEN)
    } else {
        map.if_pos + section_axiom.if_len() + SKILL_SECTION_LEN
    };

    if let Some(q) = quests {
        if version != 105 {
            result.extend_from_slice(q.as_slice());
        } else {
            // For Alpha v105, quests are in header, but we still respect the gap if it exists.
            if jm0 > skill_end_original {
                result.extend_from_slice(&bytes[skill_end_original..jm0]);
            }
        }
    } else if jm0 > skill_end_original {
        result.extend_from_slice(&bytes[skill_end_original..jm0]);
    }

    // 5. Item Sections (Player, Corpse, etc.)
    let jm0 = map.jm_positions[0];
    result.extend_from_slice(&bytes[jm0..]);

    let is_alpha = version == 105;
    rebuild_item_section(&result, items, huffman, is_alpha)
}

pub fn patch_level(bytes: &[u8], new_level: u8, huffman: &HuffmanTree) -> io::Result<Vec<u8>> {
    let map = map_core_sections(bytes)?;
    let mut attrs = AttributeSection::parse(bytes, map.gf_pos, map.if_pos)?;

    // Update level in gf section (stat_id=12, save_add=0, bit_width=7)
    attrs.set_raw(12, new_level as u32);

    let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]));
    // Synchronize header CHAR_LEVEL_OFFSET (byte 27)
    let mut working = rebuild_status_and_player_items(
        bytes,
        Some(&attrs),
        None,
        None,
        None,
        None,
        &Item::read_player_items(bytes, huffman, version == 105)?,
        huffman,
    )?;
    working[CHAR_LEVEL_OFFSET] = new_level;
    finalize_save_bytes(&mut working, false)?;
    Ok(working)
}


pub fn parse_skill_section(bytes: &[u8], map: &SaveSectionMap) -> io::Result<SkillSection> {
    crate::domain::character::skills::parse_skill_section(bytes, map.if_pos, map.jm_positions.first().cloned())
}

pub fn patch_skill_section(
    bytes: &[u8],
    map: &SaveSectionMap,
    skills: &SkillSection,
) -> io::Result<Vec<u8>> {
    let section_axiom = V105SectionMarkerAxiom::default();
    let start = map.if_pos + section_axiom.if_len();
    let end = start + SKILL_SECTION_LEN;
    if end > bytes.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "skill section truncated",
        ));
    }

    let mut rebuilt = bytes.to_vec();
    rebuilt[start..end].copy_from_slice(skills.as_slice());
    finalize_save_bytes(&mut rebuilt, false)?;
    Ok(rebuilt)
}

#[derive(Debug, Clone)]
pub struct ExpansionSection {
    pub raw_bytes: Vec<u8>,
}

impl ExpansionSection {
    pub fn from_slice(slice: &[u8]) -> Self {
        ExpansionSection {
            raw_bytes: slice.to_vec(),
        }
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.raw_bytes
    }

    pub fn is_activated_by_name(&self, difficulty: u8, name: &str) -> bool {
        if let Some(entry) = crate::data::waypoints::WAYPOINTS.iter().find(|e| e.name == name) {
            // ExpansionSection implementation for waypoints (fallback/legacy)
            // It assumes raw_bytes starts at 'WS' marker (0x2BD) or follows standard WS mapping.
            let global_bit_idx = (difficulty as usize * 24 * 8) + (10 * 8) + entry.ws_bit as usize;
            let byte_idx = global_bit_idx / 8;
            let bit_in_byte = global_bit_idx % 8;
            if byte_idx < self.raw_bytes.len() {
                return self.raw_bytes[byte_idx] & (1 << bit_in_byte) != 0;
            }
        }
        false
    }

    pub fn set_activated_by_name(&mut self, name: &str, difficulty: u8, active: bool) -> bool {
        if let Some(entry) = crate::data::waypoints::WAYPOINTS.iter().find(|e| e.name == name) {
            let global_bit_idx = (difficulty as usize * 24 * 8) + (10 * 8) + entry.ws_bit as usize;
            let byte_idx = global_bit_idx / 8;
            let bit_in_byte = global_bit_idx % 8;
            if byte_idx < self.raw_bytes.len() {
                if active {
                    self.raw_bytes[byte_idx] |= 1 << bit_in_byte;
                } else {
                    self.raw_bytes[byte_idx] &= !(1 << bit_in_byte);
                }
                return true;
            }
        }
        false
    }
}

pub fn parse_quest_section(bytes: &[u8], map: &SaveSectionMap) -> io::Result<QuestSection> {
    let section_axiom = V105SectionMarkerAxiom::default();
    let skill_end = map.if_pos + section_axiom.if_len() + SKILL_SECTION_LEN;
    let jm0 = map.jm_positions[0];
    if jm0 < skill_end {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "JM section starts before skill section ends",
        ));
    }
    Ok(QuestSection::from_slice(&bytes[skill_end..jm0]))
}

pub use crate::engine::checksum::{finalize_save_bytes, recalculate_checksum};

fn parse_ascii_field(bytes: &[u8], offset: usize, len: usize) -> io::Result<String> {
    let end = offset + len;
    if bytes.len() < end {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Save file is too small for ASCII field at offset {offset}."),
        ));
    }

    let field = &bytes[offset..end];
    let nul = field
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(field.len());
    Ok(String::from_utf8_lossy(&field[..nul]).to_string())
}

fn write_u32_le(bytes: &mut [u8], offset: usize, value: u32) -> io::Result<()> {
    let end = offset + 4;
    if end > bytes.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Buffer too small for u32 write at offset {offset}."),
        ));
    }
    bytes[offset..end].copy_from_slice(&value.to_le_bytes());
    Ok(())
}

fn write_ascii_nul_padded(
    bytes: &mut [u8],
    offset: usize,
    len: usize,
    value: &str,
) -> io::Result<()> {
    let end = offset + len;
    if end > bytes.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Buffer too small for ASCII field at offset {offset}."),
        ));
    }

    if !value.is_ascii() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Character name must be ASCII.",
        ));
    }

    let bytes_value = value.as_bytes();
    if bytes_value.len() >= len {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "Character name too long (max {} bytes without terminator).",
                len - 1
            ),
        ));
    }

    bytes[offset..end].fill(0);
    bytes[offset..offset + bytes_value.len()].copy_from_slice(bytes_value);
    Ok(())
}

impl Save {
    pub fn from_bytes(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() < MIN_HEADER_LEN {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Save file is too small for a D2R header ({MIN_HEADER_LEN} bytes)."),
            ));
        }

        let magic = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
        if magic != D2S_MAGIC {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "Invalid magic number: expected 0x{:08X}, got 0x{:08X}",
                    D2S_MAGIC, magic
                ),
            ));
        }

        let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
        let file_size = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
        let checksum = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
        let active_weapon = u32::from_le_bytes(
            bytes[ACTIVE_WEAPON_OFFSET..ACTIVE_WEAPON_OFFSET + 4]
                .try_into()
                .unwrap(),
        );
        let char_class = bytes[CHAR_CLASS_OFFSET];
        let char_level = bytes[CHAR_LEVEL_OFFSET];
        let active_act = bytes[ACTIVE_ACT_OFFSET];
        let progress_flag = bytes[PROGRESS_FLAG_OFFSET];
        let last_played = u32::from_le_bytes(
            bytes[LAST_PLAYED_OFFSET..LAST_PLAYED_OFFSET + 4]
                .try_into()
                .unwrap(),
        );
        let char_name = parse_ascii_field(bytes, CHAR_NAME_OFFSET, CHAR_NAME_LEN)?;

        let header = Header {
            magic,
            version,
            file_size,
            checksum,
            active_weapon,
            char_name,
            char_class,
            char_level,
            active_act,
            progress_flag,
            last_played,
            raw_prefix: bytes[..match version {
                105 => V105SectionMarkerAxiom::V105_HEADER_LEN, // Alpha v105 Fixed Header
                _ => MIN_HEADER_LEN,
            }
            .min(bytes.len())]
                .to_vec(),
            quests: if version == 105 {
                let map = map_core_sections(bytes).ok();
                let start = map.as_ref().and_then(|m| m.woo_pos).unwrap_or(V105SectionMarkerAxiom::V105_QUEST_OFFSET);
                let end = map.as_ref().and_then(|m| m.ws_pos).unwrap_or(V105SectionMarkerAxiom::V105_WAYPOINT_OFFSET);
                if bytes.len() >= start + 12 {
                    Some(QuestSection::from_slice(&bytes[start..end.min(bytes.len())]))
                } else {
                    None
                }
            } else {
                None
            },
            waypoints: if version == 105 {
                let map = map_core_sections(bytes).ok();
                let start = map.as_ref().and_then(|m| m.ws_pos).unwrap_or(V105SectionMarkerAxiom::V105_WAYPOINT_OFFSET);
                let end = map.as_ref().and_then(|m| m.w4_pos).unwrap_or(V105SectionMarkerAxiom::V105_NPC_OFFSET);
                if bytes.len() >= start + 2 {
                    Some(WaypointSection::from_slice(&bytes[start..end.min(bytes.len())]))
                } else {
                    None
                }
            } else {
                None
            },
            expansion: if version == 105 {
                let map = map_core_sections(bytes).ok();
                let start = map.as_ref().and_then(|m| m.w4_pos).unwrap_or(V105SectionMarkerAxiom::V105_NPC_OFFSET);
                let end = V105SectionMarkerAxiom::V105_HEADER_LEN;
                if bytes.len() >= start + 2 {
                    Some(ExpansionSection::from_slice(&bytes[start..end.min(bytes.len())]))
                } else {
                    None
                }
            } else {
                None
            },
        };

        Ok(Save { header })
    }
}

impl Header {
    pub fn to_bytes(&self) -> io::Result<Vec<u8>> {
        let mut bytes = self.raw_prefix.clone();
        let section_axiom = V105SectionMarkerAxiom::default();

        // Update QUESTS if present (Alpha v105)
        if let Some(ref qs) = self.quests {
            section_axiom.sync_quests(&mut bytes, None, None, qs.as_slice());
        }

        // Update WAYPOINTS if present (Alpha v105)
        if let Some(ref wps) = self.waypoints {
            section_axiom.sync_waypoints(&mut bytes, None, None, wps.as_slice());
        }

        // Update EXPANSION (NPC) if present (Alpha v105)
        if let Some(ref ex) = self.expansion {
            section_axiom.sync_npc_section(&mut bytes, None, ex.as_slice());
        }

        if bytes.len() < MIN_HEADER_LEN {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Stored header prefix is too short.",
            ));
        }

        write_u32_le(&mut bytes, MAGIC_OFFSET, self.magic)?;
        write_u32_le(&mut bytes, VERSION_OFFSET, self.version)?;
        write_u32_le(&mut bytes, FILE_SIZE_OFFSET, self.file_size)?;
        write_u32_le(&mut bytes, CHECKSUM_OFFSET, self.checksum)?;
        write_u32_le(&mut bytes, ACTIVE_WEAPON_OFFSET, self.active_weapon)?;

        if CHAR_CLASS_OFFSET >= bytes.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Header prefix is too short for class.",
            ));
        }
        bytes[CHAR_CLASS_OFFSET] = self.char_class;

        if CHAR_LEVEL_OFFSET >= bytes.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Header prefix is too short for level.",
            ));
        }
        bytes[CHAR_LEVEL_OFFSET] = self.char_level;
        bytes[ACTIVE_ACT_OFFSET] = self.active_act;
        bytes[PROGRESS_FLAG_OFFSET] = self.progress_flag;

        write_u32_le(&mut bytes, LAST_PLAYED_OFFSET, self.last_played)?;
        write_ascii_nul_padded(&mut bytes, CHAR_NAME_OFFSET, CHAR_NAME_LEN, &self.char_name)?;

        Ok(bytes)
    }
}

impl Save {
    pub fn apply_header_to_bytes(&self, bytes: &mut Vec<u8>) -> io::Result<()> {
        let header_bytes = self.header.to_bytes()?;
        let header_len = header_bytes.len();
        if bytes.len() < header_len {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Target buffer is too short to receive header bytes.",
            ));
        }
        bytes[..header_len].copy_from_slice(&header_bytes);
        finalize_save_bytes(bytes, false)?;
        Ok(())
    }
}

pub fn rebuild_item_section(
    bytes: &[u8],
    items: &[Item],
    huffman: &HuffmanTree,
    alpha_mode: bool,
) -> io::Result<Vec<u8>> {
    let axiom = V105JmMarkerAxiom::default();
    let jm_positions = axiom.scan(bytes);
    if jm_positions.len() < 2 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Save needs at least two JM markers (player & corpse sections)",
        ));
    }

    let jm1 = jm_positions[0];
    let count_u16 = items.iter().filter(|it| !it.is_residue()).count() as u16;
    let jm_axiom = V105JmMarkerAxiom::default();

    // Find the end of the item section based on the next marker to preserve boundary integrity
    let section_end = if jm_positions.len() > 1 {
        jm_positions[1]
    } else if let Some(last_item) = items.last() {
        jm1 + jm_axiom.header_len() + (last_item.range.end as usize + 7) / 8
    } else {
        jm1 + jm_axiom.header_len()
    };

    let section_start = jm1 + jm_axiom.header_len();
    let original_section_len = section_end - section_start;
    
    if crate::item::item_trace_enabled() {
        eprintln!("[DEBUG Rebuild] jm1: {}, section_end: {}, len: {}", jm1, section_end, original_section_len);
        if let Some(last) = items.last() {
            eprintln!("[DEBUG Rebuild] Last Item range.end: {}", last.range.end);
        }
    }

    let mut serialized_section = Item::serialize_section(items, huffman, alpha_mode)?;

    let section_start = jm1 + jm_axiom.header_len();
    let original_section_len = section_end - section_start;
    let _original_section = &bytes[section_start..section_end];
    let serialized_len_before_padding = serialized_section.len();
    if serialized_len_before_padding < original_section_len {
        let missing_start = section_start + serialized_len_before_padding;
        serialized_section.extend_from_slice(&bytes[missing_start..section_end]);
    }
    // Research Hack: Allow serialized section to differ from original.
    /*
    if serialized_section != original_section {
        let mut fallback = bytes.to_vec();
        finalize_save_bytes(&mut fallback, false)?;
        return Ok(fallback);
    }
    */
    let final_section_len = serialized_section.len();
    let mut rebuilt =
        Vec::with_capacity(bytes.len() - original_section_len + final_section_len);
    rebuilt.extend_from_slice(&bytes[..jm1]);
    
    let mut item_header_emitter = crate::domain::item::serialization::BitEmitter::new();
    // Write "JM" marker (16 bits)
    let axiom = V105JmMarkerAxiom::default();
    item_header_emitter.write_bits(axiom.jm_marker() as u32, 16).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    
    // Both Alpha v105 and Retail use 16-bit item count in these fixtures.
    item_header_emitter.write_bits(count_u16 as u32, 16).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    
    if alpha_mode {
        let version = items.iter().find(|it| !it.is_residue()).map(|it| it.version).unwrap_or(0);
        let header_bits = jm_axiom.header_bits(version);
        if header_bits > 32 {
            let residue_bits = header_bits - 32;
            let mut r_reader = bitstream_io::BitReader::endian(std::io::Cursor::new(bytes), bitstream_io::LittleEndian);
            if r_reader.skip(jm1 as u32 * 8 + 32).is_ok() {
                let mut bits: u32 = 0;
                for i in 0..residue_bits {
                    if let Ok(b) = r_reader.read_bit() {
                        if b {
                            bits |= 1 << i;
                        }
                    }
                }
                item_header_emitter.write_bits(bits as u32, residue_bits).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
            }
        }
    }

    // Write serialized items. Since Item::serialize_section now returns bit-perfect data,
    // we just need to append it bit-by-bit or byte-by-byte if it's already aligned.
    for byte in serialized_section {
        item_header_emitter.write_bits(byte as u32, 8).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    }
    
    rebuilt.extend_from_slice(&item_header_emitter.into_bytes());
    rebuilt.extend_from_slice(&bytes[section_end..]);

    finalize_save_bytes(&mut rebuilt, false)?;
    Ok(rebuilt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io;

    fn fixture_bytes(path: &str) -> Vec<u8> {
        let repo_root = env!("CARGO_MANIFEST_DIR");
        fs::read(std::path::Path::new(repo_root).join(path)).expect("fixture should exist")
    }

    #[test]
    fn header_to_bytes_matches_original_prefix() -> io::Result<()> {
        let bytes = fixture_bytes("tests/fixtures/savegames/original/amazon_empty.d2s");
        let save = Save::from_bytes(&bytes)?;
        let header_bytes = save.header.to_bytes()?;
        let prefix_len = save.header.raw_prefix.len();
        assert_eq!(header_bytes, bytes[..prefix_len]);
        Ok(())
    }

    #[test]
    fn header_writer_nul_pads_short_name() -> io::Result<()> {
        let bytes = fixture_bytes("tests/fixtures/savegames/original/amazon_empty.d2s");
        let mut save = Save::from_bytes(&bytes)?;
        save.header.char_name = "AMY".into();
        let header_bytes = save.header.to_bytes()?;

        assert_eq!(
            &header_bytes[CHAR_NAME_OFFSET..CHAR_NAME_OFFSET + 4],
            b"AMY\0"
        );
        assert!(
            header_bytes[CHAR_NAME_OFFSET + 4..CHAR_NAME_OFFSET + CHAR_NAME_LEN]
                .iter()
                .all(|&byte| byte == 0)
        );
        Ok(())
    }

    #[test]
    fn apply_header_round_trips_level_and_integrity() -> io::Result<()> {
        let mut bytes = fixture_bytes("tests/fixtures/savegames/original/amazon_empty.d2s");
        let mut save = Save::from_bytes(&bytes)?;
        save.header.char_level = 99;
        save.apply_header_to_bytes(&mut bytes)?;

        let reparsed = Save::from_bytes(&bytes)?;
        assert_eq!(reparsed.header.char_level, 99);
        assert_eq!(reparsed.header.file_size as usize, bytes.len());
        let recalculated = recalculate_checksum(&bytes)?;
        assert_eq!(reparsed.header.checksum, recalculated);
        Ok(())
    }

    #[test]
    fn alpha_v105_header_round_trip_integrity() -> io::Result<()> {
        let mut bytes = fixture_bytes("tests/fixtures/savegames/original/amazon_initial.d2s");
        let mut save = Save::from_bytes(&bytes)?;
        assert_eq!(save.header.version, 105);
        
        // Modify level
        save.header.char_level = 2;
        save.apply_header_to_bytes(&mut bytes)?;

        let reparsed = Save::from_bytes(&bytes)?;
        assert_eq!(reparsed.header.char_level, 2);
        let recalculated = recalculate_checksum(&bytes)?;
        assert_eq!(reparsed.header.checksum, recalculated);
        
        // Check if FTI logging worked (manually verified if running with tracing)
        Ok(())
    }
}
