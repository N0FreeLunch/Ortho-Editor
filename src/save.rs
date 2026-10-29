// This software is licensed under the PolyForm Noncommercial License 1.0.0.
// Required Notice: Copyright 2026 N0FreeLunch (https://github.com/N0FreeLunch/d2r-core)

pub mod transport;

use crate::item::{HuffmanTree, Item};
pub use crate::domain::progression::axiom::PROG_START_FILE;
pub use crate::domain::header::axiom::{
    ACTIVE_ACT_OFFSET, ACTIVE_WEAPON_OFFSET, CHAR_CLASS_OFFSET, CHAR_LEVEL_OFFSET,
    CHAR_NAME_LEN, CHAR_NAME_OFFSET, CHECKSUM_OFFSET, D2S_MAGIC, FILE_SIZE_OFFSET,
    LAST_PLAYED_OFFSET, MAGIC_OFFSET, MIN_HEADER_LEN, PROGRESS_FLAG_OFFSET, VERSION_OFFSET,
    EXPANSION_FLAG_OFFSET,
};
pub use crate::domain::stats::{AttributeSection, AttributeEntry};
pub use crate::domain::stats::attribute::char_stat_save_add;
pub use crate::domain::character::skills::{SkillSection, SKILL_SECTION_LEN};
pub use crate::domain::progression::{QuestSection, WaypointSection};
use crate::domain::gateway::ItemGateway;
use crate::domain::vo::InventoryPlacement;
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

/// Collect the structurally selected player stream in parent-before-child order.
/// V3 placement follows its selected dialect; inventory, belt and cube roots share
/// the coarse InventoryLike class. Socket children follow tree ownership.
pub fn collect_player_slots(
    bytes: &[u8],
    huffman: &HuffmanTree,
) -> io::Result<Vec<(Item, ItemSlotClass)>> {
    let stream = crate::domain::item::stream::PlayerItemStream::read(bytes, huffman)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let mut slots = Vec::new();
    for item in stream.items {
        push_with_children(&mut slots, item, stream.dialect, false);
    }
    Ok(slots)
}

/// Applies one coordinated save-side relocation mutation for a single item.
/// This keeps placement and owner-bucket changes aligned at the production seam.
pub fn apply_save_side_coordinated_relocation(
    item: &mut Item,
    placement: InventoryPlacement,
    page: u8,
    location: u8,
    mode: u8,
) {
    item.set_placement(placement);
    item.set_owner_bucket(page, location, mode);
}

/// Validates a relocation target first so rejected attempts leave the item untouched.
pub fn try_apply_save_side_coordinated_relocation(
    item: &mut Item,
    x: u8,
    y: u8,
    current_items: &[Item],
    page: u8,
    location: u8,
    mode: u8,
) -> Result<(), &'static str> {
    let placement = ItemGateway::verify_placement_with_current_items(item, x, y, current_items)?;
    apply_save_side_coordinated_relocation(item, placement, page, location, mode);
    Ok(())
}

