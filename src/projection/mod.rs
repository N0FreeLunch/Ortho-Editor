//! Rust-as-Store projection layer and `elm_rs` protocol boundary for Ortho Editor.
//!
//! This module projects parsed save data into immutable `SessionView` snapshots for Elm,
//! accepts typed `UiIntent` commands, validates invariants in Rust, and exports the
//! single-source-of-truth Elm `Generated.Protocol` module via `elm_rs`.

pub mod host;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use elm_rs::{Elm, ElmDecode, ElmEncode};
use serde::{Deserialize, Serialize};

use validator::Validate;

use crate::domain::character::skills::{SKILL_SECTION_LEN, SkillSection, parse_skill_section};
use crate::domain::contracts::NumericOp;
use crate::domain::contracts::character::{
    CharacterMutationContract, CharacterWireDto, LevelValue, StashGoldValue, StatPointValue,
};
use crate::domain::contracts::skill::SkillMutationContract;
use crate::domain::forensic::v105::mercenary::MercenaryState;
use crate::domain::header::axiom::EXPANSION_FLAG_OFFSET;
use crate::domain::item::builder::ItemBuilder;
use crate::domain::item::entity::ItemBitRange;
use crate::domain::item::image::{ItemImageCatalog, ItemImageKey, resolve_item_image};
use crate::domain::item::stream::{ItemContainer, PlayerItemStream};
use crate::domain::stats::entity::ItemProperty;
use crate::verify::alpha_inventory_routing::is_belt_consumable;
use crate::domain::progression::Progression;
use crate::domain::progression::axiom::{
    PROG_START_FILE, V105WaypointAxiom, V105_NPC_OFFSET, V105_WAYPOINT_OFFSET,
};
use crate::domain::progression::waypoint::WaypointSet;
use crate::engine::character_sheet::{
    Difficulty, calculate_combat_sheet,
};
use crate::engine::defense::equipped_set_piece_counts;
use crate::engine::formatter::{format_item, get_loc};
use crate::engine::mercenary_items::{collect_mercenary_items, mercenary_item_slot};
use crate::inventory::get_item_size;
use crate::item::{HuffmanTree, Item, ItemQuality};
use crate::save::{
    AttributeSection, Save, class_name, class_skill_base_id, map_core_sections,
    try_apply_character_mutation, try_apply_skill_mutation,
};

/// Active difficulty tab for resistance projection and progression editing.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Elm, ElmEncode, ElmDecode,
)]
pub enum DifficultyMode {
    Normal,
    Nightmare,
    Hell,
}

impl DifficultyMode {
    pub fn as_index(self) -> u8 {
        match self {
            DifficultyMode::Normal => 0,
            DifficultyMode::Nightmare => 1,
            DifficultyMode::Hell => 2,
        }
    }

    pub fn to_sheet_difficulty(self) -> Difficulty {
        match self {
            DifficultyMode::Normal => Difficulty::Normal,
            DifficultyMode::Nightmare => Difficulty::Nightmare,
            DifficultyMode::Hell => Difficulty::Hell,
        }
    }
}

/// Skill allocation rule mode used by the editor.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Elm, ElmEncode, ElmDecode,
)]
pub enum EditMode {
    Legit,
    Free,
}

/// Effective and capped resistance summary for the selected difficulty.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Elm, ElmEncode, ElmDecode)]
pub struct ResistanceCardView {
    pub difficulty: DifficultyMode,
    pub fire: i32,
    pub cold: i32,
    pub light: i32,
    pub poison: i32,
    pub fire_cap: i32,
    pub cold_cap: i32,
    pub light_cap: i32,
    pub poison_cap: i32,
    pub anya_bonus: i32,
    pub difficulty_penalty: i32,
}

/// Core character statistics projected for View A and View B.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Elm, ElmEncode, ElmDecode)]
pub struct CharacterStatsView {
    pub strength_base: i32,
    pub strength_bonus: i32,
    pub dexterity_base: i32,
    pub dexterity_bonus: i32,
    pub vitality_base: i32,
    pub vitality_bonus: i32,
    pub energy_base: i32,
    pub energy_bonus: i32,
    pub life: i32,
    pub max_life: i32,
    pub mana: i32,
    pub max_mana: i32,
    pub stamina: i32,
    pub max_stamina: i32,
    pub unspent_stat_points: i32,
    pub unspent_skill_points: i32,
    pub gold_inventory: i32,
    pub gold_stash: i32,
    pub active_skill_count: i32,
    #[serde(default)]
    pub fcr: i32,
    #[serde(default)]
    pub fhr: i32,
    #[serde(default)]
    pub frw: i32,
    #[serde(default)]
    pub ias: i32,
    #[serde(default)]
    pub magic_find: i32,
    #[serde(default)]
    pub all_skills_bonus: i32,
}

/// Projected class skill node for the 30-skill tree in View B.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Elm, ElmEncode, ElmDecode)]
pub struct SkillNodeView {
    pub skill_id: i32,
    pub slot_index: i32,
    pub tree_index: i32,
    pub row: i32,
    pub col: i32,
    pub name_en: String,
    pub name_ko: String,
    pub req_level: i32,
    pub level: i32,
    pub bonus_level: i32,
}

/// Editable character profile/attribute fields mapped onto Rust `CharacterMutationContract`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Elm, ElmEncode, ElmDecode)]
pub enum CharacterField {
    Level,
    Strength,
    Dexterity,
    Vitality,
    Energy,
    StatPoints,
    GoldCharacter,
    GoldStash,
}

/// Authoritative schema bounds projected by Rust for schema-guided draft UI controls.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Elm, ElmEncode, ElmDecode)]
pub struct CharacterSchemaView {
    pub name_min_len: i32,
    pub name_max_len: i32,
    pub level_min: i32,
    pub level_max: i32,
    pub stat_min: i32,
    pub stat_max: i32,
    pub stat_points_min: i32,
    pub stat_points_max: i32,
    pub gold_character_max: i32,
    pub gold_stash_max: i32,
}

/// Item counts across player containers, temporary dock, and mercenary slots.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Elm, ElmEncode, ElmDecode)]
pub struct ContainerCountsView {
    pub equipment: i32,
    pub inventory: i32,
    pub belt: i32,
    pub stash: i32,
    pub cube: i32,
    pub temp_inventory: i32,
    pub unknown: i32,
    pub mercenary: i32,
}

/// Derived mercenary combat statistics including gear and aura bonuses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Elm, ElmEncode, ElmDecode)]
pub struct MercStatsView {
    pub level: i32,
    pub base_str: i32,
    pub bonus_str: i32,
    pub base_dex: i32,
    pub bonus_dex: i32,
    pub base_life: i32,
    pub bonus_life: i32,
    pub base_def: i32,
    pub bonus_def: i32,
    pub fire_res: i32,
    pub cold_res: i32,
    pub light_res: i32,
    pub poison_res: i32,
    pub innate_aura: String,
    pub equipment_auras: Vec<String>,
}

/// Mercenary header identity and equipped item count.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Elm, ElmEncode, ElmDecode)]
pub struct MercSummaryView {
    pub hired: bool,
    pub act_index: i32,
    pub class_name: String,
    pub subtype_name: String,
    pub experience: i32,
    pub stats: MercStatsView,
    pub equipped_count: i32,
    pub persistence_supported: bool,
    pub preview_only_reason: Option<String>,
}

/// Canonical quest definition projected from `d2r-data` (`V105_QUESTS`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Elm, ElmEncode, ElmDecode)]
pub struct QuestCatalogEntryView {
    pub key: String,
    pub act: i32,
    pub index: i32,
    pub name: String,
}

/// Canonical waypoint definition projected from `d2r-data` (`WAYPOINTS`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Elm, ElmEncode, ElmDecode)]
pub struct WaypointCatalogEntryView {
    pub bit: i32,
    pub act: i32,
    pub index: i32,
    pub name: String,
}

/// Quest and waypoint completion projection across all three difficulties.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Elm, ElmEncode, ElmDecode)]
pub struct ProgressionSummaryView {
    pub is_expansion: bool,
    pub total_quests_per_difficulty: i32,
    pub total_waypoints_per_difficulty: i32,
    pub quest_catalog: Vec<QuestCatalogEntryView>,
    pub waypoint_catalog: Vec<WaypointCatalogEntryView>,
    pub normal_quests_completed: Vec<String>,
    pub nightmare_quests_completed: Vec<String>,
    pub hell_quests_completed: Vec<String>,
    pub normal_waypoints_active: Vec<i32>,
    pub nightmare_waypoints_active: Vec<i32>,
    pub hell_waypoints_active: Vec<i32>,
}

/// Projected set bonus tier for an item tooltip (`2 items`, `3 items`, or full set).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Elm, ElmEncode, ElmDecode)]
pub struct SetBonusView {
    pub active: bool,
    pub required_count: i32,
    pub lines: Vec<String>,
}

/// Authoritative container classification projected by Rust for View A/B placement and intents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Elm, ElmEncode, ElmDecode)]
pub enum ContainerKind {
    Stash,
    Inventory,
    TempInventory,
    Cube,
    Belt,
    Equipment,
    MercEquipment,
    Cursor,
    Shop,
    UnknownContainer,
}

impl ContainerKind {
    pub fn from_item_container(container: ItemContainer) -> Self {
        match container {
            ItemContainer::Equipment => ContainerKind::Equipment,
            ItemContainer::Inventory => ContainerKind::Inventory,
            ItemContainer::Belt => ContainerKind::Belt,
            ItemContainer::Stash => ContainerKind::Stash,
            ItemContainer::Cube => ContainerKind::Cube,
            ItemContainer::Unknown => ContainerKind::UnknownContainer,
        }
    }

    pub fn as_label(self) -> &'static str {
        match self {
            ContainerKind::Stash => "stash",
            ContainerKind::Inventory => "inventory",
            ContainerKind::TempInventory => "temp_inventory",
            ContainerKind::Cube => "cube",
            ContainerKind::Belt => "belt",
            ContainerKind::Equipment => "equipment",
            ContainerKind::MercEquipment => "merc_equipment",
            ContainerKind::Cursor => "cursor",
            ContainerKind::Shop => "shop",
            ContainerKind::UnknownContainer => "unknown",
        }
    }
}

/// Authoritative item quality / tier classification projected by Rust.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Elm, ElmEncode, ElmDecode)]
pub enum ItemQualityKind {
    QualityLow,
    QualityNormal,
    QualitySuperior,
    QualityMagic,
    QualitySet,
    QualityRare,
    QualityUnique,
    QualityCrafted,
    QualityRuneword,
    QualityUnknown,
}

impl ItemQualityKind {
    pub fn from_item(quality: Option<ItemQuality>, is_runeword: bool) -> Self {
        if is_runeword {
            return ItemQualityKind::QualityRuneword;
        }
        match quality {
            Some(ItemQuality::Low) => ItemQualityKind::QualityLow,
            Some(ItemQuality::Normal) => ItemQualityKind::QualityNormal,
            Some(ItemQuality::High) => ItemQualityKind::QualitySuperior,
            Some(ItemQuality::Magic) => ItemQualityKind::QualityMagic,
            Some(ItemQuality::Set) => ItemQualityKind::QualitySet,
            Some(ItemQuality::Rare) => ItemQualityKind::QualityRare,
            Some(ItemQuality::Unique) => ItemQualityKind::QualityUnique,
            Some(ItemQuality::Crafted) => ItemQualityKind::QualityCrafted,
            None => ItemQualityKind::QualityUnknown,
        }
    }

    pub fn as_label(self) -> &'static str {
        match self {
            ItemQualityKind::QualityLow => "Low",
            ItemQualityKind::QualityNormal => "Normal",
            ItemQualityKind::QualitySuperior => "Superior",
            ItemQualityKind::QualityMagic => "Magic",
            ItemQualityKind::QualitySet => "Set",
            ItemQualityKind::QualityRare => "Rare",
            ItemQualityKind::QualityUnique => "Unique",
            ItemQualityKind::QualityCrafted => "Crafted",
            ItemQualityKind::QualityRuneword => "Runeword",
            ItemQualityKind::QualityUnknown => "Unknown",
        }
    }
}

