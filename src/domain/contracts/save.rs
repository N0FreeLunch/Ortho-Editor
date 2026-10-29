//! Top-level Save Mutation DTO, unified validator, and atomic execution pipeline.
//!
//! Provides a single unified entrypoint (Unit of Work) that aggregates all sub-domain
//! mutations (character, skills, waypoints, items, batch operations), executes
//! simultaneous multi-field validation, and applies changes in a single atomic memory transaction.

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

use super::batch_item::{BatchItemContract, BatchItemWireDto};
use super::character::{
    CharacterBoundaryValidator, CharacterMutationContract, CharacterSnapshot, CharacterWireDto,
};
use super::item::{
    ItemBoundaryValidator, ItemContractError, ItemMutationContract, ItemWireDto,
};
use super::progression::{
    WaypointMutationContract, WaypointWireDto,
};
use super::skill::{
    SkillBoundaryValidator, SkillContractError, SkillMutationContract, SkillWireDto,
};
use super::validation::{FieldViolation, ValidationErrorResponse};
use schemars::JsonSchema;
use crate::domain::character::skills::parse_skill_section;
use crate::domain::item::Item;
use crate::domain::stats::AttributeSection;
use crate::item::HuffmanTree;
use crate::save::{map_core_sections, CHAR_CLASS_OFFSET};

/// Root wire DTO representing an integrated save mutation request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, JsonSchema)]
pub struct SaveMutationDto {
    /// Character attributes, level, and stats.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub character: Option<CharacterWireDto>,

    /// Waypoint activation across difficulties.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub waypoints: Option<WaypointWireDto>,

    /// Character skills and respec.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skills: Option<SkillWireDto>,

    /// Granular single-item mutations.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<ItemWireDto>,

    /// Mass/bulk item operations (repair all, identify all, socket all).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch_items: Option<BatchItemWireDto>,
}

/// Infallible top-level domain contract for atomic save mutation.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SaveMutationContract {
    pub character: Option<CharacterMutationContract>,
    pub waypoints: Option<WaypointMutationContract>,
    pub skills: Option<SkillMutationContract>,
    pub items: Vec<ItemMutationContract>,
    pub batch_items: Option<BatchItemContract>,
}

/// Unified multi-domain boundary validator.
pub struct SaveMutationValidator;

