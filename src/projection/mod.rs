//! Rust-as-Store projection layer and `elm_rs` protocol boundary for Ortho Editor.
//!
//! This module projects parsed save data into immutable `SessionView` snapshots for Elm,
//! accepts typed `UiIntent` commands, validates invariants in Rust, and exports the
//! single-source-of-truth Elm `Generated.Protocol` module via `elm_rs`.

pub mod host;

use std::collections::BTreeSet;

use elm_rs::{Elm, ElmDecode, ElmEncode};
use serde::{Deserialize, Serialize};

use validator::Validate;

use crate::domain::character::skills::parse_skill_section;
use crate::domain::contracts::NumericOp;
use crate::domain::contracts::character::{
    CharacterMutationContract, CharacterWireDto, LevelValue, StashGoldValue, StatPointValue,
};
use crate::domain::forensic::v105::mercenary::MercenaryState;
use crate::domain::header::axiom::EXPANSION_FLAG_OFFSET;
use crate::domain::item::stream::{ItemContainer, PlayerItemStream};
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
use crate::engine::mercenary_items::{collect_mercenary_items, mercenary_item_slot};
use crate::inventory::get_item_size;
use crate::item::{HuffmanTree, Item, ItemQuality};
use crate::save::{
    AttributeSection, Save, class_name, class_skill_base_id, map_core_sections,
    try_apply_character_mutation,
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

/// Projected item summary for View A container grids and View B mercenary slots.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Elm, ElmEncode, ElmDecode)]
pub struct ItemCardView {
    pub index: i32,
    pub code: String,
    pub name_en: String,
    pub name_ko: String,
    pub quality: String,
    pub container: String,
    pub slot_label: Option<String>,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub properties: Vec<String>,
}

/// Complete Elm-facing read-only save session snapshot projected by Rust.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Elm, ElmEncode, ElmDecode)]
pub struct SessionView {
    pub fixture_label: String,
    pub save_version: i32,
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
    ClearTempInventory,
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
    source_bytes: Vec<u8>,
    dirty: bool,
    fixture_label: String,
    save_version: i32,
    item_dialect: String,
    char_name: String,
    char_class: String,
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
    active_skill_count: i32,
    char_edits: CharacterMutationContract,
    has_char_edits: bool,
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
        if let Ok(parsed_skills) =
            parse_skill_section(bytes, map.if_pos, map.jm_positions.first().copied())
        {
            if let Some(base_id) = class_skill_base_id(save.header.char_class) {
                active_skill_count = parsed_skills
                    .iter_skills(base_id)
                    .filter(|skill| skill.level > 0)
                    .count() as i32;
            }
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
            let container_name = container_label(placement.container).to_string();
            item_cards.push(build_item_card(
                idx as i32,
                item,
                char_level,
                active_set_count,
                container_name,
                placement.equipment_slot.map(str::to_string),
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
                "mercenary".to_string(),
                Some(slot.to_string()),
                i32::from(item.x),
                i32::from(item.y),
            ));
            merc_items.push(item);
        }

        Ok(Self {
            source_bytes: bytes.to_vec(),
            dirty: false,
            fixture_label: fixture_label.into(),
            save_version: save.header.version as i32,
            item_dialect: dialect_str,
            char_name: save.header.char_name.clone(),
            char_class: class_name(save.header.char_class).to_string(),
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
            active_skill_count,
            char_edits: CharacterMutationContract::default(),
            has_char_edits: false,
        })
    }

    /// Seeds temporary inventory dock items (used for testing dock clear flows).
    pub fn push_temp_item(&mut self, card: ItemCardView) {
        self.temp_items.push(card);
    }

    /// Projects the current store state into an immutable `SessionView` for Elm.
    pub fn project(&self) -> SessionView {
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
        for (_, container) in &self.player_items_with_container {
            match container {
                ItemContainer::Equipment => counts.equipment += 1,
                ItemContainer::Inventory => counts.inventory += 1,
                ItemContainer::Belt => counts.belt += 1,
                ItemContainer::Stash => counts.stash += 1,
                ItemContainer::Cube => counts.cube += 1,
                ItemContainer::Unknown => counts.unknown += 1,
            }
        }

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

        SessionView {
            fixture_label: self.fixture_label.clone(),
            save_version: self.save_version,
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
        }
    }

    /// Applies one `UiIntent` in Rust and returns a `SessionEnvelope`.
    pub fn apply_intent(&mut self, intent: UiIntent) -> SessionEnvelope {
        match self.apply_intent_inner(intent) {
            Ok(()) => SessionEnvelope {
                ok: true,
                session: Some(self.project()),
                rejection: None,
            },
            Err(rejection) => SessionEnvelope {
                ok: false,
                session: Some(self.project()),
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
                self.merc_level = level;
                Ok(())
            }
            UiIntent::SelectMercAct { act_index } => {
                if !matches!(act_index, 1 | 2 | 3 | 5) {
                    return Err(ActionRejection {
                        code: "invalid_merc_act".to_string(),
                        message: format!("Mercenary act must be 1, 2, 3, or 5 (got {act_index})"),
                    });
                }
                self.merc_act_override = Some(act_index);
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
            UiIntent::ClearTempInventory => {
                self.temp_items.clear();
                Ok(())
            }
        }
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
        if self.save_version != 105 {
            return Err(format!(
                "Binary writeback only supports v105 saves (got v{})",
                self.save_version
            ));
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
        // Sync quests first; `Progression::sync_to_bytes` also writes `self.progression.waypoints`,
        // so per-difficulty `waypoints_by_diff` are written immediately afterward.
        self.progression.sync_to_bytes(&mut working, true);
        let wp_anchor = PROG_START_FILE + V105WaypointAxiom::start_offset();
        for wp_set in &self.waypoints_by_diff {
            wp_set.sync_to_bytes(&mut working[V105_WAYPOINT_OFFSET..], wp_anchor);
        }
        crate::save::finalize_save_bytes(&mut working, false)
            .map_err(|e| format!("Failed to finalize exported save checksum: {e}"))?;
        Ok(working)
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

#[allow(clippy::too_many_arguments)]
fn build_item_card(
    index: i32,
    item: &Item,
    level: u8,
    active_set_count: usize,
    container: String,
    slot_label: Option<String>,
    x: i32,
    y: i32,
) -> ItemCardView {
    let en = format_item(item, "en", active_set_count, level);
    let ko = format_item(item, "ko", active_set_count, level);
    let (w, h) = get_item_size(&item.code);
    ItemCardView {
        index,
        code: item.code.trim().to_string(),
        name_en: en.name,
        name_ko: ko.name,
        quality: quality_label(item.header.quality, item.header.is_runeword).to_string(),
        container,
        slot_label,
        x,
        y,
        width: i32::from(w),
        height: i32::from(h),
        properties: en.properties,
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
    }
}

/// Exports the Elm `Generated.Protocol` module source string via `elm_rs::export!`.
pub fn generate_elm_protocol_module() -> Result<String, String> {
    let mut target = Vec::<u8>::new();
    elm_rs::export!("Generated.Protocol", &mut target, {
        encoders: [
            DifficultyMode,
            EditMode,
            CharacterField,
            UiIntent,
        ],
        decoders: [
            DifficultyMode,
            EditMode,
            ResistanceCardView,
            CharacterSchemaView,
            CharacterStatsView,
            ContainerCountsView,
            MercStatsView,
            MercSummaryView,
            QuestCatalogEntryView,
            WaypointCatalogEntryView,
            ProgressionSummaryView,
            ItemCardView,
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