/// Target container selection for `UiIntent::SpawnShopItem`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Elm, ElmEncode, ElmDecode)]
pub enum ShopSpawnTarget {
    SpawnAuto,
    SpawnInventory,
    SpawnTempInventory,
}

/// Projected item summary for View A container grids and View B mercenary slots.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Elm, ElmEncode, ElmDecode)]
pub struct ItemCardView {
    pub index: i32,
    pub code: String,
    pub name_en: String,
    pub name_ko: String,
    pub quality: ItemQualityKind,
    pub container: ContainerKind,
    pub slot_label: Option<String>,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    #[serde(default)]
    pub item_level: i32,
    #[serde(default)]
    pub base_attributes: Vec<String>,
    pub properties: Vec<String>,
    #[serde(default)]
    pub set_name: Option<String>,
    #[serde(default)]
    pub set_pieces: Vec<String>,
    #[serde(default)]
    pub piece_set_bonuses: Vec<SetBonusView>,
    #[serde(default)]
    pub set_bonuses: Vec<SetBonusView>,
    #[serde(default)]
    pub socketed_names: Vec<String>,
    #[serde(default)]
    pub image_url: Option<String>,
    #[serde(default)]
    pub is_belt_consumable: bool,
}

/// Valid placement slot projected by Rust for the currently selected move item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Elm, ElmEncode, ElmDecode)]
pub struct AllowedMoveTargetView {
    pub container: ContainerKind,
    pub x: i32,
    pub y: i32,
}

/// Curated or data-driven item shop catalog entry projected from `d2r-data`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Elm, ElmEncode, ElmDecode)]
pub struct ShopCatalogItemView {
    pub catalog_id: String,
    pub code: String,
    pub name_en: String,
    pub name_ko: String,
    pub quality: ItemQualityKind,
    pub category: String,
    pub width: i32,
    pub height: i32,
    pub image_url: Option<String>,
    pub summary: String,
}

/// Complete Elm-facing read-only save session snapshot projected by Rust.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Elm, ElmEncode, ElmDecode)]
pub struct SessionView {
    #[serde(default)]
    pub revision: i32,
    pub fixture_label: String,
    pub save_version: i32,
    pub export_supported: bool,
    pub export_unsupported_reason: Option<String>,
    pub item_dialect: String,
    pub char_name: String,
    pub char_class: String,
    pub char_level: i32,
    pub experience: i32,
    pub is_expansion: bool,
    pub res_difficulty: DifficultyMode,
    pub skill_mode: EditMode,
    pub dirty: bool,
    pub schema: CharacterSchemaView,
    pub stats: CharacterStatsView,
    pub resistances: ResistanceCardView,
    pub container_counts: ContainerCountsView,
    pub merc: MercSummaryView,
    pub progression: ProgressionSummaryView,
    pub items: Vec<ItemCardView>,
    pub merc_items: Vec<ItemCardView>,
    pub temp_items: Vec<ItemCardView>,
    #[serde(default)]
    pub selected_move_item_index: Option<i32>,
    #[serde(default)]
    pub allowed_move_targets: Vec<AllowedMoveTargetView>,
    #[serde(default)]
    pub belt_move_restricted_reason: Option<String>,
    #[serde(default)]
    pub can_undo: bool,
    #[serde(default)]
    pub can_redo: bool,
    #[serde(default)]
    pub active_weapon_set: i32,
    #[serde(default)]
    pub skills: Vec<SkillNodeView>,
    #[serde(default)]
    pub shop_catalog: Vec<ShopCatalogItemView>,
}

/// Typed user intents sent from Elm `Main`/`HeadlessHarness` to the Rust store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Elm, ElmEncode, ElmDecode)]
pub enum UiIntent {
    SelectResDifficulty {
        difficulty: DifficultyMode,
    },
    SelectSkillMode {
        mode: EditMode,
    },
    SetMercLevel {
        level: i32,
    },
    SelectMercAct {
        act_index: i32,
    },
    SetCharacterField {
        field: CharacterField,
        value: i32,
    },
    SetCharacterName {
        name: String,
    },
    ToggleQuest {
        difficulty: DifficultyMode,
        key: String,
    },
    SetQuestsBatch {
        difficulty: DifficultyMode,
        keys: Vec<String>,
        completed: bool,
    },
    SetAllDiffQuests {
        completed: bool,
    },
    ToggleWaypoint {
        difficulty: DifficultyMode,
        bit: i32,
    },
    SetWaypointsBatch {
        difficulty: DifficultyMode,
        bits: Vec<i32>,
        active: bool,
    },
    SetAllDiffWaypoints {
        active: bool,
    },
    SelectItemForMove {
        item_index: Option<i32>,
    },
    MoveItem {
        item_index: i32,
        target_container: ContainerKind,
        x: i32,
        y: i32,
    },
    Undo,
    Redo,
    ClearTempInventory,
    ToggleWeaponSwap,
    DisposeHeldItem,
    AdjustSkillRank {
        skill_id: i32,
        delta: i32,
    },
    ResetSkills,
    SpawnShopItem {
        catalog_id: String,
        target_container: ShopSpawnTarget,
    },
}

/// Structured rejection returned when Rust rejects a `UiIntent` invariant violation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Elm, ElmEncode, ElmDecode)]
pub struct ActionRejection {
    pub code: String,
    pub message: String,
}

/// Envelope returned from Rust over ports on load or `UiIntent` application.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Elm, ElmEncode, ElmDecode)]
pub struct SessionEnvelope {
    pub ok: bool,
    pub session: Option<SessionView>,
    pub rejection: Option<ActionRejection>,
}

/// Stateful Rust editor session backing `SessionView` projection and `UiIntent` mutations.
#[derive(Debug, Clone)]
pub struct EditorSession {
    revision: i32,
    source_bytes: Vec<u8>,
    dirty: bool,
    fixture_label: String,
    save_version: i32,
    item_dialect: String,
    char_name: String,
    char_class: String,
    char_class_id: u8,
    char_level: u8,
    experience: i32,
    is_expansion: bool,
    res_difficulty: DifficultyMode,
    skill_mode: EditMode,
    attributes: AttributeSection,
    progression: Progression,
    waypoints_by_diff: [WaypointSet; 3],
    player_items_with_container: Vec<(Item, ItemContainer)>,
    item_cards: Vec<ItemCardView>,
    merc_items: Vec<Item>,
    merc_cards: Vec<ItemCardView>,
    temp_items: Vec<ItemCardView>,
    merc_state: MercenaryState,
    merc_level: i32,
    merc_act_override: Option<i32>,
    skill_section: SkillSection,
    has_skill_edits: bool,
    active_skill_count: i32,
    active_weapon_set: i32,
    char_edits: CharacterMutationContract,
    has_char_edits: bool,
    has_item_edits: bool,
    selected_move_item_index: Option<i32>,
    undo_stack: Vec<SessionSnapshot>,
    redo_stack: Vec<SessionSnapshot>,
}

/// In-memory state snapshot for Rust-owned Undo/Redo history.
#[derive(Debug, Clone)]
struct SessionSnapshot {
    dirty: bool,
    res_difficulty: DifficultyMode,
    skill_mode: EditMode,
    attributes: AttributeSection,
    progression: Progression,
    waypoints_by_diff: [WaypointSet; 3],
    player_items_with_container: Vec<(Item, ItemContainer)>,
    item_cards: Vec<ItemCardView>,
    temp_items: Vec<ItemCardView>,
    merc_state: MercenaryState,
    merc_level: i32,
    merc_act_override: Option<i32>,
    skill_section: SkillSection,
    has_skill_edits: bool,
    active_skill_count: i32,
    active_weapon_set: i32,
    char_edits: CharacterMutationContract,
    has_char_edits: bool,
    has_item_edits: bool,
    selected_move_item_index: Option<i32>,
}

impl EditorSession {
    /// Parses a save byte slice into an `EditorSession` backed by `d2r-core` domain logic.
    pub fn from_bytes(bytes: &[u8], fixture_label: impl Into<String>) -> Result<Self, String> {
        let save = Save::from_bytes(bytes).map_err(|e| e.to_string())?;
        let map = map_core_sections(bytes).map_err(|e| e.to_string())?;
        let alpha = save.header.version == 105;
        let huffman = HuffmanTree::new();

        let attributes = AttributeSection::parse(bytes, map.gf_pos, map.if_pos)
            .map_err(|e| e.to_string())?;
        let get_stat = |id: u32| attributes.actual_value(id, alpha).unwrap_or(0);
        let char_level = get_stat(12).clamp(1, 99) as u8;
        let experience = get_stat(13);

        let mut active_skill_count = 0;
        let skill_section = parse_skill_section(bytes, map.if_pos, map.jm_positions.first().copied())
            .unwrap_or(SkillSection([0u8; SKILL_SECTION_LEN]));
        if let Some(base_id) = class_skill_base_id(save.header.char_class) {
            active_skill_count = skill_section
                .iter_skills(base_id)
                .filter(|skill| skill.level > 0)
                .count() as i32;
        }

        let is_expansion = bytes
            .get(EXPANSION_FLAG_OFFSET)
            .map(|byte| byte & 0x20 != 0)
            .unwrap_or(true);

        let progression = Progression::from_bytes(bytes, alpha)
            .value
            .map_err(|e| format!("Progression parse failure: {e:?}"))?;

        let wp_bytes = bytes.get(V105_WAYPOINT_OFFSET..).unwrap_or_default();
        let wp_anchor = PROG_START_FILE + V105WaypointAxiom::start_offset();
        let waypoints_by_diff = [
            WaypointSet::from_bytes(wp_bytes, 0, wp_anchor, is_expansion),
            WaypointSet::from_bytes(wp_bytes, 1, wp_anchor, is_expansion),
            WaypointSet::from_bytes(wp_bytes, 2, wp_anchor, is_expansion),
        ];

        let stream = PlayerItemStream::read(bytes, &huffman)?;
        let dialect_str = match stream.dialect {
            crate::domain::item::stream::ItemStreamDialect::D2rV3 => "d2r-v3",
            crate::domain::item::stream::ItemStreamDialect::Alpha => "alpha",
            crate::domain::item::stream::ItemStreamDialect::Legacy => "legacy",
        }
        .to_string();

        let set_piece_counts = equipped_set_piece_counts(
            stream
                .items
                .iter()
                .filter(|item| stream.placement(item).container == ItemContainer::Equipment),
        );

        let mut player_items_with_container = Vec::with_capacity(stream.items.len());
        let mut item_cards = Vec::with_capacity(stream.items.len());
        for (idx, item) in stream.items.iter().enumerate() {
            let placement = stream.placement(item);
            let active_set_count = if placement.container == ItemContainer::Equipment
                && item.quality == Some(ItemQuality::Set)
            {
                item.unique_id
                    .and_then(|id| {
                        crate::data::set_items::SET_ITEMS
                            .iter()
                            .find(|s| s.id == id as u32)
                    })
                    .and_then(|s| set_piece_counts.get(s.set_id).copied())
                    .unwrap_or(0)
            } else {
                0
            };
            let container_kind = ContainerKind::from_item_container(placement.container);
            let resolved_slot_label = placement
                .equipment_slot
                .map(str::to_string)
                .or_else(|| {
                    if placement.container == ItemContainer::Equipment {
                        equipment_slot_label(i32::from(item.location)).map(str::to_string)
                    } else {
                        None
                    }
                });
            item_cards.push(build_item_card(
                idx as i32,
                item,
                char_level,
                active_set_count,
                container_kind,
                resolved_slot_label,
                i32::from(placement.x),
                i32::from(placement.y),
            ));
            player_items_with_container.push((item.clone(), placement.container));
        }

        let header = &bytes[..V105_WAYPOINT_OFFSET.min(bytes.len())];
        let w4_bytes = bytes.get(V105_NPC_OFFSET..);
        let merc_state = MercenaryState::from_hybrid(header, w4_bytes);
        let collected_merc = collect_mercenary_items(bytes, &map, &huffman, &stream);
        let merc_level = i32::from(merc_state.expected_level().clamp(1, 98));
        let mut merc_items = Vec::new();
        let mut merc_cards = Vec::new();
        for (idx, item) in collected_merc.items.into_iter().enumerate() {
            if collected_merc.evidence.dialect == crate::domain::item::stream::ItemStreamDialect::Legacy
                && item.code.trim().is_empty()
            {
                continue;
            }
            let (slot, _source, _kind) = mercenary_item_slot(&item, collected_merc.evidence.dialect);
            merc_cards.push(build_item_card(
                idx as i32,
                &item,
                merc_level.clamp(1, 98) as u8,
                0,
                ContainerKind::MercEquipment,
                Some(slot.to_string()),
                i32::from(item.x),
                i32::from(item.y),
            ));
            merc_items.push(item);
        }

        Ok(Self {
            revision: 1,
            source_bytes: bytes.to_vec(),
            dirty: false,
            fixture_label: fixture_label.into(),
            save_version: save.header.version as i32,
            item_dialect: dialect_str,
            char_name: save.header.char_name.clone(),
            char_class: class_name(save.header.char_class).to_string(),
            char_class_id: save.header.char_class,
            char_level,
            experience,
            is_expansion,
            res_difficulty: DifficultyMode::Normal,
            skill_mode: EditMode::Legit,
            attributes,
            progression,
            waypoints_by_diff,
            player_items_with_container,
            item_cards,
            merc_items,
            merc_cards,
            temp_items: Vec::new(),
            merc_state,
            merc_level,
            merc_act_override: None,
            skill_section,
            has_skill_edits: false,
            active_skill_count,
            active_weapon_set: 1,
            char_edits: CharacterMutationContract::default(),
            has_char_edits: false,
            has_item_edits: false,
            selected_move_item_index: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        })
    }