fn push_with_children(
    slots: &mut Vec<(Item, ItemSlotClass)>,
    mut item: Item,
    dialect: crate::domain::item::stream::ItemStreamDialect,
    is_child: bool,
) {
    use crate::domain::item::stream::{ItemContainer, ItemStreamDialect};

    let children = mem::take(&mut item.socketed_items);
    let class = if dialect == ItemStreamDialect::D2rV3 {
        if is_child {
            ItemSlotClass::SocketChild
        } else {
            match dialect.placement(&item).container {
                ItemContainer::Equipment => ItemSlotClass::EquipmentLike,
                ItemContainer::Inventory | ItemContainer::Belt | ItemContainer::Cube => {
                    ItemSlotClass::InventoryLike
                }
                ItemContainer::Stash => ItemSlotClass::StashLike,
                ItemContainer::Unknown => ItemSlotClass::Unknown,
            }
        }
    } else {
        classify_item_slot(&item)
    };
    slots.push((item, class));
    for child in children {
        push_with_children(slots, child, dialect, true);
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

    if version == 105 {
        let attr_level = attributes.and_then(|a| a.actual_value(12, true)).map(|v| v as u8);
        let plan = section_axiom.plan_rebuild(
            map.woo_pos,
            map.ws_pos,
            map.w4_pos,
            quests.map(|q| q.as_slice()),
            waypoints.map(|w| w.as_slice()),
            expansion.map(|e| e.as_slice()),
            attr_level,
            CHAR_LEVEL_OFFSET,
        );
        plan.execute(&mut header_bytes).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    } else {
        // Synchronize Header Level with Stat Section (id 12) for Retail
        if let Some(attr) = attributes {
            if let Some(lv) = attr.actual_value(12, false) {
                if header_bytes.len() > CHAR_LEVEL_OFFSET {
                    header_bytes[CHAR_LEVEL_OFFSET] = lv as u8;
                }
            }
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
    let items = if let Ok(stream) = crate::domain::item::stream::PlayerItemStream::read(bytes, huffman) {
        stream.items
    } else {
        Item::read_player_items(bytes, huffman, version == 105)?
    };
    let mut working = rebuild_status_and_player_items(
        bytes,
        Some(&attrs),
        None,
        None,
        None,
        None,
        &items,
        huffman,
    )?;
    working[CHAR_LEVEL_OFFSET] = new_level;
    finalize_save_bytes(&mut working, false)?;
    Ok(working)
}

/// Result of attempting to apply a character mutation.
/// Either the modified save bytes or a structured domain ContractError.
pub fn try_apply_character_mutation(
    bytes: &[u8],
    contract: &crate::domain::contracts::CharacterMutationContract,
    huffman: &HuffmanTree,
) -> Result<Vec<u8>, crate::domain::contracts::ContractError> {
    let map = map_core_sections(bytes)
        .map_err(|e| crate::domain::contracts::ContractError::ValidationFailed(format!("Failed to map sections: {}", e)))?;
    let mut attrs = AttributeSection::parse(bytes, map.gf_pos, map.if_pos)
        .map_err(|e| crate::domain::contracts::ContractError::ValidationFailed(format!("Failed to parse attributes: {}", e)))?;
    let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]));
    let is_alpha = version == 105;

    // Snapshot existing character attributes for resulting-value boundary validation
    let snapshot = crate::domain::contracts::CharacterSnapshot {
        level: attrs.actual_value(12, is_alpha).unwrap_or(1).clamp(1, 99) as u8,
        experience: attrs.actual_value(13, is_alpha).unwrap_or(0).max(0) as u32,
        strength: attrs.actual_value(0, is_alpha).unwrap_or(1).max(0) as u32,
        energy: attrs.actual_value(1, is_alpha).unwrap_or(1).max(0) as u32,
        dexterity: attrs.actual_value(2, is_alpha).unwrap_or(1).max(0) as u32,
        vitality: attrs.actual_value(3, is_alpha).unwrap_or(1).max(0) as u32,
        stat_points: attrs.actual_value(4, is_alpha).unwrap_or(0).max(0) as u32,
        gold_character: attrs.actual_value(14, is_alpha).unwrap_or(0).max(0) as u32,
        gold_stash: attrs.actual_value(15, is_alpha).unwrap_or(0).max(0) as u32,
    };

    // Pre-flight boundary and invariant validation
    let evaluated = crate::domain::contracts::CharacterBoundaryValidator::validate_mutation(
        &snapshot,
        contract,
    )?;

    // Apply validated resulting values
    let mut final_level: Option<u8> = None;
    if contract.level.is_some() {
        let save_add = char_stat_save_add(12, is_alpha);
        attrs.set_value(12, evaluated.level as i32, save_add);
        final_level = Some(evaluated.level);
    }
    if contract.strength.is_some() {
        let save_add = char_stat_save_add(0, is_alpha);
        attrs.set_value(0, evaluated.strength as i32, save_add);
    }
    if contract.energy.is_some() {
        let save_add = char_stat_save_add(1, is_alpha);
        attrs.set_value(1, evaluated.energy as i32, save_add);
    }
    if contract.dexterity.is_some() {
        let save_add = char_stat_save_add(2, is_alpha);
        attrs.set_value(2, evaluated.dexterity as i32, save_add);
    }
    if contract.vitality.is_some() {
        let save_add = char_stat_save_add(3, is_alpha);
        attrs.set_value(3, evaluated.vitality as i32, save_add);
    }
    if contract.stat_points.is_some() {
        let save_add = char_stat_save_add(4, is_alpha);
        attrs.set_value(4, evaluated.stat_points as i32, save_add);
    }
    if contract.experience.is_some() {
        let save_add = char_stat_save_add(13, is_alpha);
        attrs.set_value(13, evaluated.experience as i32, save_add);
    }
    if contract.gold_character.is_some() {
        let save_add = char_stat_save_add(14, is_alpha);
        attrs.set_value(14, evaluated.gold_character as i32, save_add);
    }
    if contract.gold_stash.is_some() {
        let save_add = char_stat_save_add(15, is_alpha);
        attrs.set_value(15, evaluated.gold_stash as i32, save_add);
    }

    // Rebuild sections
    let player_items = if let Ok(stream) = crate::domain::item::stream::PlayerItemStream::read(bytes, huffman) {
        stream.items
    } else {
        Item::read_player_items(bytes, huffman, is_alpha)
            .map_err(|e| crate::domain::contracts::ContractError::ValidationFailed(format!("Failed to read player items: {}", e)))?
    };

    let mut working = rebuild_status_and_player_items(
        bytes,
        Some(&attrs),
        None,
        None,
        None,
        None,
        &player_items,
        huffman,
    ).map_err(|e| crate::domain::contracts::ContractError::ValidationFailed(format!("Failed to rebuild save: {}", e)))?;

    // Synchronize header level
    if let Some(lvl) = final_level {
        if working.len() > CHAR_LEVEL_OFFSET {
            working[CHAR_LEVEL_OFFSET] = lvl;
        }
    }

    // Synchronize header name
    if let Some(ref new_name) = contract.name {
        if working.len() >= CHAR_NAME_OFFSET + CHAR_NAME_LEN {
            let name_bytes = new_name.as_bytes();
            let copy_len = name_bytes.len().min(CHAR_NAME_LEN - 1);
            working[CHAR_NAME_OFFSET..CHAR_NAME_OFFSET + CHAR_NAME_LEN].fill(0);
            working[CHAR_NAME_OFFSET..CHAR_NAME_OFFSET + copy_len].copy_from_slice(&name_bytes[..copy_len]);
        }
    }

    finalize_save_bytes(&mut working, false)
        .map_err(|e| crate::domain::contracts::ContractError::ValidationFailed(format!("Failed to finalize checksum: {}", e)))?;
    Ok(working)
}