impl SaveMutationValidator {
    /// Validates all requested mutations simultaneously against current savefile state.
    ///
    /// If ANY field fails validation across any section, all violations are aggregated
    /// and returned as a single RFC 7807 422 `ValidationErrorResponse`.
    pub fn validate(
        dto: &SaveMutationDto,
        save_bytes: &[u8],
        huffman: &HuffmanTree,
    ) -> Result<SaveMutationContract, ValidationErrorResponse> {
        let mut errors: HashMap<String, Vec<FieldViolation>> = HashMap::new();

        let map = map_core_sections(save_bytes).map_err(|e| {
            ValidationErrorResponse::single(
                "save",
                FieldViolation::new(
                    "corrupted_save",
                    format!("Failed to parse save sections: {}", e),
                    "Ensure the input savefile is valid and uncorrupted.",
                ),
            )
        })?;

        let version = u32::from_le_bytes(save_bytes[4..8].try_into().unwrap_or([0; 4]));
        let is_alpha = version == 105;
        let char_class = if CHAR_CLASS_OFFSET < save_bytes.len() {
            save_bytes[CHAR_CLASS_OFFSET]
        } else {
            0
        };

        let attrs = AttributeSection::parse(save_bytes, map.gf_pos, map.if_pos).ok();
        let current_skill_points = attrs
            .as_ref()
            .and_then(|a| a.actual_value(5, is_alpha))
            .map(|v| v.max(0) as u8)
            .unwrap_or(0);

        let skills_section = parse_skill_section(
            save_bytes,
            map.if_pos,
            map.jm_positions.first().copied(),
        )
        .ok();

        // 1. Character Boundary Validation
        let validated_character = if let Some(ref char_dto) = dto.character {
            match CharacterMutationContract::try_from(char_dto.clone()) {
                Ok(char_contract) => {
                    let snapshot = CharacterSnapshot {
                        level: attrs.as_ref().and_then(|a| a.actual_value(12, is_alpha)).unwrap_or(1).clamp(1, 99) as u8,
                        experience: attrs.as_ref().and_then(|a| a.actual_value(13, is_alpha)).unwrap_or(0).max(0) as u32,
                        strength: attrs.as_ref().and_then(|a| a.actual_value(0, is_alpha)).unwrap_or(1).max(0) as u32,
                        energy: attrs.as_ref().and_then(|a| a.actual_value(1, is_alpha)).unwrap_or(1).max(0) as u32,
                        dexterity: attrs.as_ref().and_then(|a| a.actual_value(2, is_alpha)).unwrap_or(1).max(0) as u32,
                        vitality: attrs.as_ref().and_then(|a| a.actual_value(3, is_alpha)).unwrap_or(1).max(0) as u32,
                        stat_points: attrs.as_ref().and_then(|a| a.actual_value(4, is_alpha)).unwrap_or(0).max(0) as u32,
                        gold_character: attrs.as_ref().and_then(|a| a.actual_value(14, is_alpha)).unwrap_or(0).max(0) as u32,
                        gold_stash: attrs.as_ref().and_then(|a| a.actual_value(15, is_alpha)).unwrap_or(0).max(0) as u32,
                    };

                    match CharacterBoundaryValidator::validate_mutation(&snapshot, &char_contract) {
                        Ok(_) => Some(char_contract),
                        Err(err) => {
                            let resp = ValidationErrorResponse::from(err);
                            for (k, v) in resp.errors {
                                errors.entry(k).or_default().extend(v);
                            }
                            None
                        }
                    }
                }
                Err(err) => {
                    let resp = ValidationErrorResponse::from(err);
                    for (k, v) in resp.errors {
                        errors.entry(k).or_default().extend(v);
                    }
                    None
                }
            }
        } else {
            None
        };

        // 2. Waypoint Validation
        let validated_waypoints = if let Some(ref wp_dto) = dto.waypoints {
            match WaypointMutationContract::try_from(wp_dto.clone()) {
                Ok(contract) => Some(contract),
                Err(err) => {
                    let resp = ValidationErrorResponse::from(err);
                    for (k, v) in resp.errors {
                        errors.entry(k).or_default().extend(v);
                    }
                    None
                }
            }
        } else {
            None
        };

        // 3. Skills Validation
        let validated_skills = if let Some(ref skill_dto) = dto.skills {
            match SkillBoundaryValidator::validate(
                skill_dto,
                char_class,
                skills_section.as_ref(),
                current_skill_points,
            ) {
                Ok(contract) => Some(contract),
                Err(SkillContractError::FieldViolations(errs)) => {
                    for (k, v) in errs {
                        errors.entry(k).or_default().extend(v);
                    }
                    None
                }
                Err(err) => {
                    let resp = ValidationErrorResponse::from(err);
                    for (k, v) in resp.errors {
                        errors.entry(k).or_default().extend(v);
                    }
                    None
                }
            }
        } else {
            None
        };

        // 4. Item Mutations Validation
        let (items, slot_lookup): (Vec<Item>, Box<dyn Fn(usize) -> Option<&'static str>>) =
            if let Ok(stream) = crate::domain::item::stream::PlayerItemStream::read(save_bytes, huffman) {
                let slots: Vec<Option<&'static str>> = stream
                    .items
                    .iter()
                    .map(|it| stream.placement(it).equipment_slot)
                    .collect();
                (stream.items, Box::new(move |idx| slots.get(idx).copied().flatten()))
            } else if let Ok(fallback) = Item::read_player_items(save_bytes, huffman, is_alpha) {
                (fallback, Box::new(|_| None))
            } else {
                (Vec::new(), Box::new(|_| None))
            };

        let mut validated_items = Vec::new();
        for (item_idx, item_dto) in dto.items.iter().enumerate() {
            let field_prefix = format!("items[{}]", item_idx);

            // Find matching item
            let mut matched = None;
            for (idx, it) in items.iter().enumerate() {
                let slot = slot_lookup(idx);
                if item_dto.target.matches(idx, it, slot) {
                    matched = Some(it);
                    break;
                }
            }

            let target_item = match matched {
                Some(it) => it,
                None => {
                    errors.entry(format!("{}.target", field_prefix)).or_default().push(
                        FieldViolation::new(
                            "item_not_found",
                            format!(
                                "No item found matching criteria (index={:?}, code={:?}, slot={:?}).",
                                item_dto.target.index, item_dto.target.code, item_dto.target.slot
                            ),
                            "Check target index, 3-letter item code, or equipment slot name.",
                        ),
                    );
                    continue;
                }
            };

            match ItemBoundaryValidator::validate(item_dto, Some(target_item)) {
                Ok(contract) => validated_items.push(contract),
                Err(ItemContractError::FieldViolations(errs)) => {
                    for (k, v) in errs {
                        let prefixed = k.replace("item.", &format!("{}.", field_prefix));
                        errors.entry(prefixed).or_default().extend(v);
                    }
                }
                Err(err) => {
                    let resp = ValidationErrorResponse::from(err);
                    for (k, v) in resp.errors {
                        let prefixed = k.replace("item.", &format!("{}.", field_prefix));
                        errors.entry(prefixed).or_default().extend(v);
                    }
                }
            }
        }

        // 5. Batch Items Validation
        let validated_batch_items = dto.batch_items.clone().map(BatchItemContract::from);

        if !errors.is_empty() {
            return Err(ValidationErrorResponse {
                status: "validation_error",
                code: 422,
                message: "Validation failed for save mutation payload.",
                errors,
            });
        }

        Ok(SaveMutationContract {
            character: validated_character,
            waypoints: validated_waypoints,
            skills: validated_skills,
            items: validated_items,
            batch_items: validated_batch_items,
        })
    }
}

/// Atomic execution pipeline for validated save contracts.
pub struct SaveMutationPipeline;

impl SaveMutationPipeline {
    /// Applies all mutations defined in the contract atomically in memory.
    pub fn execute(
        save_bytes: &[u8],
        contract: &SaveMutationContract,
        huffman: &HuffmanTree,
    ) -> Result<Vec<u8>, ValidationErrorResponse> {
        let mut current = save_bytes.to_vec();

        // 1. Character profile & attributes
        if let Some(ref char_c) = contract.character {
            current = crate::save::try_apply_character_mutation(&current, char_c, huffman)
                .map_err(ValidationErrorResponse::from)?;
        }

        // 2. Waypoints
        if let Some(ref wp_c) = contract.waypoints {
            current = crate::save::try_apply_waypoint_mutation(&current, wp_c)
                .map_err(ValidationErrorResponse::from)?;
        }

        // 3. Skills & Respec
        if let Some(ref skill_c) = contract.skills {
            current = crate::save::try_apply_skill_mutation(&current, skill_c, huffman)
                .map_err(ValidationErrorResponse::from)?;
        }

        // 4. Batch Item Operations
        if let Some(ref batch_c) = contract.batch_items {
            current = crate::save::try_apply_batch_item_mutation(&current, batch_c, huffman)
                .map_err(ValidationErrorResponse::from)?;
        }

        // 5. Individual Items
        for item_c in &contract.items {
            current = crate::save::try_apply_item_mutation(&current, item_c, huffman)
                .map_err(ValidationErrorResponse::from)?;
        }

        Ok(current)
    }
}