    /// Seeds temporary inventory dock items (used for testing dock clear flows).
    pub fn push_temp_item(&mut self, card: ItemCardView) {
        self.temp_items.push(card);
    }

    /// Projects the current store state into an immutable `SessionView` for Elm.
    pub fn project(&self) -> SessionView {
        self.project_with_catalog(true)
    }

    /// Projects the current store state after an intent, omitting static shop catalog entries.
    pub fn project_after_intent(&self) -> SessionView {
        self.project_with_catalog(false)
    }

    fn project_with_catalog(&self, include_catalog: bool) -> SessionView {
        let sheet = calculate_combat_sheet(
            Some(&self.attributes),
            Some(&self.progression.quests),
            &self.player_items_with_container,
            self.res_difficulty.to_sheet_difficulty(),
        );
        let alpha = self.save_version == 105;
        let get_stat = |id: u32| self.attributes.actual_value(id, alpha).unwrap_or(0);

        let resistances = ResistanceCardView {
            difficulty: self.res_difficulty,
            fire: sheet.fire_res.effective,
            cold: sheet.cold_res.effective,
            light: sheet.light_res.effective,
            poison: sheet.poison_res.effective,
            fire_cap: sheet.fire_res.max_cap,
            cold_cap: sheet.cold_res.max_cap,
            light_cap: sheet.light_res.max_cap,
            poison_cap: sheet.poison_res.max_cap,
            anya_bonus: sheet.fire_res.anya_bonus,
            difficulty_penalty: sheet.fire_res.difficulty_penalty,
        };

        let mut frw = 0;
        let mut magic_find = 0;
        let mut all_skills_bonus = 0;
        let mut single_skill_bonuses: BTreeMap<u32, i32> = BTreeMap::new();
        for (idx, (item, _)) in self.player_items_with_container.iter().enumerate() {
            let Some(card) = self.item_cards.get(idx) else {
                continue;
            };
            let is_equipped = card.container == ContainerKind::Equipment
                && match card.slot_label.as_deref() {
                    Some("Weapon1") | Some("Weapon2") => self.active_weapon_set == 1,
                    Some("Weapon1Swap") | Some("Weapon2Swap") => self.active_weapon_set == 2,
                    _ => true,
                };
            let is_inv_charm = card.container == ContainerKind::Inventory
                && matches!(item.code.trim(), "cm1" | "cm2" | "cm3");
            if !is_equipped && !is_inv_charm {
                continue;
            }
            for prop in item
                .properties
                .iter()
                .chain(item.runeword_attributes.iter())
                .chain(item.set_attributes.iter().flatten())
            {
                match prop.stat_id {
                    96 => frw += prop.value,
                    80 => magic_find += prop.value,
                    240 => magic_find += (prop.value * i32::from(self.char_level)) / 8,
                    127 => all_skills_bonus += prop.value,
                    83 if prop.param == u32::from(self.char_class_id) => {
                        all_skills_bonus += prop.value;
                    }
                    107 | 97 => {
                        *single_skill_bonuses.entry(prop.param).or_insert(0) += prop.value;
                    }
                    _ => {}
                }
            }
        }

        let stats = CharacterStatsView {
            strength_base: sheet.strength.base,
            strength_bonus: sheet.strength.bonus,
            dexterity_base: sheet.dexterity.base,
            dexterity_bonus: sheet.dexterity.bonus,
            vitality_base: sheet.vitality.base,
            vitality_bonus: sheet.vitality.bonus,
            energy_base: sheet.energy.base,
            energy_bonus: sheet.energy.bonus,
            life: get_stat(6) / 256,
            max_life: get_stat(7) / 256,
            mana: get_stat(8) / 256,
            max_mana: get_stat(9) / 256,
            stamina: get_stat(10) / 256,
            max_stamina: get_stat(11) / 256,
            unspent_stat_points: sheet.unspent_stat_points as i32,
            unspent_skill_points: sheet.unspent_skill_points as i32,
            gold_inventory: get_stat(14),
            gold_stash: get_stat(15),
            active_skill_count: self.active_skill_count,
            fcr: sheet.fcr,
            fhr: sheet.fhr,
            frw,
            ias: sheet.ias,
            magic_find,
            all_skills_bonus,
        };

        let mut counts = ContainerCountsView {
            equipment: 0,
            inventory: 0,
            belt: 0,
            stash: 0,
            cube: 0,
            temp_inventory: self.temp_items.len() as i32,
            unknown: 0,
            mercenary: self.merc_cards.len() as i32,
        };
        for card in &self.item_cards {
            match card.container {
                ContainerKind::Equipment => counts.equipment += 1,
                ContainerKind::Inventory => counts.inventory += 1,
                ContainerKind::Belt => counts.belt += 1,
                ContainerKind::Stash => counts.stash += 1,
                ContainerKind::Cube => counts.cube += 1,
                ContainerKind::TempInventory => counts.temp_inventory += 1,
                ContainerKind::Cursor => {}
                _ => counts.unknown += 1,
            }
        }

        let skills = self.project_skills(all_skills_bonus, &single_skill_bonuses);

        let progression = ProgressionSummaryView {
            is_expansion: self.is_expansion,
            total_quests_per_difficulty: if self.is_expansion { 27 } else { 21 },
            total_waypoints_per_difficulty: if self.is_expansion { 39 } else { 30 },
            quest_catalog: build_quest_catalog(self.is_expansion),
            waypoint_catalog: build_waypoint_catalog(self.is_expansion),
            normal_quests_completed: completed_quest_keys(&self.progression, 0),
            nightmare_quests_completed: completed_quest_keys(&self.progression, 1),
            hell_quests_completed: completed_quest_keys(&self.progression, 2),
            normal_waypoints_active: active_waypoint_bits(&self.waypoints_by_diff[0]),
            nightmare_waypoints_active: active_waypoint_bits(&self.waypoints_by_diff[1]),
            hell_waypoints_active: active_waypoint_bits(&self.waypoints_by_diff[2]),
        };

        let schema = CharacterSchemaView {
            name_min_len: 2,
            name_max_len: 15,
            level_min: 1,
            level_max: 99,
            stat_min: 1,
            stat_max: 1023,
            stat_points_min: 0,
            stat_points_max: 1023,
            gold_character_max: i32::from(self.char_level.clamp(1, 99)) * 10_000,
            gold_stash_max: StashGoldValue::MAX_STASH_GOLD as i32,
        };

        let export_unsupported_reason = export_unsupported_reason(self.save_version).or_else(|| {
            if self.item_cards.iter().any(|c| c.container == ContainerKind::Cursor) {
                Some("Cannot export .d2s while a swapped item is held on the cursor. Place it in a slot or click Undo.".to_string())
            } else if self.item_cards.iter().any(|c| c.container == ContainerKind::TempInventory) {
                Some("Cannot export .d2s while items remain in Temp Inventory (9x4). Move them to a permanent container or clear Temp Inventory.".to_string())
            } else {
                None
            }
        });
        let export_supported = export_unsupported_reason.is_none();

        let (allowed_move_targets, belt_move_restricted_reason) =
            self.project_allowed_move_targets();

        SessionView {
            revision: self.revision,
            fixture_label: self.fixture_label.clone(),
            save_version: self.save_version,
            export_supported,
            export_unsupported_reason,
            item_dialect: self.item_dialect.clone(),
            char_name: self.char_name.clone(),
            char_class: self.char_class.clone(),
            char_level: i32::from(self.char_level),
            experience: self.experience,
            is_expansion: self.is_expansion,
            res_difficulty: self.res_difficulty,
            skill_mode: self.skill_mode,
            dirty: self.dirty,
            schema,
            stats,
            resistances,
            container_counts: counts,
            merc: project_mercenary(
                &self.merc_state,
                self.merc_level,
                self.merc_act_override,
                &self.merc_items,
                &self.merc_cards,
                self.res_difficulty,
            ),
            progression,
            items: self.item_cards.clone(),
            merc_items: self.merc_cards.clone(),
            temp_items: self.temp_items.clone(),
            selected_move_item_index: self.selected_move_item_index,
            allowed_move_targets,
            belt_move_restricted_reason,
            can_undo: !self.undo_stack.is_empty(),
            can_redo: !self.redo_stack.is_empty(),
            active_weapon_set: self.active_weapon_set,
            skills,
            shop_catalog: if include_catalog {
                cached_shop_catalog().to_vec()
            } else {
                Vec::new()
            },
        }
    }

    fn capture_snapshot(&self) -> SessionSnapshot {
        SessionSnapshot {
            dirty: self.dirty,
            res_difficulty: self.res_difficulty,
            skill_mode: self.skill_mode,
            attributes: self.attributes.clone(),
            progression: self.progression.clone(),
            waypoints_by_diff: self.waypoints_by_diff.clone(),
            player_items_with_container: self.player_items_with_container.clone(),
            item_cards: self.item_cards.clone(),
            temp_items: self.temp_items.clone(),
            merc_state: self.merc_state.clone(),
            merc_level: self.merc_level,
            merc_act_override: self.merc_act_override,
            skill_section: self.skill_section.clone(),
            has_skill_edits: self.has_skill_edits,
            active_skill_count: self.active_skill_count,
            active_weapon_set: self.active_weapon_set,
            char_edits: self.char_edits.clone(),
            has_char_edits: self.has_char_edits,
            has_item_edits: self.has_item_edits,
            selected_move_item_index: self.selected_move_item_index,
        }
    }

    fn restore_snapshot(&mut self, snap: SessionSnapshot) {
        self.dirty = snap.dirty;
        self.res_difficulty = snap.res_difficulty;
        self.skill_mode = snap.skill_mode;
        self.attributes = snap.attributes;
        self.progression = snap.progression;
        self.waypoints_by_diff = snap.waypoints_by_diff;
        self.player_items_with_container = snap.player_items_with_container;
        self.item_cards = snap.item_cards;
        self.temp_items = snap.temp_items;
        self.merc_state = snap.merc_state;
        self.merc_level = snap.merc_level;
        self.merc_act_override = snap.merc_act_override;
        self.skill_section = snap.skill_section;
        self.has_skill_edits = snap.has_skill_edits;
        self.active_skill_count = snap.active_skill_count;
        self.active_weapon_set = snap.active_weapon_set;
        self.char_edits = snap.char_edits;
        self.has_char_edits = snap.has_char_edits;
        self.has_item_edits = snap.has_item_edits;
        self.selected_move_item_index = snap.selected_move_item_index;
    }

