//! Rust-as-Store projection layer and `elm_rs` protocol boundary for Ortho Editor.
//!
//! This module projects parsed save data into immutable `SessionView` snapshots for Elm,
//! accepts typed `UiIntent` commands, validates invariants in Rust, and exports the
//! single-source-of-truth Elm `Generated.Protocol` module via `elm_rs`.

pub mod host;
pub mod merc;

use std::collections::{BTreeMap, BTreeSet, VecDeque};
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
use crate::engine::formatter::format_item;
use crate::localization::get_loc;
use crate::engine::mercenary_items::{collect_mercenary_items, mercenary_item_slot};
use crate::inventory::get_item_size;
use crate::item::{HuffmanTree, Item, ItemQuality};
use crate::save::{
    AttributeSection, Save, class_name, class_skill_base_id, map_core_sections,
    try_apply_character_mutation, try_apply_skill_mutation,
};

/// Compute `(min_logical, max_logical, save_add)` for a stat ID from `STAT_COSTS`.
/// Returns `None` for unknown stats, `save_bits == 0`, or the `511` terminator.
fn stat_value_bounds(stat_id: u32) -> Option<(i32, i32, i32)> {
    if stat_id == 511 {
        return None;
    }
    let save_bits = crate::domain::stats::stat_save_bits(stat_id)?;
    if save_bits == 0 {
        return None;
    }
    let save_add = crate::domain::stats::stat_save_add(stat_id).unwrap_or(0);
    let raw_max: i64 = (1i64 << save_bits.min(31)) - 1;
    let min_val = (-i64::from(save_add)).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
    let max_val = (raw_max - i64::from(save_add)).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
    Some((min_val, max_val, save_add))
}

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
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Elm, ElmEncode, ElmDecode,
)]
pub enum EditMode {
    #[default]
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
    #[serde(default)]
    pub stat_points_rule_cap: i32,
    #[serde(default)]
    pub stat_points_invested: i32,
    #[serde(default)]
    pub life_pure: i32,
    #[serde(default)]
    pub life_bonus: i32,
    #[serde(default)]
    pub mana_pure: i32,
    #[serde(default)]
    pub mana_bonus: i32,
    #[serde(default)]
    pub stamina_pure: i32,
    #[serde(default)]
    pub stamina_bonus: i32,
}

/// Computed skill damage breakdown (pure skill + synergy bonus + gear +skills bonus).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Elm, ElmEncode, ElmDecode)]
pub struct SkillDamageView {
    pub elem_type: String,
    pub pure_min: i32,
    pub pure_max: i32,
    pub synergy_pct: i32,
    pub synergy_bonus_min: i32,
    pub synergy_bonus_max: i32,
    pub gear_bonus_min: i32,
    pub gear_bonus_max: i32,
    pub total_min: i32,
    pub total_max: i32,
    pub preview_only: bool,
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
    #[serde(default)]
    pub desc_en: String,
    #[serde(default)]
    pub desc_ko: String,
    #[serde(default)]
    pub prereq_ids: Vec<i32>,
    #[serde(default)]
    pub synergy_ids: Vec<i32>,
    #[serde(default)]
    pub unlock_ids: Vec<i32>,
    #[serde(default)]
    pub damage: Option<SkillDamageView>,
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
    #[serde(default)]
    pub class_base_str: i32,
    #[serde(default)]
    pub class_base_dex: i32,
    #[serde(default)]
    pub class_base_vit: i32,
    #[serde(default)]
    pub class_base_ene: i32,
    #[serde(default)]
    pub item_defense_max: i32,
    #[serde(default)]
    pub item_quantity_max: i32,
    #[serde(default)]
    pub skill_legit_max: i32,
    #[serde(default)]
    pub skill_free_max: i32,
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

/// Replaced (dismissed) mercenary metadata retained in Zone 4 staging until restored or cleared.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Elm, ElmEncode, ElmDecode)]
pub struct DismissedMercView {
    pub act_index: i32,
    pub class_name: String,
    pub subtype_name: String,
    pub level: i32,
    pub remaining_items: i32,
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
    #[serde(default)]
    pub dismissed: Option<DismissedMercView>,
}

/// Canonical quest definition projected from `d2r-data` (`V105_QUESTS`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Elm, ElmEncode, ElmDecode)]
pub struct QuestCatalogEntryView {
    pub key: String,
    pub act: i32,
    pub index: i32,
    pub name: String,
    #[serde(default)]
    pub name_ko: String,
    #[serde(default)]
    pub desc_en: String,
    #[serde(default)]
    pub desc_ko: String,
}

/// Coarse semantic progression stage derived from the 16-bit quest state word.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Elm, ElmEncode, ElmDecode)]
pub enum QuestSemanticStatusView {
    QuestUnstarted,
    QuestInProgress,
    QuestRewardPending,
    QuestCompleted,
}

/// Per-quest semantic status and raw 16-bit state for a single difficulty.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Elm, ElmEncode, ElmDecode)]
pub struct QuestStateEntryView {
    pub key: String,
    pub status: QuestSemanticStatusView,
    pub raw_state: i32,
}

/// Canonical waypoint definition projected from `d2r-data` (`WAYPOINTS`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Elm, ElmEncode, ElmDecode)]
pub struct WaypointCatalogEntryView {
    pub bit: i32,
    pub act: i32,
    pub index: i32,
    pub name: String,
    #[serde(default)]
    pub name_ko: String,
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
    #[serde(default)]
    pub normal_quest_states: Vec<QuestStateEntryView>,
    #[serde(default)]
    pub nightmare_quest_states: Vec<QuestStateEntryView>,
    #[serde(default)]
    pub hell_quest_states: Vec<QuestStateEntryView>,
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
    MercReplaced,
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
            ContainerKind::MercReplaced => "merc_replaced",
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

/// Semantic role of a stat's parameter bits for UI selection/input rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Elm, ElmEncode, ElmDecode)]
pub enum StatParamKind {
    ParamNone,
    ParamRaw,
    ParamClass,
    ParamElement,
    ParamSkillTab,
    ParamSkill,
    ParamPackedSkill,
}

impl StatParamKind {
    fn from_tag(tag: &str) -> Self {
        match tag {
            "class" => Self::ParamClass,
            "element" => Self::ParamElement,
            "skill_tab" => Self::ParamSkillTab,
            "skill" => Self::ParamSkill,
            "packed_skill" => Self::ParamPackedSkill,
            "raw" => Self::ParamRaw,
            _ => Self::ParamNone,
        }
    }
}

/// Selectable `(value, label)` choice for parameterized item stats.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Elm, ElmEncode, ElmDecode)]
pub struct ParamOptionView {
    pub value: i32,
    pub label: String,
}

/// Bit-packing bounds for `ParamPackedSkill` (`skill_id * stride + level`), sourced from `domain::stats::semantics`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Elm, ElmEncode, ElmDecode)]
pub struct PackedSkillSpecView {
    pub stride: i32,
    pub min_level: i32,
    pub max_level: i32,
}

impl Default for PackedSkillSpecView {
    fn default() -> Self {
        Self {
            stride: crate::domain::stats::semantics::SKILL_PARAM_STRIDE,
            min_level: crate::domain::stats::semantics::SKILL_LEVEL_MIN,
            max_level: crate::domain::stats::semantics::SKILL_LEVEL_MAX,
        }
    }
}

/// Static parameter choice catalogs projected once on initial session load.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Elm, ElmEncode, ElmDecode)]
pub struct ParamCatalogView {
    #[serde(default)]
    pub classes: Vec<ParamOptionView>,
    #[serde(default)]
    pub elements: Vec<ParamOptionView>,
    #[serde(default)]
    pub skill_tabs: Vec<ParamOptionView>,
    #[serde(default)]
    pub skills: Vec<ParamOptionView>,
    #[serde(default)]
    pub packed_skill: PackedSkillSpecView,
}

/// Isolated item-editing draft projected while Item Forge Studio is open.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Elm, ElmEncode, ElmDecode)]
pub struct ItemForgeDraftView {
    pub item_index: i32,
    pub dirty: bool,
    pub card: ItemCardView,
}

#[derive(Debug, Clone)]
struct ItemForgeDraft {
    item_index: i32,
    item: Item,
    dirty: bool,
}

/// Structured property detail projected for the Item Studio Inspector.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Elm, ElmEncode, ElmDecode)]
pub struct ItemPropertyView {
    pub stat_id: i32,
    pub name: String,
    pub value: i32,
    pub param: i32,
    pub param_bits: i32,
    pub min_value: i32,
    pub max_value: i32,
    pub value_label: String,
    pub param_label: String,
    pub param_kind: StatParamKind,
    pub param_preview: Option<String>,
}

/// Projected gem, rune, or jewel socketed inside a parent item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Elm, ElmEncode, ElmDecode)]
pub struct SocketedItemView {
    pub code: String,
    pub name_en: String,
    pub name_ko: String,
    pub quality: ItemQualityKind,
    #[serde(default)]
    pub image_url: Option<String>,
    #[serde(default)]
    pub properties: Vec<String>,
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
    pub detailed_properties: Vec<ItemPropertyView>,
    #[serde(default)]
    pub defense: Option<i32>,
    #[serde(default)]
    pub current_durability: Option<i32>,
    #[serde(default)]
    pub max_durability: Option<i32>,
    #[serde(default)]
    pub quantity: Option<i32>,
    #[serde(default)]
    pub is_ethereal: bool,
    #[serde(default)]
    pub set_name: Option<String>,
    #[serde(default)]
    pub set_pieces: Vec<String>,
    #[serde(default)]
    pub piece_set_bonuses: Vec<SetBonusView>,
    #[serde(default)]
    pub set_bonuses: Vec<SetBonusView>,
    #[serde(default)]
    pub socket_count: i32,
    #[serde(default)]
    pub socketed_names: Vec<String>,
    #[serde(default)]
    pub socketed_items: Vec<SocketedItemView>,
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
pub struct StatCatalogEntryView {
    pub stat_id: i32,
    pub name: String,
    pub min_value: i32,
    pub max_value: i32,
    pub param_bits: i32,
    pub value_label: String,
    pub param_label: String,
    pub param_kind: StatParamKind,
}

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
    #[serde(default)]
    pub stat_mode: EditMode,
    pub dirty: bool,
    pub schema: CharacterSchemaView,
    pub stats: CharacterStatsView,
    pub resistances: ResistanceCardView,
    pub container_counts: ContainerCountsView,
    pub merc: MercSummaryView,
    pub progression: ProgressionSummaryView,
    pub items: Vec<ItemCardView>,
    pub merc_items: Vec<ItemCardView>,
    #[serde(default)]
    pub merc_replaced_items: Vec<ItemCardView>,
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
    #[serde(default)]
    pub stat_catalog: Vec<StatCatalogEntryView>,
    #[serde(default)]
    pub param_catalog: ParamCatalogView,
    #[serde(default)]
    pub active_item_draft: Option<ItemForgeDraftView>,
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
    SelectStatMode {
        mode: EditMode,
    },
    ResetStats,
    SyncStatPointsToRules,
    SetMercLevel {
        level: i32,
    },
    SelectMercAct {
        act_index: i32,
    },
    RestoreReplacedMerc,
    ClearReplacedMerc {
        reclaim_to_storage: bool,
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
    MutateItemBase {
        item_index: i32,
        defense: Option<i32>,
        current_durability: Option<i32>,
        max_durability: Option<i32>,
        quantity: Option<i32>,
        is_ethereal: Option<bool>,
    },
    MutateItemProperty {
        item_index: i32,
        stat_id: i32,
        delta: i32,
    },
    AddItemProperty {
        item_index: i32,
        stat_id: i32,
        value: i32,
        param: i32,
    },
    RemoveItemProperty {
        item_index: i32,
        stat_id: i32,
    },
    SetItemPropertyParam {
        item_index: i32,
        stat_id: i32,
        param: i32,
    },
    OpenItemForgeDraft {
        item_index: i32,
    },
    ApplyItemForgeDraft,
    DiscardItemForgeDraft,
    QuickMoveItem {
        item_index: i32,
        target_container: ContainerKind,
    },
    DuplicateItem {
        item_index: i32,
    },
    DeleteItem {
        item_index: i32,
    },
}

/// Domain taxonomy of rejection codes returned when Rust rejects a `UiIntent`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Elm, ElmEncode, ElmDecode)]
pub enum RejectionCode {
    RejectionInventoryFull,
    RejectionSlotOccupied,
    RejectionInvalidPlacement,
    RejectionStatOutOfRange,
    RejectionSkillPrerequisiteNotMet,
    RejectionOpaqueBitstreamViolation,
    RejectionMercPersistenceNotSupported,
    RejectionNothingToUndo,
    RejectionNothingToRedo,
    RejectionShopSpawnFailed,
    RejectionUnknown,
}

impl RejectionCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::RejectionInventoryFull => "inventory_full",
            Self::RejectionSlotOccupied => "slot_occupied",
            Self::RejectionInvalidPlacement => "invalid_placement",
            Self::RejectionStatOutOfRange => "stat_out_of_range",
            Self::RejectionSkillPrerequisiteNotMet => "skill_prerequisite_not_met",
            Self::RejectionOpaqueBitstreamViolation => "opaque_bitstream_violation",
            Self::RejectionMercPersistenceNotSupported => "merc_persistence_not_supported",
            Self::RejectionNothingToUndo => "nothing_to_undo",
            Self::RejectionNothingToRedo => "nothing_to_redo",
            Self::RejectionShopSpawnFailed => "shop_spawn_failed",
            Self::RejectionUnknown => "unknown_error",
        }
    }
}

/// Suggested recovery action guided to Elm when an intent is rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Elm, ElmEncode, ElmDecode)]
pub enum SuggestedRecovery {
    SuggestUndo,
    SuggestClearSlot,
    SuggestAdjustValue,
    SuggestNone,
}

/// Structured rejection returned when Rust rejects a `UiIntent` invariant violation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Elm, ElmEncode, ElmDecode)]
pub struct ActionRejection {
    pub code: RejectionCode,
    pub raw_code: String,
    pub message: String,
    pub target_id: Option<String>,
    pub suggested_action: Option<SuggestedRecovery>,
}