/// Applies character profile and attribute mutations to save bytes based on a verified contract,
/// supporting both absolute assignment and relative delta (+/-) operations.
pub fn apply_character_mutation(
    bytes: &[u8],
    contract: &crate::domain::contracts::CharacterMutationContract,
    huffman: &HuffmanTree,
) -> io::Result<Vec<u8>> {
    try_apply_character_mutation(bytes, contract, huffman)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e.to_string()))
}

/// Result of attempting to apply a waypoint mutation.
/// Either the modified save bytes or a structured domain ProgressionContractError.
pub fn try_apply_waypoint_mutation(
    bytes: &[u8],
    contract: &crate::domain::contracts::WaypointMutationContract,
) -> Result<Vec<u8>, crate::domain::contracts::ProgressionContractError> {
    let map = map_core_sections(bytes).map_err(|e| {
        crate::domain::contracts::ProgressionContractError::ValidationFailed(format!("Failed to map sections: {}", e))
    })?;
    let ws_pos = map.ws_pos.ok_or_else(|| {
        crate::domain::contracts::ProgressionContractError::ValidationFailed("WS (Waypoint Section) marker not found".to_string())
    })?;

    let w4_pos = map.w4_pos.ok_or_else(|| {
        crate::domain::contracts::ProgressionContractError::ValidationFailed("w4 marker not found after WS section".to_string())
    })?;

    if w4_pos <= ws_pos || w4_pos > bytes.len() {
        return Err(crate::domain::contracts::ProgressionContractError::ValidationFailed(
            "Invalid waypoint section boundaries".to_string(),
        ));
    }

    let mut working = bytes.to_vec();
    let is_expansion = true; // LOD default for modern/alpha v105 saves

    let difficulties: Vec<u8> = match contract.target_difficulty {
        Some(diff) => vec![diff.to_u8()],
        None => vec![0, 1, 2], // Normal, Nightmare, Hell
    };

    let wp_anchor = ws_pos;

    for &diff in &difficulties {
        let mut wp_set = crate::domain::progression::WaypointSet::from_bytes(
            &working[ws_pos..w4_pos],
            diff,
            wp_anchor,
            is_expansion,
        );

        if contract.unlock_all {
            for wp in wp_set.waypoints_mut() {
                wp.set_active(true);
            }
        }

        for target_name in &contract.activate_waypoints {
            if let Some(wp) = wp_set.waypoints_mut().iter_mut().find(|w| w.name() == target_name) {
                wp.set_active(true);
            }
        }

        wp_set.sync_to_bytes(&mut working[ws_pos..w4_pos], wp_anchor);
    }

    finalize_save_bytes(&mut working, false).map_err(|e| {
        crate::domain::contracts::ProgressionContractError::ValidationFailed(format!("Failed to finalize checksum: {}", e))
    })?;
    Ok(working)
}