    /// Applies one `UiIntent` in Rust and returns a `SessionEnvelope`.
    pub fn apply_intent(&mut self, intent: UiIntent) -> SessionEnvelope {
        let mutates_history = matches!(
            intent,
            UiIntent::SetMercLevel { .. }
                | UiIntent::SelectMercAct { .. }
                | UiIntent::ToggleQuest { .. }
                | UiIntent::SetQuestsBatch { .. }
                | UiIntent::SetAllDiffQuests { .. }
                | UiIntent::ToggleWaypoint { .. }
                | UiIntent::SetWaypointsBatch { .. }
                | UiIntent::SetAllDiffWaypoints { .. }
                | UiIntent::SetCharacterField { .. }
                | UiIntent::SetCharacterName { .. }
                | UiIntent::MoveItem { .. }
                | UiIntent::ClearTempInventory
                | UiIntent::DisposeHeldItem
                | UiIntent::AdjustSkillRank { .. }
                | UiIntent::ResetSkills
                | UiIntent::SpawnShopItem { .. }
        );
        let pre_snapshot = if mutates_history {
            Some(self.capture_snapshot())
        } else {
            None
        };
        match self.apply_intent_inner(intent) {
            Ok(()) => {
                if let Some(snap) = pre_snapshot {
                    self.undo_stack.push(snap);
                    self.redo_stack.clear();
                }
                self.revision = self.revision.saturating_add(1);
                SessionEnvelope {
                    ok: true,
                    session: Some(self.project_after_intent()),
                    rejection: None,
                }
            }
            Err(rejection) => SessionEnvelope {
                ok: false,
                session: Some(self.project_after_intent()),
                rejection: Some(rejection),
            },
        }
    }

    fn apply_intent_inner(&mut self, intent: UiIntent) -> Result<(), ActionRejection> {
        match intent {
            UiIntent::SelectResDifficulty { difficulty } => {
                self.res_difficulty = difficulty;
                Ok(())
            }
            UiIntent::SelectSkillMode { mode } => {
                self.skill_mode = mode;
                Ok(())
            }
            UiIntent::SetMercLevel { level } => {
                if !(1..=98).contains(&level) {
                    return Err(ActionRejection {
                        code: "merc_level_out_of_range".to_string(),
                        message: format!("Mercenary level must stay within 1..=98 (got {level})"),
                    });
                }
                Err(ActionRejection {
                    code: "merc_persistence_not_supported".to_string(),
                    message: "Mercenary edits are read-only until .d2s mercenary writeback is implemented.".to_string(),
                })
            }
            UiIntent::SelectMercAct { act_index } => {
                if !matches!(act_index, 1 | 2 | 3 | 5) {
                    return Err(ActionRejection {
                        code: "invalid_merc_act".to_string(),
                        message: format!("Mercenary act must be 1, 2, 3, or 5 (got {act_index})"),
                    });
                }
                Err(ActionRejection {
                    code: "merc_persistence_not_supported".to_string(),
                    message: "Mercenary edits are read-only until .d2s mercenary writeback is implemented.".to_string(),
                })
            }
            UiIntent::ToggleQuest { difficulty, key } => {
                let diff = difficulty.as_index();
                let Some(quest) = self
                    .progression
                    .quests
                    .quests_mut()
                    .iter_mut()
                    .find(|q| q.difficulty() == diff && quest_key(q.act(), q.index()) == key)
                else {
                    return Err(ActionRejection {
                        code: "unknown_quest_key".to_string(),
                        message: format!("Unknown quest key `{key}` for {difficulty:?}"),
                    });
                };
                let next = !quest.is_completed();
                quest.set_completed(next);
                self.dirty = true;
                Ok(())
            }
            UiIntent::SetQuestsBatch {
                difficulty,
                keys,
                completed,
            } => {
                let diff = difficulty.as_index();
                let key_set: BTreeSet<String> = keys.into_iter().collect();
                let mut matched = 0usize;
                for quest in self.progression.quests.quests_mut().iter_mut() {
                    if quest.difficulty() == diff
                        && key_set.contains(&quest_key(quest.act(), quest.index()))
                    {
                        quest.set_completed(completed);
                        matched += 1;
                    }
                }
                if matched == 0 && !key_set.is_empty() {
                    return Err(ActionRejection {
                        code: "unknown_quest_batch".to_string(),
                        message: format!("No matching quests found for {difficulty:?}"),
                    });
                }
                if matched > 0 {
                    self.dirty = true;
                }
                Ok(())
            }
            UiIntent::SetAllDiffQuests { completed } => {
                let max_act = if self.is_expansion { 5 } else { 4 };
                for quest in self.progression.quests.quests_mut().iter_mut() {
                    if quest.act() <= max_act {
                        quest.set_completed(completed);
                    }
                }
                self.dirty = true;
                Ok(())
            }
            UiIntent::ToggleWaypoint { difficulty, bit } => {
                let diff = difficulty.as_index() as usize;
                let max_bits = self.waypoints_by_diff[diff].waypoints().len() as i32;
                if bit < 0 || bit >= max_bits {
                    return Err(ActionRejection {
                        code: "waypoint_bit_out_of_range".to_string(),
                        message: format!("Waypoint bit {bit} is outside 0..{max_bits}"),
                    });
                }
                let wp = &mut self.waypoints_by_diff[diff].waypoints_mut()[bit as usize];
                let next = !wp.is_active();
                wp.set_active(next);
                self.dirty = true;
                Ok(())
            }
            UiIntent::SetWaypointsBatch {
                difficulty,
                bits,
                active,
            } => {
                let diff = difficulty.as_index() as usize;
                let wps = self.waypoints_by_diff[diff].waypoints_mut();
                let touched = !bits.is_empty();
                for bit in bits {
                    if bit < 0 || (bit as usize) >= wps.len() {
                        return Err(ActionRejection {
                            code: "waypoint_bit_out_of_range".to_string(),
                            message: format!(
                                "Waypoint bit {bit} is outside 0..{}",
                                wps.len()
                            ),
                        });
                    }
                    wps[bit as usize].set_active(active);
                }
                if touched {
                    self.dirty = true;
                }
                Ok(())
            }
            UiIntent::SetAllDiffWaypoints { active } => {
                for set in &mut self.waypoints_by_diff {
                    for wp in set.waypoints_mut().iter_mut() {
                        wp.set_active(active);
                    }
                }
                self.dirty = true;
                Ok(())
            }
            UiIntent::SetCharacterField { field, value } => {
                self.apply_character_edit(|edits| {
                    match field {
                        CharacterField::Level => {
                            let lvl = u8::try_from(value).map_err(|_| {
                                format!(
                                    "Character level {value} must be an integer between 1 and 99."
                                )
                            })?;
                            let vo = LevelValue::new(lvl).map_err(|e| e.to_string())?;
                            edits.level = Some(NumericOp::Set(vo));
                        }
                        CharacterField::Strength
                        | CharacterField::Dexterity
                        | CharacterField::Vitality
                        | CharacterField::Energy
                        | CharacterField::StatPoints => {
                            let pts = u32::try_from(value).map_err(|_| {
                                format!("Stat value {value} cannot be negative.")
                            })?;
                            let vo = StatPointValue::new(pts, 1023).map_err(|e| e.to_string())?;
                            let slot = match field {
                                CharacterField::Strength => &mut edits.strength,
                                CharacterField::Dexterity => &mut edits.dexterity,
                                CharacterField::Vitality => &mut edits.vitality,
                                CharacterField::Energy => &mut edits.energy,
                                CharacterField::StatPoints => &mut edits.stat_points,
                                _ => unreachable!(),
                            };
                            *slot = Some(NumericOp::Set(vo));
                        }
                        CharacterField::GoldCharacter => {
                            let gold = u32::try_from(value).map_err(|_| {
                                format!("Character inventory gold {value} cannot be negative.")
                            })?;
                            edits.gold_character = Some(NumericOp::Set(gold));
                        }
                        CharacterField::GoldStash => {
                            let gold = u32::try_from(value).map_err(|_| {
                                format!("Private stash gold {value} cannot be negative.")
                            })?;
                            let vo = StashGoldValue::new(gold).map_err(|e| e.to_string())?;
                            edits.gold_stash = Some(NumericOp::Set(vo));
                        }
                    }
                    Ok(())
                })
            }
            UiIntent::SetCharacterName { name } => {
                let trimmed = name.trim().to_string();
                if !trimmed
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
                {
                    return Err(ActionRejection {
                        code: "invalid_character_mutation".to_string(),
                        message: "Character name must use 2 to 15 ASCII alphanumeric, '-' or '_' characters.".to_string(),
                    });
                }
                let dto = CharacterWireDto {
                    name: Some(trimmed.clone()),
                    ..Default::default()
                };
                if let Err(err) = dto.validate() {
                    return Err(ActionRejection {
                        code: "invalid_character_mutation".to_string(),
                        message: err.to_string(),
                    });
                }
                self.apply_character_edit(|edits| {
                    edits.name = Some(trimmed);
                    Ok(())
                })
            }
            UiIntent::SelectItemForMove { item_index } => {
                if self.item_cards.iter().any(|c| c.container == ContainerKind::Cursor) {
                    return Err(ActionRejection {
                        code: "cursor_item_must_be_placed".to_string(),
                        message: "Place the swapped item held on the cursor into a slot or click Undo.".to_string(),
                    });
                }
                if let Some(idx) = item_index {
                    let valid = usize::try_from(idx)
                        .ok()
                        .is_some_and(|i| i < self.player_items_with_container.len());
                    if !valid {
                        return Err(ActionRejection {
                            code: "item_not_found".to_string(),
                            message: format!("Item index {idx} was not found in the player stream."),
                        });
                    }
                }
                self.selected_move_item_index = item_index;
                Ok(())
            }
            UiIntent::MoveItem {
                item_index,
                target_container,
                x,
                y,
            } => self.apply_move_item(item_index, target_container, x, y),
            UiIntent::Undo => {
                let prev = self.undo_stack.pop().ok_or_else(|| ActionRejection {
                    code: "nothing_to_undo".to_string(),
                    message: "No actions available to undo.".to_string(),
                })?;
                let current = self.capture_snapshot();
                self.redo_stack.push(current);
                self.restore_snapshot(prev);
                Ok(())
            }
            UiIntent::Redo => {
                let next = self.redo_stack.pop().ok_or_else(|| ActionRejection {
                    code: "nothing_to_redo".to_string(),
                    message: "No actions available to redo.".to_string(),
                })?;
                let current = self.capture_snapshot();
                self.undo_stack.push(current);
                self.restore_snapshot(next);
                Ok(())
            }
            UiIntent::ClearTempInventory => {
                self.temp_items.clear();
                let mut idx = 0;
                while idx < self.item_cards.len() {
                    if self.item_cards[idx].container == ContainerKind::TempInventory {
                        self.player_items_with_container.remove(idx);
                        self.item_cards.remove(idx);
                        self.has_item_edits = true;
                        self.dirty = true;
                    } else {
                        idx += 1;
                    }
                }
                for (new_i, card) in self.item_cards.iter_mut().enumerate() {
                    card.index = new_i as i32;
                }
                self.selected_move_item_index = None;
                Ok(())
            }
            UiIntent::ToggleWeaponSwap => {
                self.active_weapon_set = if self.active_weapon_set == 2 { 1 } else { 2 };
                Ok(())
            }
            UiIntent::DisposeHeldItem => {
                let item_index = self.selected_move_item_index.ok_or_else(|| ActionRejection {
                    code: "no_held_item".to_string(),
                    message: "Pick up an item before disposing it at the shop.".to_string(),
                })?;
                self.dispose_item_by_index(item_index)
            }
            UiIntent::AdjustSkillRank { skill_id, delta } => {
                self.apply_adjust_skill_rank(skill_id, delta)
            }
            UiIntent::ResetSkills => self.apply_reset_skills(),
            UiIntent::SpawnShopItem {
                catalog_id,
                target_container,
            } => self.apply_spawn_shop_item(&catalog_id, target_container),
        }
    }