impl ActionRejection {
    pub fn new(code: RejectionCode, raw_code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code,
            raw_code: raw_code.into(),
            message: message.into(),
            target_id: None,
            suggested_action: None,
        }
    }

    pub fn with_target(mut self, target: impl Into<String>) -> Self {
        self.target_id = Some(target.into());
        self
    }

    pub fn with_recovery(mut self, recovery: SuggestedRecovery) -> Self {
        self.suggested_action = Some(recovery);
        self
    }
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
    stat_mode: EditMode,
    attributes: AttributeSection,
    progression: Progression,
    waypoints_by_diff: [WaypointSet; 3],
    player_items_with_container: Vec<(Item, ItemContainer)>,
    item_cards: Vec<ItemCardView>,
    temp_items: Vec<ItemCardView>,
    merc_state: MercenaryState,
    merc_hired: bool,
    merc_level: i32,
    merc_act_override: Option<i32>,
    has_merc_edits: bool,
    dismissed_merc: Option<(MercenaryState, i32, i32)>,
    skill_section: SkillSection,
    has_skill_edits: bool,
    active_skill_count: i32,
    active_weapon_set: i32,
    char_edits: CharacterMutationContract,
    has_char_edits: bool,
    has_item_edits: bool,
    selected_move_item_index: Option<i32>,
    active_item_draft: Option<ItemForgeDraft>,
    undo_stack: SnapshotQueue,
    redo_stack: SnapshotQueue,
}

/// Default maximum number of snapshots retained in each Undo/Redo history queue.
const DEFAULT_MAX_HISTORY_SNAPSHOTS: usize = 100;

/// Bounded FIFO-evicting snapshot queue for Rust-owned Undo/Redo history.
///
/// Acts as a LIFO stack for Undo/Redo (`push` / `pop`) while garbage-collecting
/// the oldest snapshot from the front (`pop_front`) once `max_entries` is reached.
#[derive(Debug, Clone)]
struct SnapshotQueue {
    entries: VecDeque<SessionSnapshot>,
    max_entries: usize,
}

impl SnapshotQueue {
    fn new(max_entries: usize) -> Self {
        Self {
            entries: VecDeque::with_capacity(max_entries),
            max_entries,
        }
    }

    fn push(&mut self, snapshot: SessionSnapshot) {
        if self.max_entries == 0 {
            return;
        }
        while self.entries.len() >= self.max_entries {
            self.entries.pop_front();
        }
        self.entries.push_back(snapshot);
    }

    fn pop(&mut self) -> Option<SessionSnapshot> {
        self.entries.pop_back()
    }

    fn clear(&mut self) {
        self.entries.clear();
    }

    fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.len()
    }
}

/// In-memory state snapshot for Rust-owned Undo/Redo history.
#[derive(Debug, Clone)]
struct SessionSnapshot {
    dirty: bool,
    res_difficulty: DifficultyMode,
    skill_mode: EditMode,
    stat_mode: EditMode,
    attributes: AttributeSection,
    progression: Progression,
    waypoints_by_diff: [WaypointSet; 3],
    player_items_with_container: Vec<(Item, ItemContainer)>,
    item_cards: Vec<ItemCardView>,
    temp_items: Vec<ItemCardView>,
    merc_state: MercenaryState,
    merc_hired: bool,
    merc_level: i32,
    merc_act_override: Option<i32>,
    has_merc_edits: bool,
    dismissed_merc: Option<(MercenaryState, i32, i32)>,
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
        let mut merc_item_count = 0usize;
        for item in collected_merc.items.into_iter() {
            if collected_merc.evidence.dialect == crate::domain::item::stream::ItemStreamDialect::Legacy
                && item.code.trim().is_empty()
            {
                continue;
            }
            let (slot, _source, _kind) = mercenary_item_slot(&item, collected_merc.evidence.dialect);
            let body_loc = match slot.as_str() {
                "Helm" | "Head" => 1,
                "Armor" => 3,
                "Weapon1" | "Right Hand" => 4,
                "Weapon2" | "Left Hand" | "Shield" => 5,
                _ => i32::from(item.x),
            };
            let global_idx = item_cards.len() as i32;
            item_cards.push(build_item_card(
                global_idx,
                &item,
                merc_level.clamp(1, 98) as u8,
                0,
                ContainerKind::MercEquipment,
                merc::merc_slot_label(body_loc).map(str::to_string).or(Some(slot)),
                body_loc,
                0,
            ));
            player_items_with_container.push((item, ItemContainer::Equipment));
            merc_item_count += 1;
        }
        let merc_hired = merc::inferred_merc_act(&merc_state, merc_item_count > 0) != 0;

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
            stat_mode: EditMode::Legit,
            attributes,
            progression,
            waypoints_by_diff,
            player_items_with_container,
            item_cards,
            temp_items: Vec::new(),
            merc_state,
            merc_hired,
            merc_level,
            merc_act_override: None,
            has_merc_edits: false,
            dismissed_merc: None,
            skill_section,
            has_skill_edits: false,
            active_skill_count,
            active_weapon_set: 1,
            char_edits: CharacterMutationContract::default(),
            has_char_edits: false,
            has_item_edits: false,
            selected_move_item_index: None,
            active_item_draft: None,
            undo_stack: SnapshotQueue::new(DEFAULT_MAX_HISTORY_SNAPSHOTS),
            redo_stack: SnapshotQueue::new(DEFAULT_MAX_HISTORY_SNAPSHOTS),
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

        let class_rule = crate::data::char_base_stats::class_base_rule(self.char_class_id);
        let lam_esen_bonus = crate::engine::character_sheet::count_lam_esen_bonuses(Some(&self.progression.quests));
        let golden_bird_bonus = crate::engine::character_sheet::count_golden_bird_bonuses(Some(&self.progression.quests));
        let lvl_i32 = i32::from(self.char_level.clamp(1, 99));
        let stat_points_rule_cap =
            (lvl_i32 - 1) * crate::data::char_base_stats::STAT_POINTS_PER_LEVEL + lam_esen_bonus;
        let stat_points_invested = (sheet.strength.base - class_rule.base_str).max(0)
            + (sheet.dexterity.base - class_rule.base_dex).max(0)
            + (sheet.vitality.base - class_rule.base_vit).max(0)
            + (sheet.energy.base - class_rule.base_ene).max(0);

        let life_pure = class_rule.base_life
            + ((lvl_i32 - 1) * class_rule.life_per_level_x4) / 4
            + ((sheet.vitality.base - class_rule.base_vit).max(0) * class_rule.life_per_vit_x4) / 4
            + golden_bird_bonus;
        let life_from_gear_vit = (sheet.vitality.bonus * class_rule.life_per_vit_x4) / 4;
        let life_per_lv = (sheet.gear_life_per_level_x8 * lvl_i32) / 8;
        let life_sub = life_pure + sheet.gear_life_flat + life_per_lv + life_from_gear_vit;
        let life_pct_bonus = (life_sub * sheet.gear_life_pct) / 100;
        let life_bonus = sheet.gear_life_flat + life_per_lv + life_from_gear_vit + life_pct_bonus;
        let effective_max_life = (life_pure + life_bonus).max(1);
        let current_life = (get_stat(6) / 256).clamp(0, effective_max_life);

        let mana_pure = class_rule.base_mana
            + ((lvl_i32 - 1) * class_rule.mana_per_level_x4) / 4
            + ((sheet.energy.base - class_rule.base_ene).max(0) * class_rule.mana_per_ene_x4) / 4;
        let mana_from_gear_ene = (sheet.energy.bonus * class_rule.mana_per_ene_x4) / 4;
        let mana_per_lv = (sheet.gear_mana_per_level_x8 * lvl_i32) / 8;
        let mana_sub = mana_pure + sheet.gear_mana_flat + mana_per_lv + mana_from_gear_ene;
        let mana_pct_bonus = (mana_sub * sheet.gear_mana_pct) / 100;
        let mana_bonus = sheet.gear_mana_flat + mana_per_lv + mana_from_gear_ene + mana_pct_bonus;
        let effective_max_mana = (mana_pure + mana_bonus).max(1);
        let current_mana = (get_stat(8) / 256).clamp(0, effective_max_mana);

        let stamina_pure = class_rule.base_stamina
            + ((lvl_i32 - 1) * class_rule.stamina_per_level_x4) / 4
            + ((sheet.vitality.base - class_rule.base_vit).max(0) * class_rule.stamina_per_vit_x4) / 4;
        let stamina_from_gear_vit = (sheet.vitality.bonus * class_rule.stamina_per_vit_x4) / 4;
        let stamina_per_lv = (sheet.gear_stamina_per_level_x8 * lvl_i32) / 8;
        let stamina_bonus = sheet.gear_stamina_flat + stamina_per_lv + stamina_from_gear_vit;
        let effective_max_stamina = (stamina_pure + stamina_bonus).max(1);
        let current_stamina = (get_stat(10) / 256).clamp(0, effective_max_stamina);

        let stats = CharacterStatsView {
            strength_base: sheet.strength.base,
            strength_bonus: sheet.strength.bonus,
            dexterity_base: sheet.dexterity.base,
            dexterity_bonus: sheet.dexterity.bonus,
            vitality_base: sheet.vitality.base,
            vitality_bonus: sheet.vitality.bonus,
            energy_base: sheet.energy.base,
            energy_bonus: sheet.energy.bonus,
            life: current_life,
            max_life: effective_max_life,
            mana: current_mana,
            max_mana: effective_max_mana,
            stamina: current_stamina,
            max_stamina: effective_max_stamina,
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
            stat_points_rule_cap,
            stat_points_invested,
            life_pure,
            life_bonus,
            mana_pure,
            mana_bonus,
            stamina_pure,
            stamina_bonus,
        };