/// Applies waypoint mutations to a character save file based on a verified contract.
pub fn apply_waypoint_mutation(
    bytes: &[u8],
    contract: &crate::domain::contracts::WaypointMutationContract,
) -> io::Result<Vec<u8>> {
    try_apply_waypoint_mutation(bytes, contract)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e.to_string()))
}

/// Result of attempting to apply a skill mutation.
/// Either the modified save bytes or a structured domain SkillContractError.
pub fn try_apply_skill_mutation(
    bytes: &[u8],
    contract: &crate::domain::contracts::SkillMutationContract,
    huffman: &HuffmanTree,
) -> Result<Vec<u8>, crate::domain::contracts::SkillContractError> {
    let map = map_core_sections(bytes).map_err(|e| {
        crate::domain::contracts::SkillContractError::ValidationFailed(format!("Failed to map sections: {}", e))
    })?;
    let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]));
    let is_alpha = version == 105;
    let char_class = if CHAR_CLASS_OFFSET < bytes.len() {
        bytes[CHAR_CLASS_OFFSET]
    } else {
        0
    };
    let base_skill_id = crate::domain::contracts::class_base_skill_id(char_class).unwrap_or(0);

    let mut attrs = AttributeSection::parse(bytes, map.gf_pos, map.if_pos).map_err(|e| {
        crate::domain::contracts::SkillContractError::ValidationFailed(format!("Failed to parse attributes: {}", e))
    })?;

    let mut skills_section = crate::domain::character::skills::parse_skill_section(
        bytes,
        map.if_pos,
        map.jm_positions.first().copied(),
    )
    .map_err(|e| {
        crate::domain::contracts::SkillContractError::ValidationFailed(format!("Failed to parse skills: {}", e))
    })?;

    if contract.respec {
        // Sum all allocated hard points across 30 class skills
        let total_spent: u32 = skills_section
            .iter_skills(base_skill_id)
            .map(|sl| sl.level as u32)
            .sum();

        // Zero out 30 skills in SkillSection
        for idx in 0..SKILL_SECTION_LEN {
            skills_section.0[idx] = 0;
        }

        // Refund points to stat 5 (newskills)
        let current_unspent = attrs.actual_value(5, is_alpha).unwrap_or(0).max(0) as u32;
        let save_add = char_stat_save_add(5, is_alpha);
        attrs.set_value(5, (current_unspent + total_spent) as i32, save_add);
    }

    // Apply explicit skill level assignments
    for &(skill_id, level) in &contract.skills {
        skills_section.set_level(base_skill_id, skill_id, level);
    }

    // Apply explicit skill points
    if let Some(pts) = contract.skill_points {
        let save_add = char_stat_save_add(5, is_alpha);
        attrs.set_value(5, pts as i32, save_add);
    }

    let items = if let Ok(stream) = crate::domain::item::stream::PlayerItemStream::read(bytes, huffman) {
        stream.items
    } else {
        Item::read_player_items(bytes, huffman, is_alpha).map_err(|e| {
            crate::domain::contracts::SkillContractError::ValidationFailed(format!("Failed to read player items: {}", e))
        })?
    };

    let mut rebuilt = rebuild_status_and_player_items(
        bytes,
        Some(&attrs),
        Some(&skills_section),
        None,
        None,
        None,
        &items,
        huffman,
    ).map_err(|e| {
        crate::domain::contracts::SkillContractError::ValidationFailed(format!("Failed to rebuild save: {}", e))
    })?;

    finalize_save_bytes(&mut rebuilt, false).map_err(|e| {
        crate::domain::contracts::SkillContractError::ValidationFailed(format!("Failed to finalize checksum: {}", e))
    })?;

    Ok(rebuilt)
}

