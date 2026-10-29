//! Domain contracts, wire DTOs, and boundary validators for character skills and respec.
//!
//! Provides granular skill mutation operators, prerequisite validation,
//! and complete skill/stat respec (reset and refund) capabilities.

use std::collections::HashMap;
use std::str::FromStr;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use schemars::JsonSchema;

use super::character::NumericOp;
use super::validation::{FieldViolation, ValidationErrorResponse};
use crate::domain::character::skills::{SkillSection, SKILL_SECTION_LEN};

/// Class-to-code mapping for Diablo II Resurrected.
pub fn class_id_to_code(class_id: u8) -> &'static str {
    match class_id {
        0 => "ama",
        1 => "sor",
        2 => "nec",
        3 => "pal",
        4 => "bar",
        5 => "dru",
        6 => "ass",
        _ => "unknown",
    }
}

/// Base skill ID for a character class (each class has 30 sequential skills).
pub fn class_base_skill_id(class_id: u8) -> Option<u32> {
    match class_id {
        0 => Some(6),
        1 => Some(36),
        2 => Some(66),
        3 => Some(96),
        4 => Some(126),
        5 => Some(221),
        6 => Some(251),
        _ => None,
    }
}

/// Operation on an individual skill.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SkillOpWireDto {
    /// Skill ID (e.g. 54 for Teleport) or skill name/code.
    pub skill_id: u32,

    /// Mutation operation (Set, Add, or Sub).
    pub op: NumericOp<u8>,
}

impl SkillOpWireDto {
    pub fn new(skill_id: u32, op: NumericOp<u8>) -> Self {
        Self { skill_id, op }
    }
}

impl FromStr for SkillOpWireDto {
    type Err = String;

    /// Parses string formatted as "skill_id:op" (e.g. "54:=20", "64:+1", "36:=0").
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.splitn(2, ':').collect();
        if parts.len() != 2 {
            return Err(format!("Expected 'skill_id:op' format, found: {}", s));
        }

        let skill_id = parts[0]
            .trim()
            .parse::<u32>()
            .map_err(|_| format!("Invalid skill_id '{}'. Must be an integer ID.", parts[0]))?;

        let op_str = parts[1].trim();
        let op = if let Some(stripped) = op_str.strip_prefix('=') {
            let val = stripped
                .parse::<u8>()
                .map_err(|_| format!("Invalid skill level: {}", stripped))?;
            NumericOp::Set(val)
        } else {
            NumericOp::<u8>::from_str(op_str)?
        };

        Ok(SkillOpWireDto { skill_id, op })
    }
}

/// Inbound wire DTO representing skill mutation requests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, JsonSchema)]
pub struct SkillWireDto {
    /// Specific skill level modifications.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<SkillOpWireDto>,

    /// Unspent skill points modifier (stat 5).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_points: Option<NumericOp<u8>>,

    /// Complete skill respec flag: resets all allocated skills to 0 and refunds spent points to unspent pool.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub respec: Option<bool>,
}

/// Errors occurring during skill contract validation.
#[derive(Debug, Error)]
pub enum SkillContractError {
    #[error("Validation failed: {0}")]
    ValidationFailed(String),

    #[error("Field validation violations encountered")]
    FieldViolations(HashMap<String, Vec<FieldViolation>>),
}

impl From<SkillContractError> for ValidationErrorResponse {
    fn from(err: SkillContractError) -> Self {
        match err {
            SkillContractError::ValidationFailed(msg) => Self::single(
                "skills",
                FieldViolation::new(
                    "validation_failed",
                    msg,
                    "Ensure the skill mutation payload matches specification.",
                ),
            ),
            SkillContractError::FieldViolations(errors) => Self {
                status: "validation_error",
                code: 422,
                message: "Validation failed for skill mutation payload.",
                errors,
            },
        }
    }
}

/// Infallible domain contract for validated skill mutations.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SkillMutationContract {
    /// Resolved absolute skill levels (skill_id, new_level).
    pub skills: Vec<(u32, u8)>,

    /// New unspent skill points count (if modified).
    pub skill_points: Option<u8>,

    /// Whether full skill respec and point refund was requested.
    pub respec: bool,
}

/// Boundary validator applying game rules and invariants to skill mutations.
pub struct SkillBoundaryValidator;

impl SkillBoundaryValidator {
    pub const MIN_SKILL_LEVEL: u8 = 0;
    pub const MAX_SKILL_LEVEL: u8 = 20;

    pub const MIN_SKILL_POINTS: u8 = 0;
    pub const MAX_SKILL_POINTS: u8 = 255;