    fn project_skills(
        &self,
        all_skills_bonus: i32,
        single_skill_bonuses: &BTreeMap<u32, i32>,
    ) -> Vec<SkillNodeView> {
        let Some(base_id) = class_skill_base_id(self.char_class_id) else {
            return Vec::new();
        };
        let mut nodes = Vec::with_capacity(SKILL_SECTION_LEN);
        for slot in 0..SKILL_SECTION_LEN {
            let sid = base_id + slot as u32;
            let level = i32::from(self.skill_section.0[slot]);
            let (name_en, name_ko, req_level) = crate::data::skills::SKILLS
                .iter()
                .find(|s| s.id == sid)
                .map(|s| {
                    (
                        get_loc(s.key, "en").to_string(),
                        get_loc(s.key, "ko").to_string(),
                        s.reqlevel as i32,
                    )
                })
                .unwrap_or_else(|| (format!("Skill #{sid}"), format!("Skill #{sid}"), 1));
            let single_bonus = single_skill_bonuses.get(&sid).copied().unwrap_or(0);
            let bonus_level = if level > 0 || single_bonus > 0 {
                all_skills_bonus + single_bonus
            } else {
                0
            };
            let within = (slot % 10) as i32;
            nodes.push(SkillNodeView {
                skill_id: sid as i32,
                slot_index: slot as i32,
                tree_index: (slot / 10) as i32,
                row: (within / 3) + 1,
                col: (within % 3) + 1,
                name_en,
                name_ko,
                req_level,
                level,
                bonus_level,
            });
        }
        nodes
    }

    fn dispose_item_by_index(&mut self, item_index: i32) -> Result<(), ActionRejection> {
        let idx = usize::try_from(item_index)
            .ok()
            .filter(|&i| i < self.player_items_with_container.len())
            .ok_or_else(|| ActionRejection {
                code: "item_not_found".to_string(),
                message: format!("Item index {item_index} was not found in the player stream."),
            })?;
        self.player_items_with_container.remove(idx);
        self.item_cards.remove(idx);
        for (new_i, card) in self.item_cards.iter_mut().enumerate() {
            card.index = new_i as i32;
        }
        self.selected_move_item_index = None;
        self.has_item_edits = true;
        self.dirty = true;
        Ok(())
    }

    fn apply_adjust_skill_rank(&mut self, skill_id: i32, delta: i32) -> Result<(), ActionRejection> {
        if delta == 0 {
            return Ok(());
        }
        let base_id = class_skill_base_id(self.char_class_id).ok_or_else(|| ActionRejection {
            code: "invalid_skill_mutation".to_string(),
            message: "Character class does not have a valid skill table.".to_string(),
        })?;
        let sid = u32::try_from(skill_id).map_err(|_| ActionRejection {
            code: "invalid_skill_mutation".to_string(),
            message: format!("Skill ID {skill_id} is invalid."),
        })?;
        if sid < base_id || (sid - base_id) as usize >= SKILL_SECTION_LEN {
            return Err(ActionRejection {
                code: "invalid_skill_mutation".to_string(),
                message: format!(
                    "Skill ID {skill_id} does not belong to {}.",
                    self.char_class
                ),
            });
        }
        let slot = (sid - base_id) as usize;
        let current = i32::from(self.skill_section.0[slot]);
        let max_rank = if self.skill_mode == EditMode::Free { 99 } else { 20 };
        let next = current + delta;
        if next < 0 || next > max_rank {
            return Err(ActionRejection {
                code: "skill_rank_out_of_range".to_string(),
                message: format!(
                    "Skill rank must stay within 0..={max_rank} in {:?} mode.",
                    self.skill_mode
                ),
            });
        }
        let alpha = self.save_version == 105;
        let unspent = self.attributes.actual_value(5, alpha).unwrap_or(0).max(0);
        if self.skill_mode == EditMode::Legit {
            let req_level = crate::data::skills::SKILLS
                .iter()
                .find(|s| s.id == sid)
                .map(|s| s.reqlevel as i32)
                .unwrap_or(1);
            if delta > 0 && i32::from(self.char_level) < req_level {
                return Err(ActionRejection {
                    code: "skill_level_requirement".to_string(),
                    message: format!(
                        "Requires character level {req_level} (current level {}).",
                        self.char_level
                    ),
                });
            }
            if delta > 0 && unspent < delta {
                return Err(ActionRejection {
                    code: "insufficient_skill_points".to_string(),
                    message: "No unspent skill points remaining in Legit mode.".to_string(),
                });
            }
            let next_unspent = (unspent - delta).max(0);
            let save_add = crate::domain::stats::attribute::char_stat_save_add(5, alpha);
            self.attributes.set_value(5, next_unspent, save_add);
        }
        self.skill_section.0[slot] = next as u8;
        self.active_skill_count = self
            .skill_section
            .iter_skills(base_id)
            .filter(|s| s.level > 0)
            .count() as i32;
        self.has_skill_edits = true;
        self.dirty = true;
        Ok(())
    }

    fn apply_reset_skills(&mut self) -> Result<(), ActionRejection> {
        let Some(base_id) = class_skill_base_id(self.char_class_id) else {
            return Err(ActionRejection {
                code: "invalid_skill_mutation".to_string(),
                message: "Character class does not have a valid skill table.".to_string(),
            });
        };
        let total_spent: i32 = self
            .skill_section
            .iter_skills(base_id)
            .map(|s| i32::from(s.level))
            .sum();
        self.skill_section = SkillSection([0u8; SKILL_SECTION_LEN]);
        let alpha = self.save_version == 105;
        let current_unspent = self.attributes.actual_value(5, alpha).unwrap_or(0).max(0);
        let save_add = crate::domain::stats::attribute::char_stat_save_add(5, alpha);
        self.attributes
            .set_value(5, current_unspent + total_spent, save_add);
        self.active_skill_count = 0;
        self.has_skill_edits = true;
        self.dirty = true;
        Ok(())
    }

    fn validate_move_target(
        &self,
        item_index: i32,
        target_container: ContainerKind,
        x: i32,
        y: i32,
    ) -> Result<(usize, ItemContainer, Option<usize>), ActionRejection> {
        let idx = usize::try_from(item_index)
            .ok()
            .filter(|&i| i < self.player_items_with_container.len())
            .ok_or_else(|| ActionRejection {
                code: "item_not_found".to_string(),
                message: format!("Item index {item_index} was not found in the player stream."),
            })?;

        let (item_code, width, height) = {
            let card = &self.item_cards[idx];
            (card.code.clone(), card.width.max(1), card.height.max(1))
        };

        if target_container == ContainerKind::Equipment {
            let slot_name = equipment_slot_label(x)
                .filter(|_| y == 0)
                .ok_or_else(|| ActionRejection {
                    code: "out_of_bounds".to_string(),
                    message: format!("Equipment slot ({x}, {y}) is not a valid body slot (1..=12)."),
                })?;
            if !item_fits_equipment_slot(&item_code, x) {
                return Err(ActionRejection {
                    code: "invalid_equipment_slot".to_string(),
                    message: format!(
                        "Item '{}' cannot be equipped in the {} slot.",
                        item_code.trim(),
                        slot_name
                    ),
                });
            }
            let occupied: Vec<usize> = self
                .item_cards
                .iter()
                .enumerate()
                .filter(|&(other_idx, other)| {
                    other_idx != idx
                        && other.container == ContainerKind::Equipment
                        && other.slot_label.as_deref() == Some(slot_name)
                })
                .map(|(other_idx, _)| other_idx)
                .collect();
            return match occupied.as_slice() {
                [] => Ok((idx, ItemContainer::Equipment, None)),
                [disp] => Ok((idx, ItemContainer::Equipment, Some(*disp))),
                _ => Err(ActionRejection {
                    code: "container_slot_occupied".to_string(),
                    message: format!("Equipment slot {slot_name} has multiple overlapping items."),
                }),
            };
        }

        let (target, max_w, max_h, target_kind) = match target_container {
            ContainerKind::Inventory => (ItemContainer::Inventory, 10i32, 4i32, ContainerKind::Inventory),
            ContainerKind::Stash => (ItemContainer::Stash, 10i32, 10i32, ContainerKind::Stash),
            ContainerKind::Cube => (ItemContainer::Cube, 3i32, 4i32, ContainerKind::Cube),
            ContainerKind::Belt => (ItemContainer::Belt, 4i32, 4i32, ContainerKind::Belt),
            ContainerKind::TempInventory => (ItemContainer::Unknown, 9i32, 4i32, ContainerKind::TempInventory),
            other => {
                return Err(ActionRejection {
                    code: "invalid_target_container".to_string(),
                    message: format!(
                        "Items cannot be moved to '{}'. Allowed targets: inventory, stash, cube, belt, temp_inventory, equipment, shop.",
                        other.as_label()
                    ),
                });
            }
        };

        if target == ItemContainer::Belt
            && (!is_belt_consumable(&item_code) || width != 1 || height != 1)
        {
            return Err(ActionRejection {
                code: "belt_consumable_only".to_string(),
                message: format!(
                    "Only 1x1 belt consumables (potions/scrolls) can be placed in the belt ('{}' is not allowed).",
                    item_code.trim()
                ),
            });
        }

        if x < 0 || y < 0 || x + width > max_w || y + height > max_h {
            return Err(ActionRejection {
                code: "out_of_bounds".to_string(),
                message: format!(
                    "Placement ({x}, {y}) with size {width}x{height} exceeds {} bounds ({max_w}x{max_h}).",
                    target_container.as_label()
                ),
            });
        }

        let mut overlaps = Vec::new();
        for (other_idx, other) in self.item_cards.iter().enumerate() {
            if other_idx == idx || other.container != target_kind {
                continue;
            }
            let ow = other.width.max(1);
            let oh = other.height.max(1);
            if x < other.x + ow && x + width > other.x && y < other.y + oh && y + height > other.y {
                overlaps.push(other_idx);
            }
        }

        match overlaps.as_slice() {
            [] => Ok((idx, target, None)),
            [disp] => Ok((idx, target, Some(*disp))),
            _ => Err(ActionRejection {
                code: "container_slot_occupied".to_string(),
                message: format!(
                    "Target slot ({x}, {y}) in {} overlaps with {} items and cannot swap.",
                    target_container.as_label(),
                    overlaps.len()
                ),
            }),
        }
    }

    fn project_allowed_move_targets(&self) -> (Vec<AllowedMoveTargetView>, Option<String>) {
        let Some(item_index) = self.selected_move_item_index else {
            return (Vec::new(), None);
        };
        let mut targets = Vec::new();
        for (container, max_w, max_h) in [
            (ContainerKind::Inventory, 10i32, 4i32),
            (ContainerKind::Stash, 10i32, 10i32),
            (ContainerKind::Cube, 3i32, 4i32),
            (ContainerKind::Belt, 4i32, 4i32),
            (ContainerKind::TempInventory, 9i32, 4i32),
        ] {
            for y in 0..max_h {
                for x in 0..max_w {
                    if self.validate_move_target(item_index, container, x, y).is_ok() {
                        targets.push(AllowedMoveTargetView { container, x, y });
                    }
                }
            }
        }
        for body_loc in 1..=12 {
            if self
                .validate_move_target(item_index, ContainerKind::Equipment, body_loc, 0)
                .is_ok()
            {
                targets.push(AllowedMoveTargetView {
                    container: ContainerKind::Equipment,
                    x: body_loc,
                    y: 0,
                });
            }
        }
        let belt_restricted_reason =
            match self.validate_move_target(item_index, ContainerKind::Belt, 0, 0) {
                Err(rej) if rej.code == "belt_consumable_only" => Some(rej.message),
                _ => None,
            };
        (targets, belt_restricted_reason)
    }