/// Result of attempting to apply a batch item operation.
/// Either the modified save bytes or a structured domain ItemContractError.
pub fn try_apply_batch_item_mutation(
    bytes: &[u8],
    contract: &crate::domain::contracts::BatchItemContract,
    huffman: &HuffmanTree,
) -> Result<Vec<u8>, crate::domain::contracts::ItemContractError> {
    let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]));
    let is_alpha = version == 105;

    let mut items = if let Ok(stream) = crate::domain::item::stream::PlayerItemStream::read(bytes, huffman) {
        stream.items
    } else {
        Item::read_player_items(bytes, huffman, is_alpha)
            .map_err(|e| crate::domain::contracts::ItemContractError::ValidationFailed(format!("Failed to read player items: {}", e)))?
    };

    for item in &mut items {
        let mut editor = crate::domain::item::editor::ItemEditor::new(item);

        // Repair All
        if contract.repair_all {
            if let (Some(cur), Some(max)) = (editor.item().current_durability, editor.item().max_durability) {
                if cur < max {
                    editor.set_durability(max, max);
                }
            }
        }

        // Identify All
        if contract.identify_all && !editor.item().header.is_identified {
            editor.set_identified(true);
        }

        // Socket All
        if let Some(target_sockets) = contract.socket_all {
            if editor.item().sockets.unwrap_or(0) == 0 {
                let types = crate::engine::validation::get_all_item_types(&editor.item().code);
                let ilvl = editor.item().header.level.unwrap_or(99);
                let mut max_sock = 0;
                for type_code in types {
                    if let Some(rule) = crate::data::legitimacy::SOCKET_RULES.iter().find(|r| r.item_type == type_code) {
                        max_sock = if ilvl >= 41 { rule.max_sock_high } else if ilvl >= 26 { rule.max_sock_mid } else { rule.max_sock_low };
                        break;
                    }
                }
                if max_sock > 0 {
                    let count = target_sockets.min(max_sock);
                    editor.set_sockets(count);
                }
            }
        }
    }

    let mut rebuilt = rebuild_status_and_player_items(
        bytes,
        None,
        None,
        None,
        None,
        None,
        &items,
        huffman,
    ).map_err(|e| crate::domain::contracts::ItemContractError::ValidationFailed(format!("Failed to rebuild save: {}", e)))?;

    finalize_save_bytes(&mut rebuilt, false).map_err(|e| {
        crate::domain::contracts::ItemContractError::ValidationFailed(format!("Failed to finalize checksum: {}", e))
    })?;

    Ok(rebuilt)
}