        let mut counts = ContainerCountsView {
            equipment: 0,
            inventory: 0,
            belt: 0,
            stash: 0,
            cube: 0,
            temp_inventory: self.temp_items.len() as i32,
            unknown: 0,
            mercenary: 0,
        };
        for card in &self.item_cards {
            match card.container {
                ContainerKind::Equipment => counts.equipment += 1,
                ContainerKind::Inventory => counts.inventory += 1,
                ContainerKind::Belt => counts.belt += 1,
                ContainerKind::Stash => counts.stash += 1,
                ContainerKind::Cube => counts.cube += 1,
                ContainerKind::TempInventory => counts.temp_inventory += 1,
                ContainerKind::MercEquipment => counts.mercenary += 1,
                ContainerKind::MercReplaced | ContainerKind::Cursor => {}
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
            normal_quest_states: collect_quest_states(&self.progression, 0, self.is_expansion),
            nightmare_quest_states: collect_quest_states(&self.progression, 1, self.is_expansion),
            hell_quest_states: collect_quest_states(&self.progression, 2, self.is_expansion),
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
            class_base_str: class_rule.base_str,
            class_base_dex: class_rule.base_dex,
            class_base_vit: class_rule.base_vit,
            class_base_ene: class_rule.base_ene,
            item_defense_max: 2037,
            item_quantity_max: 511,
            skill_legit_max: 20,
            skill_free_max: 20,
        };

        let export_unsupported_reason = export_unsupported_reason(self.save_version).or_else(|| {
            if self.item_cards.iter().any(|c| c.container == ContainerKind::Cursor) {
                Some("Cannot export .d2s while a swapped item is held on the cursor. Place it in a slot or click Undo.".to_string())
            } else if self.item_cards.iter().any(|c| c.container == ContainerKind::TempInventory) {
                Some("Cannot export .d2s while items remain in Temp Inventory (9x4). Move them to a permanent container or clear Temp Inventory.".to_string())
            } else if self.item_cards.iter().any(|c| c.container == ContainerKind::MercReplaced) {
                Some("Cannot export .d2s while items remain in Replaced Mercenary Staging. Reclaim, restore, or clear them first.".to_string())
            } else if self.active_item_draft.as_ref().is_some_and(|d| d.dirty) {
                Some("Cannot export .d2s while Item Forge Studio has unsaved draft changes. Save or Discard the draft first.".to_string())
            } else {
                None
            }
        });
        let export_supported = export_unsupported_reason.is_none();

        let (allowed_move_targets, belt_move_restricted_reason) =
            self.project_allowed_move_targets();

        let player_items: Vec<ItemCardView> = self
            .item_cards
            .iter()
            .filter(|c| {
                !matches!(
                    c.container,
                    ContainerKind::MercEquipment | ContainerKind::MercReplaced
                )
            })
            .cloned()
            .collect();
        let merc_cards: Vec<ItemCardView> = self
            .item_cards
            .iter()
            .filter(|c| c.container == ContainerKind::MercEquipment)
            .cloned()
            .collect();
        let merc_replaced_cards: Vec<ItemCardView> = self
            .item_cards
            .iter()
            .filter(|c| c.container == ContainerKind::MercReplaced)
            .cloned()
            .collect();
        let merc_item_refs: Vec<&Item> = self
            .player_items_with_container
            .iter()
            .enumerate()
            .filter(|(i, _)| self.item_cards[*i].container == ContainerKind::MercEquipment)
            .map(|(_, (it, _))| it)
            .collect();

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
            stat_mode: self.stat_mode,
            dirty: self.dirty,
            schema,
            stats,
            resistances,
            container_counts: counts,
            merc: project_mercenary(
                &self.merc_state,
                self.merc_level,
                self.merc_act_override,
                self.merc_hired,
                &merc_item_refs,
                &merc_cards,
                &self.dismissed_merc,
                &merc_replaced_cards,
                self.res_difficulty,
            ),
            progression,
            items: player_items,
            merc_items: merc_cards,
            merc_replaced_items: merc_replaced_cards,
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
            stat_catalog: if include_catalog {
                cached_stat_catalog().to_vec()
            } else {
                Vec::new()
            },
            param_catalog: if include_catalog {
                cached_param_catalog().clone()
            } else {
                ParamCatalogView::default()
            },
            active_item_draft: self.active_item_draft.as_ref().and_then(|draft| {
                let idx = usize::try_from(draft.item_index).ok()?;
                let (_, container) = self.player_items_with_container.get(idx)?;
                let container_kind = ContainerKind::from_item_container(*container);
                let card = build_item_card(
                    draft.item_index,
                    &draft.item,
                    self.char_level,
                    0,
                    container_kind,
                    self.item_cards.get(idx).and_then(|c| c.slot_label.clone()),
                    self.item_cards.get(idx).map_or(0, |c| c.x),
                    self.item_cards.get(idx).map_or(0, |c| c.y),
                );
                Some(ItemForgeDraftView {
                    item_index: draft.item_index,
                    dirty: draft.dirty,
                    card,
                })
            }),
        }
    }

    fn capture_snapshot(&self) -> SessionSnapshot {
        SessionSnapshot {
            dirty: self.dirty,
            res_difficulty: self.res_difficulty,
            skill_mode: self.skill_mode,
            stat_mode: self.stat_mode,
            attributes: self.attributes.clone(),
            progression: self.progression.clone(),
            waypoints_by_diff: self.waypoints_by_diff.clone(),
            player_items_with_container: self.player_items_with_container.clone(),
            item_cards: self.item_cards.clone(),
            temp_items: self.temp_items.clone(),
            merc_state: self.merc_state.clone(),
            merc_hired: self.merc_hired,
            merc_level: self.merc_level,
            merc_act_override: self.merc_act_override,
            has_merc_edits: self.has_merc_edits,
            dismissed_merc: self.dismissed_merc.clone(),
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
        self.stat_mode = snap.stat_mode;
        self.attributes = snap.attributes;
        self.progression = snap.progression;
        self.waypoints_by_diff = snap.waypoints_by_diff;
        self.player_items_with_container = snap.player_items_with_container;
        self.item_cards = snap.item_cards;
        self.temp_items = snap.temp_items;
        self.merc_state = snap.merc_state;
        self.merc_hired = snap.merc_hired;
        self.merc_level = snap.merc_level;
        self.merc_act_override = snap.merc_act_override;
        self.has_merc_edits = snap.has_merc_edits;
        self.dismissed_merc = snap.dismissed_merc;
        self.skill_section = snap.skill_section;
        self.has_skill_edits = snap.has_skill_edits;
        self.active_skill_count = snap.active_skill_count;
        self.active_weapon_set = snap.active_weapon_set;
        self.char_edits = snap.char_edits;
        self.has_char_edits = snap.has_char_edits;
        self.has_item_edits = snap.has_item_edits;
        self.selected_move_item_index = snap.selected_move_item_index;
        self.active_item_draft = None;
    }

    /// Applies one `UiIntent` in Rust and returns a `SessionEnvelope`.
    pub fn apply_intent(&mut self, intent: UiIntent) -> SessionEnvelope {
        let draft_item_idx = self.active_item_draft.as_ref().map(|d| d.item_index);
        let is_draft_scoped = match &intent {
            UiIntent::MutateItemBase { item_index, .. }
            | UiIntent::MutateItemProperty { item_index, .. }
            | UiIntent::AddItemProperty { item_index, .. }
            | UiIntent::RemoveItemProperty { item_index, .. }
            | UiIntent::SetItemPropertyParam { item_index, .. } => {
                draft_item_idx == Some(*item_index)
            }
            _ => false,
        };
        let mutates_history = !is_draft_scoped
            && matches!(
                intent,
                UiIntent::SetMercLevel { .. }
                    | UiIntent::SelectMercAct { .. }
                    | UiIntent::RestoreReplacedMerc
                    | UiIntent::ClearReplacedMerc { .. }
                    | UiIntent::ToggleQuest { .. }
                    | UiIntent::SetQuestsBatch { .. }
                    | UiIntent::SetAllDiffQuests { .. }
                    | UiIntent::ToggleWaypoint { .. }
                    | UiIntent::SetWaypointsBatch { .. }
                    | UiIntent::SetAllDiffWaypoints { .. }
                    | UiIntent::SetCharacterField { .. }
                    | UiIntent::ResetStats
                    | UiIntent::SyncStatPointsToRules
                    | UiIntent::SetCharacterName { .. }
                    | UiIntent::MoveItem { .. }
                    | UiIntent::ClearTempInventory
                    | UiIntent::DisposeHeldItem
                    | UiIntent::AdjustSkillRank { .. }
                    | UiIntent::ResetSkills
                    | UiIntent::SpawnShopItem { .. }
                    | UiIntent::MutateItemBase { .. }
                    | UiIntent::MutateItemProperty { .. }
                    | UiIntent::AddItemProperty { .. }
                    | UiIntent::RemoveItemProperty { .. }
                    | UiIntent::SetItemPropertyParam { .. }
                    | UiIntent::ApplyItemForgeDraft
                    | UiIntent::QuickMoveItem { .. }
                    | UiIntent::DuplicateItem { .. }
                    | UiIntent::DeleteItem { .. }
            )
            && !matches!(
                intent,
                UiIntent::ApplyItemForgeDraft if self.active_item_draft.is_none()
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
            UiIntent::SelectStatMode { mode } => {
                self.stat_mode = mode;
                Ok(())
            }
            UiIntent::ResetStats => self.apply_reset_stats(),
            UiIntent::SyncStatPointsToRules => self.apply_sync_stat_points_to_rules(),
            UiIntent::SetMercLevel { level } => {
                if !(1..=98).contains(&level) {
                    return Err(ActionRejection::new(
                        RejectionCode::RejectionStatOutOfRange,
                        "merc_level_out_of_range",
                        format!("Mercenary level must stay within 1..=98 (got {level})"),
                    )
                    .with_target("merc_level")
                    .with_recovery(SuggestedRecovery::SuggestAdjustValue));
                }
                self.merc_level = level;
                self.merc_state.experience = merc::exp_for_merc_level(level);
                self.has_merc_edits = true;
                self.dirty = true;
                Ok(())
            }
            UiIntent::SelectMercAct { act_index } => {
                if !matches!(act_index, 1 | 2 | 3 | 5) {
                    return Err(ActionRejection::new(
                        RejectionCode::RejectionStatOutOfRange,
                        "invalid_merc_act",
                        format!("Mercenary act must be 1, 2, 3, or 5 (got {act_index})"),
                    )
                    .with_target("merc_act")
                    .with_recovery(SuggestedRecovery::SuggestAdjustValue));
                }
                let has_merc_items = self
                    .item_cards
                    .iter()
                    .any(|c| c.container == ContainerKind::MercEquipment);
                let current_act = self
                    .merc_act_override
                    .unwrap_or_else(|| merc::inferred_merc_act(&self.merc_state, has_merc_items));
                if self.merc_hired && current_act == act_index {
                    return Ok(());
                }
                if self.merc_hired {
                    self.dismissed_merc =
                        Some((self.merc_state.clone(), self.merc_level, current_act));
                    for card in &mut self.item_cards {
                        if card.container == ContainerKind::MercEquipment {
                            card.container = ContainerKind::MercReplaced;
                        }
                    }
                }
                self.merc_hired = true;
                self.merc_act_override = Some(act_index);
                merc::apply_merc_act_preset(&mut self.merc_state, act_index);
                if self.merc_level <= 1 {
                    let target_lvl = (i32::from(self.char_level) - 1).clamp(1, 98);
                    self.merc_level = target_lvl;
                    self.merc_state.experience = merc::exp_for_merc_level(target_lvl);
                }
                self.has_merc_edits = true;
                self.dirty = true;
                Ok(())
            }
            UiIntent::RestoreReplacedMerc => {
                let has_replaced_items = self
                    .item_cards
                    .iter()
                    .any(|c| c.container == ContainerKind::MercReplaced);
                if self.dismissed_merc.is_none() && !has_replaced_items {
                    return Err(ActionRejection::new(
                        RejectionCode::RejectionInvalidPlacement,
                        "no_replaced_merc",
                        "No replaced mercenary is staged for restoration.",
                    )
                    .with_target("merc_replaced"));
                }
                let has_active_items = self
                    .item_cards
                    .iter()
                    .any(|c| c.container == ContainerKind::MercEquipment);
                let current_act = self
                    .merc_act_override
                    .unwrap_or_else(|| merc::inferred_merc_act(&self.merc_state, has_active_items));
                let prev_active = if self.merc_hired {
                    Some((self.merc_state.clone(), self.merc_level, current_act))
                } else {
                    None
                };
                if let Some((old_state, old_lvl, old_act)) = self.dismissed_merc.take() {
                    self.merc_state = old_state;
                    self.merc_level = old_lvl;
                    self.merc_act_override = Some(old_act);
                }
                self.dismissed_merc = prev_active;
                for card in &mut self.item_cards {
                    if card.container == ContainerKind::MercEquipment {
                        card.container = ContainerKind::MercReplaced;
                    } else if card.container == ContainerKind::MercReplaced {
                        card.container = ContainerKind::MercEquipment;
                    }
                }
                self.merc_hired = true;
                self.has_merc_edits = true;
                self.dirty = true;
                Ok(())
            }
            UiIntent::ClearReplacedMerc { reclaim_to_storage } => {
                let has_replaced_items = self
                    .item_cards
                    .iter()
                    .any(|c| c.container == ContainerKind::MercReplaced);
                if self.dismissed_merc.is_none() && !has_replaced_items {
                    return Err(ActionRejection::new(
                        RejectionCode::RejectionInvalidPlacement,
                        "no_replaced_merc",
                        "No replaced mercenary staging to clear.",
                    )
                    .with_target("merc_replaced"));
                }
                if reclaim_to_storage {
                    let replaced_indices: Vec<i32> = self
                        .item_cards
                        .iter()
                        .filter(|c| c.container == ContainerKind::MercReplaced)
                        .map(|c| c.index)
                        .collect();
                    for idx_val in replaced_indices {
                        let (w, h) = self
                            .item_cards
                            .get(idx_val as usize)
                            .map(|c| (c.width.max(1), c.height.max(1)))
                            .unwrap_or((1, 1));
                        if let Some((fx, fy)) =
                            self.find_free_grid_slot(ContainerKind::Inventory, 10, 4, w, h)
                        {
                            let _ = self.apply_move_item(idx_val, ContainerKind::Inventory, fx, fy);
                        } else if let Some((fx, fy)) =
                            self.find_free_grid_slot(ContainerKind::Stash, 10, 10, w, h)
                        {
                            let _ = self.apply_move_item(idx_val, ContainerKind::Stash, fx, fy);
                        } else if let Some((fx, fy)) =
                            self.find_free_grid_slot(ContainerKind::TempInventory, 9, 4, w, h)
                        {
                            let _ =
                                self.apply_move_item(idx_val, ContainerKind::TempInventory, fx, fy);
                        }
                    }
                }
                let mut kept_items = Vec::with_capacity(self.player_items_with_container.len());
                let mut kept_cards = Vec::with_capacity(self.item_cards.len());
                for (item_pair, mut card) in self
                    .player_items_with_container
                    .drain(..)
                    .zip(self.item_cards.drain(..))
                {
                    if card.container != ContainerKind::MercReplaced {
                        card.index = kept_cards.len() as i32;
                        kept_items.push(item_pair);
                        kept_cards.push(card);
                    }
                }
                self.player_items_with_container = kept_items;
                self.item_cards = kept_cards;
                self.dismissed_merc = None;
                self.has_merc_edits = true;
                self.dirty = true;
                Ok(())
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
                    return Err(ActionRejection::new(
                        RejectionCode::RejectionUnknown,
                        "unknown_quest_key",
                        format!("Unknown quest key `{key}` for {difficulty:?}"),
                    )
                    .with_target(key));
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
                    return Err(ActionRejection::new(
                        RejectionCode::RejectionUnknown,
                        "unknown_quest_batch",
                        format!("No matching quests found for {difficulty:?}"),
                    ));
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
                    return Err(ActionRejection::new(
                        RejectionCode::RejectionStatOutOfRange,
                        "waypoint_bit_out_of_range",
                        format!("Waypoint bit {bit} is outside 0..{max_bits}"),
                    )
                    .with_target(format!("wp_{diff}_{bit}"))
                    .with_recovery(SuggestedRecovery::SuggestAdjustValue));
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
                        return Err(ActionRejection::new(
                            RejectionCode::RejectionStatOutOfRange,
                            "waypoint_bit_out_of_range",
                            format!("Waypoint bit {bit} is outside 0..{}", wps.len()),
                        )
                        .with_target(format!("wp_{diff}_{bit}"))
                        .with_recovery(SuggestedRecovery::SuggestAdjustValue));
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
                let mut legit_next_unspent: Option<u32> = None;
                if self.stat_mode == EditMode::Legit {
                    let alpha = self.save_version == 105;
                    let class_rule = crate::data::char_base_stats::class_base_rule(self.char_class_id);
                    let unspent_now = self.attributes.actual_value(4, alpha).unwrap_or(0).max(0);
                    match field {
                        CharacterField::StatPoints => {
                            return Err(ActionRejection::new(
                                RejectionCode::RejectionStatOutOfRange,
                                "stat_points_locked_in_legit_mode",
                                "Unspent stat points are rule-managed in Legit mode. Switch to Free mode or use Sync to Rules.",
                            )
                            .with_target("stat_points")
                            .with_recovery(SuggestedRecovery::SuggestAdjustValue));
                        }
                        CharacterField::Strength
                        | CharacterField::Dexterity
                        | CharacterField::Vitality
                        | CharacterField::Energy => {
                            let (stat_id, base_min, target_name) = match field {
                                CharacterField::Strength => (0u32, class_rule.base_str, "strength"),
                                CharacterField::Dexterity => (2u32, class_rule.base_dex, "dexterity"),
                                CharacterField::Vitality => (3u32, class_rule.base_vit, "vitality"),
                                CharacterField::Energy => (1u32, class_rule.base_ene, "energy"),
                                _ => unreachable!(),
                            };
                            let current_val = self.attributes.actual_value(stat_id, alpha).unwrap_or(0);
                            let floor_val = base_min.min(current_val);
                            if value < floor_val {
                                return Err(ActionRejection::new(
                                    RejectionCode::RejectionStatOutOfRange,
                                    "stat_below_class_base",
                                    format!("Cannot reduce {target_name} below class base ({base_min}) in Legit mode."),
                                )
                                .with_target(target_name)
                                .with_recovery(SuggestedRecovery::SuggestAdjustValue));
                            }
                            let delta = value - current_val;
                            if delta > unspent_now {
                                return Err(ActionRejection::new(
                                    RejectionCode::RejectionStatOutOfRange,
                                    "insufficient_stat_points",
                                    format!(
                                        "Insufficient unspent stat points (need {delta}, remaining {unspent_now}) in Legit mode."
                                    ),
                                )
                                .with_target(target_name)
                                .with_recovery(SuggestedRecovery::SuggestAdjustValue));
                            }
                            let next_unspent = (unspent_now - delta).clamp(0, 1023) as u32;
                            legit_next_unspent = Some(next_unspent);
                        }
                        _ => {}
                    }
                }
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
                            if let Some(next_unspent) = legit_next_unspent {
                                let unspent_vo =
                                    StatPointValue::new(next_unspent, 1023).map_err(|e| e.to_string())?;
                                edits.stat_points = Some(NumericOp::Set(unspent_vo));
                            }
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
                    return Err(ActionRejection::new(
                        RejectionCode::RejectionStatOutOfRange,
                        "invalid_character_mutation",
                        "Character name must use 2 to 15 ASCII alphanumeric, '-' or '_' characters.",
                    )
                    .with_target("char_name")
                    .with_recovery(SuggestedRecovery::SuggestAdjustValue));
                }
                let dto = CharacterWireDto {
                    name: Some(trimmed.clone()),
                    ..Default::default()
                };
                if let Err(err) = dto.validate() {
                    return Err(ActionRejection::new(
                        RejectionCode::RejectionStatOutOfRange,
                        "invalid_character_mutation",
                        err.to_string(),
                    )
                    .with_target("char_name")
                    .with_recovery(SuggestedRecovery::SuggestAdjustValue));
                }
                self.apply_character_edit(|edits| {
                    edits.name = Some(trimmed);
                    Ok(())
                })
            }
            UiIntent::SelectItemForMove { item_index } => {
                if self.item_cards.iter().any(|c| c.container == ContainerKind::Cursor) {
                    return Err(ActionRejection::new(
                        RejectionCode::RejectionInvalidPlacement,
                        "cursor_item_must_be_placed",
                        "Place the swapped item held on the cursor into a slot or click Undo.",
                    )
                    .with_target("cursor")
                    .with_recovery(SuggestedRecovery::SuggestUndo));
                }
                if let Some(idx) = item_index {
                    let valid = usize::try_from(idx)
                        .ok()
                        .is_some_and(|i| i < self.player_items_with_container.len());
                    if !valid {
                        return Err(ActionRejection::new(
                            RejectionCode::RejectionUnknown,
                            "item_not_found",
                            format!("Item index {idx} was not found in the player stream."),
                        )
                        .with_target(format!("item_{idx}")));
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
                let prev = self.undo_stack.pop().ok_or_else(|| {
                    ActionRejection::new(
                        RejectionCode::RejectionNothingToUndo,
                        "nothing_to_undo",
                        "No actions available to undo.",
                    )
                    .with_recovery(SuggestedRecovery::SuggestNone)
                })?;
                let current = self.capture_snapshot();
                self.redo_stack.push(current);
                self.restore_snapshot(prev);
                Ok(())
            }
            UiIntent::Redo => {
                let next = self.redo_stack.pop().ok_or_else(|| {
                    ActionRejection::new(
                        RejectionCode::RejectionNothingToRedo,
                        "nothing_to_redo",
                        "No actions available to redo.",
                    )
                    .with_recovery(SuggestedRecovery::SuggestNone)
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
                let item_index = self.selected_move_item_index.ok_or_else(|| {
                    ActionRejection::new(
                        RejectionCode::RejectionUnknown,
                        "no_held_item",
                        "Pick up an item before disposing it at the shop.",
                    )
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
            UiIntent::MutateItemBase {
                item_index,
                defense,
                current_durability,
                max_durability,
                quantity,
                is_ethereal,
            } => self.apply_mutate_item_base(
                item_index,
                defense,
                current_durability,
                max_durability,
                quantity,
                is_ethereal,
            ),
            UiIntent::MutateItemProperty {
                item_index,
                stat_id,
                delta,
            } => self.apply_mutate_item_property(item_index, stat_id, delta),
            UiIntent::AddItemProperty {
                item_index,
                stat_id,
                value,
                param,
            } => self.apply_add_item_property(item_index, stat_id, value, param),
            UiIntent::RemoveItemProperty {
                item_index,
                stat_id,
            } => self.apply_remove_item_property(item_index, stat_id),
            UiIntent::SetItemPropertyParam {
                item_index,
                stat_id,
                param,
            } => self.apply_set_item_property_param(item_index, stat_id, param),
            UiIntent::OpenItemForgeDraft { item_index } => {
                self.apply_open_item_forge_draft(item_index)
            }
            UiIntent::ApplyItemForgeDraft => self.apply_apply_item_forge_draft(),
            UiIntent::DiscardItemForgeDraft => self.apply_discard_item_forge_draft(),
            UiIntent::QuickMoveItem {
                item_index,
                target_container,
            } => self.apply_quick_move_item(item_index, target_container),
            UiIntent::DuplicateItem { item_index } => self.apply_duplicate_item(item_index),
            UiIntent::DeleteItem { item_index } => self.dispose_item_by_index(item_index),
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
            let meta = crate::data::skill_tree_meta::skill_tree_meta(sid);
            let (desc_en, desc_ko, prereq_ids, synergy_ids, damage) = match meta {
                Some(m) => (
                    format_skill_desc(get_loc(m.desc_key, "en"), false),
                    format_skill_desc(get_loc(m.desc_key, "ko"), true),
                    m.prereqs.iter().map(|&id| id as i32).collect(),
                    m.synergies.iter().map(|&id| id as i32).collect(),
                    compute_skill_damage(m, level, bonus_level, base_id, &self.skill_section.0),
                ),
                None => (String::new(), String::new(), Vec::new(), Vec::new(), None),
            };
            let unlock_ids: Vec<i32> = (0..SKILL_SECTION_LEN)
                .filter_map(|other_slot| {
                    let other_sid = base_id + other_slot as u32;
                    crate::data::skill_tree_meta::skill_tree_meta(other_sid)
                        .filter(|m| m.prereqs.contains(&sid))
                        .map(|_| other_sid as i32)
                })
                .collect();
            let within = (slot % 10) as i32;
            nodes.push(SkillNodeView {
                skill_id: sid as i32,
                slot_index: slot as i32,
                tree_index: meta.map(|m| i32::from(m.tree_index)).unwrap_or((slot / 10) as i32),
                row: meta.map(|m| i32::from(m.row)).unwrap_or((within / 3) + 1),
                col: meta.map(|m| i32::from(m.col)).unwrap_or((within % 3) + 1),
                name_en,
                name_ko,
                req_level,
                level,
                bonus_level,
                desc_en,
                desc_ko,
                prereq_ids,
                synergy_ids,
                unlock_ids,
                damage,
            });
        }
        nodes
    }
}

fn format_skill_desc(raw: &str, reverse_lines: bool) -> String {
    let lines: Vec<&str> = raw
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    if reverse_lines {
        lines.into_iter().rev().collect::<Vec<_>>().join(" ")
    } else {
        lines.join(" ")
    }
}

fn d2_bracket_raw(base: i32, lev: &[i32; 5], lvl: i32) -> i32 {
    if lvl <= 0 || (base == 0 && lev.iter().all(|&x| x == 0)) {
        return 0;
    }
    let mut acc = base;
    let rem = lvl - 1;
    if rem > 0 {
        acc += rem.min(7) * lev[0];
    }
    if rem > 7 {
        acc += (rem - 7).min(8) * lev[1];
    }
    if rem > 15 {
        acc += (rem - 15).min(6) * lev[2];
    }
    if rem > 21 {
        acc += (rem - 21).min(6) * lev[3];
    }
    if rem > 27 {
        acc += (rem - 27) * lev[4];
    }
    acc
}

fn scale_hit_shift(raw: i32, hit_shift: u8) -> i32 {
    ((raw as f64) * 2.0_f64.powi(i32::from(hit_shift) - 8)).round() as i32
}

fn compute_skill_damage(
    meta: &crate::data::skill_tree_meta::SkillTreeMeta,
    pure_level: i32,
    bonus_level: i32,
    base_id: u32,
    skill_bytes: &[u8; SKILL_SECTION_LEN],
) -> Option<SkillDamageView> {
    if meta.max_dam == 0 && meta.e_max == 0 {
        return None;
    }
    let preview_only = pure_level <= 0;
    let calc_level = pure_level.max(1);
    let eff_level = if pure_level > 0 {
        (pure_level + bonus_level).max(1)
    } else {
        1
    };

    let damage_at = |lvl: i32| -> (i32, i32) {
        let raw_min = d2_bracket_raw(meta.min_dam, &meta.min_lev_dam, lvl)
            + d2_bracket_raw(meta.e_min, &meta.e_min_lev, lvl);
        let raw_max = d2_bracket_raw(meta.max_dam, &meta.max_lev_dam, lvl)
            + d2_bracket_raw(meta.e_max, &meta.e_max_lev, lvl);
        (
            scale_hit_shift(raw_min, meta.hit_shift),
            scale_hit_shift(raw_max, meta.hit_shift),
        )
    };

    let (pure_min, pure_max) = damage_at(calc_level);
    let (eff_base_min, eff_base_max) = damage_at(eff_level);

    let synergy_pct: i32 = meta
        .synergy_bonuses
        .iter()
        .map(|&(syn_id, pct)| {
            if syn_id >= base_id {
                let slot = (syn_id - base_id) as usize;
                if slot < SKILL_SECTION_LEN {
                    return i32::from(skill_bytes[slot]) * pct;
                }
            }
            0
        })
        .sum();

    let synergy_bonus_min = (pure_min * synergy_pct) / 100;
    let synergy_bonus_max = (pure_max * synergy_pct) / 100;
    let gear_bonus_min = ((eff_base_min - pure_min) * (100 + synergy_pct)) / 100;
    let gear_bonus_max = ((eff_base_max - pure_max) * (100 + synergy_pct)) / 100;
    let total_min = (eff_base_min * (100 + synergy_pct)) / 100;
    let total_max = (eff_base_max * (100 + synergy_pct)) / 100;

    let elem_label = match meta.e_type {
        "fire" => "Fire",
        "cold" => "Cold",
        "ltng" => "Lightning",
        "pois" => "Poison",
        "mag" => "Magic",
        _ => "Physical",
    };
    let elem_type = if meta.max_dam > 0 && meta.e_max > 0 && !meta.e_type.is_empty() {
        format!("Phys + {elem_label}")
    } else {
        elem_label.to_string()
    };

    Some(SkillDamageView {
        elem_type,
        pure_min,
        pure_max,
        synergy_pct,
        synergy_bonus_min,
        synergy_bonus_max,
        gear_bonus_min,
        gear_bonus_max,
        total_min,
        total_max,
        preview_only,
    })
}

impl EditorSession {

    fn dispose_item_by_index(&mut self, item_index: i32) -> Result<(), ActionRejection> {
        let idx = usize::try_from(item_index)
            .ok()
            .filter(|&i| i < self.player_items_with_container.len())
            .ok_or_else(|| {
                ActionRejection::new(
                    RejectionCode::RejectionUnknown,
                    "item_not_found",
                    format!("Item index {item_index} was not found in the player stream."),
                )
                .with_target(format!("item_{item_index}"))
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

    fn validate_player_item_idx(&self, item_index: i32) -> Result<usize, ActionRejection> {
        usize::try_from(item_index)
            .ok()
            .filter(|&i| i < self.player_items_with_container.len())
            .ok_or_else(|| {
                ActionRejection::new(
                    RejectionCode::RejectionUnknown,
                    "item_not_found",
                    format!("Item index {item_index} was not found in the player stream."),
                )
                .with_target(format!("item_{item_index}"))
            })
    }

    fn rebuild_player_item_card_at(&mut self, idx: usize) {
        let (item, container) = &self.player_items_with_container[idx];
        let container_kind = ContainerKind::from_item_container(*container);
        let new_card = build_item_card(
            idx as i32,
            item,
            self.char_level,
            0,
            container_kind,
            self.item_cards.get(idx).and_then(|c| c.slot_label.clone()),
            self.item_cards.get(idx).map_or(0, |c| c.x),
            self.item_cards.get(idx).map_or(0, |c| c.y),
        );
        self.item_cards[idx] = new_card;
    }

    fn mutate_forge_item<F>(&mut self, item_index: i32, f: F) -> Result<(), ActionRejection>
    where
        F: FnOnce(&mut Item) -> Result<(), ActionRejection>,
    {
        let idx = self.validate_player_item_idx(item_index)?;
        if let Some(draft) = self
            .active_item_draft
            .as_mut()
            .filter(|d| d.item_index == item_index)
        {
            f(&mut draft.item)?;
            draft.item.bits.clear();
            draft.dirty = true;
            return Ok(());
        }
        let item = &mut self.player_items_with_container[idx].0;
        f(item)?;
        item.bits.clear();
        self.rebuild_player_item_card_at(idx);
        self.has_item_edits = true;
        self.dirty = true;
        Ok(())
    }

    fn apply_open_item_forge_draft(&mut self, item_index: i32) -> Result<(), ActionRejection> {
        let idx = self.validate_player_item_idx(item_index)?;
        let item = self.player_items_with_container[idx].0.clone();
        self.active_item_draft = Some(ItemForgeDraft {
            item_index,
            item,
            dirty: false,
        });
        Ok(())
    }

    fn apply_apply_item_forge_draft(&mut self) -> Result<(), ActionRejection> {
        let Some(draft) = self.active_item_draft.take() else {
            return Err(ActionRejection::new(
                RejectionCode::RejectionUnknown,
                "no_active_item_draft",
                "No active Item Forge draft to apply.",
            )
            .with_target("item_forge_draft"));
        };
        let idx = match self.validate_player_item_idx(draft.item_index) {
            Ok(i) => i,
            Err(err) => {
                self.active_item_draft = Some(draft);
                return Err(err);
            }
        };
        if !draft.dirty {
            return Ok(());
        }
        let mut committed = draft.item;
        committed.bits.clear();
        self.player_items_with_container[idx].0 = committed;
        self.rebuild_player_item_card_at(idx);
        self.has_item_edits = true;
        self.dirty = true;
        Ok(())
    }

    fn apply_discard_item_forge_draft(&mut self) -> Result<(), ActionRejection> {
        self.active_item_draft = None;
        Ok(())
    }

    fn apply_mutate_item_base(
        &mut self,
        item_index: i32,
        defense: Option<i32>,
        current_durability: Option<i32>,
        max_durability: Option<i32>,
        quantity: Option<i32>,
        is_ethereal: Option<bool>,
    ) -> Result<(), ActionRejection> {
        let is_alpha_dialect = self.item_dialect == "alpha";
        self.mutate_forge_item(item_index, |item| {

        if let Some(def) = defense {
            if item.defense.is_none() {
                return Err(ActionRejection::new(
                    RejectionCode::RejectionStatOutOfRange,
                    "defense_not_applicable",
                    format!("Item index {item_index} does not have a defense field."),
                )
                .with_target("item_defense"));
            }
            let (_, max_def, _) = stat_value_bounds(31).unwrap_or((0, 2037, 10));
            if def < 0 || def > max_def {
                return Err(ActionRejection::new(
                    RejectionCode::RejectionStatOutOfRange,
                    "defense_out_of_range",
                    format!("Defense must be between 0 and {max_def} (got {def})."),
                )
                .with_target("item_defense")
                .with_recovery(SuggestedRecovery::SuggestAdjustValue));
            }
            item.defense = Some(def as u32);
            item.body.defense = Some(def as u32);
        }

        if let Some(max_dur) = max_durability {
            if item.max_durability.is_none() {
                return Err(ActionRejection::new(
                    RejectionCode::RejectionStatOutOfRange,
                    "max_durability_not_applicable",
                    format!("Item index {item_index} does not have a max durability field."),
                )
                .with_target("item_max_durability"));
            }
            let max_bits = crate::domain::stats::stat_save_bits(73).unwrap_or(8).min(31);
            let max_dur_limit = ((1i64 << max_bits) - 1) as i32;
            if max_dur < 1 || max_dur > max_dur_limit {
                return Err(ActionRejection::new(
                    RejectionCode::RejectionStatOutOfRange,
                    "max_durability_out_of_range",
                    format!("Max durability must be between 1 and {max_dur_limit} (got {max_dur})."),
                )
                .with_target("item_max_durability")
                .with_recovery(SuggestedRecovery::SuggestAdjustValue));
            }
            item.max_durability = Some(max_dur as u32);
            item.body.max_durability = Some(max_dur as u32);
            if current_durability.is_none() {
                if let Some(cur) = item.current_durability {
                    if cur > max_dur as u32 {
                        item.current_durability = Some(max_dur as u32);
                        item.body.current_durability = Some(max_dur as u32);
                    }
                }
            }
        }

        if let Some(cur_dur) = current_durability {
            if item.current_durability.is_none() {
                return Err(ActionRejection::new(
                    RejectionCode::RejectionStatOutOfRange,
                    "durability_not_applicable",
                    format!("Item index {item_index} does not have a durability field."),
                )
                .with_target("item_durability"));
            }
            let cur_bits = crate::domain::stats::stat_save_bits(72).unwrap_or(8).min(31);
            let cur_bit_limit = ((1i64 << cur_bits) - 1) as i32;
            let max_bound = (item.max_durability.unwrap_or(cur_bit_limit as u32) as i64)
                .min(i64::from(cur_bit_limit)) as i32;
            if cur_dur < 0 || cur_dur > max_bound {
                return Err(ActionRejection::new(
                    RejectionCode::RejectionStatOutOfRange,
                    "durability_out_of_range",
                    format!("Durability must be between 0 and {max_bound} (got {cur_dur})."),
                )
                .with_target("item_durability")
                .with_recovery(SuggestedRecovery::SuggestAdjustValue));
            }
            item.current_durability = Some(cur_dur as u32);
            item.body.current_durability = Some(cur_dur as u32);
        }

        if let Some(qty) = quantity {
            if item.quantity.is_none() {
                return Err(ActionRejection::new(
                    RejectionCode::RejectionStatOutOfRange,
                    "quantity_not_applicable",
                    format!("Item index {item_index} does not have a quantity field."),
                )
                .with_target("item_quantity"));
            }
            let qty_bits = crate::data::generated::layout_policy::D2rV3ExtendedPayloadWidths::QUANTITY.min(31);
            let max_qty = ((1i64 << qty_bits) - 1) as i32;
            if qty < 1 || qty > max_qty {
                return Err(ActionRejection::new(
                    RejectionCode::RejectionStatOutOfRange,
                    "quantity_out_of_range",
                    format!("Quantity must be between 1 and {max_qty} (got {qty})."),
                )
                .with_target("item_quantity")
                .with_recovery(SuggestedRecovery::SuggestAdjustValue));
            }
            item.quantity = Some(qty as u32);
            item.body.quantity = Some(qty as u32);
        }

        if let Some(eth) = is_ethereal {
            let eth_mask: u32 = if is_alpha_dialect || item.header.save_is_alpha {
                1 << 24
            } else {
                1 << 22
            };
            item.is_ethereal = eth;
            item.header.is_ethereal = eth;
            if eth {
                item.header.flags |= eth_mask;
            } else {
                item.header.flags &= !eth_mask;
            }
        }

        Ok(())
        })
    }

    fn apply_mutate_item_property(
        &mut self,
        item_index: i32,
        stat_id: i32,
        delta: i32,
    ) -> Result<(), ActionRejection> {
        self.mutate_forge_item(item_index, |item| {

        let stat_u32 = stat_id as u32;
        let exists_on_item = item.properties.iter().any(|p| p.stat_id == stat_u32)
            || item.stats.properties.iter().any(|p| p.stat_id == stat_u32);
        if !exists_on_item {
            return Err(ActionRejection::new(
                RejectionCode::RejectionStatOutOfRange,
                "stat_not_found_on_item",
                format!("Stat ID {stat_id} does not exist on item index {item_index}."),
            )
            .with_target(format!("stat_{stat_id}")));
        }

        let (min_val, max_val, save_add) = stat_value_bounds(stat_u32).ok_or_else(|| {
            ActionRejection::new(
                RejectionCode::RejectionStatOutOfRange,
                "stat_not_writable",
                format!("Stat ID {stat_id} is not writable or has no bit-width definition."),
            )
            .with_target(format!("stat_{stat_id}"))
        })?;

        let mut matched = false;
        let mut out_of_range_val: Option<i32> = None;

        for prop in &mut item.properties {
            if prop.stat_id == stat_u32 {
                let new_val = prop.value.checked_add(delta).ok_or_else(|| {
                    ActionRejection::new(
                        RejectionCode::RejectionStatOutOfRange,
                        "stat_value_out_of_range",
                        format!(
                            "Stat ID {stat_id} value overflowed when adding delta {delta} (allowed {min_val}..={max_val})."
                        ),
                    )
                    .with_target(format!("stat_{stat_id}"))
                    .with_recovery(SuggestedRecovery::SuggestAdjustValue)
                })?;
                if new_val < min_val || new_val > max_val {
                    out_of_range_val = Some(new_val);
                    break;
                }
                prop.value = new_val;
                prop.raw_value = new_val + save_add;
                matched = true;
                break;
            }
        }

        if let Some(bad_val) = out_of_range_val {
            return Err(ActionRejection::new(
                RejectionCode::RejectionStatOutOfRange,
                "stat_value_out_of_range",
                format!(
                    "Stat ID {stat_id} value {bad_val} is outside allowed bounds {min_val}..={max_val}."
                ),
            )
            .with_target(format!("stat_{stat_id}"))
            .with_recovery(SuggestedRecovery::SuggestAdjustValue));
        }

        // Also synchronize item.stats.properties so that both alias vectors remain identical
        for prop in &mut item.stats.properties {
            if prop.stat_id == stat_u32 {
                let new_val = prop.value.checked_add(delta).ok_or_else(|| {
                    ActionRejection::new(
                        RejectionCode::RejectionStatOutOfRange,
                        "stat_value_out_of_range",
                        format!(
                            "Stat ID {stat_id} value overflowed when adding delta {delta} (allowed {min_val}..={max_val})."
                        ),
                    )
                    .with_target(format!("stat_{stat_id}"))
                    .with_recovery(SuggestedRecovery::SuggestAdjustValue)
                })?;
                if new_val < min_val || new_val > max_val {
                    return Err(ActionRejection::new(
                        RejectionCode::RejectionStatOutOfRange,
                        "stat_value_out_of_range",
                        format!(
                            "Stat ID {stat_id} value {new_val} is outside allowed bounds {min_val}..={max_val}."
                        ),
                    )
                    .with_target(format!("stat_{stat_id}"))
                    .with_recovery(SuggestedRecovery::SuggestAdjustValue));
                }
                prop.value = new_val;
                prop.raw_value = new_val + save_add;
                matched = true;
                break;
            }
        }

        if !matched {
            return Err(ActionRejection::new(
                RejectionCode::RejectionStatOutOfRange,
                "stat_not_found_on_item",
                format!("Stat ID {stat_id} does not exist on item index {item_index}."),
            )
            .with_target(format!("stat_{stat_id}")));
        }

        Ok(())
        })
    }

    fn apply_add_item_property(
        &mut self,
        item_index: i32,
        stat_id: i32,
        value: i32,
        param: i32,
    ) -> Result<(), ActionRejection> {
        let stat_u32 = u32::try_from(stat_id).map_err(|_| {
            ActionRejection::new(
                RejectionCode::RejectionStatOutOfRange,
                "stat_not_writable",
                format!("Stat ID {stat_id} is invalid."),
            )
            .with_target(format!("stat_{stat_id}"))
        })?;

        let stat_def = crate::data::stat_costs::STAT_COSTS
            .iter()
            .find(|s| s.id == stat_u32)
            .ok_or_else(|| {
                ActionRejection::new(
                    RejectionCode::RejectionStatOutOfRange,
                    "stat_not_writable",
                    format!("Stat ID {stat_id} is not defined in STAT_COSTS."),
                )
                .with_target(format!("stat_{stat_id}"))
            })?;

        let param_u32 = validate_stat_param(stat_def.name, stat_def.save_param_bits, stat_id, param)?;

        let (min_val, max_val, save_add) = stat_value_bounds(stat_u32).ok_or_else(|| {
            ActionRejection::new(
                RejectionCode::RejectionStatOutOfRange,
                "stat_not_writable",
                format!("Stat ID {stat_id} is not writable."),
            )
            .with_target(format!("stat_{stat_id}"))
        })?;

        if value < min_val || value > max_val {
            return Err(ActionRejection::new(
                RejectionCode::RejectionStatOutOfRange,
                "stat_value_out_of_range",
                format!(
                    "Stat ID {stat_id} value {value} is outside allowed bounds {min_val}..={max_val}."
                ),
            )
            .with_target(format!("stat_{stat_id}"))
            .with_recovery(SuggestedRecovery::SuggestAdjustValue));
        }

        self.mutate_forge_item(item_index, |item| {
            if item.is_compact {
                return Err(ActionRejection::new(
                    RejectionCode::RejectionInvalidPlacement,
                    "compact_item_immutable",
                    "Compact items do not carry a property list.",
                )
                .with_target(format!("item_{item_index}")));
            }

            let already_exists = item.properties.iter().any(|p| p.stat_id == stat_u32)
                || item.stats.properties.iter().any(|p| p.stat_id == stat_u32);
            if already_exists {
                return Err(ActionRejection::new(
                    RejectionCode::RejectionStatOutOfRange,
                    "stat_already_exists",
                    format!("Stat ID {stat_id} already exists on item index {item_index}."),
                )
                .with_target(format!("stat_{stat_id}")));
            }

            let new_prop = crate::domain::stats::ItemProperty::new(
                stat_u32,
                stat_def.name.to_string(),
                param_u32,
                value + save_add,
                value,
                crate::domain::item::entity::ItemBitRange::default(),
            );
            insert_property_before_terminator(&mut item.properties, new_prop.clone());
            insert_property_before_terminator(&mut item.stats.properties, new_prop);
            Ok(())
        })
    }

    fn apply_set_item_property_param(
        &mut self,
        item_index: i32,
        stat_id: i32,
        param: i32,
    ) -> Result<(), ActionRejection> {
        let stat_u32 = u32::try_from(stat_id).map_err(|_| {
            ActionRejection::new(
                RejectionCode::RejectionStatOutOfRange,
                "stat_not_writable",
                format!("Stat ID {stat_id} is invalid."),
            )
            .with_target(format!("stat_{stat_id}"))
        })?;

        let stat_def = crate::data::stat_costs::STAT_COSTS
            .iter()
            .find(|s| s.id == stat_u32)
            .ok_or_else(|| {
                ActionRejection::new(
                    RejectionCode::RejectionStatOutOfRange,
                    "stat_not_writable",
                    format!("Stat ID {stat_id} is not defined in STAT_COSTS."),
                )
                .with_target(format!("stat_{stat_id}"))
            })?;

        if stat_def.save_param_bits == 0 {
            return Err(ActionRejection::new(
                RejectionCode::RejectionStatOutOfRange,
                "stat_param_not_applicable",
                format!("Stat '{}' (#{stat_id}) does not take a parameter.", stat_def.name),
            )
            .with_target(format!("stat_{stat_id}")));
        }

        let param_u32 = validate_stat_param(stat_def.name, stat_def.save_param_bits, stat_id, param)?;
        self.mutate_forge_item(item_index, |item| {
            let mut matched = false;
            for prop in &mut item.properties {
                if prop.stat_id == stat_u32 {
                    prop.param = param_u32;
                    matched = true;
                }
            }
            for prop in &mut item.stats.properties {
                if prop.stat_id == stat_u32 {
                    prop.param = param_u32;
                    matched = true;
                }
            }
            if !matched {
                return Err(ActionRejection::new(
                    RejectionCode::RejectionStatOutOfRange,
                    "stat_not_found_on_item",
                    format!("Stat ID {stat_id} does not exist on item index {item_index}."),
                )
                .with_target(format!("stat_{stat_id}")));
            }
            Ok(())
        })
    }

    fn apply_remove_item_property(
        &mut self,
        item_index: i32,
        stat_id: i32,
    ) -> Result<(), ActionRejection> {
        let stat_u32 = stat_id as u32;
        if stat_u32 == 511 {
            return Err(ActionRejection::new(
                RejectionCode::RejectionStatOutOfRange,
                "terminator_immutable",
                "Cannot remove property terminator (stat 511).",
            )
            .with_target("stat_511"));
        }
        self.mutate_forge_item(item_index, |item| {
            let before_props = item.properties.len();
            let before_stats = item.stats.properties.len();
            item.properties.retain(|p| p.stat_id != stat_u32);
            item.stats.properties.retain(|p| p.stat_id != stat_u32);

            if item.properties.len() == before_props && item.stats.properties.len() == before_stats {
                return Err(ActionRejection::new(
                    RejectionCode::RejectionStatOutOfRange,
                    "stat_not_found_on_item",
                    format!("Stat ID {stat_id} does not exist on item index {item_index}."),
                )
                .with_target(format!("stat_{stat_id}")));
            }
            Ok(())
        })
    }

    fn apply_adjust_skill_rank(&mut self, skill_id: i32, delta: i32) -> Result<(), ActionRejection> {
        if delta == 0 {
            return Ok(());
        }
        let base_id = class_skill_base_id(self.char_class_id).ok_or_else(|| {
            ActionRejection::new(
                RejectionCode::RejectionStatOutOfRange,
                "invalid_skill_mutation",
                "Character class does not have a valid skill table.",
            )
        })?;
        let sid = u32::try_from(skill_id).map_err(|_| {
            ActionRejection::new(
                RejectionCode::RejectionStatOutOfRange,
                "invalid_skill_mutation",
                format!("Skill ID {skill_id} is invalid."),
            )
        })?;
        if sid < base_id || (sid - base_id) as usize >= SKILL_SECTION_LEN {
            return Err(ActionRejection::new(
                RejectionCode::RejectionStatOutOfRange,
                "invalid_skill_mutation",
                format!("Skill ID {skill_id} does not belong to {}.", self.char_class),
            ));
        }
        let slot = (sid - base_id) as usize;
        let current = i32::from(self.skill_section.0[slot]);
        let max_rank = 20;
        let next = current + delta;
        if next < 0 || next > max_rank {
            return Err(ActionRejection::new(
                RejectionCode::RejectionStatOutOfRange,
                "skill_rank_out_of_range",
                format!("Skill rank must stay within 0..={max_rank} in {:?} mode.", self.skill_mode),
            )
            .with_target(format!("skill_{sid}"))
            .with_recovery(SuggestedRecovery::SuggestAdjustValue));
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
                return Err(ActionRejection::new(
                    RejectionCode::RejectionSkillPrerequisiteNotMet,
                    "skill_level_requirement",
                    format!("Requires character level {req_level} (current level {}).", self.char_level),
                )
                .with_target(format!("skill_{sid}"))
                .with_recovery(SuggestedRecovery::SuggestAdjustValue));
            }
            if delta > 0 && unspent < delta {
                return Err(ActionRejection::new(
                    RejectionCode::RejectionSkillPrerequisiteNotMet,
                    "insufficient_skill_points",
                    "No unspent skill points remaining in Legit mode.",
                )
                .with_target(format!("skill_{sid}"))
                .with_recovery(SuggestedRecovery::SuggestAdjustValue));
            }
            if delta > 0 {
                if let Some(meta) = crate::data::skill_tree_meta::skill_tree_meta(sid) {
                    for &prereq_id in meta.prereqs {
                        if prereq_id >= base_id {
                            let prereq_slot = (prereq_id - base_id) as usize;
                            if prereq_slot < SKILL_SECTION_LEN && self.skill_section.0[prereq_slot] < 1 {
                                return Err(ActionRejection::new(
                                    RejectionCode::RejectionSkillPrerequisiteNotMet,
                                    "skill_prereq_not_met",
                                    format!("Requires prerequisite skill ID {prereq_id} to have at least 1 point."),
                                )
                                .with_target(format!("skill_{sid}"))
                                .with_recovery(SuggestedRecovery::SuggestAdjustValue));
                            }
                        }
                    }
                }
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

    fn apply_reset_stats(&mut self) -> Result<(), ActionRejection> {
        let alpha = self.save_version == 105;
        let rule = crate::data::char_base_stats::class_base_rule(self.char_class_id);
        let cur_str = self.attributes.actual_value(0, alpha).unwrap_or(rule.base_str);
        let cur_ene = self.attributes.actual_value(1, alpha).unwrap_or(rule.base_ene);
        let cur_dex = self.attributes.actual_value(2, alpha).unwrap_or(rule.base_dex);
        let cur_vit = self.attributes.actual_value(3, alpha).unwrap_or(rule.base_vit);
        let refund = (cur_str - rule.base_str).max(0)
            + (cur_ene - rule.base_ene).max(0)
            + (cur_dex - rule.base_dex).max(0)
            + (cur_vit - rule.base_vit).max(0);
        let unspent_now = self.attributes.actual_value(4, alpha).unwrap_or(0).max(0);
        let next_unspent = (unspent_now + refund).clamp(0, 1023) as u32;

        self.apply_character_edit(|edits| {
            edits.strength = Some(NumericOp::Set(
                StatPointValue::new(rule.base_str.max(1) as u32, 1023).map_err(|e| e.to_string())?,
            ));
            edits.energy = Some(NumericOp::Set(
                StatPointValue::new(rule.base_ene.max(1) as u32, 1023).map_err(|e| e.to_string())?,
            ));
            edits.dexterity = Some(NumericOp::Set(
                StatPointValue::new(rule.base_dex.max(1) as u32, 1023).map_err(|e| e.to_string())?,
            ));
            edits.vitality = Some(NumericOp::Set(
                StatPointValue::new(rule.base_vit.max(1) as u32, 1023).map_err(|e| e.to_string())?,
            ));
            edits.stat_points = Some(NumericOp::Set(
                StatPointValue::new(next_unspent, 1023).map_err(|e| e.to_string())?,
            ));
            Ok(())
        })
    }

    fn apply_sync_stat_points_to_rules(&mut self) -> Result<(), ActionRejection> {
        let alpha = self.save_version == 105;
        let rule = crate::data::char_base_stats::class_base_rule(self.char_class_id);
        let lam_esen_bonus =
            crate::engine::character_sheet::count_lam_esen_bonuses(Some(&self.progression.quests));
        let lvl = i32::from(self.char_level.clamp(1, 99));
        let rule_cap = (lvl - 1) * crate::data::char_base_stats::STAT_POINTS_PER_LEVEL + lam_esen_bonus;
        let cur_str = self.attributes.actual_value(0, alpha).unwrap_or(rule.base_str);
        let cur_ene = self.attributes.actual_value(1, alpha).unwrap_or(rule.base_ene);
        let cur_dex = self.attributes.actual_value(2, alpha).unwrap_or(rule.base_dex);
        let cur_vit = self.attributes.actual_value(3, alpha).unwrap_or(rule.base_vit);
        let invested = (cur_str - rule.base_str).max(0)
            + (cur_ene - rule.base_ene).max(0)
            + (cur_dex - rule.base_dex).max(0)
            + (cur_vit - rule.base_vit).max(0);
        let next_unspent = (rule_cap - invested).clamp(0, 1023) as u32;
        self.apply_character_edit(|edits| {
            edits.stat_points = Some(NumericOp::Set(
                StatPointValue::new(next_unspent, 1023).map_err(|e| e.to_string())?,
            ));
            Ok(())
        })
    }

    fn apply_reset_skills(&mut self) -> Result<(), ActionRejection> {
        let Some(base_id) = class_skill_base_id(self.char_class_id) else {
            return Err(ActionRejection::new(
                RejectionCode::RejectionStatOutOfRange,
                "invalid_skill_mutation",
                "Character class does not have a valid skill table.",
            ));
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
            .ok_or_else(|| {
                ActionRejection::new(
                    RejectionCode::RejectionUnknown,
                    "item_not_found",
                    format!("Item index {item_index} was not found in the player stream."),
                )
                .with_target(format!("item_{item_index}"))
            })?;

        let (item_code, width, height) = {
            let card = &self.item_cards[idx];
            (card.code.clone(), card.width.max(1), card.height.max(1))
        };

        if target_container == ContainerKind::Equipment {
            let slot_name = equipment_slot_label(x)
                .filter(|_| y == 0)
                .ok_or_else(|| {
                    ActionRejection::new(
                        RejectionCode::RejectionInvalidPlacement,
                        "out_of_bounds",
                        format!("Equipment slot ({x}, {y}) is not a valid body slot (1..=12)."),
                    )
                    .with_target(format!("equip_{x}_{y}"))
                    .with_recovery(SuggestedRecovery::SuggestUndo)
                })?;
            if !item_fits_equipment_slot(&item_code, x) {
                return Err(ActionRejection::new(
                    RejectionCode::RejectionInvalidPlacement,
                    "invalid_equipment_slot",
                    format!(
                        "Item '{}' cannot be equipped in the {} slot.",
                        item_code.trim(),
                        slot_name
                    ),
                )
                .with_target(slot_name)
                .with_recovery(SuggestedRecovery::SuggestUndo));
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
                _ => Err(ActionRejection::new(
                    RejectionCode::RejectionSlotOccupied,
                    "container_slot_occupied",
                    format!("Equipment slot {slot_name} has multiple overlapping items."),
                )
                .with_target(slot_name)
                .with_recovery(SuggestedRecovery::SuggestUndo)),
            };
        }

        if matches!(
            target_container,
            ContainerKind::MercEquipment | ContainerKind::MercReplaced
        ) {
            let is_replaced = target_container == ContainerKind::MercReplaced;
            let has_replaced_items = self
                .item_cards
                .iter()
                .any(|c| c.container == ContainerKind::MercReplaced);
            if is_replaced && self.dismissed_merc.is_none() && !has_replaced_items {
                return Err(ActionRejection::new(
                    RejectionCode::RejectionInvalidPlacement,
                    "no_replaced_merc",
                    "Replaced mercenary staging is not active.",
                )
                .with_target("merc_replaced")
                .with_recovery(SuggestedRecovery::SuggestUndo));
            }
            if is_replaced && self.item_cards[idx].container != ContainerKind::MercReplaced {
                return Err(ActionRejection::new(
                    RejectionCode::RejectionInvalidPlacement,
                    "invalid_target_container",
                    "Only items staged in Replaced Staging can be placed back into MercReplaced.",
                )
                .with_target("merc_replaced")
                .with_recovery(SuggestedRecovery::SuggestUndo));
            }
            if !is_replaced && !self.merc_hired {
                return Err(ActionRejection::new(
                    RejectionCode::RejectionInvalidPlacement,
                    "merc_not_hired",
                    "Hire or select a mercenary Act before equipping mercenary items.",
                )
                .with_target("merc_equipment")
                .with_recovery(SuggestedRecovery::SuggestUndo));
            }
            let Some(slot_name) = merc::merc_slot_label(x) else {
                return Err(ActionRejection::new(
                    RejectionCode::RejectionInvalidPlacement,
                    "invalid_merc_slot",
                    format!("Unknown mercenary slot body_loc={x}."),
                )
                .with_target(target_container.as_label())
                .with_recovery(SuggestedRecovery::SuggestUndo));
            };
            let has_merc_items = self
                .item_cards
                .iter()
                .any(|c| c.container == ContainerKind::MercEquipment);
            let current_act = self
                .merc_act_override
                .unwrap_or_else(|| merc::inferred_merc_act(&self.merc_state, has_merc_items));
            let target_act = if is_replaced {
                self.dismissed_merc
                    .as_ref()
                    .map(|(_, _, act)| *act)
                    .unwrap_or(current_act)
            } else {
                current_act
            };
            if let Err(err_code) = merc::item_fits_merc_slot(&item_code, x, target_act) {
                return Err(ActionRejection::new(
                    RejectionCode::RejectionInvalidPlacement,
                    err_code,
                    format!(
                        "Item '{}' cannot be equipped in mercenary {} slot (Act {}).",
                        item_code.trim(),
                        slot_name,
                        target_act
                    ),
                )
                .with_target(slot_name)
                .with_recovery(SuggestedRecovery::SuggestUndo));
            }
            let occupied: Vec<usize> = self
                .item_cards
                .iter()
                .enumerate()
                .filter(|&(other_idx, other)| {
                    other_idx != idx && other.container == target_container && other.x == x
                })
                .map(|(other_idx, _)| other_idx)
                .collect();
            let storage_container = if is_replaced {
                ItemContainer::Unknown
            } else {
                ItemContainer::Equipment
            };
            return match occupied.as_slice() {
                [] => Ok((idx, storage_container, None)),
                [disp] => Ok((idx, storage_container, Some(*disp))),
                _ => Err(ActionRejection::new(
                    RejectionCode::RejectionSlotOccupied,
                    "container_slot_occupied",
                    format!("Mercenary slot {slot_name} has multiple overlapping items."),
                )
                .with_target(slot_name)
                .with_recovery(SuggestedRecovery::SuggestUndo)),
            };
        }

        let (target, max_w, max_h, target_kind) = match target_container {
            ContainerKind::Inventory => (ItemContainer::Inventory, 10i32, 4i32, ContainerKind::Inventory),
            ContainerKind::Stash => (ItemContainer::Stash, 10i32, 10i32, ContainerKind::Stash),
            ContainerKind::Cube => (ItemContainer::Cube, 3i32, 4i32, ContainerKind::Cube),
            ContainerKind::Belt => (ItemContainer::Belt, 4i32, 4i32, ContainerKind::Belt),
            ContainerKind::TempInventory => (ItemContainer::Unknown, 9i32, 4i32, ContainerKind::TempInventory),
            other => {
                return Err(ActionRejection::new(
                    RejectionCode::RejectionInvalidPlacement,
                    "invalid_target_container",
                    format!(
                        "Items cannot be moved to '{}'. Allowed targets: inventory, stash, cube, belt, temp_inventory, equipment, shop.",
                        other.as_label()
                    ),
                )
                .with_target(other.as_label())
                .with_recovery(SuggestedRecovery::SuggestUndo));
            }
        };

        if target == ItemContainer::Belt
            && (!is_belt_consumable(&item_code) || width != 1 || height != 1)
        {
            return Err(ActionRejection::new(
                RejectionCode::RejectionInvalidPlacement,
                "belt_consumable_only",
                format!(
                    "Only 1x1 belt consumables (potions/scrolls) can be placed in the belt ('{}' is not allowed).",
                    item_code.trim()
                ),
            )
            .with_target("belt")
            .with_recovery(SuggestedRecovery::SuggestUndo));
        }

        if x < 0 || y < 0 || x + width > max_w || y + height > max_h {
            return Err(ActionRejection::new(
                RejectionCode::RejectionInvalidPlacement,
                "out_of_bounds",
                format!(
                    "Placement ({x}, {y}) with size {width}x{height} exceeds {} bounds ({max_w}x{max_h}).",
                    target_container.as_label()
                ),
            )
            .with_target(target_container.as_label())
            .with_recovery(SuggestedRecovery::SuggestUndo));
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
            _ => Err(ActionRejection::new(
                RejectionCode::RejectionSlotOccupied,
                "container_slot_occupied",
                format!(
                    "Target slot ({x}, {y}) in {} overlaps with {} items and cannot swap.",
                    target_container.as_label(),
                    overlaps.len()
                ),
            )
            .with_target(target_container.as_label())
            .with_recovery(SuggestedRecovery::SuggestUndo)),
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
        for merc_container in [ContainerKind::MercEquipment, ContainerKind::MercReplaced] {
            for merc_loc in [1, 3, 4, 5] {
                if self
                    .validate_move_target(item_index, merc_container, merc_loc, 0)
                    .is_ok()
                {
                    targets.push(AllowedMoveTargetView {
                        container: merc_container,
                        x: merc_loc,
                        y: 0,
                    });
                }
            }
        }
        let belt_restricted_reason =
            match self.validate_move_target(item_index, ContainerKind::Belt, 0, 0) {
                Err(rej) if rej.raw_code == "belt_consumable_only" => Some(rej.message),
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
        let target_kind = if matches!(
            target_container,
            ContainerKind::TempInventory
                | ContainerKind::MercEquipment
                | ContainerKind::MercReplaced
        ) {
            target_container
        } else {
            ContainerKind::from_item_container(target)
        };

        let (raw_x, raw_y, page, location, mode, stored_in, new_slot_label, card_x, card_y) =
            if matches!(
                target_container,
                ContainerKind::MercEquipment | ContainerKind::MercReplaced
            ) {
                (
                    x as u8,
                    0u8,
                    0u8,
                    1u8,
                    1u8,
                    None,
                    merc::merc_slot_label(x).map(str::to_string),
                    x,
                    0,
                )
            } else {
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
                }
            };

        let (item, container_slot) = &mut self.player_items_with_container[idx];
        set_item_placement(item, raw_x, raw_y, page, location, mode, stored_in);
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

    fn container_grid_dims(container_kind: ContainerKind) -> Option<(i32, i32)> {
        match container_kind {
            ContainerKind::Inventory => Some((10, 4)),
            ContainerKind::Stash => Some((10, 10)),
            ContainerKind::Cube => Some((3, 4)),
            ContainerKind::TempInventory => Some((9, 4)),
            ContainerKind::Belt => Some((4, 4)),
            _ => None,
        }
    }

    fn find_compatible_slot(
        &self,
        item_index: i32,
        container: ContainerKind,
        slots: &[i32],
        fallback_code: &str,
        fallback_message: &str,
    ) -> Result<(i32, i32), ActionRejection> {
        let mut first_empty = None;
        let mut first_swap = None;
        let mut last_err = None;
        for &slot_loc in slots {
            match self.validate_move_target(item_index, container, slot_loc, 0) {
                Ok((_, _, None)) => {
                    first_empty = Some((slot_loc, 0));
                    break;
                }
                Ok((_, _, Some(_))) if first_swap.is_none() => {
                    first_swap = Some((slot_loc, 0));
                }
                Err(e) => {
                    if last_err.is_none() {
                        last_err = Some(e);
                    }
                }
                _ => {}
            }
        }
        first_empty.or(first_swap).ok_or_else(|| {
            last_err.unwrap_or_else(|| {
                ActionRejection::new(
                    RejectionCode::RejectionInvalidPlacement,
                    fallback_code,
                    fallback_message,
                )
            })
        })
    }

    fn apply_quick_move_item(
        &mut self,
        item_index: i32,
        target_container: ContainerKind,
    ) -> Result<(), ActionRejection> {
        let idx = usize::try_from(item_index)
            .ok()
            .filter(|&i| i < self.player_items_with_container.len())
            .ok_or_else(|| {
                ActionRejection::new(
                    RejectionCode::RejectionUnknown,
                    "item_not_found",
                    format!("Item index {item_index} was not found in the player stream."),
                )
                .with_target(format!("item_{item_index}"))
            })?;
        let (width, height) = {
            let card = &self.item_cards[idx];
            (card.width.max(1), card.height.max(1))
        };

        let (x, y) = match target_container {
            ContainerKind::Equipment => {
                const EQUIP_SLOTS: [i32; 12] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
                self.find_compatible_slot(
                    item_index,
                    ContainerKind::Equipment,
                    &EQUIP_SLOTS,
                    "invalid_equipment_slot",
                    "No compatible equipment slot found for this item.",
                )?
            }
            ContainerKind::MercEquipment => {
                const MERC_SLOTS: [i32; 4] = [1, 3, 4, 5];
                self.find_compatible_slot(
                    item_index,
                    ContainerKind::MercEquipment,
                    &MERC_SLOTS,
                    "merc_weapon_incompatible_with_act",
                    "No compatible mercenary slot found for this item.",
                )?
            }
            ContainerKind::Inventory
            | ContainerKind::Stash
            | ContainerKind::Cube
            | ContainerKind::TempInventory
            | ContainerKind::Belt => {
                let (max_w, max_h) = Self::container_grid_dims(target_container).unwrap_or((10, 4));
                let (req_w, req_h) = if target_container == ContainerKind::Belt {
                    self.validate_move_target(item_index, ContainerKind::Belt, 0, 0)?;
                    (1, 1)
                } else {
                    (width, height)
                };
                self.find_free_grid_slot(target_container, max_w, max_h, req_w, req_h)
                    .ok_or_else(|| {
                        ActionRejection::new(
                            RejectionCode::RejectionInventoryFull,
                            "no_free_slot",
                            format!(
                                "{} ({}x{}) has no free space for this item.",
                                target_container.as_label(),
                                max_w,
                                max_h
                            ),
                        )
                        .with_target(target_container.as_label())
                        .with_recovery(SuggestedRecovery::SuggestClearSlot)
                    })?
            }
            other => {
                return Err(ActionRejection::new(
                    RejectionCode::RejectionInvalidPlacement,
                    "invalid_target_container",
                    format!("Quick move to '{}' is not supported.", other.as_label()),
                )
                .with_target(other.as_label()));
            }
        };

        self.apply_move_item(item_index, target_container, x, y)
    }

    fn apply_duplicate_item(&mut self, item_index: i32) -> Result<(), ActionRejection> {
        let idx = usize::try_from(item_index)
            .ok()
            .filter(|&i| i < self.player_items_with_container.len())
            .ok_or_else(|| {
                ActionRejection::new(
                    RejectionCode::RejectionUnknown,
                    "item_not_found",
                    format!("Item index {item_index} was not found in the player stream."),
                )
                .with_target(format!("item_{item_index}"))
            })?;
        let (width, height) = {
            let card = &self.item_cards[idx];
            (card.width.max(1), card.height.max(1))
        };

        let candidates = [
            (ContainerKind::Inventory, ItemContainer::Inventory, 0u8, Some(1u8)),
            (ContainerKind::Stash, ItemContainer::Stash, 4u8, Some(5u8)),
            (ContainerKind::TempInventory, ItemContainer::Unknown, 0u8, None),
        ];
        let placement = candidates.into_iter().find_map(|(kind, enum_val, page, stored_in)| {
            let (mw, mh) = Self::container_grid_dims(kind)?;
            self.find_free_grid_slot(kind, mw, mh, width, height)
                .map(|(x, y)| (kind, enum_val, page, stored_in, x, y))
        });

        let Some((dest_kind, dest_enum, page, stored_in, x, y)) = placement else {
            return Err(ActionRejection::new(
                RejectionCode::RejectionInventoryFull,
                "no_free_slot",
                "No free space in Inventory, Stash, or Temp Inventory to duplicate this item.",
            )
            .with_target("inventory")
            .with_recovery(SuggestedRecovery::SuggestClearSlot));
        };

        let (mut cloned_item, _) = self.player_items_with_container[idx].clone();
        set_item_placement(&mut cloned_item, x as u8, y as u8, page, 0, 0, stored_in);

        let mut cloned_card = self.item_cards[idx].clone();
        cloned_card.index = self.item_cards.len() as i32;
        cloned_card.container = dest_kind;
        cloned_card.slot_label = None;
        cloned_card.x = x;
        cloned_card.y = y;

        self.player_items_with_container.push((cloned_item, dest_enum));
        self.item_cards.push(cloned_card);
        self.has_item_edits = true;
        self.dirty = true;
        self.selected_move_item_index = None;
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
        let mut item = build_shop_item_by_id(catalog_id, alpha).ok_or_else(|| {
            ActionRejection::new(
                RejectionCode::RejectionShopSpawnFailed,
                "shop_item_not_found",
                format!("Shop catalog item '{catalog_id}' was not found."),
            )
            .with_target(catalog_id)
        })?;
        let (w, h) = get_item_size(item.code.trim());
        let width = i32::from(w.max(1));
        let height = i32::from(h.max(1));

        let (dest_kind, dest_enum, raw_page) = match target_container {
            ShopSpawnTarget::SpawnInventory => {
                let (x, y) = self
                    .find_free_grid_slot(ContainerKind::Inventory, 10, 4, width, height)
                    .ok_or_else(|| {
                        ActionRejection::new(
                            RejectionCode::RejectionInventoryFull,
                            "no_free_slot",
                            "Inventory (10x4) has no free space for this item.",
                        )
                        .with_target("inventory")
                        .with_recovery(SuggestedRecovery::SuggestClearSlot)
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
                    .ok_or_else(|| {
                        ActionRejection::new(
                            RejectionCode::RejectionInventoryFull,
                            "no_free_slot",
                            "Temp Inventory (9x4) has no free space for this item.",
                        )
                        .with_target("temp_inventory")
                        .with_recovery(SuggestedRecovery::SuggestClearSlot)
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
                    return Err(ActionRejection::new(
                        RejectionCode::RejectionInventoryFull,
                        "no_free_slot",
                        "Both Inventory (10x4) and Temp Inventory (9x4) are full.",
                    )
                    .with_target("all_inventory")
                    .with_recovery(SuggestedRecovery::SuggestClearSlot));
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
                    item.to_bits_d2r_v3(&huffman).map_err(|e| {
                        ActionRejection::new(
                            RejectionCode::RejectionShopSpawnFailed,
                            "shop_serialize_failed",
                            format!(
                                "Failed to serialize spawned d2r-v3 item '{catalog_id}': {e}"
                            ),
                        )
                        .with_target(catalog_id)
                    })?;
                    item.clone()
                }
            }
        } else {
            let bytes = item.to_bytes(0, &huffman, alpha).map_err(|e| {
                ActionRejection::new(
                    RejectionCode::RejectionShopSpawnFailed,
                    "shop_serialize_failed",
                    format!("Failed to serialize spawned item '{catalog_id}': {e}"),
                )
                .with_target(catalog_id)
            })?;
            Item::from_bytes_with_hint(&bytes, &huffman, alpha, Some(item.code.trim())).map_err(
                |e| {
                    ActionRejection::new(
                        RejectionCode::RejectionShopSpawnFailed,
                        "shop_roundtrip_failed",
                        format!("Failed to roundtrip spawned item '{catalog_id}': {e}"),
                    )
                    .with_target(catalog_id)
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
        patch(&mut next_edits).map_err(|message| {
            ActionRejection::new(
                RejectionCode::RejectionStatOutOfRange,
                "invalid_character_mutation",
                message,
            )
            .with_target("character_field")
            .with_recovery(SuggestedRecovery::SuggestAdjustValue)
        })?;
        let alpha = self.save_version == 105;
        let snapshot = crate::domain::contracts::CharacterSnapshot {
            level: self.attributes.actual_value(12, alpha).unwrap_or(1).clamp(1, 99) as u8,
            experience: self.attributes.actual_value(13, alpha).unwrap_or(0).max(0) as u32,
            strength: self.attributes.actual_value(0, alpha).unwrap_or(1).max(0) as u32,
            energy: self.attributes.actual_value(1, alpha).unwrap_or(1).max(0) as u32,
            dexterity: self.attributes.actual_value(2, alpha).unwrap_or(1).max(0) as u32,
            vitality: self.attributes.actual_value(3, alpha).unwrap_or(1).max(0) as u32,
            stat_points: self.attributes.actual_value(4, alpha).unwrap_or(0).max(0) as u32,
            gold_character: self.attributes.actual_value(14, alpha).unwrap_or(0).max(0) as u32,
            gold_stash: self.attributes.actual_value(15, alpha).unwrap_or(0).max(0) as u32,
        };
        let evaluated =
            crate::domain::contracts::CharacterBoundaryValidator::validate_mutation(&snapshot, &next_edits)
                .map_err(|e| {
                    ActionRejection::new(
                        RejectionCode::RejectionStatOutOfRange,
                        "invalid_character_mutation",
                        e.to_string(),
                    )
                    .with_target("character_field")
                    .with_recovery(SuggestedRecovery::SuggestAdjustValue)
                })?;

        use crate::domain::stats::attribute::char_stat_save_add;
        if next_edits.level.is_some() {
            self.attributes
                .set_value(12, i32::from(evaluated.level), char_stat_save_add(12, alpha));
            self.char_level = evaluated.level;
        }
        if next_edits.strength.is_some() {
            self.attributes
                .set_value(0, evaluated.strength as i32, char_stat_save_add(0, alpha));
        }
        if next_edits.energy.is_some() {
            self.attributes
                .set_value(1, evaluated.energy as i32, char_stat_save_add(1, alpha));
        }
        if next_edits.dexterity.is_some() {
            self.attributes
                .set_value(2, evaluated.dexterity as i32, char_stat_save_add(2, alpha));
        }
        if next_edits.vitality.is_some() {
            self.attributes
                .set_value(3, evaluated.vitality as i32, char_stat_save_add(3, alpha));
        }
        if next_edits.stat_points.is_some() {
            self.attributes
                .set_value(4, evaluated.stat_points as i32, char_stat_save_add(4, alpha));
        }
        if next_edits.experience.is_some() {
            self.attributes
                .set_value(13, evaluated.experience as i32, char_stat_save_add(13, alpha));
            self.experience = evaluated.experience as i32;
        }
        if next_edits.gold_character.is_some() {
            self.attributes
                .set_value(14, evaluated.gold_character as i32, char_stat_save_add(14, alpha));
        }
        if next_edits.gold_stash.is_some() {
            self.attributes
                .set_value(15, evaluated.gold_stash as i32, char_stat_save_add(15, alpha));
        }
        if let Some(ref name) = next_edits.name {
            self.char_name = name.clone();
        }
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
        if self.item_cards.iter().any(|c| c.container == ContainerKind::MercReplaced) {
            return Err("Cannot export .d2s while items remain in Replaced Mercenary Staging. Reclaim, restore, or clear them first.".to_string());
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
                .enumerate()
                .filter(|(i, _)| {
                    !matches!(
                        self.item_cards[*i].container,
                        ContainerKind::MercEquipment
                            | ContainerKind::MercReplaced
                            | ContainerKind::TempInventory
                            | ContainerKind::Cursor
                    )
                })
                .map(|(_, (item, _))| item.clone())
                .collect();
            let alpha = self.item_dialect == "alpha";
            working = crate::save::rebuild_item_section(&working, &items, &huffman, alpha)
                .map_err(|e| format!("Failed to write item mutations: {e}"))?;
        }
        if self.has_merc_edits || self.has_item_edits {
            let huffman = HuffmanTree::new();
            let merc_items: Vec<Item> = self
                .player_items_with_container
                .iter()
                .enumerate()
                .filter(|(i, _)| self.item_cards[*i].container == ContainerKind::MercEquipment)
                .map(|(_, (item, _))| item.clone())
                .collect();
            let alpha = self.item_dialect == "alpha";
            working = crate::save::rebuild_mercenary_section(
                &working,
                &self.merc_state,
                &merc_items,
                self.merc_hired,
                &huffman,
                alpha,
            )
            .map_err(|e| format!("Failed to write mercenary mutations: {e}"))?;
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
    let socket_count = item
        .sockets
        .map(i32::from)
        .unwrap_or_else(|| item.socketed_items.len() as i32);
    let mut socketed_names = Vec::with_capacity(item.socketed_items.len());
    let mut socketed_items = Vec::with_capacity(item.socketed_items.len());
    for child in &item.socketed_items {
        let child_en = format_item(child, "en", 0, level);
        let child_ko = format_item(child, "ko", 0, level);
        socketed_names.push(child_en.name.clone());
        socketed_items.push(SocketedItemView {
            code: child.code.trim().to_string(),
            name_en: child_en.name,
            name_ko: child_ko.name,
            quality: ItemQualityKind::from_item(child.header.quality, child.header.is_runeword),
            image_url: resolve_item_image_url(child),
            properties: child_en.properties,
        });
    }
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
    let prop_source = if !item.properties.is_empty() {
        &item.properties
    } else {
        &item.stats.properties
    };
    let detailed_properties = prop_source
        .iter()
        .filter(|p| p.stat_id != 511)
        .map(|p| {
            let (min_value, max_value, _) = stat_value_bounds(p.stat_id).unwrap_or((0, 0, 0));
            let param_bits = crate::data::stat_costs::STAT_COSTS
                .iter()
                .find(|s| s.id == p.stat_id)
                .map(|s| s.save_param_bits as i32)
                .unwrap_or(0);
            let (value_label, param_label, kind_tag) =
                crate::engine::stat_catalog::describe_stat_input_labels(p.stat_id);
            let param_preview =
                crate::engine::stat_catalog::describe_stat_param_value(p.stat_id, p.param, "en");
            ItemPropertyView {
                stat_id: p.stat_id as i32,
                name: p.name.clone(),
                value: p.value,
                param: p.param as i32,
                param_bits,
                min_value,
                max_value,
                value_label: value_label.to_string(),
                param_label: param_label.to_string(),
                param_kind: StatParamKind::from_tag(kind_tag),
                param_preview,
            }
        })
        .collect();

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
        detailed_properties,
        defense: item.defense.map(|d| d as i32),
        current_durability: item.current_durability.map(|d| d as i32),
        max_durability: item.max_durability.map(|d| d as i32),
        quantity: item.quantity.map(|q| q as i32),
        is_ethereal: item.header.is_ethereal,
        set_name: en.set_name,
        set_pieces: en.set_pieces,
        piece_set_bonuses: map_bonuses(en.piece_set_bonuses),
        set_bonuses: map_bonuses(en.set_bonuses),
        socket_count,
        socketed_names,
        socketed_items,
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
            name: get_loc(entry.loc_key, "en").to_string(),
            name_ko: get_loc(entry.loc_key, "ko").to_string(),
            desc_en: get_loc(entry.desc_loc_key, "en").to_string(),
            desc_ko: get_loc(entry.desc_loc_key, "ko").to_string(),
        })
        .collect()
}

fn collect_quest_states(
    progression: &Progression,
    difficulty: u8,
    is_expansion: bool,
) -> Vec<QuestStateEntryView> {
    let max_act = if is_expansion { 5 } else { 4 };
    progression
        .quests
        .quests()
        .iter()
        .filter(|q| q.difficulty() == difficulty && q.act() <= max_act)
        .map(|q| {
            let status = match q.semantic_status() {
                crate::domain::progression::quest::QuestSemanticStatus::Unstarted => {
                    QuestSemanticStatusView::QuestUnstarted
                }
                crate::domain::progression::quest::QuestSemanticStatus::InProgress => {
                    QuestSemanticStatusView::QuestInProgress
                }
                crate::domain::progression::quest::QuestSemanticStatus::RewardPending => {
                    QuestSemanticStatusView::QuestRewardPending
                }
                crate::domain::progression::quest::QuestSemanticStatus::Completed => {
                    QuestSemanticStatusView::QuestCompleted
                }
            };
            QuestStateEntryView {
                key: quest_key(q.act(), q.index()),
                status,
                raw_state: i32::from(q.state()),
            }
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
            name: get_loc(entry.loc_key, "en").to_string(),
            name_ko: get_loc(entry.loc_key, "ko").to_string(),
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
    hired: bool,
    merc_items: &[&Item],
    merc_cards: &[ItemCardView],
    dismissed: &Option<(MercenaryState, i32, i32)>,
    merc_replaced_cards: &[ItemCardView],
    difficulty: DifficultyMode,
) -> MercSummaryView {
    let inferred_act = merc::inferred_merc_act(state, !merc_cards.is_empty());
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

    for item in merc_items.iter().copied() {
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

    let dismissed_view = dismissed.as_ref().map(|(d_state, d_lvl, d_act)| {
        let d_class = match *d_act {
            1 => "Rogue Scout (Act 1)".to_string(),
            2 => "Desert Mercenary (Act 2)".to_string(),
            3 => "Iron Wolf (Act 3)".to_string(),
            5 => "Barbarian (Act 5)".to_string(),
            _ => d_state.class_name(),
        };
        DismissedMercView {
            act_index: *d_act,
            class_name: d_class,
            subtype_name: d_state.subtype_name(),
            level: *d_lvl,
            remaining_items: merc_replaced_cards.len() as i32,
        }
    });

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
        persistence_supported: true,
        preview_only_reason: None,
        dismissed: dismissed_view,
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

fn validate_stat_param(
    stat_name: &str,
    save_param_bits: u8,
    stat_id: i32,
    param: i32,
) -> Result<u32, ActionRejection> {
    if save_param_bits == 0 {
        return Ok(0);
    }
    if save_param_bits > 31 {
        return Err(ActionRejection::new(
            RejectionCode::RejectionStatOutOfRange,
            "stat_param_unsupported",
            format!(
                "Stat '{stat_name}' (#{stat_id}) parameter bit width ({save_param_bits}) exceeds 31 bits."
            ),
        )
        .with_target(format!("stat_{stat_id}")));
    }
    let max_param = ((1i64 << save_param_bits) - 1) as i32;
    if param < 0 || param > max_param {
        return Err(ActionRejection::new(
            RejectionCode::RejectionStatOutOfRange,
            "stat_param_out_of_range",
            format!(
                "Stat '{stat_name}' (#{stat_id}) param {param} is outside allowed bounds 0..={max_param}."
            ),
        )
        .with_target(format!("stat_{stat_id}"))
        .with_recovery(SuggestedRecovery::SuggestAdjustValue));
    }
    Ok(param as u32)
}

fn insert_property_before_terminator(
    list: &mut Vec<crate::domain::stats::ItemProperty>,
    prop: crate::domain::stats::ItemProperty,
) {
    if let Some(last) = list.last() {
        if last.stat_id == 511 {
            let pos = list.len() - 1;
            list.insert(pos, prop);
            return;
        }
    }
    list.push(prop);
}

const STAT_TERMINATOR_ID: u32 = 511;

fn cached_stat_catalog() -> &'static [StatCatalogEntryView] {
    static STAT_CATALOG: OnceLock<Vec<StatCatalogEntryView>> = OnceLock::new();
    STAT_CATALOG.get_or_init(|| {
        let mut entries: Vec<StatCatalogEntryView> = crate::data::stat_costs::STAT_COSTS
            .iter()
            .filter(|s| s.id != STAT_TERMINATOR_ID && s.save_bits > 0 && s.save_param_bits <= 31)
            .filter_map(|s| {
                let (min_value, max_value, _) = stat_value_bounds(s.id)?;
                let (value_label, param_label, kind_tag) =
                    crate::engine::stat_catalog::describe_stat_input_labels(s.id);
                Some(StatCatalogEntryView {
                    stat_id: s.id as i32,
                    name: s.name.to_string(),
                    min_value,
                    max_value,
                    param_bits: s.save_param_bits as i32,
                    value_label: value_label.to_string(),
                    param_label: param_label.to_string(),
                    param_kind: StatParamKind::from_tag(kind_tag),
                })
            })
            .collect();
        entries.sort_by_key(|e| e.stat_id);
        entries
    })
}

fn cached_param_catalog() -> &'static ParamCatalogView {
    static PARAM_CATALOG: OnceLock<ParamCatalogView> = OnceLock::new();
    PARAM_CATALOG.get_or_init(|| {
        let (classes, elements, skill_tabs, skills) =
            crate::engine::stat_catalog::list_param_choices("en");
        let to_opts = |items: Vec<(i32, String)>| -> Vec<ParamOptionView> {
            items
                .into_iter()
                .map(|(value, label)| ParamOptionView { value, label })
                .collect()
        };
        ParamCatalogView {
            classes: to_opts(classes),
            elements: to_opts(elements),
            skill_tabs: to_opts(skill_tabs),
            skills: to_opts(skills),
            packed_skill: PackedSkillSpecView::default(),
        }
    })
}

fn set_item_placement(
    item: &mut crate::item::Item,
    x: u8,
    y: u8,
    page: u8,
    location: u8,
    mode: u8,
    stored_in: Option<u8>,
) {
    item.x = x;
    item.y = y;
    item.page = page;
    item.location = location;
    item.mode = mode;
    item.header.x = x;
    item.header.y = y;
    item.header.page = page;
    item.header.location = location;
    item.header.mode = mode;
    item.body.x = x;
    item.body.y = y;
    item.body.page = page;
    item.body.location = location;
    item.body.mode = mode;
    if let Some(aux) = item.body.d2r_v3_auxiliary.as_mut() {
        aux.stored_in = stored_in;
    }
    item.bits.clear();
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
            DismissedMercView,
            MercSummaryView,
            QuestCatalogEntryView,
            QuestSemanticStatusView,
            QuestStateEntryView,
            WaypointCatalogEntryView,
            ProgressionSummaryView,
            SetBonusView,
            ItemPropertyView,
            SocketedItemView,
            ItemCardView,
            AllowedMoveTargetView,
            SkillDamageView,
            SkillNodeView,
            ShopCatalogItemView,
            StatParamKind,
            ParamOptionView,
            PackedSkillSpecView,
            ParamCatalogView,
            ItemForgeDraftView,
            StatCatalogEntryView,
            SessionView,
            RejectionCode,
            SuggestedRecovery,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn load_test_session() -> EditorSession {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/savegames/original/amazon_10_scrolls.d2s");
        let bytes = std::fs::read(&path)
            .unwrap_or_else(|e| panic!("Failed to read fixture {}: {e}", path.display()));
        EditorSession::from_bytes(&bytes, "amazon_10_scrolls.d2s")
            .expect("fixture should parse into EditorSession")
    }

    #[test]
    fn snapshot_queue_evicts_oldest_fifo_when_capacity_reached() {
        let session = load_test_session();
        let base_snap = session.capture_snapshot();
        let mut queue = SnapshotQueue::new(3);

        for marker in 1..=5 {
            let mut snap = base_snap.clone();
            snap.active_weapon_set = marker;
            queue.push(snap);
        }

        assert_eq!(queue.len(), 3);
        assert_eq!(queue.pop().map(|s| s.active_weapon_set), Some(5));
        assert_eq!(queue.pop().map(|s| s.active_weapon_set), Some(4));
        assert_eq!(queue.pop().map(|s| s.active_weapon_set), Some(3));
        assert!(queue.pop().is_none());
        assert!(queue.is_empty());
    }

    #[test]
    fn snapshot_queue_zero_capacity_retains_nothing() {
        let session = load_test_session();
        let mut queue = SnapshotQueue::new(0);
        queue.push(session.capture_snapshot());
        assert_eq!(queue.len(), 0);
        assert!(queue.is_empty());
        assert!(queue.pop().is_none());
    }

    #[test]
    fn editor_session_caps_undo_history_with_fifo_gc() {
        let mut session = load_test_session();
        let total_edits = DEFAULT_MAX_HISTORY_SNAPSHOTS + 5;

        for step in 0..total_edits {
            let env = session.apply_intent(UiIntent::SetCharacterField {
                field: CharacterField::Level,
                value: ((step % 90) + 1) as i32,
            });
            assert!(env.ok, "edit step {step} should succeed: {:?}", env.rejection);
        }

        assert_eq!(session.undo_stack.len(), DEFAULT_MAX_HISTORY_SNAPSHOTS);

        for _ in 0..DEFAULT_MAX_HISTORY_SNAPSHOTS {
            let env = session.apply_intent(UiIntent::Undo);
            assert!(env.ok, "undo within capacity should succeed");
        }

        assert!(session.undo_stack.is_empty());
        let overflow_undo = session.apply_intent(UiIntent::Undo);
        assert!(!overflow_undo.ok);
        assert_eq!(
            overflow_undo.rejection.map(|r| r.code),
            Some(RejectionCode::RejectionNothingToUndo)
        );
    }
}