    fn apply_move_item(
        &mut self,
        item_index: i32,
        target_container: ContainerKind,
        x: i32,
        y: i32,
    ) -> Result<(), ActionRejection> {
        if target_container == ContainerKind::Shop {
            return self.dispose_item_by_index(item_index);
        }
        let (idx, target, displaced_idx) =
            self.validate_move_target(item_index, target_container, x, y)?;
        let target_kind = if target_container == ContainerKind::TempInventory {
            ContainerKind::TempInventory
        } else {
            ContainerKind::from_item_container(target)
        };

        let (raw_x, raw_y, page, location, mode, stored_in, new_slot_label, card_x, card_y) =
            match target {
                ItemContainer::Inventory => (x as u8, y as u8, 0u8, 0u8, 0u8, Some(1u8), None, x, y),
                ItemContainer::Cube => (x as u8, y as u8, 3u8, 0u8, 0u8, Some(4u8), None, x, y),
                ItemContainer::Stash => (x as u8, y as u8, 4u8, 0u8, 0u8, Some(5u8), None, x, y),
                ItemContainer::Belt => ((y * 4 + x) as u8, 0u8, 0u8, 0u8, 2u8, None, None, x, y),
                ItemContainer::Equipment => (
                    0u8,
                    0u8,
                    0u8,
                    x as u8,
                    1u8,
                    None,
                    equipment_slot_label(x).map(str::to_string),
                    0,
                    0,
                ),
                ItemContainer::Unknown => (x as u8, y as u8, 0u8, 0u8, 0u8, None, None, x, y),
            };

        let (item, container_slot) = &mut self.player_items_with_container[idx];
        item.x = raw_x;
        item.y = raw_y;
        item.page = page;
        item.location = location;
        item.mode = mode;
        item.header.x = raw_x;
        item.header.y = raw_y;
        item.header.page = page;
        item.header.location = location;
        item.header.mode = mode;
        item.body.x = raw_x;
        item.body.y = raw_y;
        item.body.page = page;
        item.body.location = location;
        item.body.mode = mode;
        if let Some(aux) = item.body.d2r_v3_auxiliary.as_mut() {
            aux.stored_in = stored_in;
        }
        item.bits.clear();
        *container_slot = target;

        let card = &mut self.item_cards[idx];
        card.container = target_kind;
        card.slot_label = new_slot_label;
        card.x = card_x;
        card.y = card_y;

        self.has_item_edits = true;
        self.dirty = true;
        if let Some(disp) = displaced_idx {
            self.item_cards[disp].container = ContainerKind::Cursor;
            self.item_cards[disp].slot_label = None;
            self.selected_move_item_index = Some(disp as i32);
        } else {
            self.selected_move_item_index = None;
        }
        Ok(())
    }

    fn find_free_grid_slot(
        &self,
        container_kind: ContainerKind,
        max_w: i32,
        max_h: i32,
        w: i32,
        h: i32,
    ) -> Option<(i32, i32)> {
        for y in 0..=(max_h - h).max(-1) {
            for x in 0..=(max_w - w).max(-1) {
                let overlaps = self.item_cards.iter().any(|other| {
                    if other.container != container_kind {
                        return false;
                    }
                    let ow = other.width.max(1);
                    let oh = other.height.max(1);
                    x < other.x + ow && x + w > other.x && y < other.y + oh && y + h > other.y
                });
                if !overlaps {
                    return Some((x, y));
                }
            }
        }
        None
    }

    fn apply_spawn_shop_item(
        &mut self,
        catalog_id: &str,
        target_container: ShopSpawnTarget,
    ) -> Result<(), ActionRejection> {
        let alpha = self.item_dialect == "alpha";
        let mut item = build_shop_item_by_id(catalog_id, alpha).ok_or_else(|| ActionRejection {
            code: "shop_item_not_found".to_string(),
            message: format!("Shop catalog item '{catalog_id}' was not found."),
        })?;
        let (w, h) = get_item_size(item.code.trim());
        let width = i32::from(w.max(1));
        let height = i32::from(h.max(1));

        let (dest_kind, dest_enum, raw_page) = match target_container {
            ShopSpawnTarget::SpawnInventory => {
                let (x, y) = self
                    .find_free_grid_slot(ContainerKind::Inventory, 10, 4, width, height)
                    .ok_or_else(|| ActionRejection {
                        code: "no_free_slot".to_string(),
                        message: "Inventory (10x4) has no free space for this item.".to_string(),
                    })?;
                item.x = x as u8;
                item.y = y as u8;
                item.header.x = x as u8;
                item.header.y = y as u8;
                item.body.x = x as u8;
                item.body.y = y as u8;
                (ContainerKind::Inventory, ItemContainer::Inventory, 0u8)
            }
            ShopSpawnTarget::SpawnTempInventory => {
                let (x, y) = self
                    .find_free_grid_slot(ContainerKind::TempInventory, 9, 4, width, height)
                    .ok_or_else(|| ActionRejection {
                        code: "no_free_slot".to_string(),
                        message: "Temp Inventory (9x4) has no free space for this item.".to_string(),
                    })?;
                item.x = x as u8;
                item.y = y as u8;
                item.header.x = x as u8;
                item.header.y = y as u8;
                item.body.x = x as u8;
                item.body.y = y as u8;
                (ContainerKind::TempInventory, ItemContainer::Unknown, 0u8)
            }
            ShopSpawnTarget::SpawnAuto => {
                if let Some((x, y)) =
                    self.find_free_grid_slot(ContainerKind::Inventory, 10, 4, width, height)
                {
                    item.x = x as u8;
                    item.y = y as u8;
                    item.header.x = x as u8;
                    item.header.y = y as u8;
                    item.body.x = x as u8;
                    item.body.y = y as u8;
                    (ContainerKind::Inventory, ItemContainer::Inventory, 0u8)
                } else if let Some((x, y)) =
                    self.find_free_grid_slot(ContainerKind::TempInventory, 9, 4, width, height)
                {
                    item.x = x as u8;
                    item.y = y as u8;
                    item.header.x = x as u8;
                    item.header.y = y as u8;
                    item.body.x = x as u8;
                    item.body.y = y as u8;
                    (ContainerKind::TempInventory, ItemContainer::Unknown, 0u8)
                } else {
                    return Err(ActionRejection {
                        code: "no_free_slot".to_string(),
                        message: "Both Inventory (10x4) and Temp Inventory (9x4) are full."
                            .to_string(),
                    });
                }
            }
        };

        item.page = raw_page;
        item.location = 0;
        item.mode = 0;
        item.header.page = raw_page;
        item.header.location = 0;
        item.header.mode = 0;
        item.body.page = raw_page;
        item.body.location = 0;
        item.body.mode = 0;
        item.bits.clear();
        if self.item_dialect == "d2r-v3" {
            let aux = item
                .body
                .d2r_v3_auxiliary
                .get_or_insert_with(Default::default);
            aux.save_version = self.save_version.max(0) as u32;
            aux.stored_in = Some(1);
        }

        let huffman = HuffmanTree::new();
        let mut canonical = if self.item_dialect == "d2r-v3" {
            let mut solved = None;
            for pad in 0..8 {
                if let Some(aux) = item.body.d2r_v3_auxiliary.as_mut() {
                    aux.alignment_padding = Some(vec![false; pad]);
                }
                if item.to_bits_d2r_v3(&huffman).is_ok() {
                    solved = Some(item.clone());
                    break;
                }
            }
            match solved {
                Some(c) => c,
                None => {
                    item.to_bits_d2r_v3(&huffman).map_err(|e| ActionRejection {
                        code: "shop_serialize_failed".to_string(),
                        message: format!(
                            "Failed to serialize spawned d2r-v3 item '{catalog_id}': {e}"
                        ),
                    })?;
                    item.clone()
                }
            }
        } else {
            let bytes = item.to_bytes(0, &huffman, alpha).map_err(|e| ActionRejection {
                code: "shop_serialize_failed".to_string(),
                message: format!("Failed to serialize spawned item '{catalog_id}': {e}"),
            })?;
            Item::from_bytes_with_hint(&bytes, &huffman, alpha, Some(item.code.trim())).map_err(
                |e| ActionRejection {
                    code: "shop_roundtrip_failed".to_string(),
                    message: format!("Failed to roundtrip spawned item '{catalog_id}': {e}"),
                },
            )?
        };
        canonical.x = item.x;
        canonical.y = item.y;
        canonical.page = raw_page;
        canonical.location = 0;
        canonical.mode = 0;
        canonical.header.x = item.x;
        canonical.header.y = item.y;
        canonical.header.page = raw_page;
        canonical.header.location = 0;
        canonical.header.mode = 0;
        canonical.body.x = item.x;
        canonical.body.y = item.y;
        canonical.body.page = raw_page;
        canonical.body.location = 0;
        canonical.body.mode = 0;

        let new_idx = self.item_cards.len();
        let card = build_item_card(
            new_idx as i32,
            &canonical,
            self.char_level,
            0,
            dest_kind,
            None,
            i32::from(canonical.x),
            i32::from(canonical.y),
        );

        self.player_items_with_container.push((canonical, dest_enum));
        self.item_cards.push(card);
        self.selected_move_item_index = None;
        self.has_item_edits = true;
        self.dirty = true;
        Ok(())
    }

    fn apply_character_edit(
        &mut self,
        patch: impl FnOnce(&mut CharacterMutationContract) -> Result<(), String>,
    ) -> Result<(), ActionRejection> {
        let mut next_edits = self.char_edits.clone();
        patch(&mut next_edits).map_err(|message| ActionRejection {
            code: "invalid_character_mutation".to_string(),
            message,
        })?;
        let huffman = HuffmanTree::new();
        let rebuilt = try_apply_character_mutation(&self.source_bytes, &next_edits, &huffman)
            .map_err(|e| ActionRejection {
                code: "invalid_character_mutation".to_string(),
                message: e.to_string(),
            })?;
        let rebuilt_map = map_core_sections(&rebuilt).map_err(|e| ActionRejection {
            code: "invalid_character_mutation".to_string(),
            message: format!("Failed to remap character sections: {e}"),
        })?;
        let rebuilt_attrs =
            AttributeSection::parse(&rebuilt, rebuilt_map.gf_pos, rebuilt_map.if_pos).map_err(
                |e| ActionRejection {
                    code: "invalid_character_mutation".to_string(),
                    message: format!("Failed to parse updated attributes: {e}"),
                },
            )?;
        let alpha = self.save_version == 105;
        self.char_level = rebuilt_attrs
            .actual_value(12, alpha)
            .unwrap_or(i32::from(self.char_level))
            .clamp(1, 99) as u8;
        self.experience = rebuilt_attrs
            .actual_value(13, alpha)
            .unwrap_or(self.experience);
        if let Some(ref name) = next_edits.name {
            self.char_name = name.clone();
        }
        self.attributes = rebuilt_attrs;
        self.char_edits = next_edits;
        self.has_char_edits = true;
        self.dirty = true;
        Ok(())
    }

    /// Serializes save-persisted edits back into a checksum-valid `.d2s` byte vector.
    pub fn to_save_bytes(&self) -> Result<Vec<u8>, String> {
        if self.item_cards.iter().any(|c| c.container == ContainerKind::Cursor) {
            return Err("Cannot export .d2s while a swapped item is held on the cursor. Place it in a slot or click Undo.".to_string());
        }
        if self.item_cards.iter().any(|c| c.container == ContainerKind::TempInventory) {
            return Err("Cannot export .d2s while items remain in Temp Inventory (9x4). Move them to a permanent container or clear Temp Inventory.".to_string());
        }
        if let Some(reason) = export_unsupported_reason(self.save_version) {
            return Err(reason);
        }
        if self.source_bytes.len() <= V105_WAYPOINT_OFFSET {
            return Err("Source save buffer is too small for progression writeback".to_string());
        }
        let mut working = if self.has_char_edits {
            let huffman = HuffmanTree::new();
            try_apply_character_mutation(&self.source_bytes, &self.char_edits, &huffman)
                .map_err(|e| format!("Failed to write character mutations: {e}"))?
        } else {
            self.source_bytes.clone()
        };
        if self.has_skill_edits {
            if let Some(base_id) = class_skill_base_id(self.char_class_id) {
                let huffman = HuffmanTree::new();
                let alpha = self.save_version == 105;
                let unspent = self
                    .attributes
                    .actual_value(5, alpha)
                    .unwrap_or(0)
                    .clamp(0, 255) as u8;
                let skills: Vec<(u32, u8)> = self
                    .skill_section
                    .iter_skills(base_id)
                    .map(|s| (s.skill_id, s.level))
                    .collect();
                let contract = SkillMutationContract {
                    skills,
                    skill_points: Some(unspent),
                    respec: false,
                };
                working = try_apply_skill_mutation(&working, &contract, &huffman)
                    .map_err(|e| format!("Failed to write skill mutations: {e}"))?;
            }
        }
        // Sync quests first; `Progression::sync_to_bytes` also writes `self.progression.waypoints`,
        // so per-difficulty `waypoints_by_diff` are written immediately afterward.
        self.progression.sync_to_bytes(&mut working, true);
        let wp_anchor = PROG_START_FILE + V105WaypointAxiom::start_offset();
        for wp_set in &self.waypoints_by_diff {
            wp_set.sync_to_bytes(&mut working[V105_WAYPOINT_OFFSET..], wp_anchor);
        }
        if self.has_item_edits {
            let huffman = HuffmanTree::new();
            let items: Vec<Item> = self
                .player_items_with_container
                .iter()
                .map(|(item, _)| item.clone())
                .collect();
            let alpha = self.item_dialect == "alpha";
            working = crate::save::rebuild_item_section(&working, &items, &huffman, alpha)
                .map_err(|e| format!("Failed to write item mutations: {e}"))?;
        }
        crate::save::finalize_save_bytes(&mut working, false)
            .map_err(|e| format!("Failed to finalize exported save checksum: {e}"))?;
        Ok(working)
    }
}