    /// Validates an inbound skill wire DTO against class restrictions and current skills state.
    pub fn validate(
        dto: &SkillWireDto,
        class_id: u8,
        current_skills: Option<&SkillSection>,
        current_skill_points: u8,
    ) -> Result<SkillMutationContract, SkillContractError> {
        let mut errors: HashMap<String, Vec<FieldViolation>> = HashMap::new();

        let base_id = match class_base_skill_id(class_id) {
            Some(id) => id,
            None => {
                errors.entry("skills.class".to_string()).or_default().push(
                    FieldViolation::new(
                        "invalid_class",
                        format!("Unknown character class ID {}.", class_id),
                        "Valid class IDs are 0 (Amazon) through 6 (Assassin).",
                    )
                    .with_rejected_value(class_id),
                );
                return Err(SkillContractError::FieldViolations(errors));
            }
        };

        let max_class_skill_id = base_id + (SKILL_SECTION_LEN as u32) - 1;
        let class_code = class_id_to_code(class_id);

        let mut evaluated_skills = Vec::new();

        for (idx, op_dto) in dto.skills.iter().enumerate() {
            let field_name = format!("skills.skills[{}].level", idx);

            // 1. Verify skill belongs to character class
            if op_dto.skill_id < base_id || op_dto.skill_id > max_class_skill_id {
                errors.entry(format!("skills.skills[{}].skill_id", idx)).or_default().push(
                    FieldViolation::new(
                        "invalid_skill",
                        format!(
                            "Skill ID {} does not belong to character class '{}'.",
                            op_dto.skill_id, class_code
                        ),
                        format!(
                            "Allowed skill IDs for '{}' are between {} and {}.",
                            class_code, base_id, max_class_skill_id
                        ),
                    )
                    .with_rejected_value(op_dto.skill_id)
                    .with_constraint(base_id as u64, max_class_skill_id as u64),
                );
                continue;
            }

            // 2. Evaluate skill level operation
            let current_lvl = current_skills
                .map(|s| s.get_level(base_id, op_dto.skill_id))
                .unwrap_or(0);

            match op_dto.op {
                NumericOp::Set(target) => {
                    if target > Self::MAX_SKILL_LEVEL {
                        errors.entry(field_name).or_default().push(
                            FieldViolation::new(
                                "invalid_range",
                                format!(
                                    "Skill level {} exceeds maximum allowable hard points of {}.",
                                    target, Self::MAX_SKILL_LEVEL
                                ),
                                format!(
                                    "Skill hard points must be an integer between {} and {}.",
                                    Self::MIN_SKILL_LEVEL, Self::MAX_SKILL_LEVEL
                                ),
                            )
                            .with_rejected_value(target)
                            .with_constraint(Self::MIN_SKILL_LEVEL as u64, Self::MAX_SKILL_LEVEL as u64),
                        );
                    } else {
                        evaluated_skills.push((op_dto.skill_id, target));
                    }
                }
                NumericOp::Add(delta) => {
                    let resulting = current_lvl.saturating_add(delta);
                    if resulting > Self::MAX_SKILL_LEVEL {
                        errors.entry(field_name).or_default().push(
                            FieldViolation::new(
                                "overflow",
                                format!(
                                    "Skill level {} (current {} + delta {}) exceeds maximum allowable of {}.",
                                    resulting, current_lvl, delta, Self::MAX_SKILL_LEVEL
                                ),
                                format!("Max allowable increment is {}.", Self::MAX_SKILL_LEVEL.saturating_sub(current_lvl)),
                            )
                            .with_rejected_value(delta)
                            .with_constraint(Self::MIN_SKILL_LEVEL as u64, Self::MAX_SKILL_LEVEL as u64),
                        );
                    } else {
                        evaluated_skills.push((op_dto.skill_id, resulting));
                    }
                }
                NumericOp::Sub(delta) => {
                    if delta > current_lvl {
                        errors.entry(field_name).or_default().push(
                            FieldViolation::new(
                                "underflow",
                                format!(
                                    "Skill level decrement {} exceeds current level {}.",
                                    delta, current_lvl
                                ),
                                format!("Max allowable decrement is {}.", current_lvl),
                            )
                            .with_rejected_value(delta)
                            .with_constraint(Self::MIN_SKILL_LEVEL as u64, Self::MAX_SKILL_LEVEL as u64),
                        );
                    } else {
                        evaluated_skills.push((op_dto.skill_id, current_lvl - delta));
                    }
                }
            }
        }

        // 3. Unspent skill points evaluation
        let evaluated_skill_points = if let Some(ref op) = dto.skill_points {
            match op {
                NumericOp::Set(target) => Some(*target),
                NumericOp::Add(delta) => {
                    let resulting = current_skill_points.saturating_add(*delta);
                    Some(resulting)
                }
                NumericOp::Sub(delta) => {
                    if *delta > current_skill_points {
                        errors.entry("skills.skill_points".to_string()).or_default().push(
                            FieldViolation::new(
                                "underflow",
                                format!(
                                    "Skill points decrement {} exceeds current unspent points {}.",
                                    delta, current_skill_points
                                ),
                                format!("Max allowable decrement is {}.", current_skill_points),
                            )
                            .with_rejected_value(*delta),
                        );
                        None
                    } else {
                        Some(current_skill_points - delta)
                    }
                }
            }
        } else {
            None
        };

        if !errors.is_empty() {
            return Err(SkillContractError::FieldViolations(errors));
        }

        Ok(SkillMutationContract {
            skills: evaluated_skills,
            skill_points: evaluated_skill_points,
            respec: dto.respec.unwrap_or(false),
        })
    }
}