/// Result of attempting to apply an item mutation.
/// Either the modified save bytes or a structured domain ItemContractError.
pub fn try_apply_item_mutation(
    bytes: &[u8],
    contract: &crate::domain::contracts::ItemMutationContract,
    huffman: &HuffmanTree,
) -> Result<Vec<u8>, crate::domain::contracts::ItemContractError> {
    let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]));
    let is_alpha = version == 105;

    // Read player items with version-aware detection (supports modern v3, alpha, legacy)
    let (mut items, slot_lookup): (Vec<Item>, Box<dyn Fn(usize) -> Option<&'static str>>) =
        if let Ok(stream) = crate::domain::item::stream::PlayerItemStream::read(bytes, huffman) {
            let slots: Vec<Option<&'static str>> = stream.items.iter().map(|it| stream.placement(it).equipment_slot).collect();
            (stream.items, Box::new(move |idx| slots.get(idx).copied().flatten()))
        } else {
            let fallback_items = Item::read_player_items(bytes, huffman, is_alpha)
                .map_err(|e| crate::domain::contracts::ItemContractError::ValidationFailed(format!("Failed to read player items: {}", e)))?;
            (fallback_items, Box::new(|_| None))
        };

    let mut matched_idx = None;
    for (idx, item) in items.iter().enumerate() {
        let equipment_slot = slot_lookup(idx);

        if contract.target.matches(idx, item, equipment_slot) {
            matched_idx = Some(idx);
            break;
        }
    }

    let target_idx = matched_idx.ok_or_else(|| {
        crate::domain::contracts::ItemContractError::ItemNotFound(format!(
            "No item matched criteria: index={:?}, code={:?}, slot={:?}",
            contract.target.index, contract.target.code, contract.target.slot
        ))
    })?;

    // Mutate the target item using ItemEditor
    {
        let item = &mut items[target_idx];
        let original_cur_durability = item.current_durability.unwrap_or(0);
        let original_max_durability = item.max_durability.unwrap_or(original_cur_durability);
        let existing_stats: Vec<u32> = item.stats.properties.iter().map(|p| p.stat_id).collect();

        let mut editor = crate::domain::item::editor::ItemEditor::new(item);

        if let Some(defense) = contract.defense {
            editor.set_defense(defense);
        }

        if contract.durability.is_some() || contract.max_durability.is_some() {
            let cur = contract.durability.unwrap_or(original_cur_durability);
            let max = contract.max_durability.unwrap_or(original_max_durability);
            editor.set_durability(cur, max);
        }

        if let Some(quantity) = contract.quantity {
            editor.set_quantity(quantity);
        }

        if let Some(sockets) = contract.sockets {
            editor.set_sockets(sockets);
        }

        if let Some(ethereal) = contract.is_ethereal {
            editor.set_ethereal(ethereal);
        }

        if let Some(identified) = contract.is_identified {
            editor.set_identified(identified);
        }

        for &(stat_id, value) in &contract.stats {
            let exists = existing_stats.contains(&stat_id);
            if exists {
                editor.set_stat(stat_id, value);
            } else {
                editor.add_property(stat_id, value);
            }
        }
    }

    // Rebuild save with modified items
    let mut working = rebuild_status_and_player_items(
        bytes,
        None,
        None,
        None,
        None,
        None,
        &items,
        huffman,
    ).map_err(|e| crate::domain::contracts::ItemContractError::ValidationFailed(format!("Failed to rebuild save: {}", e)))?;

    finalize_save_bytes(&mut working, false)
        .map_err(|e| crate::domain::contracts::ItemContractError::ValidationFailed(format!("Failed to finalize checksum: {}", e)))?;

    Ok(working)
}

/// Applies item mutation to save bytes, converting domain errors to standard io::Error.
pub fn apply_item_mutation(
    bytes: &[u8],
    contract: &crate::domain::contracts::ItemMutationContract,
    huffman: &HuffmanTree,
) -> io::Result<Vec<u8>> {
    try_apply_item_mutation(bytes, contract, huffman)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e.to_string()))
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
    pub is_expansion: bool,
}