fn equipment_slot_label(body_loc: i32) -> Option<&'static str> {
    match body_loc {
        1 => Some("Head"),
        2 => Some("Amulet"),
        3 => Some("Armor"),
        4 => Some("Weapon1"),
        5 => Some("Weapon2"),
        6 => Some("Ring1"),
        7 => Some("Ring2"),
        8 => Some("Belt"),
        9 => Some("Boots"),
        10 => Some("Gloves"),
        11 => Some("Weapon1Swap"),
        12 => Some("Weapon2Swap"),
        _ => None,
    }
}

fn item_fits_equipment_slot(item_code: &str, body_loc: i32) -> bool {
    let types = crate::engine::validation::get_all_item_types(item_code);
    let has_any = |candidates: &[&str]| types.iter().any(|t| candidates.contains(t));
    match body_loc {
        1 => has_any(&["helm", "circ", "pelt", "phlm"]),
        2 => has_any(&["amul"]),
        3 => has_any(&["tors"]),
        4 | 11 => has_any(&["weap"]),
        5 | 12 => has_any(&["shld", "ashd", "head", "bowq", "xboq", "weap"]),
        6 | 7 => has_any(&["ring"]),
        8 => has_any(&["belt"]),
        9 => has_any(&["boot"]),
        10 => has_any(&["glov"]),
        _ => false,
    }
}

fn container_label(container: ItemContainer) -> &'static str {
    match container {
        ItemContainer::Equipment => "equipment",
        ItemContainer::Inventory => "inventory",
        ItemContainer::Belt => "belt",
        ItemContainer::Stash => "stash",
        ItemContainer::Cube => "cube",
        ItemContainer::Unknown => "unknown",
    }
}

fn quality_label(quality: Option<ItemQuality>, is_runeword: bool) -> &'static str {
    if is_runeword {
        return "Runeword";
    }
    match quality {
        Some(ItemQuality::Low) => "Low",
        Some(ItemQuality::Normal) => "Normal",
        Some(ItemQuality::High) => "Superior",
        Some(ItemQuality::Magic) => "Magic",
        Some(ItemQuality::Set) => "Set",
        Some(ItemQuality::Rare) => "Rare",
        Some(ItemQuality::Unique) => "Unique",
        Some(ItemQuality::Crafted) => "Crafted",
        None => "Unknown",
    }
}

#[derive(Debug, Deserialize)]
struct VectorAssetRecord {
    file: String,
}

#[derive(Debug, Deserialize)]
struct VectorAssetCatalog {
    assets: BTreeMap<String, VectorAssetRecord>,
}

struct ProjectionImageCatalogs {
    images: Option<ItemImageCatalog>,
    vectors: Option<VectorAssetCatalog>,
}

fn d2r_data_assets_root() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let data = std::env::var_os("D2R_DATA_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| manifest_dir.join("../d2r-data"));
    let root = if data.is_absolute() {
        data
    } else {
        manifest_dir.join(data)
    };
    root.join("assets")
}

fn projection_image_catalogs() -> &'static ProjectionImageCatalogs {
    static CATALOGS: OnceLock<ProjectionImageCatalogs> = OnceLock::new();
    CATALOGS.get_or_init(|| {
        let assets = d2r_data_assets_root();
        let images = std::fs::read(assets.join("item-images/catalog.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<ItemImageCatalog>(&bytes).ok());
        let vectors = std::fs::read(assets.join("item-vectors/catalog.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<VectorAssetCatalog>(&bytes).ok());
        ProjectionImageCatalogs { images, vectors }
    })
}

fn resolve_item_image_url(item: &Item) -> Option<String> {
    let catalogs = projection_image_catalogs();
    let key = ItemImageKey::from(item);
    let binding = resolve_item_image(&key, catalogs.images.as_ref());
    let image_ref = binding.image()?;
    if let Some(rec) = catalogs
        .vectors
        .as_ref()
        .and_then(|vc| vc.assets.get(&image_ref.asset_key))
    {
        return Some(format!("/item-vectors/{}", rec.file));
    }
    catalogs
        .images
        .as_ref()
        .and_then(|ic| ic.assets.get(&image_ref.asset_key))
        .map(|file| format!("/item-images/{}", file.file))
}

#[allow(clippy::too_many_arguments)]
fn build_item_card(
    index: i32,
    item: &Item,
    level: u8,
    active_set_count: usize,
    container: ContainerKind,
    slot_label: Option<String>,
    x: i32,
    y: i32,
) -> ItemCardView {
    let en = format_item(item, "en", active_set_count, level);
    let ko = format_item(item, "ko", active_set_count, level);
    let (w, h) = get_item_size(&item.code);
    let socketed_names = item
        .socketed_items
        .iter()
        .map(|child| format_item(child, "en", 0, level).name)
        .collect();
    let map_bonuses = |bonuses: Vec<crate::engine::formatter::FormattedSetBonus>| -> Vec<SetBonusView> {
        bonuses
            .into_iter()
            .map(|b| SetBonusView {
                active: b.active,
                required_count: i32::from(b.required_count),
                lines: b.lines,
            })
            .collect()
    };
    ItemCardView {
        index,
        code: item.code.trim().to_string(),
        name_en: en.name,
        name_ko: ko.name,
        quality: ItemQualityKind::from_item(item.header.quality, item.header.is_runeword),
        container,
        slot_label,
        x,
        y,
        width: i32::from(w),
        height: i32::from(h),
        item_level: i32::from(en.level),
        base_attributes: en.base_attributes,
        properties: en.properties,
        set_name: en.set_name,
        set_pieces: en.set_pieces,
        piece_set_bonuses: map_bonuses(en.piece_set_bonuses),
        set_bonuses: map_bonuses(en.set_bonuses),
        socketed_names,
        image_url: resolve_item_image_url(item),
        is_belt_consumable: is_belt_consumable(&item.code),
    }
}

fn quest_key(act: u8, index: u8) -> String {
    format!("a{act}q{}", index + 1)
}

fn build_quest_catalog(is_expansion: bool) -> Vec<QuestCatalogEntryView> {
    let max_act = if is_expansion { 5 } else { 4 };
    crate::data::quests::V105_QUESTS
        .iter()
        .filter(|entry| entry.difficulty == 0 && entry.act <= max_act)
        .map(|entry| QuestCatalogEntryView {
            key: quest_key(entry.act, entry.index),
            act: i32::from(entry.act),
            index: i32::from(entry.index),
            name: entry.name.to_string(),
        })
        .collect()
}

fn build_waypoint_catalog(is_expansion: bool) -> Vec<WaypointCatalogEntryView> {
    let max_act = if is_expansion { 5 } else { 4 };
    crate::data::waypoints::WAYPOINTS
        .iter()
        .filter(|entry| entry.act <= max_act)
        .map(|entry| WaypointCatalogEntryView {
            bit: i32::from(entry.ws_bit),
            act: i32::from(entry.act),
            index: i32::from(entry.index),
            name: entry.name.to_string(),
        })
        .collect()
}

fn completed_quest_keys(progression: &Progression, difficulty: u8) -> Vec<String> {
    progression
        .quests
        .quests()
        .iter()
        .filter(|q| q.difficulty() == difficulty && q.is_completed())
        .map(|q| quest_key(q.act(), q.index()))
        .collect()
}

fn active_waypoint_bits(set: &WaypointSet) -> Vec<i32> {
    set.waypoints()
        .iter()
        .filter(|wp| wp.is_active())
        .map(|wp| i32::from(wp.ws_bit()))
        .collect()
}

fn project_mercenary(
    state: &MercenaryState,
    level: i32,
    act_override: Option<i32>,
    merc_items: &[Item],
    merc_cards: &[ItemCardView],
    difficulty: DifficultyMode,
) -> MercSummaryView {
    let hired = act_override.is_some()
        || state.hireling_id != 0
        || state.class_id != 0
        || state.experience > 0
        || !merc_cards.is_empty();
    let inferred_act = if !hired {
        0
    } else if state.class_id == 9 {
        5
    } else if state.class_id == 1 {
        3
    } else if state.hireling_id >= 8 {
        2
    } else {
        1
    };
    let act_index = act_override.unwrap_or(inferred_act);
    let resolved_class_name = match act_index {
        1 => "Rogue Scout (Act 1)".to_string(),
        2 => "Desert Mercenary (Act 2)".to_string(),
        3 => "Iron Wolf (Act 3)".to_string(),
        5 => "Barbarian (Act 5)".to_string(),
        _ => state.class_name(),
    };

    let (base_str, base_dex, base_life, base_def, base_res, innate_aura) = match act_index {
        1 => (
            35 + level * 2,
            52 + level * 2,
            40 + level * 8,
            10 + level * 6,
            40 + (level * 3) / 4,
            "Inner Sight / Cold or Fire Arrow".to_string(),
        ),
        2 => (
            48 + (level * 3) / 2,
            38 + (level * 3) / 2,
            50 + level * 10,
            18 + level * 7,
            42 + (level * 3) / 4,
            "Paladin Aura (Act 2)".to_string(),
        ),
        3 => (
            40 + level,
            42 + level,
            45 + level * 8,
            14 + level * 6,
            48 + (level * 3) / 4,
            format!("Iron Wolf ({})", state.subtype_name()),
        ),
        5 => (
            58 + level * 2,
            34 + level,
            60 + level * 12,
            24 + level * 8,
            40 + (level * 3) / 4,
            "Bash / Stun / Battle Cry".to_string(),
        ),
        _ => (0, 0, 0, 0, 0, "None".to_string()),
    };

    let mut bonus_str = 0;
    let mut bonus_dex = 0;
    let mut bonus_life = 0;
    let mut bonus_def = 0;
    let mut bonus_fire = 0;
    let mut bonus_cold = 0;
    let mut bonus_light = 0;
    let mut bonus_poison = 0;
    let mut equipment_auras = Vec::new();

    for item in merc_items {
        bonus_def += item.defense.unwrap_or(0) as i32;
        for prop in item
            .properties
            .iter()
            .chain(item.runeword_attributes.iter())
            .chain(item.set_attributes.iter().flatten())
        {
            match prop.stat_id {
                0 => bonus_str += prop.value,
                2 => bonus_dex += prop.value,
                7 => bonus_life += prop.value / 256,
                39 => bonus_fire += prop.value,
                41 => bonus_light += prop.value,
                43 => bonus_cold += prop.value,
                45 => bonus_poison += prop.value,
                151 => {
                    let aura_name = format!("Aura #{} Lv.{}", prop.param, prop.value);
                    if !equipment_auras.contains(&aura_name) {
                        equipment_auras.push(aura_name);
                    }
                }
                _ => {}
            }
        }
    }

    let penalty = difficulty.to_sheet_difficulty().penalty();
    let calc_res = |bonus: i32| (base_res + bonus - penalty).clamp(-100, 75);

    MercSummaryView {
        hired,
        act_index,
        class_name: resolved_class_name,
        subtype_name: state.subtype_name(),
        experience: state.experience as i32,
        stats: MercStatsView {
            level,
            base_str,
            bonus_str,
            base_dex,
            bonus_dex,
            base_life,
            bonus_life,
            base_def,
            bonus_def,
            fire_res: calc_res(bonus_fire),
            cold_res: calc_res(bonus_cold),
            light_res: calc_res(bonus_light),
            poison_res: calc_res(bonus_poison),
            innate_aura,
            equipment_auras,
        },
        equipped_count: merc_cards.len() as i32,
        persistence_supported: false,
        preview_only_reason: Some(
            "Read-only: mercenary edits are disabled until .d2s mercenary writeback is implemented."
                .to_string(),
        ),
    }
}

fn export_unsupported_reason(save_version: i32) -> Option<String> {
    if save_version == 105 {
        None
    } else {
        Some(format!(
            "Binary .d2s export supports v105 saves only (loaded v{save_version})."
        ))
    }
}

fn shop_category_for_code(code: &str) -> &'static str {
    let trimmed = code.trim();
    if is_belt_consumable(trimmed) {
        "consumables"
    } else if (trimmed.starts_with('r') && trimmed.len() == 3 && trimmed[1..].chars().all(|c| c.is_ascii_digit()))
        || matches!(trimmed, "gpv" | "gpw" | "gpr" | "gpb" | "gpg" | "gpy" | "skz")
    {
        "runes_gems"
    } else if matches!(trimmed, "rin" | "amu" | "cm1" | "cm2" | "cm3" | "jew") {
        "jewelry_charms"
    } else if crate::engine::validation::get_all_item_types(trimmed).contains(&"weap") {
        "weapons"
    } else {
        "armor"
    }
}

fn shop_is_compact_code(code: &str) -> bool {
    let trimmed = code.trim();
    is_belt_consumable(trimmed)
        || (trimmed.starts_with('r') && trimmed.len() == 3 && trimmed[1..].chars().all(|c| c.is_ascii_digit()))
        || matches!(trimmed, "gpv" | "gpw" | "gpr" | "gpb" | "gpg" | "gpy" | "skz")
}

fn shop_make_property(stat_id: u32, raw_val: i32, param: u32) -> Option<ItemProperty> {
    let sc = crate::data::stat_costs::STAT_COSTS.iter().find(|s| s.id == stat_id)?;
    let bits = sc.save_bits;
    if bits == 0 || (sc.save_param_bits > 0 && param == 0 && stat_id == 204) {
        return None;
    }
    let raw_value = raw_val + sc.save_add;
    if raw_value < 0 {
        return None;
    }
    Some(ItemProperty::new(
        stat_id,
        sc.name.to_string(),
        param,
        raw_value,
        raw_val,
        ItemBitRange { start: 0, end: 0 },
    ))
}

fn shop_build_base_item(
    code: &str,
    quality: ItemQuality,
    is_compact: bool,
    alpha: bool,
    magic_prefix: Option<u16>,
    magic_suffix: Option<u16>,
    unique_id: Option<u16>,
    set_id: Option<u16>,
    stats: &[crate::data::item_specs::ItemStatRange],
) -> Item {
    let padded = format!("{code:<4}");
    let mut header = crate::domain::item::ItemHeader::default();
    header.flags = 0x0010; // Identified
    if is_compact {
        header.flags |= 1 << 21;
    }
    header.is_identified = true;
    header.version = if alpha { 5 } else { 2 };
    header.is_compact = is_compact;
    header.save_is_alpha = alpha;
    if !is_compact {
        header.quality = Some(quality);
        header.level = Some(85);
        header.id = Some(0x12345678);
    }
    if alpha {
        header.alpha_quality_raw = Some(quality as u8);
    }

    let mut props: Vec<ItemProperty> = if is_compact {
        Vec::new()
    } else {
        let mut list: Vec<ItemProperty> = stats
            .iter()
            .filter_map(|sr| {
                let val = if sr.max >= sr.min { sr.max } else { sr.min };
                shop_make_property(sr.stat_id, val, sr.param)
            })
            .collect();
        if !alpha {
            list.push(ItemProperty::new(
                511,
                "end".to_string(),
                0,
                0,
                0,
                ItemBitRange { start: 0, end: 0 },
            ));
        }
        list
    };

    let mut builder = ItemBuilder::new().header(header).code(padded);
    if !is_compact {
        builder = builder
            .quality(quality)
            .magic_affixes(magic_prefix, magic_suffix)
            .properties(props)
            .properties_complete(true);
    }
    let mut item = builder.build();
    item.is_compact = is_compact;
    if is_compact {
        item.quality = None;
        item.header.quality = None;
        item.level = None;
        item.header.level = None;
        item.id = None;
        item.header.id = None;
    } else {
        item.level = Some(85);
        item.header.level = Some(85);
        item.id = Some(0x12345678);
        item.header.id = Some(0x12345678);
        item.unique_id = unique_id.or(set_id);
        let template = crate::data::repository::legacy_item_template(code.trim());
        if template.is_some_and(|t| t.is_armor) {
            item.defense = Some(100);
            item.body.defense = Some(100);
        }
        if template.is_some_and(|t| t.has_durability) {
            item.max_durability = Some(30);
            item.body.max_durability = Some(30);
            item.current_durability = Some(30);
            item.body.current_durability = Some(30);
        }
        if quality == ItemQuality::Magic {
            if item.magic_prefix.is_none() {
                item.magic_prefix = Some(0);
            }
            if item.magic_suffix.is_none() {
                item.magic_suffix = Some(0);
            }
        }
        if quality == ItemQuality::Rare {
            item.rare_name_1 = Some(1);
            item.rare_name_2 = Some(2);
        }
    }
    if !alpha {
        item.body.d2r_v3_auxiliary = Some(crate::domain::item::entity::D2rV3AuxiliaryData {
            save_version: 105,
            stored_in: Some(1),
            ..Default::default()
        });
    }
    item
}

fn shop_catalog_entries_spec() -> Vec<(&'static str, ItemQuality, &'static str)> {
    vec![
        // Normal: Runes, Gems, Consumables, Bases
        ("base:r33", ItemQuality::Normal, "r33"),
        ("base:r31", ItemQuality::Normal, "r31"),
        ("base:r30", ItemQuality::Normal, "r30"),
        ("base:r29", ItemQuality::Normal, "r29"),
        ("base:r28", ItemQuality::Normal, "r28"),
        ("base:r27", ItemQuality::Normal, "r27"),
        ("base:r26", ItemQuality::Normal, "r26"),
        ("base:r24", ItemQuality::Normal, "r24"),
        ("base:hp5", ItemQuality::Normal, "hp5"),
        ("base:mp5", ItemQuality::Normal, "mp5"),
        ("base:rvl", ItemQuality::Normal, "rvl"),
        ("base:tsc", ItemQuality::Normal, "tsc"),
        ("base:isc", ItemQuality::Normal, "isc"),
        ("base:gpv", ItemQuality::Normal, "gpv"),
        ("base:gpw", ItemQuality::Normal, "gpw"),
        ("base:xtp", ItemQuality::Normal, "xtp"),
        ("base:uap", ItemQuality::Normal, "uap"),
        ("base:ci3", ItemQuality::Normal, "ci3"),
        // Magic
        ("magic:cm1", ItemQuality::Magic, "cm1"),
        ("magic:cm3", ItemQuality::Magic, "cm3"),
        ("magic:rin", ItemQuality::Magic, "rin"),
        ("magic:amu", ItemQuality::Magic, "amu"),
        ("magic:ci3", ItemQuality::Magic, "ci3"),
        ("magic:jew", ItemQuality::Magic, "jew"),
        // Rare
        ("rare:rin", ItemQuality::Rare, "rin"),
        ("rare:amu", ItemQuality::Rare, "amu"),
        ("rare:ci3", ItemQuality::Rare, "ci3"),
        ("rare:ulb", ItemQuality::Rare, "ulb"),
        ("rare:tbt", ItemQuality::Rare, "tbt"),
    ]
}

fn build_shop_item_by_id(catalog_id: &str, alpha: bool) -> Option<Item> {
    if let Some(id_str) = catalog_id.strip_prefix("uniq:") {
        let id: u32 = id_str.parse().ok()?;
        let entry = crate::data::unique_items::UNIQUE_ITEMS.iter().find(|u| u.id == id)?;
        return Some(shop_build_base_item(
            entry.code,
            ItemQuality::Unique,
            false,
            alpha,
            None,
            None,
            Some(entry.id as u16),
            None,
            entry.stats,
        ));
    }
    if let Some(id_str) = catalog_id.strip_prefix("set:") {
        let id: u32 = id_str.parse().ok()?;
        let entry = crate::data::set_items::SET_ITEMS.iter().find(|s| s.id == id)?;
        return Some(shop_build_base_item(
            entry.code,
            ItemQuality::Set,
            false,
            alpha,
            None,
            None,
            None,
            Some(entry.id as u16),
            entry.stats,
        ));
    }
    let (_, quality, code) = shop_catalog_entries_spec()
        .into_iter()
        .find(|(cid, _, _)| *cid == catalog_id)?;
    let (prefix, suffix) = match quality {
        ItemQuality::Magic => (Some(12), Some(34)),
        ItemQuality::Rare => (Some(10), Some(15)),
        _ => (None, None),
    };
    Some(shop_build_base_item(
        code,
        quality,
        shop_is_compact_code(code),
        alpha,
        prefix,
        suffix,
        None,
        None,
        &[],
    ))
}

fn cached_shop_catalog() -> &'static [ShopCatalogItemView] {
    static CATALOG: OnceLock<Vec<ShopCatalogItemView>> = OnceLock::new();
    CATALOG.get_or_init(|| {
        let huffman = HuffmanTree::new();
        let mut out = Vec::new();
        let mut try_push = |catalog_id: String, name_en: String, name_ko: String, summary: String| {
            let Some(item) = build_shop_item_by_id(&catalog_id, true) else {
                return;
            };
            let Ok(bytes) = item.to_bytes(0, &huffman, true) else {
                return;
            };
            if Item::from_bytes_with_hint(&bytes, &huffman, true, Some(item.code.trim())).is_err() {
                return;
            }
            let (w, h) = get_item_size(item.code.trim());
            let quality = ItemQualityKind::from_item(item.header.quality, item.header.is_runeword);
            let category = shop_category_for_code(item.code.trim()).to_string();
            let image_url = resolve_item_image_url(&item);
            out.push(ShopCatalogItemView {
                catalog_id,
                code: item.code.trim().to_string(),
                name_en,
                name_ko,
                quality,
                category,
                width: i32::from(w.max(1)),
                height: i32::from(h.max(1)),
                image_url,
                summary,
            });
        };

        for (cid, quality, code) in shop_catalog_entries_spec() {
            let name_en = get_loc(code, "en").to_string();
            let name_ko = get_loc(code, "ko").to_string();
            let summary = format!("{} · {}", quality_label(Some(quality), false), code);
            try_push(cid.to_string(), name_en, name_ko, summary);
        }

        for s in crate::data::set_items::SET_ITEMS.iter().take(24) {
            let cid = format!("set:{}", s.id);
            let name_en = get_loc(s.index, "en").to_string();
            let name_ko = get_loc(s.index, "ko").to_string();
            let summary = format!("Set · {}", s.set_id);
            try_push(cid, name_en, name_ko, summary);
        }

        for u in crate::data::unique_items::UNIQUE_ITEMS.iter().take(36) {
            let cid = format!("uniq:{}", u.id);
            let name_en = get_loc(u.index, "en").to_string();
            let name_ko = get_loc(u.index, "ko").to_string();
            let summary = format!("Unique · qlvl {}", u.qlvl);
            try_push(cid, name_en, name_ko, summary);
        }

        out
    })
}

/// Exports the Elm `Generated.Protocol` module source string via `elm_rs::export!`.
pub fn generate_elm_protocol_module() -> Result<String, String> {
    let mut target = Vec::<u8>::new();
    elm_rs::export!("Generated.Protocol", &mut target, {
        encoders: [
            DifficultyMode,
            EditMode,
            CharacterField,
            ContainerKind,
            ItemQualityKind,
            ShopSpawnTarget,
            UiIntent,
        ],
        decoders: [
            DifficultyMode,
            EditMode,
            ContainerKind,
            ItemQualityKind,
            ShopSpawnTarget,
            ResistanceCardView,
            CharacterSchemaView,
            CharacterStatsView,
            ContainerCountsView,
            MercStatsView,
            MercSummaryView,
            QuestCatalogEntryView,
            WaypointCatalogEntryView,
            ProgressionSummaryView,
            SetBonusView,
            ItemCardView,
            AllowedMoveTargetView,
            SkillNodeView,
            ShopCatalogItemView,
            SessionView,
            ActionRejection,
            SessionEnvelope,
        ],
    })
    .map_err(|e| format!("elm_rs export failed: {e}"))?;

    let mut out = String::from_utf8(target).map_err(|e| format!("Invalid UTF-8 from elm_rs: {e}"))?;
    if !out.ends_with('\n') {
        out.push('\n');
    }
    Ok(out)
}