impl ExpansionSection {
    pub fn from_slice(slice: &[u8]) -> Self {
        ExpansionSection {
            raw_bytes: slice.to_vec(),
            is_expansion: true, // Default to true for backward compatibility
        }
    }

    pub fn with_expansion(mut self, is_expansion: bool) -> Self {
        self.is_expansion = is_expansion;
        self
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.raw_bytes
    }

    pub fn is_activated_by_name(&self, difficulty: u8, name: &str) -> bool {
        if let Some(entry) = crate::data::waypoints::WAYPOINTS.iter().find(|e| e.name == name) {
            // ExpansionSection implementation for waypoints (fallback/legacy)
            // It assumes raw_bytes starts at 'WS' marker (0x2BD) or follows standard WS mapping.
            let axiom = crate::domain::progression::waypoint::VersionalWaypointAxiom::resolve(self.is_expansion);
            let global_bit_idx = (difficulty as usize * axiom.difficulty_stride_bits) + (10 * 8) + entry.ws_bit as usize;
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
            let axiom = crate::domain::progression::waypoint::VersionalWaypointAxiom::resolve(self.is_expansion);
            let global_bit_idx = (difficulty as usize * axiom.difficulty_stride_bits) + (10 * 8) + entry.ws_bit as usize;
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
        crate::init_rayon_thread_pool();
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

        let is_expansion = if version == 105 {
            true // Alpha v105 is structurally expansion even if offset 271 is 0
        } else if bytes.len() > EXPANSION_FLAG_OFFSET {
            (bytes[EXPANSION_FLAG_OFFSET] & 0x20) != 0
        } else {
            true
        };

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
                    Some(QuestSection::from_slice(&bytes[start..end.min(bytes.len())]).with_expansion(is_expansion))
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
                    Some(WaypointSection::from_slice(&bytes[start..end.min(bytes.len())]).with_expansion(is_expansion))
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
                    Some(ExpansionSection::from_slice(&bytes[start..end.min(bytes.len())]).with_expansion(is_expansion))
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

        if self.version == 105 {
            let plan = section_axiom.plan_rebuild(
                None,
                None,
                None,
                self.quests.as_ref().map(|qs| qs.as_slice()),
                self.waypoints.as_ref().map(|wps| wps.as_slice()),
                self.expansion.as_ref().map(|ex| ex.as_slice()),
                None,
                CHAR_LEVEL_OFFSET,
            );
            plan.execute(&mut bytes)
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
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
    let mut count_u16 = items.iter().filter(|it| !it.is_residue()).count() as u16;
    // In Alpha v105, composite runewords (e.g. Authority cuirass `xrs`) contain a shadow header
    // segment or socket children embedded in their bitstream, which the original save format counted as an additional item.
    if alpha_mode && items.iter().any(|it| it.is_runeword && (it.code.trim() == "xrs" || !it.socketed_items.is_empty())) {
        if jm1 + 3 < bytes.len() {
            let orig_count = u16::from_le_bytes([bytes[jm1 + 2], bytes[jm1 + 3]]);
            if orig_count == count_u16 + 1 {
                count_u16 = orig_count;
            }
        }
    }
    let jm_axiom = V105JmMarkerAxiom::default();

    // Find the end of the item section based on the next marker to preserve boundary integrity
    let section_end = if jm_positions.len() > 1 {
        jm_positions[1]
    } else if let Some(last_item) = items.last() {
        jm1 + jm_axiom.header_len() + (last_item.range.end as usize + 7) / 8
    } else {
        jm1 + jm_axiom.header_len()
    };

    let mut serialized_section = Item::serialize_section(items, huffman, alpha_mode)?;

    let section_start = jm1 + jm_axiom.header_len();
    let original_section_len = section_end - section_start;
    let _original_section = &bytes[section_start..section_end];
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
